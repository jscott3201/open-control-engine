//! The model-wide constructed budget through the full `Engine::load_cxf` pipeline (issue #700).
//!
//! A `CDL.Reals.MatrixGain` with no `K` cell bound holds `nin x nout` identity gains: a valid
//! document whose blocks hold megabytes behind a few thousand ports. The load constructs every
//! block against `oce_graph::MAX_CONSTRUCTED_WORDS` and refuses the first block past it with a
//! typed `BuildError::ConstructedTooLarge`, as the blob decoder does for the same model. These
//! tests pin the budget at its boundary (exactly the budget loads, one word more refuses), the
//! receipt record, the refusal's place among the load's other refusals, and the peak held.
//!
//! Peaks are `allocation_counter`'s `bytes_max`, never wall-clock time.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{DiagnosticStage, DiagnosticSubject, Engine, OcError, RunMode, Value};
use oce_graph::BuildError;

/// `oce_graph::MAX_CONSTRUCTED_WORDS`, restated so this test fails if the published budget moves
/// without review.
const MAX_CONSTRUCTED_WORDS: u64 = 1 << 22;

const ROOT: &str = "http://example.org#M";

/// What a dense `MatrixGain` of `nin` by `nout` reports: its gains and its two port-kind
/// buffers, each rounded up to whole words.
fn gain_words(nin: u64, nout: u64) -> u64 {
    nin * nout + nin.div_ceil(8) + nout.div_ceil(8)
}

fn real(id: &str, kind: &str, target: Option<&str>) -> String {
    let wire = target.map_or(String::new(), |to| {
        format!(r#","S231:isConnectedTo":{{"@id":"{to}"}}"#)
    });
    format!(
        r#"{{"@id":"{id}","@type":"S231:Real{kind}","S231:isOfDataType":{{"@id":"S231:Real"}}{wire}}}"#
    )
}

fn integer(id: &str, value: i64) -> String {
    format!(
        r#"{{"@id":"{id}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:Integer"}},"S231:value":{value}}}"#
    )
}

fn refs(ids: impl Iterator<Item = String>) -> String {
    ids.map(|id| format!(r#"{{"@id":"{id}"}}"#))
        .collect::<Vec<_>>()
        .join(",")
}

/// One leaf block of the root: its nodes and the inputs the root's boundary inputs drive.
#[derive(Clone)]
struct Leaf {
    name: &'static str,
    nodes: Vec<String>,
    real_inputs: Vec<String>,
    boolean_inputs: Vec<String>,
}

/// One `MatrixGain` leaf `name` with no `K`: its two size parameters and one connector per
/// port, every input driven by the root's Real boundary input.
fn gain(name: &'static str, nin: u64, nout: u64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    let mut nodes = vec![format!(
        r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MatrixGain","S231:hasParameter":[{{"@id":"{id}.nout"}},{{"@id":"{id}.nin"}}],"S231:hasInput":[{}],"S231:hasOutput":[{}]}}"#,
        refs((1..=nin).map(|i| format!("{id}.u{i}"))),
        refs((1..=nout).map(|i| format!("{id}.y{i}"))),
    )];
    nodes.push(integer(&format!("{id}.nout"), i64::try_from(nout).unwrap()));
    nodes.push(integer(&format!("{id}.nin"), i64::try_from(nin).unwrap()));
    nodes.extend((1..=nin).map(|i| real(&format!("{id}.u{i}"), "Input", None)));
    nodes.extend((1..=nout).map(|i| real(&format!("{id}.y{i}"), "Output", None)));
    Leaf {
        name,
        nodes,
        real_inputs: (1..=nin).map(|i| format!("{id}.u{i}")).collect(),
        boolean_inputs: Vec::new(),
    }
}

/// `from.y1` drives `to.u1` instead of the root's boundary input.
fn feed_first_input(from: &mut Leaf, to: &mut Leaf) {
    let source = format!("{ROOT}.{}.y1", from.name);
    let target = format!("{ROOT}.{}.u1", to.name);
    let unwired = real(&source, "Output", None);
    let position = from
        .nodes
        .iter()
        .position(|node| *node == unwired)
        .expect("the gain's first output");
    from.nodes[position] = real(&source, "Output", Some(&target));
    to.real_inputs.retain(|input| *input != target);
}

/// One `CDL.Logical.MultiAnd` leaf of one input: a block that holds exactly one word.
fn one_word_block(name: &'static str) -> Leaf {
    let id = format!("{ROOT}.{name}");
    let boolean = |port: &str, kind: &str| {
        format!(
            r#"{{"@id":"{id}.{port}","@type":"S231:Boolean{kind}","S231:isOfDataType":{{"@id":"S231:Boolean"}}}}"#
        )
    };
    Leaf {
        name,
        nodes: vec![
            format!(
                r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Logical.MultiAnd","S231:hasParameter":{{"@id":"{id}.nin"}},"S231:hasInput":{{"@id":"{id}.u1"}},"S231:hasOutput":{{"@id":"{id}.y"}}}}"#
            ),
            integer(&format!("{id}.nin"), 1),
            boolean("u1", "Input"),
            boolean("y", "Output"),
        ],
        real_inputs: Vec::new(),
        boolean_inputs: vec![format!("{id}.u1")],
    }
}

/// One `CDL.Discrete.TriggeredMovingMean` leaf with window `n`: `4 + n` state words.
fn moving_mean(name: &'static str, n: i64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    Leaf {
        name,
        nodes: vec![
            format!(
                r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Discrete.TriggeredMovingMean","S231:hasParameter":{{"@id":"{id}.n"}},"S231:hasInput":[{{"@id":"{id}.u"}},{{"@id":"{id}.trigger"}}],"S231:hasOutput":{{"@id":"{id}.y"}}}}"#
            ),
            integer(&format!("{id}.n"), n),
            real(&format!("{id}.u"), "Input", None),
            format!(
                r#"{{"@id":"{id}.trigger","@type":"S231:BooleanInput","S231:isOfDataType":{{"@id":"S231:Boolean"}}}}"#
            ),
            real(&format!("{id}.y"), "Output", None),
        ],
        real_inputs: vec![format!("{id}.u")],
        boolean_inputs: vec![format!("{id}.trigger")],
    }
}

/// A document whose root contains `leaves` in this order, with a Real and a Boolean boundary
/// input (each only if a leaf needs it) driving every leaf input not driven otherwise.
fn document(leaves: &[Leaf]) -> Vec<u8> {
    let driven = |inputs: &dyn Fn(&Leaf) -> &Vec<String>| -> Vec<String> {
        leaves
            .iter()
            .flat_map(|leaf| inputs(leaf).iter().cloned())
            .collect()
    };
    let boundary = |port: &str, kind: &str, targets: Vec<String>| {
        format!(
            r#"{{"@id":"{ROOT}.{port}","@type":"S231:{kind}Input","S231:isOfDataType":{{"@id":"S231:{kind}"}},"S231:isConnectedTo":[{}]}}"#,
            refs(targets.into_iter())
        )
    };
    let mut inputs = Vec::new();
    for (port, kind, targets) in [
        ("u", "Real", driven(&|leaf| &leaf.real_inputs)),
        ("t", "Boolean", driven(&|leaf| &leaf.boolean_inputs)),
    ] {
        if !targets.is_empty() {
            inputs.push((format!("{ROOT}.{port}"), boundary(port, kind, targets)));
        }
    }
    let mut nodes = vec![format!(
        r#"{{"@id":"{ROOT}","@type":"S231:Block","S231:containsBlock":[{}],"S231:hasInput":[{}]}}"#,
        refs(leaves.iter().map(|leaf| format!("{ROOT}.{}", leaf.name))),
        refs(inputs.iter().map(|(id, _)| id.clone())),
    )];
    nodes.extend(inputs.into_iter().map(|(_, node)| node));
    for leaf in leaves {
        nodes.extend(leaf.nodes.iter().cloned());
    }
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        nodes.join(",\n")
    )
    .into_bytes()
}

/// Three dense 1024 x 1024 gains and one of 1024 x 1023: exactly the budget.
fn gains_at_the_budget() -> Vec<Leaf> {
    assert_eq!(
        3 * gain_words(1024, 1024) + gain_words(1024, 1023),
        MAX_CONSTRUCTED_WORDS
    );
    vec![
        gain("g0", 1024, 1024),
        gain("g1", 1024, 1024),
        gain("g2", 1024, 1024),
        gain("g3", 1024, 1023),
    ]
}

fn load(bytes: &[u8]) -> (Engine, Result<oce_api::LoadReport, OcError>, AllocationInfo) {
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    (engine, result.unwrap().map(|(report, _)| report), info)
}

fn constructed_refusal(result: Result<oce_api::LoadReport, OcError>) -> (String, u64, usize) {
    match result.expect_err("past the constructed budget") {
        OcError::Build(BuildError::ConstructedTooLarge {
            block,
            requested,
            limit,
        }) => (block.0, requested, limit),
        other => panic!("expected ConstructedTooLarge, got {other:?}"),
    }
}

#[test]
fn blocks_holding_exactly_the_constructed_budget_load_and_one_word_more_is_refused() {
    let mut leaves = gains_at_the_budget();
    let (mut engine, accepted, info) = load(&document(&leaves));
    let report = accepted.expect("a model holding exactly MAX_CONSTRUCTED_WORDS loads");
    assert_eq!(report.block_count, 4);
    assert!(
        info.bytes_max >= 8 * MAX_CONSTRUCTED_WORDS,
        "the gains are held: {info:?}"
    );

    // The loaded model re-instantiates under the same budget: a halt and resume keeps it.
    engine.halt().unwrap();
    engine
        .set_param(&format!("{ROOT}.g0.nin"), Value::Integer(8))
        .expect_err("a structural parameter is not editable at rest");
    engine.resume().expect("a model at the budget resumes");
    assert_eq!(engine.mode(), RunMode::Running);
    drop(engine);

    // One more block that holds a single word crosses the budget, and is the block named.
    leaves.push(one_word_block("and"));
    let (_, refused, _) = load(&document(&leaves));
    assert_eq!(
        constructed_refusal(refused),
        (
            format!("{ROOT}.and"),
            MAX_CONSTRUCTED_WORDS + 1,
            MAX_CONSTRUCTED_WORDS as usize
        )
    );

    // The budget is model-wide and counted in declaration order: with the small block first,
    // the gain that crosses is the one named.
    leaves.rotate_right(1);
    let (_, refused, _) = load(&document(&leaves));
    assert_eq!(
        constructed_refusal(refused),
        (
            format!("{ROOT}.g3"),
            MAX_CONSTRUCTED_WORDS + 1,
            MAX_CONSTRUCTED_WORDS as usize
        )
    );
}

#[test]
fn dense_gains_past_the_budget_are_a_typed_refusal_holding_the_budget_and_one_block() {
    // Six dense gains would hold 48 MiB; before the budget nothing summed them.
    let leaves: Vec<Leaf> = ["g0", "g1", "g2", "g3", "g4", "g5"]
        .into_iter()
        .map(|name| gain(name, 1024, 1024))
        .collect();
    let bytes = document(&leaves);
    let (_, refused, info) = load(&bytes);
    assert_eq!(
        constructed_refusal(refused),
        (
            format!("{ROOT}.g3"),
            4 * gain_words(1024, 1024),
            MAX_CONSTRUCTED_WORDS as usize
        )
    );
    // Three gains kept and the fourth constructed, beside what the load holds for the document
    // itself. That linear part is measured on a twin: the same document with every gain
    // replaced by a `RealVectorFilter` of the same 1024 inputs and outputs, which holds 1,280
    // words instead of 1,048,832 and loads.
    let twin: Vec<Leaf> = leaves
        .iter()
        .map(|leaf| {
            let mut twin = leaf.clone();
            twin.nodes[0] =
                twin.nodes[0].replace("CDL.Reals.MatrixGain", "CDL.Routing.RealVectorFilter");
            twin
        })
        .collect();
    let twin = document(&twin);
    let (_, accepted, linear) = load(&twin);
    assert_eq!(accepted.expect("the twin loads").block_count, 6);
    assert!(
        twin.len().abs_diff(bytes.len()) < 100,
        "the twin is the same size"
    );
    let bound = linear.bytes_max
        + 8 * MAX_CONSTRUCTED_WORDS
        + oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES as u64;
    assert!(
        info.bytes_max <= bound,
        "refused holding {} bytes for a {}-byte document, over {bound}",
        info.bytes_max,
        bytes.len()
    );
}

#[test]
fn the_receipt_records_the_refusal_as_a_resource_limit_at_instantiation() {
    let mut leaves = gains_at_the_budget();
    leaves.push(one_word_block("and"));
    let mut engine = Engine::new();
    let failure = engine
        .load_cxf_with_receipt(&document(&leaves))
        .expect_err("one word past the budget refuses");
    assert_eq!(failure.stage(), DiagnosticStage::Instantiation);
    assert!(matches!(
        failure.error(),
        OcError::Build(BuildError::ConstructedTooLarge { .. })
    ));
    let records = failure.diagnostics().records();
    let limits: Vec<_> = records
        .iter()
        .filter(|record| record.key().code() == "resource-limit-exceeded")
        .collect();
    assert_eq!(limits.len(), 1, "{records:?}");
    let record = limits[0];
    assert_eq!(record.key().stage(), DiagnosticStage::Instantiation);
    assert_eq!(
        record.key().subject(),
        &DiagnosticSubject::Opaque(format!("{ROOT}.and"))
    );
    assert_eq!(
        record.message(),
        "constructed blocks too large: 4194305 word(s) held by the blocks through this block \
         exceed the supported maximum (4194304 words). Remedy: reduce table, matrix or vector \
         sizes, or split the model."
    );
    assert_eq!(
        failure.error().to_string(),
        format!(
            "model build/schedule error: constructed blocks too large: 4194305 word(s) held by the blocks through block \
             {ROOT}.and exceed the supported maximum (4194304 words)"
        )
    );
    // A refused load leaves the engine without a model.
    assert!(engine.state_words().is_empty());
    assert!(engine.compile_blob().is_err());
}

#[test]
fn the_constructed_refusal_comes_after_validation_and_before_the_schedule_and_the_state() {
    // Two gains that feed each other's first input: an algebraic loop, reported as one while
    // the blocks fit the budget.
    let looped = |extra: &[&'static str]| -> Vec<Leaf> {
        let (mut a, mut b) = (gain("a", 1024, 1024), gain("b", 1024, 1024));
        feed_first_input(&mut a, &mut b);
        feed_first_input(&mut b, &mut a);
        let mut leaves = vec![a, b];
        leaves.extend(extra.iter().map(|name| gain(name, 1024, 1024)));
        leaves
    };
    let (_, within, _) = load(&document(&looped(&[])));
    assert!(
        matches!(
            within,
            Err(OcError::Build(BuildError::AlgebraicLoop { .. }))
        ),
        "{within:?}"
    );
    // Past the budget the same loop is never scheduled: the blocks are refused first.
    let (_, past, _) = load(&document(&looped(&["c", "d"])));
    assert_eq!(constructed_refusal(past).0, format!("{ROOT}.d"));

    // A window one word past the state budget, declared first, is reported only when the
    // constructed blocks fit: state is allocated after every block is constructed.
    let window = (1i64 << 22) - 3;
    let mut leaves = vec![moving_mean("mean", window)];
    let (_, state, _) = load(&document(&leaves));
    assert!(
        matches!(state, Err(OcError::Build(BuildError::StateTooLarge { .. }))),
        "{state:?}"
    );
    leaves.extend(gains_at_the_budget());
    leaves.push(one_word_block("and"));
    let (_, both, _) = load(&document(&leaves));
    assert_eq!(
        constructed_refusal(both),
        (
            format!("{ROOT}.and"),
            MAX_CONSTRUCTED_WORDS + 1,
            MAX_CONSTRUCTED_WORDS as usize
        )
    );

    // A document the validator refuses never reaches the budget: a gain whose port list does
    // not match its size is refused before any block is kept, however large the others are.
    let mut leaves: Vec<Leaf> = ["g0", "g1", "g2", "g3", "g4"]
        .into_iter()
        .map(|name| gain(name, 1024, 1024))
        .collect();
    let mut mismatched = gain("bad", 4, 4);
    mismatched.nodes[1] = integer(&format!("{ROOT}.bad.nout"), 5);
    leaves.insert(0, mismatched);
    let (_, invalid, _) = load(&document(&leaves));
    let error = invalid.expect_err("a port list that does not match the size refuses");
    assert!(
        !matches!(error, OcError::Build(_)),
        "validation refuses before any build step: {error:?}"
    );
}
