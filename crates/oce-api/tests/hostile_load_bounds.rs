//! Hostile-CXF load bounds through the full `Engine::load_cxf` pipeline (issue #365).
//!
//! Each hostile shape from the issue is reproduced as a small document and must end in a typed
//! refusal — never a panic, an allocation abort, or an unbounded load — with its cost bounded
//! deterministically: assertions measure heap bytes and allocation counts with
//! `allocation_counter` (no wall-clock thresholds). Each new limit is also pinned at its
//! boundary: the limit itself is accepted and one past it refused.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{DiagnosticSubject, Engine, OcError};
use oce_diag::DiagCode;

const MIB: u64 = 1024 * 1024;

/// `oce-graph`'s model-wide `[S]` state budget (`oce_graph::MAX_STATE_WORDS`), restated so this
/// test fails if the published budget moves without review.
const MAX_STATE_WORDS: i64 = 1 << 22;
/// `oce-cxf`'s per-document array-parameter element budget, restated likewise.
const MAX_DOCUMENT_ARRAY_ELEMENTS: usize = 1 << 20;
/// `TriggeredMovingMean` state words besides its `n` sample words.
const MOVING_MEAN_FIXED_WORDS: i64 = 4;

const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn head(root_children: &[String]) -> String {
    let children: Vec<String> = root_children
        .iter()
        .map(|id| format!(r#"{{ "@id": "{id}" }}"#))
        .collect();
    format!(
        r#"{{ "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": [ {} ] }}"#,
        children.join(", ")
    )
}

fn document(nodes: &[String]) -> Vec<u8> {
    format!(
        r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231P#" }}, "@graph": [ {} ] }}"#,
        nodes.join(",\n")
    )
    .into_bytes()
}

/// One `CDL.Discrete.TriggeredMovingMean` leaf `id` with window `n` (inputs left as boundary
/// points; outputs unconnected).
fn moving_mean(id: &str, n: i64) -> Vec<String> {
    vec![
        format!(
            r#"{{ "@id": "{id}",
                 "@type": "http://example.org#Buildings.Controls.OBC.CDL.Discrete.TriggeredMovingMean",
                 "S231:hasParameter": {{ "@id": "{id}.n" }},
                 "S231:hasInput": [ {{ "@id": "{id}.u" }}, {{ "@id": "{id}.trigger" }} ],
                 "S231:hasOutput": {{ "@id": "{id}.y" }} }}"#
        ),
        format!(
            r#"{{ "@id": "{id}.n", "@type": "S231:Parameter",
                 "S231:isOfDataType": {{ "@id": "S231:Integer" }},
                 "S231:value": {{ "@value": "{n}", "@type": "{XSD_INTEGER}" }} }}"#
        ),
        format!(
            r#"{{ "@id": "{id}.u", "@type": "S231:RealInput",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }} }}"#
        ),
        format!(
            r#"{{ "@id": "{id}.trigger", "@type": "S231:BooleanInput",
                 "S231:isOfDataType": {{ "@id": "S231:Boolean" }} }}"#
        ),
        format!(
            r#"{{ "@id": "{id}.y", "@type": "S231:RealOutput",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }} }}"#
        ),
    ]
}

/// Moving-mean leaves with the given windows, every `u`/`trigger` driven by the root's boundary
/// inputs `M.u` / `M.trigger`.
fn moving_mean_document(windows: &[i64]) -> Vec<u8> {
    let ids: Vec<String> = (0..windows.len())
        .map(|i| format!("http://example.org#M.mean{i}"))
        .collect();
    let targets = |port: &str| {
        ids.iter()
            .map(|id| format!(r#"{{ "@id": "{id}.{port}" }}"#))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut nodes = vec![
        format!(
            r#"{{ "@id": "http://example.org#M", "@type": "S231:Block",
                  "S231:containsBlock": [ {} ],
                  "S231:hasInput": [ {{ "@id": "http://example.org#M.u" }},
                                     {{ "@id": "http://example.org#M.trigger" }} ] }}"#,
            ids.iter()
                .map(|id| format!(r#"{{ "@id": "{id}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!(
            r#"{{ "@id": "http://example.org#M.u", "@type": "S231:RealInput",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }},
                 "S231:isConnectedTo": [ {} ] }}"#,
            targets("u")
        ),
        format!(
            r#"{{ "@id": "http://example.org#M.trigger", "@type": "S231:BooleanInput",
                 "S231:isOfDataType": {{ "@id": "S231:Boolean" }},
                 "S231:isConnectedTo": [ {} ] }}"#,
            targets("trigger")
        ),
    ];
    for (id, &n) in ids.iter().zip(windows) {
        nodes.extend(moving_mean(id, n));
    }
    document(&nodes)
}

/// A `CDL.Reals.Sources.Constant` leaf `id` with scalar `k` plus extra parameter nodes.
fn constant(id: &str, extra_params: &[(String, String)]) -> Vec<String> {
    let mut param_refs = vec![format!(r#"{{ "@id": "{id}.k" }}"#)];
    param_refs.extend(
        extra_params
            .iter()
            .map(|(pid, _)| format!(r#"{{ "@id": "{pid}" }}"#)),
    );
    let mut nodes = vec![
        format!(
            r#"{{ "@id": "{id}",
                 "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                 "S231:hasParameter": [ {} ],
                 "S231:hasOutput": {{ "@id": "{id}.y" }} }}"#,
            param_refs.join(", ")
        ),
        format!(
            r#"{{ "@id": "{id}.k", "@type": "S231:Parameter",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }}, "S231:value": 1.5 }}"#
        ),
        format!(
            r#"{{ "@id": "{id}.y", "@type": "S231:RealOutput",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }} }}"#
        ),
    ];
    nodes.extend(extra_params.iter().map(|(_, node)| node.clone()));
    nodes
}

/// A preserved broadcast array parameter `p<index>[len]` on leaf `owner`.
fn broadcast_array(owner: &str, index: usize, len: usize) -> (String, String) {
    let id = format!("{owner}.p{index}");
    let node = format!(
        r#"{{ "@id": "{id}", "S231:label": "p{index}[{len}]", "S231:isArray": true,
             "S231:numberDimensions": 1, "S231:sizeOfDimensions": "({len})",
             "S231:value": [ 1.0 ] }}"#
    );
    (id, node)
}

fn array_document(lengths: &[usize]) -> Vec<u8> {
    let owner = "http://example.org#M.con";
    let extras: Vec<(String, String)> = lengths
        .iter()
        .enumerate()
        .map(|(i, &len)| broadcast_array(owner, i, len))
        .collect();
    let mut nodes = vec![head(&[owner.to_owned()])];
    nodes.extend(constant(owner, &extras));
    document(&nodes)
}

fn load(bytes: &[u8]) -> (Result<oce_api::LoadReport, OcError>, AllocationInfo) {
    let mut engine = Engine::in_memory();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    (result.unwrap(), info)
}

fn error_codes(error: &OcError) -> Vec<DiagCode> {
    error.all_diagnostics().map(|d| d.code).collect()
}

#[test]
fn moving_mean_window_past_the_state_budget_is_a_typed_build_refusal_without_allocating_it() {
    // The issue's exact panic input: n = 2^62 drove `words.resize` into "capacity overflow".
    let (result, info) = load(&moving_mean_document(&[4_611_686_018_427_387_904]));
    match result.expect_err("an over-budget window must refuse") {
        OcError::Build(oce_graph::BuildError::StateTooLarge {
            block,
            requested,
            limit,
        }) => {
            assert_eq!(block.0, "http://example.org#M.mean0");
            assert_eq!(requested, (1 << 62) + 4);
            assert_eq!(limit, 1 << 22);
        }
        other => panic!("expected StateTooLarge, got {other:?}"),
    }
    // Refused before the state buffer is reserved: the whole load stays far below even the
    // budget's own 32 MiB.
    assert!(info.bytes_max < 8 * MIB, "{info:?}");
}

#[test]
fn moving_mean_window_at_i64_max_and_the_receipt_record_are_typed() {
    let bytes = moving_mean_document(&[i64::MAX]);
    let mut engine = Engine::in_memory();
    let failure = engine
        .load_cxf_with_receipt(&bytes)
        .expect_err("i64::MAX window refuses");
    let records = failure.diagnostics().records();
    let record = records
        .iter()
        .find(|r| r.key().code() == "resource-limit-exceeded")
        .unwrap_or_else(|| panic!("no resource-limit record in {records:?}"));
    assert_eq!(
        record.key().subject(),
        &DiagnosticSubject::Opaque("http://example.org#M.mean0".to_owned())
    );
    assert!(record.message().contains("4194304 words"), "{record:?}");
}

#[test]
fn state_budget_admits_exactly_its_limit_and_refuses_one_word_more() {
    let at_limit = MAX_STATE_WORDS - MOVING_MEAN_FIXED_WORDS;
    let (accepted, _) = load(&moving_mean_document(&[at_limit]));
    accepted.expect("a model using exactly MAX_STATE_WORDS loads");

    let (refused, _) = load(&moving_mean_document(&[at_limit + 1]));
    match refused.expect_err("one word past the budget refuses") {
        OcError::Build(oce_graph::BuildError::StateTooLarge { requested, .. }) => {
            assert_eq!(requested, (MAX_STATE_WORDS + 1) as u64);
        }
        other => panic!("expected StateTooLarge, got {other:?}"),
    }

    // The budget is model-wide: two windows that each fit alone refuse together, naming the
    // block whose region crosses it.
    let half = MAX_STATE_WORDS / 2;
    let (pair, _) = load(&moving_mean_document(&[half, half]));
    match pair.expect_err("two half-budget windows plus fixed words exceed the budget") {
        OcError::Build(oce_graph::BuildError::StateTooLarge { block, .. }) => {
            assert_eq!(block.0, "http://example.org#M.mean1");
        }
        other => panic!("expected StateTooLarge, got {other:?}"),
    }
}

#[test]
fn four_maximal_broadcast_arrays_refuse_on_the_document_budget_with_bounded_memory() {
    // The issue's 1.5 KB document: four broadcast p[1048576] parameters once loaded at ~1 GB.
    let bytes = array_document(&[1 << 20; 4]);
    assert!(bytes.len() < 4096, "the hostile document stays tiny");
    let (result, info) = load(&bytes);
    let error = result.expect_err("the second maximal array crosses the document budget");
    let codes = error_codes(&error);
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == DiagCode::ResourceLimitExceeded)
            .count(),
        3,
        "the first array fits; each later one is refused: {codes:?}"
    );
    let first = error
        .all_diagnostics()
        .find(|d| d.code == DiagCode::ResourceLimitExceeded)
        .unwrap();
    assert_eq!(
        first.subject.as_deref(),
        Some("http://example.org#M.con.p1")
    );
    assert_eq!(
        first.message,
        "array parameter needs 1048576 element(s) but the document's array parameters already \
         use 1048576 of the 1048576-element per-document budget"
    );
    // Only the admitted first array is ever materialized.
    assert!(info.bytes_max < 512 * MIB, "{info:?}");
}

#[test]
fn document_array_budget_admits_exactly_its_limit_and_refuses_one_element_more() {
    let half = MAX_DOCUMENT_ARRAY_ELEMENTS / 2;
    let (accepted, _) = load(&array_document(&[half, half]));
    accepted.expect("arrays totalling exactly the budget load");

    let (refused, _) = load(&array_document(&[half, half + 1]));
    let error = refused.expect_err("one element past the budget refuses");
    let over: Vec<_> = error
        .all_diagnostics()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .collect();
    assert_eq!(over.len(), 1, "{over:?}");
    assert_eq!(
        over[0].subject.as_deref(),
        Some("http://example.org#M.con.p1")
    );

    // A parameter refused by the per-parameter cap draws nothing from the document budget.
    let (cap_then_fit, _) = load(&array_document(&[(1 << 20) + 1, 1 << 20]));
    let codes = error_codes(&cap_then_fit.expect_err("the over-cap parameter refuses"));
    assert!(codes.contains(&DiagCode::MalformedDocument), "{codes:?}");
    assert!(
        !codes.contains(&DiagCode::ResourceLimitExceeded),
        "{codes:?}"
    );
}

/// A chain of `depth` composites where each lists its child twice (`[C_{i+1}, C_{i+1}]`), ending
/// in one constant leaf. Unrefused, the walk doubles per level.
fn doubling_chain(depth: usize) -> Vec<u8> {
    let id = |level: usize| format!("http://example.org#M.c{level}");
    let mut nodes = vec![head(&[id(1), id(1)])];
    for level in 1..depth {
        nodes.push(format!(
            r#"{{ "@id": "{}", "@type": "S231:Block",
                 "S231:containsBlock": [ {{ "@id": "{}" }}, {{ "@id": "{}" }} ] }}"#,
            id(level),
            id(level + 1),
            id(level + 1)
        ));
    }
    let leaf = "http://example.org#M.leaf".to_owned();
    nodes.push(format!(
        r#"{{ "@id": "{}", "@type": "S231:Block",
             "S231:containsBlock": [ {{ "@id": "{leaf}" }}, {{ "@id": "{leaf}" }} ] }}"#,
        id(depth)
    ));
    nodes.extend(constant(&leaf, &[]));
    document(&nodes)
}

#[test]
fn doubly_listed_composites_refuse_once_per_level_with_linear_cost() {
    // Depth 23 once took ~8.5 s and 3.1 GB; refused second reaches keep the walk linear.
    let costs: Vec<AllocationInfo> = [8usize, 16, 32]
        .iter()
        .map(|&depth| {
            let (result, info) = load(&doubling_chain(depth));
            let error = result.expect_err("a shared child refuses the load");
            let shared: Vec<_> = error
                .all_diagnostics()
                .filter(|d| d.code == DiagCode::SharedInstance)
                .collect();
            // One refusal per composite level (root through c{depth-1}) plus the leaf's.
            assert_eq!(shared.len(), depth + 1, "depth {depth}: {shared:?}");
            assert_eq!(
                shared.last().unwrap().subject.as_deref(),
                Some("http://example.org#M.leaf")
            );
            info
        })
        .collect();
    // Doubling the depth must not square the work: allocation counts grow at most ~linearly
    // (an exponential walk would multiply them by 2^8 and then 2^16).
    assert!(costs[2].count_total < 4 * costs[1].count_total, "{costs:?}");
    assert!(costs[1].count_total < 4 * costs[0].count_total, "{costs:?}");
    assert!(costs[2].bytes_max < 16 * MIB, "{costs:?}");
}

#[test]
fn a_leaf_listed_by_two_composites_is_refused_not_duplicated() {
    let leaf = "http://example.org#M.leaf".to_owned();
    let mut nodes = vec![head(&[
        "http://example.org#M.a".to_owned(),
        "http://example.org#M.b".to_owned(),
    ])];
    for owner in ["a", "b"] {
        nodes.push(format!(
            r#"{{ "@id": "http://example.org#M.{owner}", "@type": "S231:Block",
                 "S231:containsBlock": {{ "@id": "{leaf}" }} }}"#
        ));
    }
    nodes.extend(constant(&leaf, &[]));
    let (result, _) = load(&document(&nodes));
    let error = result.expect_err("a doubly-parented leaf refuses");
    let shared: Vec<_> = error
        .all_diagnostics()
        .filter(|d| d.code == DiagCode::SharedInstance)
        .collect();
    assert_eq!(shared.len(), 1, "{shared:?}");
    assert_eq!(shared[0].subject.as_deref(), Some(leaf.as_str()));
    assert!(
        shared[0]
            .message
            .contains("listed again by http://example.org#M.b"),
        "{shared:?}"
    );
}

/// One composite declaring `params` Real parameters and containing `leaves` constant leaves.
fn wide_composite(params: usize, leaves: usize) -> Vec<u8> {
    let leaf_ids: Vec<String> = (0..leaves)
        .map(|i| format!("http://example.org#M.s.l{i}"))
        .collect();
    let param_refs: Vec<String> = (0..params)
        .map(|i| format!(r#"{{ "@id": "http://example.org#M.s.q{i}" }}"#))
        .collect();
    let mut nodes = vec![head(&["http://example.org#M.s".to_owned()])];
    nodes.push(format!(
        r#"{{ "@id": "http://example.org#M.s", "@type": "S231:Block",
             "S231:hasParameter": [ {} ], "S231:containsBlock": [ {} ] }}"#,
        param_refs.join(", "),
        leaf_ids
            .iter()
            .map(|id| format!(r#"{{ "@id": "{id}" }}"#))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for i in 0..params {
        nodes.push(format!(
            r#"{{ "@id": "http://example.org#M.s.q{i}", "@type": "S231:Parameter",
                 "S231:isOfDataType": {{ "@id": "S231:Real" }}, "S231:value": {i}.5 }}"#
        ));
    }
    for id in &leaf_ids {
        nodes.extend(constant(id, &[]));
    }
    document(&nodes)
}

#[test]
fn inherited_scope_memory_is_linear_in_parameters_plus_leaves() {
    // P parameters inherited by L leaves: a per-leaf scope copy costs P × L entries. Doubling
    // both P and L quadruples a product but only doubles a sum.
    let small = load(&wide_composite(1_000, 1_000));
    small.0.expect("the wide composite loads");
    let large = load(&wide_composite(2_000, 2_000));
    large.0.expect("the wider composite loads");
    assert!(
        large.1.bytes_total < 3 * small.1.bytes_total,
        "small {:?} large {:?}",
        small.1,
        large.1
    );
}

#[test]
fn quadratic_binding_expression_is_a_typed_grounding_refusal() {
    let owner = "http://example.org#M.con";
    let extra = (
        format!("{owner}.slow"),
        format!(
            r#"{{ "@id": "{owner}.slow", "@type": "S231:Parameter",
                 "S231:isOfDataType": {{ "@id": "S231:Integer" }},
                 "S231:value": "sum({{sum(1:16384) for i in 1:16384}})" }}"#
        ),
    );
    let mut nodes = vec![head(&[owner.to_owned()])];
    nodes.extend(constant(owner, &[extra]));
    let (result, info) = load(&document(&nodes));
    let error = result.expect_err("an over-budget binding refuses");
    let grounding: Vec<_> = error
        .all_diagnostics()
        .filter(|d| d.code == DiagCode::GroundingFailed)
        .collect();
    assert_eq!(grounding.len(), 1, "{grounding:?}");
    assert!(
        grounding[0].message.contains("expression too expensive"),
        "{grounding:?}"
    );
    assert!(info.bytes_max < 64 * MIB, "{info:?}");
}

/// One constant leaf carrying `n` copies of the review's costly binding (about 15.7M units each,
/// just under the per-evaluation cap).
fn costly_bindings(n: usize) -> Vec<u8> {
    let owner = "http://example.org#M.con";
    let extras: Vec<(String, String)> = (0..n)
        .map(|i| {
            (
                format!("{owner}.e{i}"),
                format!(
                    r#"{{ "@id": "{owner}.e{i}", "@type": "S231:Parameter",
                         "S231:isOfDataType": {{ "@id": "S231:Integer" }},
                         "S231:value": "sum({{sum(1:2800) for i in 1:2800}})" }}"#
                ),
            )
        })
        .collect();
    let mut nodes = vec![head(&[owner.to_owned()])];
    nodes.extend(constant(owner, &extras));
    document(&nodes)
}

#[test]
fn many_costly_bindings_share_one_document_evaluation_budget() {
    // Each binding fits the per-evaluation cap, so before the document budget N of them cost N
    // full evaluations (about 10 s for N = 200). Now four fit the shared 2^26-unit budget and
    // every later one is refused at its first unit, whatever N is.
    let outcomes: Vec<(usize, AllocationInfo)> = [50usize, 200]
        .iter()
        .map(|&n| {
            let (result, info) = load(&costly_bindings(n));
            let error = result.expect_err("the exhausted document budget refuses the load");
            let refused: Vec<_> = error
                .all_diagnostics()
                .filter(|d| d.code == DiagCode::GroundingFailed)
                .collect();
            assert_eq!(refused.len(), n - 4, "n = {n}");
            assert!(
                refused
                    .iter()
                    .all(|d| d.message.contains("work budget (67108864 units)")),
                "{refused:?}"
            );
            // Exactly the fifth binding onward is refused (diagnostics are subject-sorted).
            let mut subjects: Vec<&str> = refused
                .iter()
                .filter_map(|d| d.subject.as_deref())
                .collect();
            subjects.sort_unstable();
            let mut expected: Vec<String> = (4..n)
                .map(|i| format!("http://example.org#M.con.e{i}"))
                .collect();
            expected.sort_unstable();
            assert_eq!(subjects, expected, "n = {n}");
            (n, info)
        })
        .collect();
    // Quadrupling the bindings adds only the cheap refusals: the evaluation work (and the large
    // array allocations it makes) is fixed by the budget, not by N.
    let (small, large) = (&outcomes[0].1, &outcomes[1].1);
    assert!(
        large.bytes_total < small.bytes_total + small.bytes_total / 10,
        "n=50 {small:?} n=200 {large:?}"
    );
}

#[test]
fn resume_past_the_state_budget_refuses_typed_and_stays_halted_until_corrected() {
    use oce_api::{EngineStateError, RunMode, Value};

    let path = "http://example.org#M.mean0.n";
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(&moving_mean_document(&[8]))
        .expect("a small window loads");
    engine.halt().unwrap();

    // An edited window of 2^23 words needs twice the budget.
    engine.set_param(path, Value::Integer(1 << 23)).unwrap();
    match engine.resume() {
        Err(OcError::Build(oce_graph::BuildError::StateTooLarge {
            block,
            requested,
            limit,
        })) => {
            assert_eq!(block.0, "http://example.org#M.mean0");
            assert_eq!(requested, (1 << 23) + 4);
            assert_eq!(limit, 1 << 22);
        }
        other => panic!("expected StateTooLarge, got {other:?}"),
    }
    assert_eq!(
        engine.mode(),
        RunMode::Halted,
        "a refused resume stays halted"
    );
    assert!(
        matches!(
            engine.checkpoint(),
            Err(OcError::State(EngineStateError::PendingParameterEdits))
        ),
        "the refused edit is still pending"
    );

    // Correcting the window lets the same engine resume.
    engine.set_param(path, Value::Integer(16)).unwrap();
    engine.resume().expect("a corrected window resumes");
    assert_eq!(engine.mode(), RunMode::Running);

    // Resuming at exactly the budget succeeds.
    engine.halt().unwrap();
    engine
        .set_param(
            path,
            Value::Integer(MAX_STATE_WORDS - MOVING_MEAN_FIXED_WORDS),
        )
        .unwrap();
    engine.resume().expect("exactly MAX_STATE_WORDS resumes");
    assert!(
        engine.checkpoint().is_ok(),
        "a budget-sized model checkpoints"
    );
}
