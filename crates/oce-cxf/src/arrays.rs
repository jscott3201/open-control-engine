//! Array-parameter normalization machinery (doc 04 §3.6.1) — the resolver-owned expansion of a
//! **preserved** array parameter (`isArray=true`) into per-element scalar entries keyed by the
//! 1-based row-major underscore name (`k[2]` → `k_1`,`k_2`; `B[2,2]` → `B_1_1`,`B_1_2`,`B_2_1`,
//! `B_2_2`, last index fastest). Values arrive either as a JSON list of element literals or as an
//! array *expression* string (`fill(1, nin)`, `{1, 2}`, `1:3`) evaluated through the `oce-expr`
//! array subset. Split out of `resolve.rs` to keep that file under the 700-LOC cap; the only entry
//! point the resolver calls is [`expand_array_param`].

use std::collections::HashSet;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{ArrayValue, EvalResult};
use oce_model::Value;

use crate::dto::{CxfValue, Node};
use crate::ground::{GroundErr, ParamScope, ground_value};
use crate::resolve::local_name;

/// Strip a trailing array-decoration `[...]` from a local name: `k[2]` → `k`, `k` → `k`. Total —
/// used so the preserved encoding's decorated base (`k[2]`) yields the same `k` base the flattened
/// encoding's element `@id`s (`…k_1`) reduce to via [`local_name`] (doc 04 §3.6.1).
fn strip_array_label(name: &str) -> &str {
    match name.split_once('[') {
        Some((base, _)) => base,
        None => name,
    }
}

/// Parse a CXF `S231:sizeOfDimensions` string `"(d1, d2, …)"` into per-dimension sizes (doc 04
/// §3.6.1). Each `di` is a non-negative integer literal, or a symbolic expression evaluated against
/// `scope` to an `Integer` (so `"(nin)"` resolves when `nin` is a ground *earlier* parameter — the
/// same forward-reference limitation Step 7 has for value bindings). Dimensions resolve on the
/// **undivided** latest-wins view (`ParamScope::new`), not the enclosing-first split value
/// expressions use (issue #239): an enclosing-only name still resolves for shape, and a sibling
/// binding shadows a same-named enclosing one for shape purposes — when the sibling is already
/// grounded; member array order decides which binding the dims read. Owner-ruled; the pin set is
/// the count-divergence refusal, the one-name-two-readings corollary fixture, and the
/// member-order characterization in `resolve_param_precedence.rs`. Returns the
/// dimension sizes in declared order, or a human message for a `MalformedDocument`. Total; never
/// panics (no `unwrap`/index on input text — the type-domain discipline).
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

/// Maximum number of elements one array parameter may expand to — a DoS / OOM-abort guard.
/// The element count is purely input-derived from untrusted CXF; without a ceiling a single
/// `sizeOfDimensions "(2000000000)"` (or a symbolic dimension resolving to a huge earlier param)
/// would drive a multi-gigabyte `Vec::with_capacity` that aborts the (embeddable, safety-critical)
/// process *uncatchably*, defeating the panic-free typed-`OcError` contract. `1 << 20` is far
/// beyond any realistic equipment-scale parameter array; revisit the cap if a legitimate model ever
/// approaches it (doc 04 §3.6.1).
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

/// Evaluate an array-*expression* binding (`fill(1, nin)`, `{1, 2}`, `1:3`) against `scope` and
/// shape-check it against the declared dims. Returns the evaluated elements, or the
/// `GroundingFailed` message. Rules (doc 04 §3.6.1):
///
/// - Declared dims of rank ≥ 2 with an expression value are rejected up front — `oce-expr` cannot
///   construct 2-D arrays, and reshaping a flat 1-D result into a matrix would invent structure the
///   author never wrote.
/// - A scalar-evaluating expression is an error, **never** a fill-like broadcast — the author
///   writes `fill(x, n)` explicitly.
/// - A 1-D result must have exactly `expected` elements (`av.len() == n`, including `0 == 0` for a
///   declared size-0 array).
///
/// The element count is doubly bounded: the declared-dims side by `MAX_ARRAY_ELEMENTS` (checked in
/// [`array_element_names`] before this runs) and the expression side by `oce-expr`'s own 2^20
/// construction cap — a hostile `fill(1, 2000000000)` returns `ExprError::ArrayTooLarge`, surfaced
/// here as the error message. Total; never panics.
fn eval_array_expression(
    text: &str,
    dims: &[usize],
    expected: usize,
    scope: &dyn oce_expr::Scope,
) -> Result<ArrayValue, String> {
    if dims.len() > 1 {
        return Err(format!(
            "array expression value on a {}-dimensional array parameter is not supported \
             (multi-dimensional array expressions are deferred; oce-expr arrays are 1-D)",
            dims.len()
        ));
    }
    match oce_expr::eval_str(text, scope) {
        Ok(EvalResult::Array(av)) => {
            if av.len() != expected {
                return Err(format!(
                    "array expression evaluated to {} element(s) but the declared dimensions \
                     imply {expected}",
                    av.len()
                ));
            }
            Ok(av)
        }
        // `EvalResult` is `#[non_exhaustive]`; any non-array result (a scalar today) cannot fill
        // an array parameter — no broadcast, see above.
        Ok(_) => Err("array parameter expression must evaluate to an array".to_owned()),
        Err(e) => Err(GroundErr::Expr(e).to_string()),
    }
}

/// Report the `ArrayFlattenCollision` diagnostic when a minted element name collides with a
/// sibling parameter's local name. Returns `true` when a collision was reported (the caller skips
/// the element).
fn collides_with_sibling(
    siblings: &HashSet<&str>,
    ename: &str,
    piri: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if !siblings.contains(ename) {
        return false;
    }
    diags.push(
        Diagnostic::error(
            DiagCode::ArrayFlattenCollision,
            format!(
                "array element {ename:?} collides with an existing sibling parameter of the same name"
            ),
        )
        .with_subject(piri.to_owned()),
    );
    true
}

/// Append one minted per-element entry to both the param table and the incremental scope (so a
/// later sibling binding can reference it by its `k_i` name).
fn mint_element(
    ename: &str,
    value: Value,
    table: &mut Vec<(Arc<str>, Value)>,
    scope_entries: &mut Vec<(Arc<str>, EvalResult)>,
) {
    let key: Arc<str> = Arc::from(ename);
    scope_entries.push((Arc::clone(&key), EvalResult::Scalar(value.clone())));
    table.push((key, value));
}

/// Expand one **preserved** array parameter (`isArray=true`) into per-element scalar entries on the
/// owning instance's `table`/`scope_entries`, in 1-based row-major order (doc 04 §3.6.1).
/// The flattened encoding (separate `k_1`/`k_2` scalar nodes) needs no expansion — it is the
/// convergence target — so both encodings yield the identical ordered `ParamTable`.
///
/// Value rules:
/// - A [`CxfValue::List`] of element literals: `m == N` is positional (including the empty
///   `0 == 0` case), `m == 1` broadcasts the single value to all `N` **when `N >= 1`** (the
///   structural `fill(value, N)` equivalent). A non-empty list against a declared *empty* (size-0)
///   array — or any other length — is `GroundingFailed`; broadcasting one value into a
///   zero-element array would otherwise silently drop it (even a malformed value), accepting
///   broken input as valid.
/// - A [`CxfValue::Expr`] array expression (`fill(1, nin)`, `{1, 2}`, `1:3`): evaluated through
///   `oce-expr` against the same incremental scope the list path uses (earlier-declared sibling
///   parameters are visible; a *later*-declared one is not — the Step-7 forward-reference
///   limitation), then shape-checked per [`eval_array_expression`]. The evaluated elements are
///   minted exactly as list elements are, already canonicalized by `oce-expr`.
/// - Any other shape (a bare/typed scalar literal) is `GroundingFailed`.
///
/// Scope split (issue #239): both value paths above ground on the split view — `enclosing`
/// leading `scope_entries` came from the enclosing scope chain and win over a same-named sibling
/// — while `sizeOfDimensions` parsing keeps the undivided latest-wins view. Corollary
/// (owner-ruled): a name bound both by a sibling member and the enclosing scope is read twice —
/// the sibling drives the SHAPE when the sibling is grounded earlier (member array order still
/// decides the dimension reading; values are order-invariant under member order, dimensions are
/// not), the enclosing binding drives the VALUES. An element-count divergence between the two
/// readings refuses (`GroundingFailed`, both counts in the message); a value divergence with a
/// matching count is silent, exactly like the scalar path.
///
/// Minted element names (`k_1`..`k_n`) enter the scope as SIBLING entries, so a later member's
/// value reference to one is shadowed by a same-named enclosing binding — `w = "k_1"` reads an
/// enclosing `k_1` when one exists, not the minted element — while a same-named sibling
/// parameter collides and refuses via `ArrayFlattenCollision`.
///
/// Every failure is a typed diagnostic; never panics (no `unwrap`/index on input-derived data).
#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_array_param(
    piri: &str,
    pnode: &Node,
    cxf_val: &CxfValue,
    enclosing: usize,
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
    // Dimensions parse on the UNDIVIDED latest-wins view — deliberately not the split view the
    // value paths below use (issue #239 ruling; see `parse_size_dims`).
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
    let n = names.len();
    // Sibling local-names (every OTHER param node on this instance) for the minted-name collision
    // check. Lookup-only set — never iterated into a model id/vector order (determinism contract).
    // Current scope: flat single-level instances (no hasInstance nesting); revisit name scoping when
    // nested instances are lowered here.
    let siblings: HashSet<&str> = param_iris
        .iter()
        .filter(|&&p| p != piri)
        .map(|&p| local_name(p))
        .collect();
    match cxf_val {
        // Preserved per-element values as a JSON list of element literals.
        CxfValue::List(elems) => {
            let m = elems.len();
            // Positional (m == n, incl. the empty 0 == 0 case) or broadcast (one value to N >= 1
            // elements). A non-empty list against a declared empty (size-0) array is NOT a
            // broadcast — the value would be silently dropped — so it falls through to this
            // GroundingFailed length error.
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
            for (k, ename) in names.iter().enumerate() {
                if collides_with_sibling(&siblings, ename, piri, diags) {
                    continue;
                }
                let elem = if m == 1 { &elems[0] } else { &elems[k] };
                let scope = ParamScope::with_enclosing(&scope_entries[..], enclosing);
                match ground_value(elem, &scope) {
                    Ok(v) => mint_element(ename, v, table, scope_entries),
                    Err(e) => diags.push(
                        Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                            .with_subject(piri.to_owned()),
                    ),
                }
            }
        }
        // An array *expression* string, evaluated through oce-expr against the incremental
        // scope's split view (enclosing-first for values, issue #239). Evaluated before the match
        // so the immutable scope borrow ends before elements are minted. Note the G36
        // integer-constant shim lives only in scalar `ground_value`, so a G36 constant path inside
        // an array expression fails as an unknown identifier (documented asymmetry, tracked
        // separately).
        CxfValue::Expr(text) => {
            let scope = ParamScope::with_enclosing(&scope_entries[..], enclosing);
            let evaluated = eval_array_expression(text, &dims, n, &scope);
            match evaluated {
                Ok(av) => {
                    // av.len() == names.len() (shape-checked above); elements are already
                    // canonicalized by oce-expr, so they are pushed as-is.
                    for (ename, v) in names.iter().zip(av.as_slice()) {
                        if collides_with_sibling(&siblings, ename, piri, diags) {
                            continue;
                        }
                        mint_element(ename, v.clone(), table, scope_entries);
                    }
                }
                Err(msg) => diags.push(
                    Diagnostic::error(DiagCode::GroundingFailed, msg).with_subject(piri.to_owned()),
                ),
            }
        }
        // A bare/typed scalar literal on an isArray parameter is malformed.
        _ => diags.push(
            Diagnostic::error(
                DiagCode::GroundingFailed,
                "array parameter value must be a JSON list of element literals or an \
                 array expression string",
            )
            .with_subject(piri.to_owned()),
        ),
    }
}
