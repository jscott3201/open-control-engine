//! The two load-time budgets of issue #815 at their edges: the canonical encoding's length, 4 MiB,
//! and the label bytes a document's connectors carry after §7.10 unification, 8 MiB. Each is
//! pinned exactly at the budget (loads) and one byte past it (refused, `resource-limit-exceeded`),
//! with what the check costs and where it sits among the other refusals.
//!
//! The canonical length is counted from the records, never encoded: a refused model's load
//! requests a small fraction of the bytes it would have encoded. Measured with `allocation-counter`,
//! whose counters are local to the measuring thread.

use allocation_counter::measure;
use oce_api::{Engine, OcError};
use oce_diag::DiagCode;

const T: &str = "http://example.org#Buildings.Controls.OBC.CDL.";

/// The canonical budget (`MAX_CANONICAL_ENCODING_BYTES`), restated.
const CANONICAL_BUDGET: usize = 4 * 1024 * 1024;

/// The label budget (`oce-validate`'s `MAX_DOCUMENT_LABEL_BYTES`), restated.
const LABEL_BUDGET: usize = 8 * 1024 * 1024;

fn port(id: &str, kind: &str, extra: &str) -> String {
    format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"S231:Real{kind}\", \"S231:isOfDataType\": \
         {{ \"@id\": \"S231:Real\" }}{extra} }}"
    )
}

fn wired_to(targets: &[String]) -> String {
    let items: Vec<String> = targets
        .iter()
        .map(|target| format!("{{ \"@id\": \"{target}\" }}"))
        .collect();
    format!(", \"S231:isConnectedTo\": [ {} ]", items.join(", "))
}

fn constant(id: &str, output: &str, nodes: &mut Vec<String>) {
    nodes.push(format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.Sources.Constant\", \"S231:hasParameter\": \
         {{ \"@id\": \"{id}.k\" }}, \"S231:hasOutput\": {{ \"@id\": \"{id}.y\" }} }}"
    ));
    nodes.push(format!("{{ \"@id\": \"{id}.k\", \"S231:value\": 1.0 }}"));
    nodes.push(port(&format!("{id}.y"), "Output", output));
}

fn abs(id: &str, input: &str, output: &str, nodes: &mut Vec<String>) {
    nodes.push(format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.Abs\", \"S231:hasInput\": {{ \"@id\": \
         \"{id}.u\" }}, \"S231:hasOutput\": {{ \"@id\": \"{id}.y\" }} }}"
    ));
    nodes.push(port(&format!("{id}.u"), "Input", input));
    nodes.push(port(&format!("{id}.y"), "Output", output));
}

/// A document of `blocks` under the root `urn:top`, with `prefix` bound to `p` when given.
fn document(prefix: Option<&str>, blocks: &[String], nodes: &[String]) -> Vec<u8> {
    let context = prefix.map_or_else(String::new, |p| format!(", \"p\": \"{p}\""));
    let listed: Vec<String> = blocks
        .iter()
        .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
        .collect();
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\"{context} }}, \"@graph\": [ \
         {{ \"@id\": \"urn:top\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [ {} ] }},\n{} ] }}",
        listed.join(", "),
        nodes.join(",\n")
    )
    .into_bytes()
}

/// A `Constant` whose relative name is `name_bytes` long (written once, under a prefix) driving
/// `targets` `Abs` blocks, and an unwired `Constant` whose relative name is `pad` bytes long:
/// the pad's name enters the canonical encoding once, so it moves the length a byte at a time.
fn fanned_out(name_bytes: usize, targets: usize, pad: usize) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut blocks = vec!["p:c".to_owned()];
    let fed: Vec<String> = (0..targets).map(|i| format!("urn:a{i}.u")).collect();
    constant("p:c", &wired_to(&fed), &mut nodes);
    for i in 0..targets {
        abs(&format!("urn:a{i}"), "", "", &mut nodes);
        blocks.push(format!("urn:a{i}"));
    }
    let pad_block = format!("urn:pad:{}", "q".repeat(pad));
    constant(&pad_block, "", &mut nodes);
    blocks.push(pad_block);
    document(
        Some(&format!("urn:x:{}", "k".repeat(name_bytes))),
        &blocks,
        &nodes,
    )
}

/// The canonical encoding's length of a document that loads.
fn canonical_length(document: &[u8]) -> usize {
    let mut engine = Engine::new();
    engine.load_cxf(document).expect("loads");
    engine.canonical_content_bytes().expect("bytes").len()
}

/// The one `resource-limit-exceeded` error the terminal stage of `error` carries: the load gate's
/// refusal, with no other error beside it.
fn budget_refusal(error: &OcError) -> String {
    let terminal: Vec<_> = error
        .diagnostics()
        .iter()
        .filter(|d| d.is_error())
        .collect();
    assert_eq!(terminal.len(), 1, "one refusal: {error:?}");
    assert_eq!(
        terminal[0].code,
        DiagCode::ResourceLimitExceeded,
        "{error:?}"
    );
    terminal[0].message.clone()
}

#[test]
fn a_canonical_encoding_at_the_budget_loads_and_one_byte_more_is_refused() {
    // 60 wires from a 64 KiB name encode a little under 4 MiB; the pad's name makes up the rest.
    let short = canonical_length(&fanned_out(65_536, 60, 1));
    let pad = 1 + CANONICAL_BUDGET - short;
    let at_budget = fanned_out(65_536, 60, pad);
    let mut engine = Engine::new();
    engine
        .load_cxf(&at_budget)
        .expect("the budget's edge loads");
    assert_eq!(
        engine.canonical_content_bytes().expect("bytes").len(),
        CANONICAL_BUDGET
    );
    engine.content_id().expect("an id");
    engine.compile_blob().expect("a blob");
    engine.state_snapshot().expect("a snapshot");

    let error = Engine::new()
        .load_cxf(&fanned_out(65_536, 60, pad + 1))
        .map(drop)
        .expect_err("one byte past the budget");
    let message = budget_refusal(&error);
    assert!(
        message.contains(&format!("would take {} bytes", CANONICAL_BUDGET + 1)),
        "{message}"
    );
    assert!(message.contains("more than the 4194304 bytes"), "{message}");
}

#[test]
fn a_canonical_refusal_costs_the_records_not_the_bytes_they_encode() {
    // 64 and then 256 wires from a 128 KiB and from a 256 KiB name: encodings of 8 to 64 MiB, all
    // refused. Each further wire would add its source's name to the encoding; to the refusal it
    // adds its records and its nodes, the same whatever the name's length. [measured: the 192
    // further wires request about 5.5 MB at either length, 29 KB a wire for the reader's and the
    // validator's nodes; the two lengths differ by under 1 KiB a wire, the ceiling below]
    let requested = |name_bytes: usize, wires: usize| {
        let document = fanned_out(name_bytes, wires, 1);
        let mut outcome = None;
        let info = measure(|| outcome = Some(Engine::new().load_cxf(&document)));
        let error = outcome
            .expect("ran")
            .map(drop)
            .expect_err("past the budget");
        let message = budget_refusal(&error);
        assert!(message.contains("canonical encoding"), "{message}");
        i128::from(info.bytes_total)
    };
    let further = |name_bytes| requested(name_bytes, 256) - requested(name_bytes, 64);
    let (shorter, longer) = (further(131_072), further(262_144));
    assert!(
        (longer - shorter).abs() < 192 * 1024,
        "192 further wires: {shorter} bytes under a 128 KiB name, {longer} under a 256 KiB one"
    );
}

#[test]
fn the_canonical_budget_closes_the_load_gate_before_any_block_is_built() {
    // A document past the budget whose blocks also form an algebraic loop (a schedule refusal,
    // after instantiation) is refused for the budget alone.
    let mut nodes = Vec::new();
    let fed: Vec<String> = (0..64).map(|i| format!("urn:a{i}.u")).collect();
    constant("p:c", &wired_to(&fed), &mut nodes);
    let mut blocks = vec!["p:c".to_owned()];
    for i in 0..64 {
        abs(&format!("urn:a{i}"), "", "", &mut nodes);
        blocks.push(format!("urn:a{i}"));
    }
    abs(
        "urn:l0",
        "",
        &wired_to(&["urn:l1.u".to_owned()]),
        &mut nodes,
    );
    abs(
        "urn:l1",
        "",
        &wired_to(&["urn:l0.u".to_owned()]),
        &mut nodes,
    );
    blocks.extend(["urn:l0".to_owned(), "urn:l1".to_owned()]);
    let looped = |name_bytes: usize| {
        document(
            Some(&format!("urn:x:{}", "k".repeat(name_bytes))),
            &blocks,
            &nodes,
        )
    };
    // Under the budget the loop is what refuses the document.
    let error = Engine::new()
        .load_cxf(&looped(8))
        .map(drop)
        .expect_err("a loop");
    assert!(format!("{error:?}").contains("AlgebraicLoop"), "{error:?}");
    assert!(error.diagnostics().is_empty(), "{error:?}");
    // Past it, the budget refuses it first.
    let error = Engine::new()
        .load_cxf(&looped(131_072))
        .map(drop)
        .expect_err("past the budget");
    budget_refusal(&error);
}

/// An output whose `unit` is `unit_bytes` long drives `targets` `Abs` inputs that declare none, so
/// unification gives each the unit; the first `literal` of them write the same unit themselves,
/// and one more byte of label, a `displayUnit` of one letter, is written on the last block's
/// output when `extra`. With `reversed`, every node and every list is written in reverse order.
fn labelled(
    unit_bytes: usize,
    targets: usize,
    literal: usize,
    extra: bool,
    reversed: bool,
) -> Vec<u8> {
    let unit = format!(", \"S231:unit\": \"{}\"", "u".repeat(unit_bytes));
    let mut fed: Vec<String> = (0..targets).map(|i| format!("urn:a{i}.u")).collect();
    if reversed {
        fed.reverse();
    }
    let mut nodes = Vec::new();
    constant("urn:c", &format!("{unit}{}", wired_to(&fed)), &mut nodes);
    let mut blocks = vec!["urn:c".to_owned()];
    for i in 0..targets {
        let input = if i < literal { unit.as_str() } else { "" };
        let output = if extra && i + 1 == targets {
            ", \"S231:displayUnit\": \"d\""
        } else {
            ""
        };
        abs(&format!("urn:a{i}"), input, output, &mut nodes);
        blocks.push(format!("urn:a{i}"));
    }
    if reversed {
        nodes.reverse();
        blocks.reverse();
    }
    document(None, &blocks, &nodes)
}

#[test]
fn labels_at_the_budget_load_and_one_byte_more_is_refused() {
    // A 64 KiB unit on one output and the 127 inputs it reaches: 128 connectors carry it, exactly
    // 8 MiB. One more letter of label anywhere is refused, whatever the order or spelling.
    for literal in [0, 32] {
        for reversed in [false, true] {
            let at = labelled(65_536, 127, literal, false, reversed);
            Engine::new()
                .load_cxf(&at)
                .unwrap_or_else(|e| panic!("{literal} literal, reversed {reversed}: {e:?}"));
            let past = labelled(65_536, 127, literal, true, reversed);
            let error = Engine::new()
                .load_cxf(&past)
                .map(drop)
                .expect_err("one byte past");
            let message = budget_refusal(&error);
            assert!(
                message.contains(&format!("take {} bytes", LABEL_BUDGET + 1)),
                "{literal} literal, reversed {reversed}: {message}"
            );
        }
    }
}

#[test]
fn the_label_budget_is_checked_before_the_canonical_budget() {
    // A long block name fanned out (an encoding past 4 MiB) whose output also carries a unit
    // that, propagated, passes 8 MiB: unification refuses the labels, and the load stops there.
    let unit = format!(", \"S231:unit\": \"{}\"", "u".repeat(135_168));
    let fed: Vec<String> = (0..64).map(|i| format!("urn:a{i}.u")).collect();
    let mut nodes = Vec::new();
    constant("p:c", &format!("{unit}{}", wired_to(&fed)), &mut nodes);
    let mut blocks = vec!["p:c".to_owned()];
    for i in 0..64 {
        abs(&format!("urn:a{i}"), "", "", &mut nodes);
        blocks.push(format!("urn:a{i}"));
    }
    let document = document(
        Some(&format!("urn:x:{}", "k".repeat(131_072))),
        &blocks,
        &nodes,
    );
    let error = Engine::new()
        .load_cxf(&document)
        .map(drop)
        .expect_err("both budgets");
    let message = budget_refusal(&error);
    assert!(message.contains("unification"), "{message}");
}

/// A root input `p:u`, whose path is `path_bytes` longer than written, driving `targets` `Abs`
/// blocks: the snapshot spells the path once per input, the canonical encoding not at all.
fn boundary_fanned_out(path_bytes: usize, targets: usize) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut blocks = Vec::new();
    let fed: Vec<String> = (0..targets).map(|i| format!("urn:a{i}.u")).collect();
    nodes.push(port("p:u", "Input", &wired_to(&fed)));
    for i in 0..targets {
        abs(&format!("urn:a{i}"), "", "", &mut nodes);
        blocks.push(format!("urn:a{i}"));
    }
    let listed: Vec<String> = blocks
        .iter()
        .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
        .collect();
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\", \"p\": \"urn:x:{}\" }}, \
         \"@graph\": [ {{ \"@id\": \"urn:top\", \"@type\": \"S231:Block\", \
         \"S231:containsBlock\": [ {} ], \"S231:hasInput\": [ {{ \"@id\": \"p:u\" }} ] }},\n{} ] }}",
        "k".repeat(path_bytes),
        listed.join(", "),
        nodes.join(",\n")
    )
    .into_bytes()
}

#[test]
fn a_snapshot_past_the_cap_is_refused_at_load_at_the_cost_of_its_records() {
    // 80 and then 160 inputs driven by a 1 MiB and by a 2 MiB path: snapshots of 80 to 320 MiB,
    // all refused at load. Each further input would add the path to the snapshot; to the
    // refusal it adds its records and its nodes, the same whatever the path's length.
    let requested = |path_bytes: usize, targets: usize| {
        let document = boundary_fanned_out(path_bytes, targets);
        let mut outcome = None;
        let info = measure(|| outcome = Some(Engine::new().load_cxf(&document)));
        let error = outcome.expect("ran").map(drop).expect_err("past the cap");
        let message = budget_refusal(&error);
        assert!(message.contains("durable state snapshot"), "{message}");
        i128::from(info.bytes_total)
    };
    let further = |path_bytes| requested(path_bytes, 160) - requested(path_bytes, 80);
    let (shorter, longer) = (further(1 << 20), further(2 << 20));
    assert!(
        (longer - shorter).abs() < 80 * 1024,
        "80 further inputs: {shorter} bytes under a 1 MiB path, {longer} under a 2 MiB one"
    );
}

#[test]
fn the_canonical_budget_is_checked_before_the_snapshot_is_counted() {
    // 70 wires from a 512 KiB name: an encoding of about 35 MiB and a snapshot of about 70 MiB,
    // past both. The canonical budget, which runs first, is the one refusal.
    let error = Engine::new()
        .load_cxf(&fanned_out(524_288, 70, 1))
        .map(drop)
        .expect_err("past both");
    let message = budget_refusal(&error);
    assert!(message.contains("canonical encoding"), "{message}");
}

#[test]
fn a_long_block_with_many_unwired_ports_is_refused_for_its_snapshot() {
    // A `RealScalarReplicator` whose 64 KiB IRI is written once, with 1,100 short-IRI outputs
    // that drive nothing: the canonical encoding names the block once, and the snapshot names it
    // in every key of every port, about 430 MB. The keys of the connectors' values pass the cap
    // on their own, which the count measures rather than stopping at.
    let outputs: Vec<String> = (1..=1_100).map(|i| format!("urn:r.y{i}")).collect();
    let listed: Vec<String> = outputs
        .iter()
        .map(|output| format!("{{ \"@id\": \"{output}\" }}"))
        .collect();
    let mut nodes = Vec::new();
    constant("urn:c", &wired_to(&["urn:r.u".to_owned()]), &mut nodes);
    nodes.push(format!(
        "{{ \"@id\": \"p:r\", \"@type\": \"{T}Routing.RealScalarReplicator\", \
         \"S231:hasParameter\": {{ \"@id\": \"p:r.nout\" }}, \"S231:hasInput\": \
         {{ \"@id\": \"urn:r.u\" }}, \"S231:hasOutput\": [ {} ] }}",
        listed.join(", ")
    ));
    nodes.push("{ \"@id\": \"p:r.nout\", \"S231:value\": 1100 }".to_owned());
    nodes.push(port("urn:r.u", "Input", ""));
    for output in &outputs {
        nodes.push(port(output, "Output", ""));
    }
    let document = document(
        Some(&format!("urn:x:{}", "k".repeat(65_536))),
        &["urn:c".to_owned(), "p:r".to_owned()],
        &nodes,
    );
    let error = Engine::new()
        .load_cxf(&document)
        .map(drop)
        .expect_err("past the cap");
    let message = budget_refusal(&error);
    assert!(message.contains("durable state snapshot"), "{message}");
    // The count runs past the cap and names the whole length, about 430 MB.
    let taken: usize = message
        .split("would take ")
        .nth(1)
        .and_then(|rest| rest.split(" bytes").next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_else(|| panic!("a length in bytes: {message}"));
    assert!(taken > 6 * 1_100 * 65_536, "{message}");
}
