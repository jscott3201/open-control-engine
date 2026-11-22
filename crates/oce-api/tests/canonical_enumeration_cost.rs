//! The cost of many enumeration values through the load (issue #762): resolving a value by its
//! canonical class path, or refusing one written under another path, is constant work per value,
//! so a document of thousands of enumeration values costs allocations in proportion to them.
//! Allocation counts, never wall-clock time.

use allocation_counter::measure;
use oce_api::Engine;

const SIMPLE_CONTROLLER: &str = "Buildings.Controls.OBC.CDL.Types.SimpleController";
const NS: &str = "http://example.org#";

/// A root with `n` parameters declared `SimpleController`, each holding `<path>.PD`, and one
/// `Logical.Sources.Constant` block wired to the root output.
fn document(n: usize, path: &str) -> String {
    let refs: Vec<String> = (0..n)
        .map(|i| format!(r#"{{"@id":"{NS}S.p{i}"}}"#))
        .collect();
    let nodes: Vec<String> = (0..n)
        .map(|i| {
            format!(
                r#"{{"@id":"{NS}S.p{i}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"{NS}{SIMPLE_CONTROLLER}"}},"S231:value":"{path}.PD"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[
{{"@id":"{NS}S","@type":"S231:Block","S231:hasParameter":[{}],"S231:containsBlock":{{"@id":"{NS}S.c"}},"S231:hasOutput":{{"@id":"{NS}S.y"}}}},
{},
{{"@id":"{NS}S.y","@type":"S231:BooleanOutput","S231:isOfDataType":{{"@id":"S231:Boolean"}}}},
{{"@id":"{NS}S.c","@type":"{NS}Buildings.Controls.OBC.CDL.Logical.Sources.Constant","S231:hasParameter":{{"@id":"{NS}S.c.k"}},"S231:hasOutput":{{"@id":"{NS}S.c.y"}}}},
{{"@id":"{NS}S.c.k","S231:value":true}},
{{"@id":"{NS}S.c.y","@type":"S231:BooleanOutput","S231:isOfDataType":{{"@id":"S231:Boolean"}},"S231:isConnectedTo":{{"@id":"{NS}S.y"}}}}
]}}"#,
        refs.join(","),
        nodes.join(",\n")
    )
}

fn load(text: &str) -> bool {
    Engine::new().load_cxf(text.as_bytes()).is_ok()
}

#[test]
fn many_enumeration_values_cost_in_proportion_to_their_number() {
    assert!(load(&document(4_000, SIMPLE_CONTROLLER)));
    assert!(!load(&document(4_000, "Foo.SimpleController")));
    for path in [
        SIMPLE_CONTROLLER,
        "Foo.SimpleController",
        "SimpleController",
    ] {
        let cost = |n: usize| {
            let text = document(n, path);
            measure(|| {
                load(&text);
            })
        };
        let (small, large) = (cost(1_000), cost(4_000));
        assert!(
            large.count_total <= 5 * small.count_total,
            "{path}: {} allocations for 1,000 values, {} for 4,000",
            small.count_total,
            large.count_total
        );
        assert!(
            large.bytes_total <= 5 * small.bytes_total,
            "{path}: {} bytes for 1,000 values, {} for 4,000",
            small.bytes_total,
            large.bytes_total
        );
    }
}
