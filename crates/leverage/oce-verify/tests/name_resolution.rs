//! Name resolution at bind through the index, end to end (issue #571). The reviewer's shape of
//! GitHub #570, scaled to the vector file's 4 MiB bound: suffix-named parameter keys, 256 to a
//! scenario, against a subject of 4,000 `Reals.Hysteresis` blocks (8,000 parameters, each block's
//! two named alike). Every key resolves, so the run reaches the budget step and is refused at the
//! load-byte budget.
//!
//! The scaling is pinned by `oce-replay`'s `path_index` unit tests, which count the comparisons
//! the index makes; this test asserts no time (`TESTING.md`). The before-and-after timings of this
//! shape are recorded in the work-budget note.

mod support;

/// `blocks` hysteresis blocks all fed from `top.u`, block 0 driving `top.y`.
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

/// `scenarios` scenarios, each editing 256 parameters by suffix (`h<i>.uLow`, its current
/// value), the blocks taken in turn across the bank.
fn suffix_keyed_set(scenarios: usize, blocks: usize) -> String {
    let mut text = String::from(
        r#"{"schema":"oce/vectors/v1","clock":{"step_s":60,"horizon_s":60},"scenarios":["#,
    );
    for index in 0..scenarios {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&format!(r#"{{"name":"s{index}","params":{{"#));
        for key in 0..256 {
            if key > 0 {
                text.push(',');
            }
            let block = (index * 256 + key) % blocks;
            text.push_str(&format!(r#""h{block}.uLow":1.0"#));
        }
        text.push_str(r#"},"inputs":{"u":0.0},"expect":[]}"#);
    }
    text.push_str("]}");
    text
}

#[test]
fn suffix_keys_against_thousands_of_parameters_all_resolve_before_the_budget_step() {
    let subject = bank(4_000);
    // 800 scenarios of 256 keys: 204,800 suffix keys in a set just under the 4 MiB bound.
    let set = suffix_keyed_set(800, 4_000);
    assert!(set.len() < 4 * 1024 * 1024, "{}", set.len());
    // A key that did not resolve would be refused at its name, before the budget step.
    let error = support::refused(&set, &subject);
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-load-bytes", "scenarios"),
        "{error}"
    );
}
