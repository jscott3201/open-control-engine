//! Array #5 normalization machinery (M1-PR-9; doc 04 §3.6.1) — the resolver-owned expansion of a
//! **preserved** array parameter (`isArray=true`) into per-element scalar entries keyed by the
//! 1-based row-major underscore name (`k[2]` → `k_1`,`k_2`; `B[2,2]` → `B_1_1`,`B_1_2`,`B_2_1`,
//! `B_2_2`, last index fastest). Split out of `resolve.rs` to keep that file under the 700-LOC cap;
//! the only entry point the resolver calls is [`expand_array_param`].

use std::collections::HashSet;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;
use oce_model::Value;

use crate::dto::{CxfValue, Node};
use crate::ground::{ParamScope, ground_value};
use crate::resolve::local_name;

/// Strip a trailing array-decoration `[...]` from a local name: `k[2]` → `k`, `k` → `k`. Total —
/// used so the preserved encoding's decorated base (`k[2]`) yields the same `k` base the flattened
/// encoding's element `@id`s (`…k_1`) reduce to via [`local_name`] (M1-PR-9, doc 04 §3.6.1).
fn strip_array_label(name: &str) -> &str {
    match name.split_once('[') {
        Some((base, _)) => base,
        None => name,
    }
}

/// Parse a CXF `S231:sizeOfDimensions` string `"(d1, d2, …)"` into per-dimension sizes (doc 04
/// §3.6.1). Each `di` is a non-negative integer literal, or a symbolic expression evaluated against
/// `scope` to an `Integer` (so `"(nin)"` resolves when `nin` is a ground *earlier* parameter — the
/// same forward-reference limitation Step 7 already has for scalar `Expr` bindings). Returns the
/// dimension sizes in declared order, or a human message for a `MalformedDocument`. Total; never
/// panics (no `unwrap`/index on input text — the C1 type-domain discipline).
fn parse_size_dims(
    size: &str,
    n_dims: Option<i64>,
    scope: &dyn oce_expr::Scope,
) -> Result<Vec<usize>, String> {
    let inner = size
        .trim()
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| format!("sizeOfDimensions {size:?} is not parenthesized"))?;
    let mut dims: Vec<usize> = Vec::new();
    for part in inner.split(',') {
        let term = part.trim();
        if term.is_empty() {
            return Err(format!(
                "empty array dimension in sizeOfDimensions {size:?}"
            ));
        }
        // A bare non-negative integer literal first; else evaluate the term symbolically.
        let n: i64 = if let Ok(lit) = term.parse::<i64>() {
            lit
        } else {
            match oce_expr::eval_str(term, scope) {
                Ok(EvalResult::Scalar(Value::Integer(v))) => v,
                Ok(_) => {
                    return Err(format!(
                        "array dimension {term:?} did not evaluate to an Integer"
                    ));
                }
                Err(e) => return Err(format!("array dimension {term:?} did not evaluate: {e}")),
            }
        };
        if n < 0 {
            return Err(format!(
                "negative array dimension {n} in sizeOfDimensions {size:?}"
            ));
        }
        dims.push(n as usize);
    }
    if let Some(nd) = n_dims
        && (nd < 0 || nd as usize != dims.len())
    {
        return Err(format!(
            "numberDimensions {nd} disagrees with sizeOfDimensions arity {}",
            dims.len()
        ));
    }
    Ok(dims)
}

/// Maximum number of elements one array parameter may expand to in M1 — a DoS / OOM-abort guard.
/// The element count is purely input-derived from untrusted CXF; without a ceiling a single
/// `sizeOfDimensions "(2000000000)"` (or a symbolic dimension resolving to a huge earlier param)
/// would drive a multi-gigabyte `Vec::with_capacity` that aborts the (embeddable, safety-critical)
/// process *uncatchably*, defeating the panic-free typed-`OcError` contract (M1 exit #6). `1 << 20`
/// is far beyond any realistic equipment-scale parameter array; revisit in M2 if a legitimate model
/// ever approaches it (doc 04 §3.6.1).
const MAX_ARRAY_ELEMENTS: usize = 1 << 20;

/// Enumerate the 1-based row-major element names of an array `base` with the given per-dimension
/// sizes (doc 04 §3.6.1): `("k", &[2])` → `["k_1","k_2"]`; `("B", &[2,2])` →
/// `["B_1_1","B_1_2","B_2_1","B_2_2"]` (last index varies fastest). Empty when any dimension is 0
/// (a legal empty array). Returns `Err(msg)` (→ `MalformedDocument`) when the element count
/// overflows `usize` **or** exceeds [`MAX_ARRAY_ELEMENTS`] — the ceiling is checked *before* any
/// allocation, so a hostile dimension can never drive an OOM abort. Pure function of `base` + `dims`
/// — never map order, so deterministic.
fn array_element_names(base: &str, dims: &[usize]) -> Result<Vec<String>, String> {
    let mut count: usize = 1;
    for &d in dims {
        count = count
            .checked_mul(d)
            .ok_or_else(|| "array element count overflows usize".to_owned())?;
    }
    if count > MAX_ARRAY_ELEMENTS {
        return Err(format!(
            "array element count {count} exceeds the maximum supported ({MAX_ARRAY_ELEMENTS})"
        ));
    }
    let mut names = Vec::with_capacity(count);
    let mut idx = vec![1usize; dims.len()]; // odometer of 1-based indices
    for _ in 0..count {
        let mut name = String::from(base);
        for &i in &idx {
            name.push('_');
            name.push_str(&i.to_string());
        }
        names.push(name);
        // Increment the odometer from the last (fastest) dimension.
        for d in (0..dims.len()).rev() {
            idx[d] += 1;
            if idx[d] <= dims[d] {
                break;
            }
            idx[d] = 1;
        }
    }
    Ok(names)
}

/// Expand one **preserved** array parameter (`isArray=true`) into per-element scalar entries on the
/// owning instance's `table`/`scope_entries`, in 1-based row-major order (doc 04 §3.6.1, M1-PR-9).
/// The flattened encoding (separate `k_1`/`k_2` scalar nodes) needs no expansion — it is the
/// convergence target — so both encodings yield the identical ordered `ParamTable`.
///
/// Value rule: the binding must be a [`CxfValue::List`] of element literals; `m == N` is positional
/// (including the empty `0 == 0` case), `m == 1` broadcasts the single value to all `N` **when
/// `N >= 1`** (the structural `fill(value, N)` equivalent). A non-empty list against a declared
/// *empty* (size-0) array — or any other length — is `GroundingFailed`; broadcasting one value into
/// a zero-element array would otherwise silently drop it (even a malformed value), accepting broken
/// input as valid. An array *expression* (`fill(...)`) arrives as [`CxfValue::Expr`] (not a `List`)
/// and is rejected `GroundingFailed` — array `oce-expr` is M2.
/// Every failure is a typed diagnostic; never panics (no `unwrap`/index on input-derived data).
#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_array_param(
    piri: &str,
    pnode: &Node,
    cxf_val: &CxfValue,
    param_iris: &[&str],
    table: &mut Vec<(Arc<str>, Value)>,
    scope_entries: &mut Vec<(Arc<str>, EvalResult)>,
    diags: &mut Vec<Diagnostic>,
) {
    let base = strip_array_label(local_name(piri));
    let Some(size) = pnode.size_dims.as_deref() else {
        diags.push(
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "array parameter (isArray) lacks sizeOfDimensions",
            )
            .with_subject(piri.to_owned()),
        );
        return;
    };
    let dims = match parse_size_dims(size, pnode.n_dims, &ParamScope::new(&scope_entries[..])) {
        Ok(d) => d,
        Err(msg) => {
            diags.push(
                Diagnostic::error(DiagCode::MalformedDocument, msg).with_subject(piri.to_owned()),
            );
            return;
        }
    };
    let names = match array_element_names(base, &dims) {
        Ok(n) => n,
        Err(msg) => {
            diags.push(
                Diagnostic::error(DiagCode::MalformedDocument, msg).with_subject(piri.to_owned()),
            );
            return;
        }
    };
    // Preserved per-element values are a JSON list; an array EXPRESSION (fill/comprehension) is M2.
    let CxfValue::List(elems) = cxf_val else {
        diags.push(
            Diagnostic::error(
                DiagCode::GroundingFailed,
                "array parameter value must be a JSON list of element literals \
                 (array expressions such as fill(...) are M2)",
            )
            .with_subject(piri.to_owned()),
        );
        return;
    };
    let n = names.len();
    let m = elems.len();
    // Positional (m == n, incl. the empty 0 == 0 case) or broadcast (one value to N >= 1 elements).
    // A non-empty list against a declared empty (size-0) array is NOT a broadcast — the value would
    // be silently dropped — so it falls through to this GroundingFailed length error.
    if !(m == n || (m == 1 && n >= 1)) {
        diags.push(
            Diagnostic::error(
                DiagCode::GroundingFailed,
                format!(
                    "array value list has {m} element(s) but the declared dimensions imply {n}"
                ),
            )
            .with_subject(piri.to_owned()),
        );
        return;
    }
    // Sibling local-names (every OTHER param node on this instance) for the minted-name collision
    // check. Lookup-only set — never iterated into a model id/vector order (determinism contract).
    // M1 scope: flat single-level instances (no hasInstance nesting); revisit name scoping in M2.
    let siblings: HashSet<&str> = param_iris
        .iter()
        .filter(|&&p| p != piri)
        .map(|&p| local_name(p))
        .collect();
    for (k, ename) in names.iter().enumerate() {
        if siblings.contains(ename.as_str()) {
            diags.push(
                Diagnostic::error(
                    DiagCode::ArrayFlattenCollision,
                    format!(
                        "array element {ename:?} collides with an existing sibling parameter of the same name"
                    ),
                )
                .with_subject(piri.to_owned()),
            );
            continue;
        }
        let elem = if m == 1 { &elems[0] } else { &elems[k] };
        match ground_value(elem, &ParamScope::new(&scope_entries[..])) {
            Ok(v) => {
                let key: Arc<str> = Arc::from(ename.as_str());
                scope_entries.push((Arc::clone(&key), EvalResult::Scalar(v.clone())));
                table.push((key, v));
            }
            Err(e) => diags.push(
                Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                    .with_subject(piri.to_owned()),
            ),
        }
    }
}
