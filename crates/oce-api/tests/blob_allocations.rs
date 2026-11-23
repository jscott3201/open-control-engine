//! The compiled-schedule decoder's allocation count, pinned exactly for every golden (GitHub #400).
//!
//! `oce_graph::blob` documents what `decode` allocates: what it returns, one default-parameter block
//! per class for its signature hash, and four scratch vectors.
//! This test turns that list into a formula over the decoded executable and requires the measured
//! allocation count to equal it for all 46 goldens, so an undocumented allocation fails here. Block
//! constructors allocate whatever they allocate on the host too; their share is measured by
//! constructing each block again with its decoded parameters. A generated pulse model at 1x, 2x
//! and 4x shows the event plan's share is a fixed count of buffers with linear bytes (issue #800).

// Shared helper, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_corpus;

use std::hint::black_box;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, golden};
use oce_graph::StateSlot;
use oce_graph::blob::{Decoded, decode};
use oce_model::{BlockId, Value};

fn some(n: usize) -> u64 {
    u64::from(n > 0)
}

/// The documented allocation count of `decode` for this executable.
fn documented(decoded: &Decoded<'_>) -> u64 {
    let model = &decoded.model;
    let blocks = model.blocks.len();
    let connectors = model.connectors.len();
    let mut classes: Vec<&str> = model.blocks.iter().map(|b| b.class_iri.as_ref()).collect();
    classes.sort_unstable();
    classes.dedup();
    let largest_params = model
        .blocks
        .iter()
        .map(|b| b.params.values.len())
        .max()
        .unwrap_or(0);
    // Gather scratch holds the declared inputs and a `w`, `s` pair per settled event input.
    let max_arity = model
        .blocks
        .iter()
        .zip(&decoded.blocks)
        .map(|(b, block)| b.inputs.len() + 2 * block.settled_event_inputs().len())
        .max()
        .unwrap_or(0);

    // Returned: the model arenas.
    let mut count = some(blocks) + some(connectors) + some(model.connections.len());
    // One shared Arc<str> per class.
    count += classes.len() as u64;
    // Per block: port and parameter vectors when non-empty, one Arc per name and String value.
    for block in &model.blocks {
        count += some(block.inputs.len()) + some(block.outputs.len());
        count += some(block.params.values.len()) + block.params.values.len() as u64;
        count += block
            .params
            .values
            .iter()
            .filter(|(_, v)| matches!(v, Value::String(_)))
            .count() as u64;
    }
    // The block vector, then each constructor (its Box included), measured by re-construction.
    count += some(blocks);
    for block in &model.blocks {
        let entry = oce_blocks::lookup(&block.class_iri).unwrap();
        count += measure(|| {
            black_box((entry.make)(&block.params));
        })
        .count_total;
    }
    // Schedule: order and driver_of.
    count += some(blocks) + some(connectors);
    // The event plan (issue #800), only when some block declares settled event inputs and some
    // block is a pulse or a sample: thirteen fixed buffers, each sized once.
    let plans = oce_graph::needs_plan(&decoded.blocks);
    if plans {
        let mut owned = vec![false; connectors];
        for block in &model.blocks {
            for out in &block.outputs {
                owned[out.0 as usize] = true;
            }
        }
        let reader_edges = model
            .blocks
            .iter()
            .flat_map(|b| &b.inputs)
            .filter(|i| owned[decoded.schedule.driver_of[i.0 as usize].0 as usize])
            .count();
        let capacity: usize = model
            .blocks
            .iter()
            .zip(&decoded.blocks)
            .map(|(b, block)| {
                let logic = if block.event_signal() == oce_blocks::EventSignal::Logic {
                    b.inputs.len()
                } else {
                    0
                };
                block.settled_event_inputs().len() + logic
            })
            .sum();
        // owner, signal, counts, fill, readers, tainted, queue, summary, steps, hidden, sources,
        // marked, stack.
        count += some(connectors) + some(blocks) + 2 + some(reader_edges) + 2 * some(blocks);
        count += 4 * some(blocks) + 2 * some(capacity);
    }
    // RunState: words (one reservation), slots (pushed one at a time), slot_of, values, scratch.
    count += some(decoded.state.words.len());
    let stateful = decoded.state.slots.len();
    count += measure(|| {
        let mut slots = Vec::new();
        for _ in 0..stateful {
            slots.push(StateSlot {
                block: BlockId(0),
                offset: 0,
                len: 0,
            });
        }
        black_box(slots);
    })
    .count_total;
    count += some(blocks) + some(connectors) + some(max_arity);
    // RunState: the shadow buffers `w`, `s` and the initial frame's `i`, and their scratch, with
    // the event plan.
    if plans {
        count += 3 * some(connectors) + some(max_arity);
    }
    // I/O tables: the two lists, one target vector per input; bounds are never Strings.
    count += some(decoded.inputs.len()) + decoded.inputs.len() as u64 + some(decoded.outputs.len());
    // Scratch: one default-parameter construction per class for its signature hash (freed at
    // once), measured by re-construction.
    for class in &classes {
        let entry = oce_blocks::lookup(class).unwrap();
        count += measure(|| {
            black_box((entry.make)(&oce_model::ParamTable::default()));
        })
        .count_total;
    }
    // Scratch: the class table, the parameter-name buffer, the per-connector table and the
    // schedule-position table.
    count += some(classes.len()) + some(largest_params) + some(connectors) + some(blocks);
    count
}

#[test]
fn decode_allocates_exactly_what_its_documentation_lists() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    for name in FIXTURES {
        let bytes = golden(name);
        let mut decoded = None;
        let measured = measure(|| {
            decoded = Some(decode(&bytes).unwrap());
        });
        let decoded = decoded.unwrap();
        assert_eq!(measured.count_total, documented(&decoded), "{name}");
        // Nothing the decoder returns copies the input: every path and payload borrows `bytes`.
        let range = bytes.as_ptr_range();
        for path in decoded
            .inputs
            .iter()
            .map(|i| i.path)
            .chain(decoded.outputs.iter().map(|o| o.path))
        {
            assert!(range.contains(&path.as_ptr()), "{name}: {path} is copied");
        }
    }
}

/// `Change(u)` through `n` `Not` blocks into `m` counters, each reset by its own `false` constant,
/// as a CXF document; the first counter's count is the model's output.
fn pulse_chain(n: usize, m: usize) -> Vec<u8> {
    use serde_json::{Value as J, json};
    const P: &str = "http://example.org#chain.";
    const CDL: &str = "http://example.org#Buildings.Controls.OBC.CDL.";
    let id = |s: &str| json!({ "@id": format!("{P}{s}") });
    let port = |s: &str, kind: &str, to: &[String]| {
        let mut node = json!({
            "@id": format!("{P}{s}"),
            "@type": format!("S231:{kind}"),
            "S231:isOfDataType": { "@id": format!("S231:{}", kind.trim_end_matches("Input").trim_end_matches("Output")) },
        });
        if !to.is_empty() {
            node["S231:isConnectedTo"] = J::Array(to.iter().map(|t| id(t)).collect());
        }
        node
    };
    let mut graph = Vec::new();
    let mut members = Vec::new();
    let mut block = |graph: &mut Vec<J>,
                     name: &str,
                     class: &str,
                     ins: &[&str],
                     outs: &[&str],
                     params: &[&str]| {
        members.push(id(name));
        let mut node = json!({
            "@id": format!("{P}{name}"),
            "@type": format!("{CDL}{class}"),
            "S231:label": name,
            "S231:hasInput": ins.iter().map(|i| id(&format!("{name}.{i}"))).collect::<Vec<_>>(),
            "S231:hasOutput": outs.iter().map(|o| id(&format!("{name}.{o}"))).collect::<Vec<_>>(),
        });
        if !params.is_empty() {
            node["S231:hasParameter"] =
                J::Array(params.iter().map(|p| id(&format!("{name}.{p}"))).collect());
        }
        graph.push(node);
    };
    block(&mut graph, "cha", "Logical.Change", &["u"], &["y"], &[]);
    graph.push(port("cha.u", "BooleanInput", &[]));
    let mut last = String::from("cha");
    for k in 0..n {
        let name = format!("n{k}");
        block(&mut graph, &name, "Logical.Not", &["u"], &["y"], &[]);
        graph.push(port(&format!("{name}.u"), "BooleanInput", &[]));
        graph.push(port(
            &format!("{last}.y"),
            "BooleanOutput",
            &[format!("{name}.u")],
        ));
        last = name;
    }
    let mut readers = Vec::new();
    for c in 0..m {
        let (cnt, fal) = (format!("c{c}"), format!("f{c}"));
        block(
            &mut graph,
            &cnt,
            "Integers.OnCounter",
            &["trigger", "reset"],
            &["y"],
            &["y_start"],
        );
        block(
            &mut graph,
            &fal,
            "Logical.Sources.Constant",
            &[],
            &["y"],
            &["k"],
        );
        graph.push(json!({ "@id": format!("{P}{cnt}.y_start"), "S231:value": { "@value": "0", "@type": "http://www.w3.org/2001/XMLSchema#integer" } }));
        graph.push(json!({ "@id": format!("{P}{fal}.k"), "S231:value": { "@value": "false", "@type": "http://www.w3.org/2001/XMLSchema#boolean" } }));
        graph.push(port(&format!("{cnt}.trigger"), "BooleanInput", &[]));
        graph.push(port(&format!("{cnt}.reset"), "BooleanInput", &[]));
        let to: Vec<String> = if c == 0 {
            vec![String::from("y")]
        } else {
            Vec::new()
        };
        graph.push(port(&format!("{cnt}.y"), "IntegerOutput", &to));
        graph.push(port(
            &format!("{fal}.y"),
            "BooleanOutput",
            &[format!("{cnt}.reset")],
        ));
        readers.push(format!("{cnt}.trigger"));
    }
    graph.push(port(&format!("{last}.y"), "BooleanOutput", &readers));
    let mut input = port("u", "BooleanInput", &[String::from("cha.u")]);
    input["S231:label"] = json!("chain.u");
    graph.push(input);
    graph.push(port("y", "IntegerOutput", &[]));
    graph.insert(
        0,
        json!({
            "@id": "http://example.org#chain",
            "@type": "S231:Block",
            "S231:label": "chain",
            "S231:containsBlock": members,
            "S231:hasInput": [id("u")],
            "S231:hasOutput": [id("y")],
        }),
    );
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" },
        "@graph": graph,
    });
    serde_json::to_vec(&doc).unwrap()
}

#[test]
fn the_event_plan_allocates_a_fixed_count_of_buffers_whose_bytes_grow_linearly() {
    // 1x, 2x, 4x: a 100-deep chain into 10 counters, scaled together, so blocks, connectors and
    // edges are all `a + b * scale`. The plan is thirteen buffers whatever the size, each sized
    // once from those counts, so its bytes are exactly affine in the scale; and `decode` still
    // allocates exactly what its documentation lists.
    let mut plan_bytes = Vec::new();
    for scale in [1usize, 2, 4] {
        let mut engine = oce_api::Engine::new();
        engine
            .load_cxf(&pulse_chain(100 * scale, 10 * scale))
            .expect("the generated chain loads");
        let bytes = engine.compile_blob().expect("compiles").bytes;
        let mut decoded = None;
        let measured = measure(|| {
            decoded = Some(decode(&bytes).unwrap());
        });
        let decoded = decoded.unwrap();
        assert_eq!(measured.count_total, documented(&decoded), "scale {scale}");
        assert!(!decoded.schedule.events.is_empty(), "the plan is active");
        let mut plan = None;
        let derive = measure(|| {
            plan = Some(oce_graph::EventPlan::derive(
                &decoded.model,
                &decoded.blocks,
                &decoded.schedule,
            ));
        });
        assert_eq!(plan.unwrap(), decoded.schedule.events, "scale {scale}");
        assert_eq!(derive.count_total, 13, "scale {scale}: thirteen buffers");
        plan_bytes.push(derive.bytes_total);
    }
    let (b1, b2, b4) = (plan_bytes[0], plan_bytes[1], plan_bytes[2]);
    assert!(b2 > b1, "{plan_bytes:?}");
    assert_eq!(b4 - b2, 2 * (b2 - b1), "linear: {plan_bytes:?}");
}
