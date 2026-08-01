//! Which port a cascade warning names.
//!
//! When the deferral cascade reaches a block, the warning tells the host *why*: "all drivers of
//! input connector `in{k}` were deferred". That `k` is the block's own port-list position, and it
//! is the same number `crate::export::plan` mints into the port `@id`, so a host can navigate from
//! the warning to the port in the emitted document.
//!
//! The name used to be reconstructed rather than carried. The cascade loop stands inside one
//! block's `inputs` vector, but the naming helper looked up `g.connectors[c].block` — the block the
//! *connector claims* owns it — and searched that block's port list instead. For a graph the
//! resolver produced the two are always the same, so the bug was invisible there. For a hand-built
//! graph they need not be, and then the search either missed (falling back to position 0) or
//! succeeded at the wrong index in the wrong list, naming a port that exists but is not the one
//! that deferred. Both shipped inside an `Ok` export carrying no error diagnostic, so nothing told
//! the host the name was untrustworthy.
//!
//! Taking `k` from the iteration removes the class of defect rather than the instance: there is no
//! longer a lookup that can resolve against the wrong vector.
//!
//! These tests live in their own file because `tests/export_deferral_diagnostics.rs` is at 639 of
//! the 700-LOC cap.

use std::sync::Arc;

use oce_cxf::{CxfError, ExportReport, export_with_report};
use oce_diag::{DiagCode, Severity};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, ModelGraph,
    ParamTable, Value, ValueType,
};

const PREFIX: &str = "http://example.org#Cascade.";

fn iri(name: &str) -> String {
    format!("{PREFIX}{name}")
}

fn block(
    idx: u32,
    class: &str,
    name: &str,
    inputs: &[u32],
    outputs: &[u32],
    params: Vec<(Arc<str>, Value)>,
) -> BlockInstance {
    BlockInstance {
        id: BlockId(idx),
        class_iri: Arc::from(class),
        inputs: inputs.iter().copied().map(ConnectorId).collect(),
        outputs: outputs.iter().copied().map(ConnectorId).collect(),
        params: ParamTable { values: params },
        decl_order: idx,
        instance_iri: Some(Arc::from(iri(name).as_str())),
    }
}

fn conn(idx: u32, owner: u32, dir: Dir) -> Connector {
    Connector::new(ConnectorId(idx), BlockId(owner), dir, ValueType::Real, 0)
}

fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

fn enum_param() -> Vec<(Arc<str>, Value)> {
    vec![(
        Arc::from("controllerType"),
        Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1,
        },
    )]
}

fn real_param(k: f64) -> Vec<(Arc<str>, Value)> {
    vec![(Arc::from("k"), Value::Real(k))]
}

fn export_ok(g: &ModelGraph) -> ExportReport {
    match export_with_report(g) {
        Ok(report) => report,
        Err(CxfError::Validation(diags)) => {
            panic!("expected a deferring export to succeed, but it rejected: {diags:?}")
        }
        Err(other) => panic!("unexpected error shape: {other:?}"),
    }
}

/// The rendered cascade warning naming `subject`, or `None`. Cascade warnings are the ones whose
/// text is about drivers; the enum-connector and enum-parameter warnings name a different cause
/// and carry no port at all.
fn cascade_warning_for(report: &ExportReport, subject: &str) -> Option<String> {
    report
        .warnings
        .iter()
        .filter(|d| {
            assert_eq!(
                d.code,
                DiagCode::ExportDeferred,
                "every warning defers: {d:?}"
            );
            assert_eq!(d.severity, Severity::Warning, "deferral is non-aborting");
            d.message.contains("all drivers of input connector")
        })
        .find(|d| d.subject.as_deref() == Some(subject))
        .map(|d| d.message.clone())
}

/// A graph whose cascade trigger sits at input position **1**, with that connector deliberately
/// claiming the wrong owner.
///
/// `sink.inputs` is `[1, 2]`. Position 0 (connector 1) is undriven, so the loop skips it; position
/// 1 (connector 2) is driven only by the deferred `enumsrc`, so that is what cascades — the truth
/// the warning must report is `in1`.
///
/// Connector 2 claims `enumsrc` as its owner while living in `sink`'s list. That is the whole
/// point: a lookup keyed on the claimed owner searches `enumsrc.inputs`, which is empty, misses,
/// and falls back to position 0. `keeper` exists so something survives — total deferral is a
/// rejection, not a warning, and would mask the behaviour under test.
fn misowned_connector_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param(),
            ),
            block(1, "CDL.Reals.Add", "sink", &[1, 2], &[3], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "keeper",
                &[],
                &[4],
                real_param(3.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out),
            conn(1, 1, Dir::In),
            // The mis-owned one: listed at `sink.inputs[1]`, claims block 0.
            conn(2, 0, Dir::In),
            conn(3, 1, Dir::Out),
            conn(4, 2, Dir::Out),
        ],
        connections: vec![wire(0, 2)],
        external_inputs: vec![],
    }
}

#[test]
fn a_cascade_warning_names_the_deferred_blocks_own_input_position() {
    let report = export_ok(&misowned_connector_graph());
    let warning = cascade_warning_for(&report, &iri("sink"))
        .expect("`sink` cascade-defers and must carry a cascade warning");

    assert!(
        warning.contains("input connector `in1`"),
        "the trigger is `sink.inputs[1]`, so the warning must say in1: {warning}"
    );
    assert!(
        !warning.contains("input connector `in0`"),
        "in0 is `sink`'s OTHER input, which is undriven and did not cascade: {warning}"
    );
}

#[test]
fn a_misowned_connector_does_not_turn_the_cascade_warning_into_an_error() {
    // Context for the test above: the graph it uses is accepted. Nothing in the report tells a
    // host the warning might be untrustworthy, which is exactly why naming the wrong port matters
    // — there is no accompanying error to prompt a second look.
    let report = export_ok(&misowned_connector_graph());
    assert_eq!(
        report.warnings.len(),
        2,
        "enumsrc defers on its parameter and sink cascades: {:?}",
        report.warnings
    );
}

#[test]
fn a_cascade_warning_never_names_an_output_port() {
    // The loop iterates `b.inputs` only, so `in{k}` is the sole shape the slot can carry. The
    // previous helper had an `out{k}` branch reachable whenever a mis-owned connector's claimed
    // direction was `Out`, which rendered the self-contradictory "all drivers of input connector
    // `out0`". Deleting the branch is what makes that unrepresentable.
    let mut g = misowned_connector_graph();
    g.connectors[2] = conn(2, 0, Dir::Out);

    // Direction mismatch is itself a rejection; the warnings ride along with the error list.
    let diags = match export_with_report(&g) {
        Err(CxfError::Validation(diags)) => diags,
        Ok(report) => report.warnings,
        Err(other) => panic!("unexpected error shape: {other:?}"),
    };
    for d in &diags {
        assert!(
            !d.message.contains("input connector `out"),
            "an input-connector message must never name an out port: {d:?}"
        );
    }
}

#[test]
fn every_cascade_warning_names_an_input_whose_drivers_are_all_deferred() {
    // The invariant behind the name, asserted rather than spot-checked. For each cascade warning:
    // resolve the subject to its block, read `k` out of the message, index that block's OWN inputs
    // vector at `k`, and require that the port found there really is driven and really has every
    // driver deferred.
    //
    // This is what catches a `k` sourced from any list other than the deferred block's own inputs.
    // A wrong index either falls outside the vector or lands on a port whose drivers are not all
    // deferred — the single-position check in the first test cannot see the second case.
    for g in [
        misowned_connector_graph(),
        staggered_cascade_graph(),
        deep_position_cascade_graph(),
    ] {
        let report = export_ok(&g);
        let deferred: Vec<&str> = report
            .warnings
            .iter()
            .filter_map(|d| d.subject.as_deref())
            .collect();

        for d in &report.warnings {
            let Some(rest) = d.message.split("input connector `in").nth(1) else {
                continue; // not a cascade warning
            };
            let k: usize = rest
                .split('`')
                .next()
                .expect("the port name is delimited by a backtick")
                .parse()
                .expect("the cascade slot renders a bare index");

            let subject = d.subject.as_deref().expect("a warning names its block");
            // The subject must identify ONE block. `k` is a position in a specific block's input
            // list, so it means nothing if two blocks share an `instance_iri` — and export's
            // duplicate-`@id` claim only covers SURVIVORS, so a deferred block sharing an IRI with
            // a survivor is not rejected. That is a pre-existing gap in the duplicate-id surface
            // rather than a naming defect, but it bounds what "the warning names the port" can
            // mean, so the ambiguity is asserted away here rather than assumed absent.
            let matches: Vec<&BlockInstance> = g
                .blocks
                .iter()
                .filter(|b| b.instance_iri.as_deref() == Some(subject))
                .collect();
            assert_eq!(
                matches.len(),
                1,
                "`{subject}` must identify exactly one block for `in{k}` to be meaningful"
            );
            let b = matches[0];

            let cid = b
                .inputs
                .get(k)
                .unwrap_or_else(|| panic!("`{subject}` has no input at position {k}"));
            let drivers: Vec<&Connection> = g.connections.iter().filter(|c| c.to == *cid).collect();
            assert!(
                !drivers.is_empty(),
                "`{subject}.in{k}` is undriven, so it cannot have cascaded"
            );
            for c in drivers {
                let owner = g.connectors[c.from.0 as usize].block.0 as usize;
                let owner_iri = g.blocks[owner]
                    .instance_iri
                    .as_deref()
                    .expect("driver blocks carry an IRI here");
                assert!(
                    deferred.contains(&owner_iri),
                    "`{subject}.in{k}` still has the surviving driver `{owner_iri}`, so naming it \
                     as the cascade trigger is wrong"
                );
            }
        }
    }
}

/// Two cascades whose triggers sit at different port positions, so a name pinned to any single
/// constant is wrong for one of them. `mid` cascades on its input 1 and `tail` on its input 0.
fn staggered_cascade_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param(),
            ),
            // `mid.inputs[0]` is a boundary input (undriven), so the trigger is position 1.
            block(1, "CDL.Reals.Add", "mid", &[1, 2], &[3], vec![]),
            block(2, "CDL.Reals.Abs", "tail", &[4], &[5], vec![]),
            block(
                3,
                "CDL.Reals.Sources.Constant",
                "keeper",
                &[],
                &[6],
                real_param(1.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out),
            conn(1, 1, Dir::In).with_iri("http://example.org#Cascade.uSet"),
            conn(2, 1, Dir::In),
            conn(3, 1, Dir::Out),
            conn(4, 2, Dir::In),
            conn(5, 2, Dir::Out),
            conn(6, 3, Dir::Out),
        ],
        connections: vec![wire(0, 2), wire(3, 4)],
        external_inputs: vec![ConnectorId(1)],
    }
}

/// A cascade trigger at port position **2**, with positions 0 and 1 both boundary inputs.
///
/// Positions 0 and 1 alone cannot distinguish a correct `k` from one clamped to the first two
/// slots: `format!("in{}", k.min(1))` satisfies every test that only reaches position 1. The
/// deferral loop breaks at the first triggering input, so 0 and 1 must be undriven for the trigger
/// to land at 2.
fn deep_position_cascade_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param(),
            ),
            block(1, "CDL.Reals.MultiSum", "deep", &[1, 2, 3], &[4], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "keeper",
                &[],
                &[5],
                real_param(2.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out),
            conn(1, 1, Dir::In).with_iri("http://example.org#Cascade.uOne"),
            conn(2, 1, Dir::In).with_iri("http://example.org#Cascade.uTwo"),
            conn(3, 1, Dir::In),
            conn(4, 1, Dir::Out),
            conn(5, 2, Dir::Out),
        ],
        connections: vec![wire(0, 3)],
        external_inputs: vec![ConnectorId(1), ConnectorId(2)],
    }
}

#[test]
fn a_cascade_trigger_beyond_the_first_two_positions_is_named_exactly() {
    // Guards against a `k` clamped to a small range. Only an index taken from the real port-list
    // position can produce `in2` here; anything saturating at 1 reports the wrong port, and the
    // two ports it would name are boundary inputs that did not cascade.
    let report = export_ok(&deep_position_cascade_graph());
    let warning = cascade_warning_for(&report, &iri("deep"))
        .expect("`deep` cascade-defers on its third input");
    assert!(
        warning.contains("input connector `in2`"),
        "the trigger is `deep.inputs[2]`: {warning}"
    );
}

#[test]
fn cascade_positions_are_read_per_block_not_from_a_shared_constant() {
    let report = export_ok(&staggered_cascade_graph());

    let mid = cascade_warning_for(&report, &iri("mid")).expect("`mid` cascades on its input 1");
    assert!(
        mid.contains("input connector `in1`"),
        "mid's boundary input at position 0 is not the trigger: {mid}"
    );

    let tail = cascade_warning_for(&report, &iri("tail")).expect("`tail` cascades on its input 0");
    assert!(
        tail.contains("input connector `in0`"),
        "tail's only input is at position 0: {tail}"
    );
}
