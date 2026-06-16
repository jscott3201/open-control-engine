//! The Layer-A → flat-`ModelGraph` resolver (doc 04 §7.1; `_spec/11-m1-cxf-plan.md` AD-1/AD-2).
//!
//! AD-1: there is **no** hierarchical Layer-B — composites are flattened away here and the resolver
//! emits the flat `oce_model::ModelGraph` (D1's executable truth) directly. Only the elementary
//! instances named by the top composite's `containsBlock` become [`oce_model::BlockInstance`]s; the
//! composite itself contributes only its boundary ports (`hasInput`/`hasOutput`) and child list.
//!
//! ## Determinism (load-bearing for M1 exit #4)
//! Every assignment of a `BlockId`, `ConnectorId`, vector position, or sort key is driven by an
//! **array** order — `@graph` array position, `containsBlock` order, an instance's `hasInput`/
//! `hasOutput`/`hasParameter` array order, or `isConnectedTo` array order. The `by_id` /
//! `block_of_iri` / `conn_of_iri` maps are **lookup-only**: their iteration order never feeds a
//! model id, a vector position, or a diagnostic order. PR-11 re-imports twice and byte-compares the
//! whole `ModelGraph`, so any `HashMap`-iteration-order leak here is a defect.
//!
//! ## Boundary-input elision (AD-2)
//! A flat `Connection` is output→input only. A composite boundary **input** wired to a child input
//! is an illegal `In→In` edge: instead of a connection, the driven child input is recorded in
//! `ModelGraph.external_inputs` (legal in-degree 0) and tagged with the boundary's IRI. A child
//! **output** wired to a composite boundary output is likewise elided — the child output *is* the
//! model output. Boundary ports therefore receive **no `ConnectorId`**.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, has_errors};
use oce_expr::EvalResult;
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};

use crate::dto::{CxfDocument, CxfValue, Node};
use crate::ground::{ParamScope, ground_value};
use crate::{CxfError, bridge};

/// The Ground-mode import mode. Only `Ground` exists in M1: `oce_model::Value` has no symbolic
/// variant, so a `Symbolic` mode would have no representable output. `#[non_exhaustive]` reserves
/// the future `Symbolic`/round-trip mode (OQ-3) without a breaking change.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum ImportMode {
    /// Evaluate all parameter bindings to ground literals (the executable mode).
    #[default]
    Ground,
}

/// Options for [`import_cxf`](crate::import_cxf). `#[non_exhaustive]` + [`Default`] so a future
/// `LibraryIndex` / `shacl` field is non-breaking. (A `LibraryIndex` is deferred: every M1 fixture
/// declares its elementary interfaces inline, so the "library join" collapses to a registry
/// existence check via the internal `bridge` module.)
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ResolveOptions {
    /// Import mode (Ground only in M1).
    pub mode: ImportMode,
    /// If `true`, any `Warning` also fails the load (doc 04 §9). Default `false`.
    pub deny_warnings: bool,
}

/// The resolver's diagnostics in deterministic order. On the `Ok` path it carries `Warning`/`Info`
/// only — any `Error` is returned inside [`CxfError::Validation`] instead, with the graph withheld
/// (it may be structurally unsound). Invariant enforced by construction in the resolver.
#[derive(Clone, Debug, Default)]
pub struct ValidationReport {
    /// The (sorted, error-free on the `Ok` path) diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether the report carries no diagnostics at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Whether any diagnostic is an error (always `false` on the `Ok` path).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}

/// First `@type` term of a node, if any (defensive against an empty/multi `@type` array — never
/// indexes `[0]`).
fn first_type(node: &Node) -> Option<&str> {
    node.r#type
        .as_ref()
        .and_then(|t| t.as_slice().first())
        .map(String::as_str)
}

/// The trailing term of a `prefix:Local` / `…#Local` / `…/Local` IRI (for datatype/term matching).
fn term_of(iri: &str) -> &str {
    iri.rsplit([':', '#', '/']).next().unwrap_or(iri)
}

/// The local member name of a dotted instance-member `@id` — the segment after the **last `.`**,
/// e.g. `http://example.org#MinLoop.con.k` → `k`. This is the parameter key the block registry
/// looks up (`oce_blocks` reads `real_param(p, "k", …)`), so it MUST be the bare member name, not
/// the dotted path — a wrong name would silently fall back to the block's default value.
fn local_name(iri: &str) -> &str {
    iri.rsplit('.').next().unwrap_or(iri)
}

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
fn expand_array_param(
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

/// Map an `isOfDataType` term (`S231:Real` → [`ValueType::Real`], etc.). `None` if unrecognized.
fn value_type_of_datatype(iri: &str) -> Option<ValueType> {
    match term_of(iri) {
        "Real" => Some(ValueType::Real),
        "Integer" => Some(ValueType::Integer),
        "Boolean" => Some(ValueType::Boolean),
        _ => None,
    }
}

/// Resolve a CXF document into the flat [`ModelGraph`] (doc 04 §7.1). See the module docs for the
/// determinism and boundary-elision contracts.
pub(crate) fn resolve(
    doc: &CxfDocument,
    opts: &ResolveOptions,
) -> Result<(ModelGraph, ValidationReport), CxfError> {
    let mut diags: Vec<Diagnostic> = Vec::new();

    // --- Step 1: @graph presence + index by @id (DuplicateId / MalformedDocument). by_id is
    // lookup-only. A poisoned index (empty graph, duplicate ids) is fatal — return immediately.
    if doc.graph.is_empty() {
        return Err(CxfError::Validation(vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "CXF @graph is empty — nothing to resolve",
        )]));
    }
    let mut by_id: HashMap<&str, &Node> = HashMap::with_capacity(doc.graph.len());
    for node in &doc.graph {
        if by_id.insert(node.id.as_str(), node).is_some() {
            diags.push(
                Diagnostic::error(DiagCode::DuplicateId, "duplicate @id in @graph")
                    .with_subject(node.id.clone()),
            );
        }
    }
    if has_errors(&diags) {
        return Err(CxfError::Validation(finalize_diags(diags, &HashMap::new())));
    }

    // --- Step 2: classify — the top composite (G-1: presence of containsBlock) names the
    // instances and the boundary ports. Exactly one composite in M1 (single level).
    let composites: Vec<&Node> = doc
        .graph
        .iter()
        .filter(|n| !n.contains_block.is_empty())
        .collect();
    let top = match composites.as_slice() {
        [one] => *one,
        _ => {
            return Err(CxfError::Validation(vec![Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "expected exactly one top composite (containsBlock), found {}",
                    composites.len()
                ),
            )]));
        }
    };
    let child_iris: Vec<&str> = top.contains_block.iter().map(|r| r.id.as_str()).collect();
    let boundary_in: HashSet<&str> = top.has_input.iter().map(|r| r.id.as_str()).collect();
    let boundary_out: HashSet<&str> = top.has_output.iter().map(|r| r.id.as_str()).collect();

    // --- Step 3: assign BlockId in containsBlock array order; bridge @type → class_path and check
    // the registry (Step 4 folded in). block_of_iri is lookup-only.
    let mut block_of_iri: HashMap<&str, BlockId> = HashMap::with_capacity(child_iris.len());
    let mut blocks: Vec<BlockInstance> = Vec::with_capacity(child_iris.len());
    // Remember each instance's node + port/param IRIs (in array order) for wiring/grounding.
    struct Inst<'a> {
        id: BlockId,
        node: &'a Node,
        input_iris: Vec<&'a str>,
        output_iris: Vec<&'a str>,
    }
    let mut insts: Vec<Inst> = Vec::with_capacity(child_iris.len());
    for (k, &child) in child_iris.iter().enumerate() {
        let Some(node) = by_id.get(child).copied() else {
            diags.push(
                Diagnostic::error(
                    DiagCode::UnresolvedReference,
                    "containsBlock child not found",
                )
                .with_subject(child.to_owned()),
            );
            continue;
        };
        let id = BlockId(k as u32);
        block_of_iri.insert(child, id);
        // IRI → class_path bridge + registry existence (ClassNotFound covers non-subset, e.g. PID).
        let class_path = match first_type(node) {
            Some(t) => bridge::class_path_of(t),
            None => {
                diags.push(
                    Diagnostic::error(DiagCode::ClassNotFound, "instance node has no @type")
                        .with_subject(child.to_owned()),
                );
                ""
            }
        };
        if !class_path.is_empty() {
            match oce_blocks::lookup(class_path) {
                None => diags.push(
                    Diagnostic::error(
                        DiagCode::ClassNotFound,
                        format!("no registered block class for `{class_path}`"),
                    )
                    .with_subject(child.to_owned()),
                ),
                Some(entry) => {
                    // Arity check (closes the unowned gap the PR-6 review found): the document's
                    // declared interface must match the class signature, or the engine's
                    // emit-by-port-index would later index past `outputs`/`inputs` and PANIC on the
                    // tick (a malformed model would otherwise load, then crash). The block signature
                    // is param-independent, so a default-param instance reads it safely. (Connector
                    // value_type ↔ PortKind agreement is the deeper §7.10 check, owned by PR-8.)
                    let probe = (entry.make)(&ParamTable::default());
                    let sig = probe.signature();
                    let (got_in, got_out) = (node.has_input.len(), node.has_output.len());
                    let (want_in, want_out) = (sig.inputs.len(), sig.outputs.len());
                    if got_in != want_in || got_out != want_out {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::MalformedDocument,
                                format!(
                                    "block interface mismatch for `{class_path}`: declared \
                                     {got_in} input(s)/{got_out} output(s), class requires \
                                     {want_in}/{want_out}"
                                ),
                            )
                            .with_subject(child.to_owned()),
                        );
                    }
                }
            }
        }
        insts.push(Inst {
            id,
            node,
            input_iris: node.has_input.iter().map(|r| r.id.as_str()).collect(),
            output_iris: node.has_output.iter().map(|r| r.id.as_str()).collect(),
        });
        blocks.push(BlockInstance {
            id,
            // NOTE (C-E): `class_iri` holds the *bridged class_path* (e.g. "CDL.Reals.Add"), the
            // join key for `oce_blocks::lookup` in PR-6 — NOT the full @type IRI. Field name is a
            // known wart inherited from the spec sketch.
            class_iri: Arc::from(class_path),
            inputs: Vec::new(),            // filled in Step 5b
            outputs: Vec::new(),           // filled in Step 5b
            params: ParamTable::default(), // filled in Step 7
            decl_order: k as u32,
            instance_iri: Some(Arc::from(child)),
        });
    }

    // --- Step 5a: assign ConnectorId in @graph array order to every node referenced by an
    // instance port (boundary ports are referenced by the COMPOSITE, never an instance, so they are
    // naturally excluded → no ConnectorId). conn_of_iri is lookup-only.
    let instance_port_ids: HashSet<&str> = insts
        .iter()
        .flat_map(|i| i.input_iris.iter().chain(i.output_iris.iter()).copied())
        .collect();
    let mut conn_nodes: Vec<&Node> = Vec::new();
    let mut conn_of_iri: HashMap<&str, ConnectorId> = HashMap::new();
    for node in &doc.graph {
        if instance_port_ids.contains(node.id.as_str())
            && !conn_of_iri.contains_key(node.id.as_str())
        {
            let id = ConnectorId(conn_nodes.len() as u32);
            conn_of_iri.insert(node.id.as_str(), id);
            conn_nodes.push(node);
        }
    }

    // --- Step 5b: wiring — direction + owner come authoritatively from the instance port lists
    // (the side a connector is referenced on). Fill instance.inputs/outputs in array order.
    let mut owner_dir: Vec<Option<(BlockId, Dir)>> = vec![None; conn_nodes.len()];
    for inst in &insts {
        for (slot, iris, dir) in [
            (true, &inst.input_iris, Dir::In),
            (false, &inst.output_iris, Dir::Out),
        ] {
            for &iri in iris {
                match conn_of_iri.get(iri).copied() {
                    Some(cid) if owner_dir[cid.0 as usize].is_some() => {
                        // A connector IRI referenced by more than one instance port would otherwise
                        // be silently wired into two blocks (last-writer-wins owner + double-listed
                        // in both `inputs` vectors), which the scheduler would treat as one shared
                        // runtime cell. Reject it as malformed; do not overwrite or double-list.
                        diags.push(
                            Diagnostic::error(
                                DiagCode::MalformedDocument,
                                "connector referenced by multiple instance ports",
                            )
                            .with_subject(iri.to_owned()),
                        );
                    }
                    Some(cid) => {
                        owner_dir[cid.0 as usize] = Some((inst.id, dir));
                        let b = &mut blocks[inst.id.0 as usize];
                        if slot {
                            b.inputs.push(cid)
                        } else {
                            b.outputs.push(cid)
                        }
                    }
                    None => diags.push(
                        Diagnostic::error(DiagCode::UnresolvedReference, "instance port not found")
                            .with_subject(iri.to_owned()),
                    ),
                }
            }
        }
    }

    // --- Step 6: build connectors in ConnectorId order (value_type from isOfDataType, falling back
    // to the @type port term; dir/owner from Step 5b). Default attrs — §7.10 unit unification is
    // oce-validate's job (AD-8).
    let mut connectors: Vec<Connector> = Vec::with_capacity(conn_nodes.len());
    for (i, &node) in conn_nodes.iter().enumerate() {
        let vt = derive_value_type(node, &mut diags);
        let (block, dir) = owner_dir[i].unwrap_or_else(|| {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "connector owned by no instance",
                )
                .with_subject(node.id.clone()),
            );
            (BlockId(0), Dir::In)
        });
        connectors.push(Connector::new(
            ConnectorId(i as u32),
            block,
            dir,
            vt,
            i as u32,
        ));
    }

    // --- Step 7: ground parameters (Ground mode) in hasParameter/hasConstant array order. A later
    // binding may reference an earlier one via the incrementally-built ParamScope.
    for inst in &insts {
        let mut table: Vec<(Arc<str>, Value)> = Vec::new();
        let mut scope_entries: Vec<(Arc<str>, EvalResult)> = Vec::new();
        // Collected (not lazily iterated) so the array branch can build the sibling-name set for its
        // collision check. Order = hasParameter array order, then hasConstant array order.
        let param_iris: Vec<&str> = inst
            .node
            .has_parameter
            .iter()
            .chain(inst.node.has_constant.iter())
            .map(|r| r.id.as_str())
            .collect();
        for &piri in &param_iris {
            let Some(pnode) = by_id.get(piri).copied() else {
                diags.push(
                    Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                        .with_subject(piri.to_owned()),
                );
                continue;
            };
            let Some(cxf_val) = &pnode.value else {
                diags.push(
                    Diagnostic::error(
                        DiagCode::GroundingFailed,
                        "parameter has no value (Ground mode)",
                    )
                    .with_subject(piri.to_owned()),
                );
                continue;
            };
            if pnode.is_array == Some(true) {
                // PR-9: a preserved array parameter expands to per-element scalar entries (doc 04
                // §3.6.1). Both CXF encodings (this, and pre-flattened k_1/k_2 scalars) converge here.
                expand_array_param(
                    piri,
                    pnode,
                    cxf_val,
                    &param_iris,
                    &mut table,
                    &mut scope_entries,
                    &mut diags,
                );
            } else {
                // Scalar parameter (the PR-6 path, unchanged).
                let name: Arc<str> = Arc::from(local_name(piri));
                match ground_value(cxf_val, &ParamScope::new(&scope_entries)) {
                    Ok(v) => {
                        scope_entries.push((Arc::clone(&name), EvalResult::Scalar(v.clone())));
                        table.push((name, v));
                    }
                    Err(e) => diags.push(
                        Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                            .with_subject(piri.to_owned()),
                    ),
                }
            }
        }
        blocks[inst.id.0 as usize].params = ParamTable { values: table };
    }

    // --- Step 9: collect connections with boundary elision (source @graph order, isConnectedTo
    // array order). Mutates connectors[].iri for the elided boundary-input child.
    let mut connections: Vec<Connection> = Vec::new();
    let mut external_inputs: Vec<ConnectorId> = Vec::new();
    for node in &doc.graph {
        let source = node.id.as_str();
        let src_is_boundary_in = boundary_in.contains(source);
        for tref in node.is_connected_to.iter() {
            let target = tref.id.as_str();
            if src_is_boundary_in {
                // boundary input → child input: elide; record external + attach boundary IRI (AD-2).
                match conn_of_iri.get(target).copied() {
                    Some(to) if connectors[to.0 as usize].dir != Dir::In => {
                        // A composite boundary INPUT may only drive a child INPUT — `external_inputs`
                        // are inputs by contract (oce-model). Driving an output is a direction error,
                        // and this elision path bypasses Step 10, so it must be checked here.
                        diags.push(
                            Diagnostic::error(
                                DiagCode::DirectionMismatch,
                                "boundary input drives a non-input connector",
                            )
                            .with_subject(target.to_owned()),
                        );
                    }
                    Some(to) => {
                        if !external_inputs.contains(&to) {
                            external_inputs.push(to);
                        }
                        connectors[to.0 as usize].iri = Some(Arc::from(source));
                    }
                    None => diags.push(
                        Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "boundary-input target not found",
                        )
                        .with_subject(target.to_owned()),
                    ),
                }
                continue;
            }
            if boundary_out.contains(target) {
                // child output → boundary output: elide (the child output IS the model output).
                continue;
            }
            match (
                conn_of_iri.get(source).copied(),
                conn_of_iri.get(target).copied(),
            ) {
                (Some(from), Some(to)) => connections.push(Connection { from, to }),
                (from, to) => {
                    if from.is_none() {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::UnresolvedReference,
                                "connection source not found",
                            )
                            .with_subject(source.to_owned()),
                        );
                    }
                    if to.is_none() {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::UnresolvedReference,
                                "connection target not found",
                            )
                            .with_subject(target.to_owned()),
                        );
                    }
                }
            }
        }
    }

    // --- Step 10: gross direction/type fail-fast on emitted edges (AD-8; deep §7.10 is oce-validate).
    for c in &connections {
        let (f, t) = (&connectors[c.from.0 as usize], &connectors[c.to.0 as usize]);
        if f.dir != Dir::Out || t.dir != Dir::In {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection is not output→input",
                )
                .with_subject(subject_of(f)),
            );
        }
        if f.value_type != t.value_type {
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "connected types differ: {:?} → {:?}",
                        f.value_type, t.value_type
                    ),
                )
                .with_subject(subject_of(t)),
            );
        }
    }

    // --- Step 11: boundary-aware single-assignment pre-check (AD-2). in-degree per input over the
    // emitted connections; in-degree 0 is legal iff the input is an external boundary input.
    let mut in_degree: HashMap<ConnectorId, u32> = HashMap::new();
    for c in &connections {
        *in_degree.entry(c.to).or_insert(0) += 1;
    }
    for conn in &connectors {
        if conn.dir != Dir::In {
            continue;
        }
        let deg = in_degree.get(&conn.id).copied().unwrap_or(0);
        if deg == 1 {
            continue;
        }
        if deg == 0 {
            if !external_inputs.contains(&conn.id) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::SingleAssignment,
                        "input is undriven (in-degree 0)",
                    )
                    .with_subject(subject_of(conn)),
                );
            }
        } else {
            diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    format!("input is multiply driven (in-degree {deg})"),
                )
                .with_subject(subject_of(conn)),
            );
        }
    }

    // --- Step 12: deterministic sort + return. On any Error (or any Warning under deny_warnings),
    // withhold the graph and return Err(CxfError::Validation).
    let diags = finalize_diags(diags, &conn_of_iri);
    if has_errors(&diags) || (opts.deny_warnings && !diags.is_empty()) {
        return Err(CxfError::Validation(diags));
    }
    let graph = ModelGraph {
        blocks,
        connectors,
        connections,
        external_inputs,
    };
    Ok((graph, ValidationReport { diagnostics: diags }))
}

/// Derive a connector's [`ValueType`], preferring `isOfDataType`, falling back to the `@type` port
/// term. Pushes a diagnostic (and returns a `Real` placeholder) on an unrecognized/absent type — so
/// the connectors vector stays dense and the load still fails via the diagnostic.
fn derive_value_type(node: &Node, diags: &mut Vec<Diagnostic>) -> ValueType {
    if let Some(dt) = &node.is_of_data_type {
        return value_type_of_datatype(&dt.id).unwrap_or_else(|| {
            diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "unresolved isOfDataType")
                    .with_subject(dt.id.clone()),
            );
            ValueType::Real
        });
    }
    match first_type(node).map(term_of) {
        Some(t) if t.starts_with("Real") => ValueType::Real,
        Some(t) if t.starts_with("Integer") => ValueType::Integer,
        Some(t) if t.starts_with("Boolean") => ValueType::Boolean,
        Some(t) if t.starts_with("Analog") => {
            diags.push(
                Diagnostic::warning(
                    DiagCode::AnalogCoercedToReal,
                    "Analog connector coerced to Real",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
        Some(t) if t.starts_with("String") => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "String connector not permitted (§7.8)",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
        _ => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "connector lacks a recognized data type",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
    }
}

/// The diagnostic subject for a connector — its source IRI when known, else a synthetic id.
fn subject_of(c: &Connector) -> Arc<str> {
    match &c.iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("connector#{}", c.id.0)),
    }
}

/// Sort diagnostics into the pinned deterministic order: by the subject's `ConnectorId.0` ascending
/// (connector subjects first), then by `DiagCode` string, then message. Non-connector subjects sort
/// after all connectors (`u32::MAX`) by their raw subject string — total and panic-free.
fn finalize_diags(
    mut diags: Vec<Diagnostic>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
) -> Vec<Diagnostic> {
    // Resolve a subject IRI to its `ConnectorId.0`: either via a real source IRI (in `conn_of_iri`)
    // OR the synthetic `connector#N` form `subject_of` mints for an IRI-less connector. Both must
    // map to the numeric id so the structural diagnostics (single-assignment / direction / type) —
    // which are IRI-less in practice — sort by ascending `ConnectorId.0` per the pinned rule, not
    // by lexicographic string order (where `connector#10` would precede `connector#3`).
    let key_cid = |d: &Diagnostic| -> u32 {
        d.subject
            .as_deref()
            .and_then(|s| {
                conn_of_iri.get(s).map(|c| c.0).or_else(|| {
                    s.strip_prefix("connector#")
                        .and_then(|n| n.parse::<u32>().ok())
                })
            })
            .unwrap_or(u32::MAX)
    };
    diags.sort_by(|a, b| {
        key_cid(a)
            .cmp(&key_cid(b))
            .then_with(|| a.subject.as_deref().cmp(&b.subject.as_deref()))
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags
}
