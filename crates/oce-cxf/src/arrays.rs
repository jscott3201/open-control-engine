//! Array-parameter normalization machinery (doc 04 §3.6.1) — the resolver-owned expansion of a
//! **preserved** array parameter (`isArray=true`) into per-element scalar entries keyed by the
//! 1-based row-major underscore name (`k[2]` → `k_1`,`k_2`; `B[2,2]` → `B_1_1`,`B_1_2`,`B_2_1`,
//! `B_2_2`, last index fastest). Values arrive either as a JSON list of element literals or as an
//! array *expression* string (`fill(1, nin)`, `{1, 2}`, `1:3`) evaluated through the `oce-expr`
//! array subset. Split out of `resolve.rs` to keep that file under the 700-LOC cap; the only entry
//! point the resolver calls is [`expand_array_param`].

use std::fmt::Write as _;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{ArrayValue, EvalResult};
use oce_model::Value;

use crate::dto::{CxfValue, Node};
use crate::ground::GroundErr;
use crate::parameter_reads::{
    InstanceReads, SiblingNames, StringBudgetRefusal, StringParameterBudget,
};
use crate::resolve::local_name;
use crate::scope_chain::{IndexedEntries, ParamScope, ScopeFrame};
use oce_diag::DiagnosticList;

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
/// **undivided** latest-wins view (`ParamScope::undivided`), not the enclosing-first split value
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
            match oce_expr::eval_str_finite(term, scope) {
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

/// Maximum number of elements one array parameter may expand to. The element count is purely
/// input-derived from untrusted CXF; without a ceiling a single `sizeOfDimensions "(2000000000)"`
/// (or a symbolic dimension resolving to a huge earlier param) would drive a multi-gigabyte
/// `Vec::with_capacity`. `1 << 20` is far beyond any realistic equipment-scale parameter array;
/// revisit the cap if a legitimate model ever approaches it (doc 04 §3.6.1).
const MAX_ARRAY_ELEMENTS: usize = 1 << 20;

/// Maximum number of elements ALL preserved array parameters of one document may expand to
/// together (issue #365). The per-parameter cap alone bounds one array, not the document: four
/// broadcast `p[1048576]` parameters in a 1.5 KB document once loaded at about 1 GB resident, and
/// a few dozen aborted the host. Each minted element costs on the order of a few hundred bytes
/// across the resolver and the loaded model besides its path, so this budget bounds that part of
/// array-parameter memory to a few hundred MiB per load; the paths have a budget of their own,
/// [`MAX_DOCUMENT_ARRAY_PATH_BYTES`]. It equals the per-parameter cap, so any document whose
/// arrays fit one maximal parameter still loads; crossing it is the typed
/// [`DiagCode::ResourceLimitExceeded`], checked before the crossing parameter allocates.
pub(crate) const MAX_DOCUMENT_ARRAY_ELEMENTS: usize = 1 << 20;

/// Maximum number of bytes the element paths of ALL preserved array parameters of one document
/// may take together (issue #710): 64 MiB, the element budget at 64 bytes an element.
///
/// Every element of `k[n]` on an instance `c` is minted under a name of its own, `k_1` to `k_n`,
/// and the loaded model keeps each one; the facade's parameter table keys each by its path,
/// `c.k_1` to `c.k_n`, where `c` is the instance's full IRI, and this reader spells each path once
/// to check minted aliases. So an array's paths take `n` times the length of `c` and of `k`, plus
/// its indices, while the document writes `c` and `k` a few times whatever `n` is, and the
/// element budget does not count bytes. A 1 KiB name over 262,144 elements, a 7.5 KB document,
/// once asked for 860 MB, and a 4 KiB instance name over an array of 65,536, a 38 KB document, for
/// 854 MB. An element's path is counted as the facade spells it: the instance IRI, a `.`, the
/// element's name. A load holds at most about four copies of the paths at its peak (the IRIs and
/// the names are copied a different number of times), so this budget holds their memory to about
/// 256 MiB.
///
/// What the value admits: a one-dimensional array of the whole element budget, `k[1048576]`, only
/// while the instance IRI and the base name take at most 56 bytes together (each element adds its
/// `.` and its index, 8,326,080 bytes in all); at most 933,610 elements under 64 bytes of the two,
/// and 494,264 under 128; an array of 65,536 elements under at most 1,017. Past those the path
/// budget binds before the element budget. Every tracked document and every document of
/// `open-control-library` is far inside it: no model either loads holds more than 12,217 bytes of
/// parameter paths in all, element paths included, and none a path longer than 137 bytes.
/// Crossing it is [`DiagCode::ResourceLimitExceeded`], checked with the element budget, before
/// the crossing parameter's value is read or any of its names is built.
pub(crate) const MAX_DOCUMENT_ARRAY_PATH_BYTES: usize = 64 * MAX_DOCUMENT_ARRAY_ELEMENTS;

/// Maximum number of elements ALL preserved array parameters of one document may **mint**
/// together: elements made from one value the document writes once, rather than written out one
/// by one (issue #756; owner ruling of 2026-11-22). 65,536.
///
/// A preserved array's elements are minted when its value is a broadcast (`k[n]` with the value
/// `[v]`, `n` of at least 2: every element is a copy of the one written value) or an array
/// expression (`fill(v, n)`, `1:n`, `{a, b}`: every element is the expression's). Neither costs
/// the document a byte per element: `q[1048576]` broadcasting `1.5` is a 5.6 KB document, and
/// each element it mints is held at about 339 bytes at the reader's peak (11.2 / 22.3 / 44.4 MB at
/// 32,768 / 65,536 / 131,072 elements) and about 1 KB requested over a whole load, so the element
/// budget alone admitted a 5.6 KB document that read at a 355 MB peak and once loaded at 786 MB.
/// A positional list writes each element out, so it pays for its elements in document bytes and
/// keeps the element budget alone. An array expression that writes its elements out (`{a, b}`) is
/// counted as minted too: it can hold at most 4,096 of them (`oce_expr::MAX_EXPR_NODES`), far
/// inside this budget, so the rule needs no reading of the expression.
///
/// A minted element is counted against [`MAX_DOCUMENT_ARRAY_ELEMENTS`] too, as every element is,
/// so the element budget keeps meaning every element of the document. Every element of the
/// array is counted, the one written value included, once per listing: an array node many
/// instances list mints its elements for each of them, as separate nodes would, whether or not
/// they share its grounding (issue #749). No tracked or `open-control-library` model mints more
/// than 12 elements (the vendored `ReliefFanGroup` translation, refused for other reasons), and
/// none that loads mints one. Crossing it is [`DiagCode::ResourceLimitExceeded`] on the
/// parameter node, checked with the other array budgets, before the crossing parameter's value is
/// read or any of its names is built, and after them, so a document either of them refuses is
/// refused as before.
pub(crate) const MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS: usize = 1 << 16;

/// The running per-document budgets every preserved array parameter draws on: elements, and the
/// bytes of their paths.
#[derive(Debug)]
pub(crate) struct ArrayElementBudget {
    remaining: usize,
    remaining_path_bytes: usize,
    remaining_minted: usize,
}

impl ArrayElementBudget {
    /// Full budgets of [`MAX_DOCUMENT_ARRAY_ELEMENTS`], [`MAX_DOCUMENT_ARRAY_PATH_BYTES`] and
    /// [`MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS`] for one document.
    pub(crate) fn per_document() -> Self {
        Self {
            remaining: MAX_DOCUMENT_ARRAY_ELEMENTS,
            remaining_path_bytes: MAX_DOCUMENT_ARRAY_PATH_BYTES,
            remaining_minted: MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS,
        }
    }

    /// Reserve `count` elements whose paths take `path_bytes` bytes, `minted` of them minted
    /// (0 or `count`), or explain the refusal without reserving any. The element budget is checked
    /// first, then the path budget, then the minted budget.
    fn take(&mut self, count: usize, path_bytes: u128, minted: usize) -> Result<(), String> {
        if count > self.remaining {
            let used = MAX_DOCUMENT_ARRAY_ELEMENTS - self.remaining;
            return Err(format!(
                "array parameter needs {count} element(s) but the document's array parameters \
                 already use {used} of the {MAX_DOCUMENT_ARRAY_ELEMENTS}-element per-document \
                 budget"
            ));
        }
        let Some(bytes) = usize::try_from(path_bytes)
            .ok()
            .filter(|&bytes| bytes <= self.remaining_path_bytes)
        else {
            let used = MAX_DOCUMENT_ARRAY_PATH_BYTES - self.remaining_path_bytes;
            return Err(format!(
                "array parameter's {count} element path(s) (instance IRI, `.`, element name) \
                 need {path_bytes} byte(s) but the element paths of the document's array \
                 parameters already use {used} of the {MAX_DOCUMENT_ARRAY_PATH_BYTES}-byte \
                 per-document budget"
            ));
        };
        if minted > self.remaining_minted {
            let used = MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS - self.remaining_minted;
            return Err(format!(
                "array parameter mints {minted} element(s) from a value it writes once (a \
                 broadcast or an array expression) but the document's array parameters already \
                 mint {used} of the {MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS} minted elements a \
                 document may hold"
            ));
        }
        self.remaining -= count;
        self.remaining_path_bytes -= bytes;
        self.remaining_minted -= minted;
        Ok(())
    }
}

/// How many of an array's `count` elements its value `value` mints rather than writes out
/// ([`MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS`]): all of them for a broadcast (a one-element list on
/// two or more elements) and for an array expression, none for a positional list (one element
/// written for each) or for a value of any other shape (refused before an element is minted).
fn minted_elements(value: &CxfValue, count: usize) -> usize {
    match value {
        CxfValue::List(elems) if elems.len() == 1 && count >= 2 => count,
        CxfValue::Expr(_) => count,
        _ => 0,
    }
}

/// The element count of an array with the given per-dimension sizes. Returns `Err(msg)`
/// (→ `MalformedDocument`) when the count overflows `usize` **or** exceeds [`MAX_ARRAY_ELEMENTS`]
/// — checked *before* any allocation. Pure and total.
fn array_element_count(dims: &[usize]) -> Result<usize, String> {
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
    Ok(count)
}

/// The bytes the paths of every element of an array take together, `count` of them (`count` is
/// [`array_element_count`]`(dims)`): each path is the instance IRI (`owner_bytes` long), a `.`
/// and the name [`ElementNames`] builds, which is the base name (`base_bytes` long) and, per
/// dimension, `_` and a 1-based index; an index of a dimension of `n` appears in `count / n`
/// names. That is the path the facade's parameter table keys the element by. Computed without
/// building a name, in `u128`: exact for a count within [`MAX_ARRAY_ELEMENTS`], and saturating,
/// never wrapping, past it. Zero when any dimension is 0 (a legal empty array). Pure and total.
fn array_element_path_bytes(
    owner_bytes: usize,
    base_bytes: usize,
    dims: &[usize],
    count: usize,
) -> u128 {
    if count == 0 {
        return 0;
    }
    let count = count as u128;
    let each = owner_bytes as u128 + 1 + base_bytes as u128 + dims.len() as u128;
    let fixed = count.saturating_mul(each);
    // Every dimension is at least 1 here, since `count` is their product and not zero.
    dims.iter().fold(fixed, |total, &n| {
        total.saturating_add((count / n.max(1) as u128).saturating_mul(index_digits(n)))
    })
}

/// The decimal digits of the indices `1` to `n` together: `index_digits(12)` is 9 + 3 × 2 = 15.
/// Counted by decade (1-9, 10-99, …), so it costs one step a digit of `n`. No step overflows: `n`
/// is a `usize` and the arithmetic is `u128`. Pure and total.
fn index_digits(n: usize) -> u128 {
    let n = n as u128;
    let (mut total, mut low, mut width) = (0u128, 1u128, 1u128);
    while low <= n {
        let high = n.min(low * 10 - 1);
        total += (high - low + 1) * width;
        low *= 10;
        width += 1;
    }
    total
}

/// The 1-based row-major element names of an array `base` with the given per-dimension sizes
/// (doc 04 §3.6.1), built one at a time into one buffer: `("k", &[2])` reads `k_1`, `k_2`;
/// `("B", &[2,2])` reads `B_1_1`, `B_1_2`, `B_2_1`, `B_2_2` (last index varies fastest). A name
/// is built where its element is minted and nothing holds the others, so a refused value builds
/// none (issue #710). Pure function of `base` + `dims` — never map order, so deterministic.
struct ElementNames<'a> {
    /// The per-dimension sizes.
    dims: &'a [usize],
    /// The odometer of 1-based indices of the next name, one per dimension.
    index: Vec<usize>,
    /// The length of the base name, which `name` always starts with.
    base_bytes: usize,
    /// The base name, then the indices of the name last read.
    name: String,
}

impl<'a> ElementNames<'a> {
    /// The names of `base` over `dims`, from the first.
    fn new(base: &str, dims: &'a [usize]) -> Self {
        Self {
            dims,
            index: vec![1; dims.len()],
            base_bytes: base.len(),
            name: String::from(base),
        }
    }

    /// The next name in row-major order. The caller reads exactly [`array_element_count`] names,
    /// one per element; past the last the odometer starts again from the first.
    fn next_name(&mut self) -> &str {
        // `base_bytes` is the length of a `str` the buffer starts with: a char boundary.
        self.name.truncate(self.base_bytes);
        for &i in &self.index {
            // Writing to a `String` cannot fail.
            let _ = write!(self.name, "_{i}");
        }
        // Increment the odometer from the last (fastest) dimension.
        for (i, &n) in self.index.iter_mut().zip(self.dims).rev() {
            *i += 1;
            if *i <= n {
                break;
            }
            *i = 1;
        }
        &self.name
    }
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
/// The element count is doubly bounded: the declared-dims side by `MAX_ARRAY_ELEMENTS` and the
/// per-document budget (both checked in [`expand_array_param`] before this runs) and the expression side by `oce-expr`'s own 2^20
/// construction cap — a hostile `fill(1, 2000000000)` returns `ExprError::ArrayTooLarge`, surfaced
/// here as the error message. A non-finite element or intermediate is a typed
/// [`GroundErr::NonFinite`] (issue #362). Total; never panics.
///
/// The evaluation is made by the caller, through the document's groundings, so a shared node's
/// expression that reads no name is evaluated once per document (issue #749); `evaluated` is
/// `None` for declared dims of rank 2 or more, which are refused before anything is evaluated.
fn array_expression_elements<'e>(
    evaluated: Option<&'e Result<EvalResult, oce_expr::ExprError>>,
    dims: &[usize],
    expected: usize,
) -> Result<&'e ArrayValue, GroundErr> {
    let Some(evaluated) = evaluated.filter(|_| dims.len() <= 1) else {
        return Err(GroundErr::ArrayShape(format!(
            "array expression value on a {}-dimensional array parameter is not supported \
             (multi-dimensional array expressions are deferred; oce-expr arrays are 1-D)",
            dims.len()
        )));
    };
    match evaluated {
        Ok(EvalResult::Array(av)) => {
            if av.len() != expected {
                return Err(GroundErr::ArrayShape(format!(
                    "array expression evaluated to {} element(s) but the declared dimensions \
                     imply {expected}",
                    av.len()
                )));
            }
            Ok(av)
        }
        // `EvalResult` is `#[non_exhaustive]`; any non-array result (a scalar today) cannot fill
        // an array parameter — no broadcast, see above.
        Ok(_) => Err(GroundErr::ArrayShape(
            "array parameter expression must evaluate to an array".to_owned(),
        )),
        Err(e) => Err(crate::ground::expr_failure(e.clone())),
    }
}

/// Report the `ArrayFlattenCollision` diagnostic when a minted element name collides with a
/// sibling parameter's local name. Returns `true` when a collision was reported (the caller skips
/// the element). The diagnostic is raised, so once the list has ended a collision costs a count
/// and copies nothing (issue #711).
fn collides_with_sibling(
    siblings: &SiblingNames<'_>,
    ename: &str,
    piri: &str,
    diags: &mut DiagnosticList,
) -> bool {
    if !siblings.contains(ename) {
        return false;
    }
    diags.raise(|| {
        Diagnostic::error(
            DiagCode::ArrayFlattenCollision,
            format!(
                "array element {ename:?} collides with an existing sibling parameter of the same name"
            ),
        )
        .with_subject(piri.to_owned())
    });
    true
}

/// Where the elements of one array parameter are bound: the block's class, the owning instance
/// and the array's parameter node, for holding each element in its catalog type.
struct ElementOwner<'a> {
    class_path: &'a str,
    owner: &'a str,
    piri: &'a str,
}

/// Append one minted per-element entry to both the param table and the incremental sibling
/// scope (so a later sibling binding can reference it by its `k_i` name), once its string bytes,
/// if it is a string, are reserved from the document's `strings` budget (issue #736). The element
/// is held in its catalog type first (issues #683, #735), so the table and the sibling scope a
/// later member reads hold the same value. Mints nothing and returns the refusal when the string
/// bytes do not fit: the caller refuses the array there with one `ResourceLimitExceeded` and mints
/// none of its later elements.
fn mint_element(
    ename: &str,
    value: Value,
    table: &mut Vec<(Arc<str>, Value)>,
    siblings: &mut IndexedEntries,
    at: &ElementOwner<'_>,
    strings: &mut StringParameterBudget,
    diags: &mut DiagnosticList,
) -> Result<(), StringBudgetRefusal> {
    strings.take(&value)?;
    let value =
        crate::resolve::hold_in_catalog_type(at.class_path, at.owner, at.piri, ename, value, diags);
    let key: Arc<str> = Arc::from(ename);
    siblings.push(Arc::clone(&key), EvalResult::Scalar(value.clone()));
    table.push((key, value));
    Ok(())
}

/// Expand one **preserved** array parameter (`isArray=true`) into per-element scalar entries on the
/// owning instance's `table`/`siblings`, in 1-based row-major order (doc 04 §3.6.1).
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
///   limitation), then shape-checked per [`array_expression_elements`]. The evaluated elements are
///   minted exactly as list elements are, already canonicalized by `oce-expr`.
/// - Any other shape (a bare/typed scalar literal) is `GroundingFailed`.
///
/// Scope split (issue #239): both value paths above ground on the split view — the `enclosing`
/// frames win over a same-named sibling in `siblings` — while `sizeOfDimensions` parsing keeps
/// the undivided latest-wins view. Corollary
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
/// Resource bounds: the element count is checked against [`MAX_ARRAY_ELEMENTS`]
/// (`MalformedDocument`), and then the count and the bytes of the element paths (`owner`, `.`,
/// element name) are reserved from the document's `budget` ([`MAX_DOCUMENT_ARRAY_ELEMENTS`] and
/// [`MAX_DOCUMENT_ARRAY_PATH_BYTES`], `ResourceLimitExceeded`), and so are the elements a
/// broadcast or an array expression mints ([`MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS`], issue #756),
/// before any element name is built or any value expression is evaluated. So the elements an
/// array mints and their names and paths stay within those budgets. A parameter refused for size
/// draws nothing from any budget. Names are built one at a time where their elements are minted, once the value's shape
/// is checked (issue #710): a refused value builds none, and nothing holds every name beside the
/// minted entries.
///
/// Values (issue #736): the one value of a broadcast is grounded once, through `reads`, at the
/// first element that is not refused for a collision, and every element shares the result, a
/// string value as one allocation and a failure as one error raised once per element. So is
/// every value of a node the instance lists more than once
/// ([`Groundings`](crate::parameter_reads::Groundings)). Each minted string value is counted
/// against the document's `strings` budget, once per element; the first that does not fit
/// refuses the array with one `ResourceLimitExceeded` and mints none of its later elements. The collision check reads the instance's sibling names, built once per instance
/// ([`SiblingNames`], issue #731).
///
/// Every failure is a typed diagnostic; never panics (no `unwrap`/index on input-derived data).
/// A non-finite element or expression intermediate reports `non-finite-parameter` on `owner`, the
/// declaring instance, exactly like a scalar parameter (issue #362). Every diagnostic here is
/// raised, so once the document's list has ended none is built (issue #711).
#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_array_param(
    piri: &str,
    owner: &str,
    class_path: &str,
    pnode: &Node,
    cxf_val: &CxfValue,
    enclosing: &ScopeFrame,
    reads: &mut InstanceReads<'_>,
    table: &mut Vec<(Arc<str>, Value)>,
    siblings: &mut IndexedEntries,
    budget: &mut ArrayElementBudget,
    strings: &mut StringParameterBudget,
    diags: &mut DiagnosticList,
) {
    let refuse = |diags: &mut DiagnosticList, code: DiagCode, message: String| {
        diags.raise(|| Diagnostic::error(code, message).with_subject(piri.to_owned()));
    };
    let base = strip_array_label(local_name(piri));
    let Some(size) = pnode.size_dims.as_deref() else {
        refuse(
            diags,
            DiagCode::MalformedDocument,
            "array parameter (isArray) lacks sizeOfDimensions".to_owned(),
        );
        return;
    };
    // Dimensions parse on the UNDIVIDED latest-wins view — deliberately not the split view the
    // value paths below use (issue #239 ruling; see `parse_size_dims`).
    let dims = match parse_size_dims(
        size,
        pnode.n_dims,
        &ParamScope::undivided(enclosing, siblings),
    ) {
        Ok(d) => d,
        Err(msg) => return refuse(diags, DiagCode::MalformedDocument, msg),
    };
    let count = match array_element_count(&dims) {
        Ok(count) => count,
        Err(msg) => return refuse(diags, DiagCode::MalformedDocument, msg),
    };
    let path_bytes = array_element_path_bytes(owner.len(), base.len(), &dims, count);
    if let Err(msg) = budget.take(count, path_bytes, minted_elements(cxf_val, count)) {
        return refuse(diags, DiagCode::ResourceLimitExceeded, msg);
    }
    let n = count;
    // Every value of a node the instance lists again is kept for its later listings.
    let repeated = reads.groundings.is_repeated(piri);
    // Each element is held in its catalog type as it is minted, before the table or the sibling
    // scope sees it (issues #683, #735); a refusal names this array's parameter node.
    let at = ElementOwner {
        class_path,
        owner,
        piri,
    };
    match cxf_val {
        // Preserved per-element values as a JSON list of element literals.
        CxfValue::List(elems) => {
            let m = elems.len();
            // Positional (m == n, incl. the empty 0 == 0 case) or broadcast (one value to N >= 1
            // elements). A non-empty list against a declared empty (size-0) array is NOT a
            // broadcast — the value would be silently dropped — so it falls through to this
            // GroundingFailed length error.
            if !(m == n || (m == 1 && n >= 1)) {
                return refuse(
                    diags,
                    DiagCode::GroundingFailed,
                    format!(
                        "array value list has {m} element(s) but the declared dimensions imply {n}"
                    ),
                );
            }
            // Each element's name is built where it is minted, once the list's length is known
            // to fit (issue #710). A broadcast's one value is grounded once and kept for every
            // element (issue #736).
            let mut names = ElementNames::new(base, &dims);
            for k in 0..n {
                let ename = names.next_name();
                if collides_with_sibling(&reads.sibling_names, ename, piri, diags) {
                    continue;
                }
                let elem = if m == 1 { &elems[0] } else { &elems[k] };
                let scope = ParamScope::with_enclosing(enclosing, siblings);
                let grounded = reads
                    .groundings
                    .ground(pnode, elem, &scope, m == 1 || repeated);
                let value = match &*grounded {
                    Ok(value) => value.clone(),
                    Err(e) => {
                        diags.raise(|| crate::ground::ground_failure(e, piri, owner));
                        continue;
                    }
                };
                if let Err(refusal) =
                    mint_element(ename, value, table, siblings, &at, strings, diags)
                {
                    return diags.raise(|| refusal.diagnostic(piri, owner));
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
            let scope = ParamScope::with_enclosing(enclosing, siblings);
            let evaluated = if dims.len() > 1 {
                None
            } else {
                Some(
                    reads
                        .groundings
                        .document()
                        .evaluate_array(pnode, cxf_val, text, n, &scope),
                )
            };
            match array_expression_elements(evaluated.as_deref(), &dims, n) {
                Ok(av) => {
                    // av.len() == n (shape-checked above), so one name is read per element;
                    // elements are already canonicalized by oce-expr, so they are pushed as-is.
                    let mut names = ElementNames::new(base, &dims);
                    for v in av.as_slice() {
                        let ename = names.next_name();
                        if collides_with_sibling(&reads.sibling_names, ename, piri, diags) {
                            continue;
                        }
                        if let Err(refusal) =
                            mint_element(ename, v.clone(), table, siblings, &at, strings, diags)
                        {
                            return diags.raise(|| refusal.diagnostic(piri, owner));
                        }
                    }
                }
                Err(e) => diags.raise(|| crate::ground::ground_failure(&e, piri, owner)),
            }
        }
        // A bare/typed scalar literal on an isArray parameter is malformed.
        _ => refuse(
            diags,
            DiagCode::GroundingFailed,
            "array parameter value must be a JSON list of element literals or an array \
             expression string"
                .to_owned(),
        ),
    }
}

#[cfg(test)]
#[path = "arrays_tests.rs"]
mod tests;
