//! Algebraic-loop refusals carry a stable diagnostic code, not only display prose.
//!
//! A host is told to branch on a diagnostic's code and never on message text, so a schedule
//! refusal must reach the load receipt as coded records whose subjects name the cycle members by
//! their authored identities. These tests drive the public receipt path end to end from CXF.
//!
//! Each record's message is bounded independently of the cycle length — it names the member's
//! position, the member count and the next member — so the receipt grows linearly with the loop,
//! and the loop order the receipt's subject sort discards is recoverable from the records.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::{DiagnosticReceipt, DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, Engine};
use crate::{OcError, OperationFailure};
use serde_json::{Map, Value, json};

const ANALOG_ALGEBRAIC_LOOP: &[u8] =
    include_bytes!("fixtures/analog_warning_algebraic_loop.jsonld");
const DELAY_BROKEN_LOOP: &[u8] =
    include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const LOOP_AFTER_WARNING_GOLDEN: &str =
    include_str!("fixtures/golden/algebraic_loop_after_warning.receipt.txt");
/// `RealExtractSignal` (`y1 = u1`, `y2 = u2`) whose `y1` returns through a gain to its own `u2`:
/// acyclic at connector level, cyclic for atomic block firing.
const EXTRACT_SIGNAL_BLOCK_LOOP: &[u8] =
    include_bytes!("fixtures/extract_signal_block_loop.jsonld");
const EXTRACT_SIGNAL_BLOCK_LOOP_GOLDEN: &str =
    include_str!("fixtures/golden/extract_signal_block_loop.receipt.txt");
/// Upper bound on one record's message bytes beyond the next member's identity: the fixed cause and
/// remedy text plus two decimal counts. Independent of the cycle length by construction. Sized for
/// the longer kind: the block-granularity text is 290 fixed bytes (the connector-level text 265),
/// plus two counts of at most 20 decimal digits each (`usize::MAX`), so it holds for any cycle.
const MESSAGE_OVERHEAD_BYTES: usize = 290 + 2 * 20;

const PREFIX: &str = "http://example.org#m.";

fn iri(local: &str) -> String {
    format!("{PREFIX}{local}")
}

/// A CXF document of `Reals.Add` blocks wired by `wires` (`from` → `to` local names). `u` is the
/// one boundary input and `y` the one boundary output; every other endpoint is `<block>.<port>`.
fn add_network<A: AsRef<str>, W: AsRef<str>>(adds: &[A], wires: &[(W, W)]) -> Vec<u8> {
    let mut targets: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for (from, to) in wires {
        targets
            .entry(from.as_ref())
            .or_default()
            .push(json!({ "@id": iri(to.as_ref()) }));
    }
    let connector = |local: &str, kind: &str| -> Value {
        let mut node = Map::new();
        node.insert("@id".into(), json!(iri(local)));
        node.insert("@type".into(), json!(kind));
        node.insert("S231:isOfDataType".into(), json!({ "@id": "S231:Real" }));
        if let Some(driven) = targets.get(local) {
            node.insert("S231:isConnectedTo".into(), Value::Array(driven.clone()));
        }
        Value::Object(node)
    };
    let adds: Vec<&str> = adds.iter().map(AsRef::as_ref).collect();
    let mut graph = vec![
        json!({
            "@id": "http://example.org#m",
            "@type": "S231:Block",
            "S231:containsBlock": adds.iter().map(|add| json!({ "@id": iri(add) })).collect::<Vec<_>>(),
            "S231:hasInput": { "@id": iri("u") },
            "S231:hasOutput": { "@id": iri("y") },
        }),
        connector("u", "S231:RealInput"),
        connector("y", "S231:RealOutput"),
    ];
    for add in &adds {
        graph.push(json!({
            "@id": iri(add),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
            "S231:hasInput": [
                { "@id": iri(&format!("{add}.u1")) },
                { "@id": iri(&format!("{add}.u2")) },
            ],
            "S231:hasOutput": { "@id": iri(&format!("{add}.y")) },
        }));
        graph.push(connector(&format!("{add}.u1"), "S231:RealInput"));
        graph.push(connector(&format!("{add}.u2"), "S231:RealInput"));
        graph.push(connector(&format!("{add}.y"), "S231:RealOutput"));
    }
    serde_json::to_vec(&json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph,
    }))
    .expect("serialize document")
}

/// The issue's reproduction: one `Add` whose output feeds its own `u2`.
fn self_loop() -> Vec<u8> {
    add_network(
        &["add"],
        &[("u", "add.u1"), ("add.y", "add.u2"), ("add.y", "y")],
    )
}

/// Three `Add` blocks in one ring: `a.y → b.u2 → b.y → c.u2 → c.y → a.u2`.
fn three_block_ring() -> Vec<u8> {
    add_network(
        &["a", "b", "c"],
        &[
            ("u", "a.u1"),
            ("u", "b.u1"),
            ("u", "c.u1"),
            ("a.y", "b.u2"),
            ("b.y", "c.u2"),
            ("c.y", "a.u2"),
            ("c.y", "y"),
        ],
    )
}

/// `count` `Add` blocks `r0 … r<count-1>` in one ring, each `y` feeding the next block's `u2`.
fn add_ring(count: usize) -> Vec<u8> {
    let adds: Vec<String> = (0..count).map(|index| format!("r{index}")).collect();
    let mut wires: Vec<(String, String)> = Vec::with_capacity(2 * count + 1);
    for index in 0..count {
        wires.push(("u".into(), format!("r{index}.u1")));
        wires.push((
            format!("r{index}.y"),
            format!("r{}.u2", (index + 1) % count),
        ));
    }
    wires.push((format!("r{}.y", count - 1), "y".into()));
    add_network(&adds, &wires)
}

/// `count` `RealExtractSignal` blocks `e0 … e<count-1>` (`nin = nout = 2`, so `y1 = u1` and
/// `y2 = u2`) in one ring: each `y1` feeds the next block's `u2`, every `u1` reads the boundary input.
/// No output depends on the input its ring predecessor drives, so the connector graph is acyclic and
/// the whole ring is one block-granularity cycle of `count` members.
fn extract_ring(count: usize) -> Vec<u8> {
    let local = |index: usize, port: &str| format!("e{index}.{port}");
    let connector = |id: String, kind: &str, data: &str, to: Option<String>| -> Value {
        let mut node = Map::new();
        node.insert("@id".into(), json!(iri(&id)));
        node.insert("@type".into(), json!(kind));
        node.insert("S231:isOfDataType".into(), json!({ "@id": data }));
        if let Some(to) = to {
            node.insert("S231:isConnectedTo".into(), json!({ "@id": iri(&to) }));
        }
        Value::Object(node)
    };
    let mut boundary_input = Map::new();
    boundary_input.insert("@id".into(), json!(iri("u")));
    boundary_input.insert("@type".into(), json!("S231:RealInput"));
    boundary_input.insert("S231:isOfDataType".into(), json!({ "@id": "S231:Real" }));
    boundary_input.insert(
        "S231:isConnectedTo".into(),
        Value::Array(
            (0..count)
                .map(|i| json!({ "@id": iri(&local(i, "u1")) }))
                .collect(),
        ),
    );
    let mut graph = vec![
        json!({
            "@id": "http://example.org#m",
            "@type": "S231:Block",
            "S231:containsBlock": (0..count).map(|i| json!({ "@id": iri(&format!("e{i}")) })).collect::<Vec<_>>(),
            "S231:hasInput": { "@id": iri("u") },
            "S231:hasOutput": { "@id": iri("y") },
        }),
        Value::Object(boundary_input),
        connector("y".into(), "S231:RealOutput", "S231:Real", None),
    ];
    for index in 0..count {
        let block = format!("e{index}");
        graph.push(json!({
            "@id": iri(&block),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Routing.RealExtractSignal",
            "S231:hasParameter": [
                { "@id": iri(&local(index, "nin")) },
                { "@id": iri(&local(index, "nout")) },
            ],
            "S231:hasInput": [
                { "@id": iri(&local(index, "u1")) },
                { "@id": iri(&local(index, "u2")) },
            ],
            "S231:hasOutput": [
                { "@id": iri(&local(index, "y1")) },
                { "@id": iri(&local(index, "y2")) },
            ],
        }));
        for name in ["nin", "nout"] {
            graph.push(json!({
                "@id": iri(&local(index, name)),
                "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Integer" },
                "S231:value": 2,
            }));
        }
        graph.push(connector(
            local(index, "u1"),
            "S231:RealInput",
            "S231:Real",
            None,
        ));
        graph.push(connector(
            local(index, "u2"),
            "S231:RealInput",
            "S231:Real",
            None,
        ));
        graph.push(connector(
            local(index, "y1"),
            "S231:RealOutput",
            "S231:Real",
            Some(local((index + 1) % count, "u2")),
        ));
        let y2_target = (index == count - 1).then(|| "y".to_owned());
        graph.push(connector(
            local(index, "y2"),
            "S231:RealOutput",
            "S231:Real",
            y2_target,
        ));
    }
    serde_json::to_vec(&json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph,
    }))
    .expect("serialize document")
}

/// Two independent self-loops, `p` and `q`, declared in `order`.
fn two_disjoint_self_loops(order: &[&str]) -> Vec<u8> {
    add_network(
        order,
        &[
            ("u", "p.u1"),
            ("u", "q.u1"),
            ("p.y", "p.u2"),
            ("q.y", "q.u2"),
            ("p.y", "y"),
        ],
    )
}

pub(super) fn refuse(bytes: &[u8]) -> OperationFailure {
    Engine::in_memory()
        .load_cxf_with_receipt(bytes)
        .expect_err("fixture must be refused")
}

/// The terminal build error's own display text, without the facade's variant prefix.
pub(super) fn build_message(failure: &OperationFailure) -> String {
    let terminal = std::error::Error::source(failure.error())
        .and_then(|source| source.downcast_ref::<OcError>())
        .unwrap_or(failure.error());
    let OcError::Build(build) = terminal else {
        panic!("terminal error must be a build refusal, got {terminal:?}")
    };
    build.to_string()
}

/// Machine fields of every `algebraic-loop` record, in receipt order.
fn loop_records(receipt: &DiagnosticReceipt) -> Vec<(DiagnosticStage, DiagnosticSeverity, String)> {
    receipt
        .records()
        .iter()
        .filter(|record| record.key().code() == "algebraic-loop")
        .map(|record| {
            let DiagnosticSubject::Opaque(subject) = record.key().subject() else {
                panic!("an algebraic-loop record must name a member: {record:?}")
            };
            (
                record.key().stage(),
                record.key().severity(),
                subject.clone(),
            )
        })
        .collect()
}

fn schedule_errors(locals: &[&str]) -> Vec<(DiagnosticStage, DiagnosticSeverity, String)> {
    locals
        .iter()
        .map(|local| {
            (
                DiagnosticStage::Schedule,
                DiagnosticSeverity::Error,
                iri(local),
            )
        })
        .collect()
}

/// Reconstruct the reported loop from the records of `code`, without the build error: order the
/// records by the member position each message states, then check every record's stated next
/// member is the subject of the following record (wrapping to the first).
pub(super) fn loop_order(receipt: &DiagnosticReceipt, code: &str) -> Vec<String> {
    let mut positioned: Vec<(usize, usize, String, String)> = receipt
        .records()
        .iter()
        .filter(|record| record.key().code() == code)
        .map(|record| {
            let DiagnosticSubject::Opaque(subject) = record.key().subject() else {
                panic!("a loop record must name a member: {record:?}")
            };
            let (head, tail) = record
                .message()
                .split_once("; next member: ")
                .expect("message states the next member");
            let (_, counts) = head
                .rsplit_once(" member ")
                .expect("message states the position");
            let (position, count) = counts.split_once(" of ").expect("position of count");
            let (next, _) = tail
                .split_once(". The ")
                .expect("next member ends the clause");
            (
                position.parse().expect("decimal position"),
                count.parse().expect("decimal count"),
                subject.clone(),
                next.to_owned(),
            )
        })
        .collect();
    positioned.sort_by_key(|(position, ..)| *position);
    let count = positioned.len();
    for (index, (position, stated_count, _, next)) in positioned.iter().enumerate() {
        assert_eq!(*position, index + 1, "positions are 1..=n without gaps");
        assert_eq!(*stated_count, count, "every record states the same count");
        assert_eq!(
            next,
            &positioned[(index + 1) % count].2,
            "next member links the loop"
        );
    }
    positioned
        .into_iter()
        .map(|(_, _, subject, _)| subject)
        .collect()
}

pub(super) fn render(receipt: &DiagnosticReceipt) -> String {
    let mut rendered = String::new();
    for record in receipt.records() {
        let subject = match record.key().subject() {
            DiagnosticSubject::Absent => "<none>",
            DiagnosticSubject::Opaque(subject) => subject.as_str(),
        };
        writeln!(
            rendered,
            "{:?}|{:?}|{}|{}|{}",
            record.key().stage(),
            record.key().severity(),
            record.key().code(),
            subject,
            record.message()
        )
        .expect("write record");
    }
    rendered
}

#[test]
fn self_loop_refusal_reports_one_algebraic_loop_error_per_member_at_schedule() {
    let failure = refuse(&self_loop());
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    let records = failure.diagnostics().records();
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(
        loop_records(failure.diagnostics()),
        schedule_errors(&["add.u2", "add.y"])
    );
    let remedy = "The connectors form a direct-feedthrough cycle not broken by a state-holding \
                  block. CDL §7.16 forbids algebraic loops. Remedy: insert a delay block \
                  (CDL.Logical.Pre, CDL.Discrete.UnitDelay) or an integrator into the loop.";
    assert_eq!(
        records[0].message(),
        format!(
            "algebraic loop member 1 of 2; next member: {}. {remedy}",
            iri("add.y")
        )
    );
    assert_eq!(
        records[1].message(),
        format!(
            "algebraic loop member 2 of 2; next member: {}. {remedy}",
            iri("add.u2")
        )
    );
    // The refusal's own display text still lists every member, in loop order.
    let message = build_message(&failure);
    assert!(
        message.ends_with(&format!(
            "Cycle members: [{:?}, {:?}]",
            iri("add.u2"),
            iri("add.y")
        )),
        "{message}"
    );
}

#[test]
fn coded_records_leave_the_legacy_error_chain_unchanged() {
    let failure = refuse(&self_loop());
    assert!(matches!(failure.error(), OcError::Build(_)), "{failure:?}");
    assert!(failure.error().diagnostics().is_empty());
    assert_eq!(failure.error().all_diagnostics().count(), 0);
    assert_eq!(failure.to_string(), failure.error().to_string());

    let legacy = Engine::in_memory()
        .load_cxf(&self_loop())
        .expect_err("legacy load must refuse the same loop");
    assert!(matches!(legacy, OcError::Build(_)), "{legacy:?}");
    assert_eq!(legacy.to_string(), failure.error().to_string());
}

#[test]
fn multi_block_ring_names_every_member_connector_once() {
    let failure = refuse(&three_block_ring());
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    assert_eq!(
        loop_records(failure.diagnostics()),
        schedule_errors(&["a.u2", "a.y", "b.u2", "b.y", "c.u2", "c.y"])
    );
    assert_eq!(failure.diagnostics().records().len(), 6);
    // Receipt order is subject order; the stated positions recover the loop order itself.
    assert_eq!(
        loop_order(failure.diagnostics(), "algebraic-loop"),
        ["a.u2", "a.y", "b.u2", "b.y", "c.u2", "c.y"].map(iri)
    );
}

#[test]
fn disjoint_cycles_report_the_first_declared_cycle_only() {
    // One concrete cycle is reported, chosen by declaration order rather than by hash or by the
    // order connections happen to appear, so swapping the declarations swaps the reported cycle.
    let p_first = refuse(&two_disjoint_self_loops(&["p", "q"]));
    assert_eq!(
        loop_records(p_first.diagnostics()),
        schedule_errors(&["p.u2", "p.y"])
    );
    let q_first = refuse(&two_disjoint_self_loops(&["q", "p"]));
    assert_eq!(
        loop_records(q_first.diagnostics()),
        schedule_errors(&["q.u2", "q.y"])
    );
}

#[test]
fn delay_broken_feedback_loads_without_an_algebraic_loop_record() {
    let receipt = Engine::in_memory()
        .load_cxf_with_receipt(DELAY_BROKEN_LOOP)
        .expect("a UnitDelay cuts the feedback, so the model must load");
    assert!(
        receipt
            .diagnostics()
            .records()
            .iter()
            .all(|record| !record.key().code().contains("algebraic-loop")),
        "{:?}",
        receipt.diagnostics()
    );
}

#[test]
fn prior_warning_and_loop_records_match_the_golden() {
    let failure = refuse(ANALOG_ALGEBRAIC_LOOP);
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    assert_eq!(render(failure.diagnostics()), LOOP_AFTER_WARNING_GOLDEN);
    // The warning still contextualizes the terminal failure exactly as before.
    assert!(matches!(failure.error(), OcError::LoadContext(_)));
    assert!(failure.error().diagnostics().is_empty());
    assert_eq!(failure.error().all_diagnostics().count(), 1);
}

#[test]
fn repeated_refusals_produce_identical_records() {
    for bytes in [
        self_loop(),
        three_block_ring(),
        ANALOG_ALGEBRAIC_LOOP.to_vec(),
    ] {
        let first = render(refuse(&bytes).diagnostics());
        let second = render(refuse(&bytes).diagnostics());
        assert_eq!(first, second);

        // Refusing a reload over a loaded model records the same evidence as a fresh engine.
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(DELAY_BROKEN_LOOP)
            .expect("baseline model must load");
        let reload = engine
            .load_cxf_with_receipt(&bytes)
            .expect_err("reload must be refused");
        assert_eq!(render(reload.diagnostics()), first);
    }
}

#[test]
fn large_ring_receipt_grows_linearly_with_the_loop() {
    // 1000 blocks, 2000 member connectors. A message that repeated the member list would make the
    // receipt quadratic (126 MB here before the bound); each message is now fixed text plus the
    // next member's identity.
    let bytes = add_ring(1000);
    let failure = refuse(&bytes);
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    let records = failure.diagnostics().records();
    assert_eq!(records.len(), 2000);
    let mut total = 0;
    let mut bound = 0;
    for record in records {
        let (_, tail) = record
            .message()
            .split_once("; next member: ")
            .expect("message states the next member");
        let (next, _) = tail
            .split_once(". The ")
            .expect("next member ends the clause");
        assert!(
            record.message().len() <= MESSAGE_OVERHEAD_BYTES + next.len(),
            "{} bytes: {}",
            record.message().len(),
            record.message()
        );
        total += record.message().len();
        bound += MESSAGE_OVERHEAD_BYTES + next.len();
    }
    assert!(total <= bound, "{total} > {bound}");
    assert!(
        total < bytes.len(),
        "{total} record bytes for {} CXF bytes",
        bytes.len()
    );
    let order = loop_order(failure.diagnostics(), "algebraic-loop");
    assert_eq!(order.len(), 2000);
    assert_eq!(order[0], iri("r0.u2"));
    assert_eq!(order[1], iri("r0.y"));
}

#[test]
fn large_block_ring_receipt_stays_bounded_per_member() {
    // 500 extract blocks, one block-granularity cycle of 500 members: the longer
    // "may be CDL-legal" cause text keeps every record within the same fixed overhead.
    let bytes = extract_ring(500);
    let failure = refuse(&bytes);
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    let records = failure.diagnostics().records();
    assert_eq!(records.len(), 500);
    for record in records {
        assert_eq!(record.key().code(), "block-algebraic-loop");
        let (_, tail) = record
            .message()
            .split_once("; next member: ")
            .expect("message states the next member");
        let (next, _) = tail
            .split_once(". The ")
            .expect("next member ends the clause");
        assert!(
            record.message().len() <= MESSAGE_OVERHEAD_BYTES + next.len(),
            "{} bytes: {}",
            record.message().len(),
            record.message()
        );
    }
    let order = loop_order(failure.diagnostics(), "block-algebraic-loop");
    assert_eq!(order.len(), 500);
    assert_eq!(order[0], iri("e0"));
    assert_eq!(order[1], iri("e1"));
    assert_eq!(order[499], iri("e499"));
}

#[test]
fn block_granularity_loop_receipt_and_error_say_the_model_may_be_cdl_legal() {
    let failure = refuse(EXTRACT_SIGNAL_BLOCK_LOOP);
    assert_eq!(
        render(failure.diagnostics()),
        EXTRACT_SIGNAL_BLOCK_LOOP_GOLDEN
    );
    let message = build_message(&failure);
    for needle in [
        "block-granularity algebraic loop: 2 block(s)",
        "no connector-level feedthrough cycle exists",
        "The model may be CDL-legal",
        "fires all of its outputs together",
        "split the block",
        "CDL.Discrete.UnitDelay",
    ] {
        assert!(message.contains(needle), "missing {needle:?}: {message}");
    }
    assert!(!message.contains("§7.16 forbids"), "{message}");
    assert!(
        message.ends_with(
            r#"Cycle members: ["http://example.org#ext.sel", "http://example.org#ext.gai"]"#
        ),
        "{message}"
    );
}

#[test]
fn block_granularity_loop_from_cxf_names_member_instances() {
    let failure = refuse(EXTRACT_SIGNAL_BLOCK_LOOP);
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    let records = failure.diagnostics().records();
    let observed: Vec<_> = records
        .iter()
        .map(|record| {
            (
                record.key().stage(),
                record.key().severity(),
                record.key().code(),
                record.key().subject().clone(),
            )
        })
        .collect();
    assert_eq!(
        observed,
        ["http://example.org#ext.gai", "http://example.org#ext.sel"].map(|subject| (
            DiagnosticStage::Schedule,
            DiagnosticSeverity::Error,
            "block-algebraic-loop",
            DiagnosticSubject::Opaque(subject.to_owned()),
        ))
    );
    assert_eq!(
        loop_order(failure.diagnostics(), "block-algebraic-loop"),
        ["http://example.org#ext.sel", "http://example.org#ext.gai"]
    );
    assert!(
        records[0]
            .message()
            .starts_with("block-granularity algebraic loop member 2 of 2; next member: "),
        "{}",
        records[0].message()
    );
    assert!(matches!(failure.error(), OcError::Build(_)), "{failure:?}");
}

#[test]
fn refused_loop_reload_keeps_the_previous_model_serving() {
    const SETPOINT: &str = "http://example.org#MinLoop.uSet";
    const ALARM: &str = "http://example.org#MinLoop.yAlarm";
    let advance = |engine: &mut Engine, time: f64, value: f64| {
        let frame = engine
            .prepare_frame(time, &[(SETPOINT, crate::Value::Real(value))])
            .expect("previous model still accepts its inputs");
        engine
            .execute_frame(frame)
            .expect("previous model still executes");
    };
    for bytes in [
        self_loop(),
        ANALOG_ALGEBRAIC_LOOP.to_vec(),
        EXTRACT_SIGNAL_BLOCK_LOOP.to_vec(),
    ] {
        let mut engine = Engine::in_memory();
        let loaded = engine
            .load_cxf_with_receipt(DELAY_BROKEN_LOOP)
            .expect("baseline model must load");
        advance(&mut engine, 0.0, 1.0);
        let inputs = format!("{:?}", engine.input_definitions().expect("inputs"));
        let exported = engine.export_cxf().expect("export").bytes;
        let state = engine.state_snapshot().expect("snapshot").into_bytes();

        let refused = engine
            .load_cxf_with_receipt(&bytes)
            .expect_err("loop reload must be refused");
        assert_eq!(refused.stage(), DiagnosticStage::Schedule);

        assert_eq!(engine.model_id, loaded.report().model_id);
        assert_eq!(format!("{:?}", engine.input_definitions().unwrap()), inputs);
        assert_eq!(engine.export_cxf().unwrap().bytes, exported);
        assert_eq!(engine.state_snapshot().unwrap().into_bytes(), state);

        // The previous model keeps advancing exactly like one that never saw the refusal.
        advance(&mut engine, 1.0, 3.0);
        let mut twin = Engine::in_memory();
        twin.load_cxf(DELAY_BROKEN_LOOP).unwrap();
        advance(&mut twin, 0.0, 1.0);
        advance(&mut twin, 1.0, 3.0);
        assert!(
            engine
                .get_output(ALARM)
                .unwrap()
                .bit_eq(&twin.get_output(ALARM).unwrap())
        );
        assert_eq!(
            engine.state_snapshot().unwrap().into_bytes(),
            twin.state_snapshot().unwrap().into_bytes()
        );
    }
}
