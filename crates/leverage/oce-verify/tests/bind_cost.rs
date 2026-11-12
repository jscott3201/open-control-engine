//! What binding holds before the work budgets, in allocations, never in wall-clock time (issue
//! #571): the set's own `params` are held once, not copied into every scenario, so a set of 4,096
//! scenarios under 256 set-level edits binds in memory linear in its documents.
//!
//! Measured with `allocation-counter` (counters local to the measuring thread). The run is refused
//! at the load-byte budget, lowered to one byte, so what is measured is the binding up to the
//! budget step: the subject's load, every name resolved and every value typed.

mod support;

use oce_verify::{VectorInputs, VerifyLimits, VerifyOptions, run_vectors};

use support::load;

/// `blocks` hysteresis blocks all fed from `top.u`, block 0 driving `top.y`: two parameters a
/// block, every block's named alike.
fn bank(blocks: usize) -> Vec<u8> {
    let r = "urn:verify:bank#top";
    let mut nodes = Vec::with_capacity(blocks * 5 + 3);
    let members: Vec<String> = (0..blocks)
        .map(|i| format!(r#"{{"@id":"{r}.h{i}"}}"#))
        .collect();
    let targets: Vec<String> = (0..blocks)
        .map(|i| format!(r#"{{"@id":"{r}.h{i}.u"}}"#))
        .collect();
    nodes.push(format!(
        r#"{{"@id":"{r}","@type":"S231:Block","S231:containsBlock":[{}],"S231:hasInput":[{{"@id":"{r}.u"}}],"S231:hasOutput":[{{"@id":"{r}.y"}}]}}"#,
        members.join(",")
    ));
    for i in 0..blocks {
        nodes.push(format!(
            r#"{{"@id":"{r}.h{i}","@type":"CDL.Reals.Hysteresis","S231:hasParameter":[{{"@id":"{r}.h{i}.uLow"}},{{"@id":"{r}.h{i}.uHigh"}}],"S231:hasInput":{{"@id":"{r}.h{i}.u"}},"S231:hasOutput":{{"@id":"{r}.h{i}.y"}}}}"#
        ));
        for (name, value) in [("uLow", "1.0"), ("uHigh", "2.0")] {
            nodes.push(format!(
                r#"{{"@id":"{r}.h{i}.{name}","S231:value":{{"@value":"{value}","@type":"http://www.w3.org/2001/XMLSchema#double"}}}}"#
            ));
        }
        nodes.push(format!(
            r#"{{"@id":"{r}.h{i}.u","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}}}}"#
        ));
        let to = if i == 0 {
            format!(r#","S231:isConnectedTo":{{"@id":"{r}.y"}}"#)
        } else {
            String::new()
        };
        nodes.push(format!(
            r#"{{"@id":"{r}.h{i}.y","@type":"S231:BooleanOutput","S231:isOfDataType":{{"@id":"S231:Boolean"}}{to}}}"#
        ));
    }
    nodes.push(format!(
        r#"{{"@id":"{r}.u","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":[{}]}}"#,
        targets.join(",")
    ));
    nodes.push(format!(
        r#"{{"@id":"{r}.y","@type":"S231:BooleanOutput","S231:isOfDataType":{{"@id":"S231:Boolean"}}}}"#
    ));
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        nodes.join(",")
    )
    .into_bytes()
}

/// `scenarios` scenarios with no `params` of their own, under a set-level map editing every
/// parameter of a bank of `blocks` blocks by suffix, each to its current value.
fn shared_edits_set(scenarios: usize, blocks: usize) -> String {
    let params: Vec<String> = (0..blocks)
        .flat_map(|i| {
            [
                format!(r#""h{i}.uLow":1.0"#),
                format!(r#""h{i}.uHigh":2.0"#),
            ]
        })
        .collect();
    let all: Vec<String> = (0..scenarios)
        .map(|index| format!(r#"{{"name":"s{index}","inputs":{{"u":0.0}},"expect":[]}}"#))
        .collect();
    format!(
        r#"{{"schema":"oce/vectors/v1","clock":{{"step_s":60,"horizon_s":60}},"params":{{{}}},"scenarios":[{}]}}"#,
        params.join(","),
        all.join(",")
    )
}

#[test]
fn set_level_edits_are_held_once_not_once_per_scenario() {
    let subject = bank(128);
    let options = VerifyOptions {
        limits: VerifyLimits {
            max_total_load_bytes: 1,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let measured: Vec<(u64, u64)> = [1, 4_096]
        .into_iter()
        .map(|scenarios| {
            let set = load(&shared_edits_set(scenarios, 128)).expect("the set loads");
            let mut code = "";
            let info = allocation_counter::measure(|| {
                code = run_vectors(&set, &VectorInputs::new(&subject), &options)
                    .expect_err("refused at the load-byte budget")
                    .code();
            });
            assert_eq!(code, "too-many-load-bytes");
            (info.bytes_max, info.count_total)
        })
        .collect();
    let [(one_peak, one_count), (many_peak, many_count)] = measured[..] else {
        unreachable!("two shapes measured")
    };
    // 4,095 more scenarios under 256 shared edits add at most 256 bytes held and 16 allocations
    // each [measured: no byte to the peak, which is the subject's load, and 8 allocations]. A copy
    // of the set's edits into every scenario held 72,811,014 bytes in 1,221,928 allocations.
    assert!(
        many_peak <= one_peak + 4_095 * 256,
        "{many_peak} bytes held against {one_peak}"
    );
    assert!(
        many_count <= one_count + 4_095 * 16,
        "{many_count} allocations against {one_count}"
    );
}
