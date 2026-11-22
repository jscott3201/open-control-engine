//! A parameter node many owners list loads exactly as the same document with a node of its own
//! for each owner (issue #749): the same outcome, content id, blob bytes, warnings and
//! diagnostics, once the separate nodes' ids are renamed to the shared one's.
//!
//! Each owner of a shared node now takes the document's one grounding of its value when that
//! grounding looked up no name, charged what grounding it again would spend. These cases hold
//! that to what each owner grounding its own copy gives, through the facade, with parameters the
//! catalog declares: `k` of `Reals.Sources.Constant` (Real) and of `Integers.Sources.Constant`
//! (Integer) shared by leaves of both classes, and a composite's declaration typed Real or
//! Integer, shared by composites with and without a conditional child, that each composite's
//! leaf reads. The separate nodes are named so that their ids sort where the shared one does.

use std::fmt::Write as _;

use oce_api::Engine;

/// The values every case binds: an Integer on a Real, an Integer past 2^53, a non-finite
/// expression, a string, a Boolean, two typed literals, an enumeration literal, an undefined
/// name, an array on a scalar, an overflowing product, a parse error, and a Real and an Integer
/// expression.
const VALUES: [&str; 14] = [
    "2",
    "9007199254740993",
    "\"1/0\"",
    "\"\\\"s\\\"\"",
    "true",
    "{ \"@value\": \"2.5\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }",
    "{ \"@value\": \"3\", \"@type\": \"http://www.w3.org/2001/XMLSchema#integer\" }",
    "\"Buildings.Controls.OBC.CDL.Types.SimpleController.PI\"",
    "\"nope\"",
    "\"{1, 2}\"",
    "\"1e308*10\"",
    "\"1 +\"",
    "\"2.5 * 2\"",
    "\"2 + 3\"",
];

/// The classes the leaves alternate between.
const CLASSES: [&str; 2] = [
    "Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
    "Buildings.Controls.OBC.CDL.Integers.Sources.Constant",
];

/// A root over `leaves` constants alternating between [`CLASSES`], leaf `i` listing the `k`
/// node `k(i)`; `nodes` are the `k` nodes to declare, each bound to `value`.
fn leaves(leaves: usize, k: impl Fn(usize) -> String, nodes: &[String], value: &str) -> String {
    let members: Vec<String> = (0..leaves)
        .map(|i| format!("{{ \"@id\": \"urn:c{i}\" }}"))
        .collect();
    let mut graph = format!(
        "{{ \"@id\": \"urn:r\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [ {} ] }}",
        members.join(", ")
    );
    for i in 0..leaves {
        let (class, output) = if i % 2 == 0 {
            (CLASSES[0], "Real")
        } else {
            (CLASSES[1], "Integer")
        };
        let _ = write!(
            graph,
            ",\n{{ \"@id\": \"urn:c{i}\", \"@type\": \"http://example.org#{class}\", \
             \"S231:hasParameter\": {{ \"@id\": \"{}\" }}, \
             \"S231:hasOutput\": {{ \"@id\": \"urn:c{i}.y\" }} }},\n\
             {{ \"@id\": \"urn:c{i}.y\", \"@type\": \"S231:{output}Output\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{output}\" }} }}",
            k(i)
        );
    }
    for id in nodes {
        let _ = write!(graph, ",\n{{ \"@id\": \"{id}\", \"S231:value\": {value} }}");
    }
    document(&graph)
}

/// A root over `composites` composites `urn:p<j>`, each declaring the node `x(j)` (typed
/// `kind`) and holding a `Reals.Sources.Constant` whose `k` reads `x`; with `conditional` each
/// also declares a `flag` and holds a `Logical.Not` conditional on it. `nodes` are the `x` nodes
/// to declare, each bound to `value`.
fn composites(
    composites: usize,
    x: impl Fn(usize) -> String,
    nodes: &[String],
    kind: &str,
    conditional: bool,
    value: &str,
) -> String {
    let members: Vec<String> = (0..composites)
        .map(|j| format!("{{ \"@id\": \"urn:p{j}\" }}"))
        .collect();
    let mut graph = format!(
        "{{ \"@id\": \"urn:r\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [ {} ] }}",
        members.join(", ")
    );
    for j in 0..composites {
        let (flag, cond, cond_node) = if conditional {
            (
                format!(", {{ \"@id\": \"urn:p{j}.flag\" }}"),
                format!(", {{ \"@id\": \"urn:p{j}.cond\" }}"),
                format!(
                    ",\n{{ \"@id\": \"urn:p{j}.flag\", \"@type\": \"S231:Parameter\", \
                     \"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }}, \"S231:value\": false }},\n\
                     {{ \"@id\": \"urn:p{j}.cond\", \
                     \"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Logical.Not\", \
                     \"S231:isConditionalComponent\": true, \"S231:conditionalExpression\": \"flag\" }}"
                ),
            )
        } else {
            (String::new(), String::new(), String::new())
        };
        let _ = write!(
            graph,
            ",\n{{ \"@id\": \"urn:p{j}\", \"@type\": \"http://example.org#Vendor.Sub\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"{}\" }}{flag} ], \
             \"S231:containsBlock\": [ {{ \"@id\": \"urn:p{j}.con\" }}{cond} ] }}{cond_node},\n\
             {{ \"@id\": \"urn:p{j}.con\", \
             \"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": {{ \"@id\": \"urn:p{j}.con.k\" }}, \
             \"S231:hasOutput\": {{ \"@id\": \"urn:p{j}.con.y\" }} }},\n\
             {{ \"@id\": \"urn:p{j}.con.k\", \"S231:value\": \"x\" }},\n\
             {{ \"@id\": \"urn:p{j}.con.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}",
            x(j)
        );
    }
    for id in nodes {
        let _ = write!(
            graph,
            ",\n{{ \"@id\": \"{id}\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }}, \"S231:value\": {value} }}"
        );
    }
    document(&graph)
}

/// A document of the `@graph` members `graph`.
fn document(graph: &str) -> String {
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [\n{graph}\n] }}"
    )
}

/// Everything a load of `text` gives a host: the outcome, content id, blob bytes, warnings and
/// diagnostics, with every separate id `<stem><nn>.<name>` renamed to the shared
/// `<stem>.<name>`.
fn record(text: &str, stem: &str, name: &str, owners: usize) -> String {
    let mut engine = Engine::new();
    let mut out = String::new();
    match engine.load_cxf(text.as_bytes()) {
        Ok((report, _)) => {
            let _ = writeln!(out, "loaded {} blocks", report.block_count);
            let _ = writeln!(
                out,
                "id {:?}",
                engine.content_id().map(|c| c.as_str().to_owned())
            );
            match engine.compile_blob() {
                Ok(blob) => {
                    let _ = writeln!(out, "blob {:?}", blob.bytes);
                }
                Err(error) => {
                    let _ = writeln!(out, "blob refused {error}");
                }
            }
            let _ = writeln!(out, "warnings {:?}", report.warnings);
        }
        Err(error) => {
            let _ = writeln!(out, "refused {error}");
            for diagnostic in error.all_diagnostics() {
                let _ = writeln!(out, "{diagnostic:?}");
            }
        }
    }
    (0..owners).fold(out, |out, i| {
        out.replace(&format!("{stem}{i:02}.{name}"), &format!("{stem}.{name}"))
    })
}

#[test]
fn leaves_of_two_classes_sharing_k_load_as_leaves_with_a_k_of_their_own() {
    for count in [2, 5] {
        for value in VALUES {
            let shared = leaves(
                count,
                |_| "urn:s.k".to_owned(),
                &["urn:s.k".to_owned()],
                value,
            );
            let own: Vec<String> = (0..count).map(|i| format!("urn:s{i:02}.k")).collect();
            let separate = leaves(count, |i| own[i].clone(), &own, value);
            assert_eq!(
                record(&shared, "urn:s", "k", count),
                record(&separate, "urn:s", "k", count),
                "{count} leaves, k = {value}"
            );
        }
    }
}

#[test]
fn composites_sharing_a_declaration_load_as_composites_with_one_of_their_own() {
    for kind in ["Real", "Integer"] {
        for conditional in [false, true] {
            for value in VALUES {
                let count = 3;
                let shared = composites(
                    count,
                    |_| "urn:d.x".to_owned(),
                    &["urn:d.x".to_owned()],
                    kind,
                    conditional,
                    value,
                );
                let own: Vec<String> = (0..count).map(|j| format!("urn:d{j:02}.x")).collect();
                let separate =
                    composites(count, |j| own[j].clone(), &own, kind, conditional, value);
                assert_eq!(
                    record(&shared, "urn:d", "x", count),
                    record(&separate, "urn:d", "x", count),
                    "{kind} x = {value}, conditional child {conditional}"
                );
            }
        }
    }
}

#[test]
fn the_cases_load_and_refuse_as_expected() {
    // A control that the cases above exercise both outcomes: `2 + 3` loads for every shape (a
    // Real `k` holds it as 5.0), and `1/0` refuses.
    let loads = leaves(
        2,
        |_| "urn:s.k".to_owned(),
        &["urn:s.k".to_owned()],
        "\"2 + 3\"",
    );
    assert!(record(&loads, "urn:s", "k", 2).starts_with("loaded"));
    let refused = leaves(
        2,
        |_| "urn:s.k".to_owned(),
        &["urn:s.k".to_owned()],
        "\"1/0\"",
    );
    assert!(record(&refused, "urn:s", "k", 2).starts_with("refused"));
    let loads = composites(
        3,
        |_| "urn:d.x".to_owned(),
        &["urn:d.x".to_owned()],
        "Real",
        true,
        "\"2 + 3\"",
    );
    assert!(
        record(&loads, "urn:d", "x", 3).starts_with("loaded"),
        "{}",
        record(&loads, "urn:d", "x", 3)
    );
}
