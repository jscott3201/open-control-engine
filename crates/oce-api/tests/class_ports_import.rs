//! `class_ports` and `param_element_name` against hand-written CXF documents (GitHub #440).
//!
//! The G36 corpus exercises only vector connectors on a few classes. Each case here is a small
//! document written by hand in the upstream flattened spelling (`u1`, `y2_1`, `yIdx3`, `index`,
//! `table_2_3`) for a class group the corpus does not reach. The CXF import resolves it, and the
//! local names of the connectors it bound at each port index must equal both the hand-written list
//! and the names `class_ports` gives for the same parameters. Nothing here derives a connector
//! name from `class_ports` itself, so a wrong naming rule cannot agree with itself.

use oce_api::{Value, class_ports, param_element_name};

/// A parameter node of the hand-written document.
enum Param {
    Integer(&'static str, i64),
    Real(&'static str, f64),
    Boolean(&'static str, bool),
    /// A preserved array: label (`k[3]`), `sizeOfDimensions` (`(3)`), Real elements row-major.
    Array(&'static str, &'static str, Vec<f64>),
}

const BASE: &str = "http://example.org#doc";

fn real_literal(value: f64) -> String {
    format!(
        "{{ \"@value\": \"{value:?}\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}"
    )
}

/// One block of `class` with `params`, its inputs and outputs named exactly as given, every
/// input driven by a `Sources.Constant` of its kind (so single assignment holds without a
/// boundary connector replacing the port's own identity).
fn document(
    class: &str,
    params: &[Param],
    inputs: &[(&str, &str)],
    outputs: &[(&str, &str)],
) -> String {
    let mut nodes = Vec::new();
    let mut members = vec![format!("{{ \"@id\": \"{BASE}.blk\" }}")];
    let mut parameter_ids = Vec::new();
    for param in params {
        let (name, node) = match param {
            Param::Integer(name, v) => (
                *name,
                format!(
                    "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
                     \"S231:Integer\" }}, \"S231:value\": {v}"
                ),
            ),
            Param::Real(name, v) => (
                *name,
                format!(
                    "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
                     \"S231:Real\" }}, \"S231:value\": {}",
                    real_literal(*v)
                ),
            ),
            Param::Boolean(name, v) => (
                *name,
                format!(
                    "\"@type\": \"S231:Parameter\", \"S231:isOfDataType\": {{ \"@id\": \
                     \"S231:Boolean\" }}, \"S231:value\": {v}"
                ),
            ),
            Param::Array(label, size, elements) => {
                let rank = size.matches(',').count() + 1;
                let literals: Vec<String> = elements.iter().map(|v| real_literal(*v)).collect();
                (
                    label.split('[').next().unwrap(),
                    format!(
                        "\"S231:label\": \"{label}\", \"S231:isArray\": true, \
                         \"S231:numberDimensions\": {rank}, \"S231:sizeOfDimensions\": \
                         \"{size}\", \"S231:value\": [{}]",
                        literals.join(", ")
                    ),
                )
            }
        };
        parameter_ids.push(format!("{{ \"@id\": \"{BASE}.blk.{name}\" }}"));
        nodes.push(format!("{{ \"@id\": \"{BASE}.blk.{name}\", {node} }}"));
    }
    let mut input_ids = Vec::new();
    for (name, kind) in inputs {
        input_ids.push(format!("{{ \"@id\": \"{BASE}.blk.{name}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{BASE}.blk.{name}\", \"@type\": \"S231:{kind}Input\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }} }}"
        ));
        let (family, value) = match *kind {
            "Real" => ("Reals", "1.0".to_owned()),
            "Integer" => ("Integers", "1".to_owned()),
            _ => ("Logical", "true".to_owned()),
        };
        let driver = format!("{BASE}.drv_{name}");
        members.push(format!("{{ \"@id\": \"{driver}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{driver}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.{family}.Sources.Constant\", \
             \"S231:hasParameter\": [{{ \"@id\": \"{driver}.k\" }}], \
             \"S231:hasOutput\": [{{ \"@id\": \"{driver}.y\" }}] }}, \
             {{ \"@id\": \"{driver}.k\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }}, \"S231:value\": {value} }}, \
             {{ \"@id\": \"{driver}.y\", \"@type\": \"S231:{kind}Output\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }}, \
             \"S231:isConnectedTo\": {{ \"@id\": \"{BASE}.blk.{name}\" }} }}"
        ));
    }
    let mut output_ids = Vec::new();
    for (name, kind) in outputs {
        output_ids.push(format!("{{ \"@id\": \"{BASE}.blk.{name}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{BASE}.blk.{name}\", \"@type\": \"S231:{kind}Output\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }} }}"
        ));
    }
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231P#\" }}, \"@graph\": [\
         {{ \"@id\": \"{BASE}\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [{}] }}, \
         {{ \"@id\": \"{BASE}.blk\", \
            \"@type\": \"http://example.org#Buildings.Controls.OBC.{class}\", \
            \"S231:hasParameter\": [{}], \"S231:hasInput\": [{}], \"S231:hasOutput\": [{}] }}, \
         {} ] }}",
        members.join(", "),
        parameter_ids.join(", "),
        input_ids.join(", "),
        output_ids.join(", "),
        nodes.join(", ")
    )
}

/// The block under test as the import resolved it: its flattened parameters in import order and
/// the local names of the connectors bound at each input and output index.
struct Imported {
    params: Vec<(String, Value)>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn import(document: &str) -> Imported {
    let (model, _) = oce_cxf::import_cxf(document.as_bytes(), &oce_cxf::ResolveOptions::default())
        .unwrap_or_else(|error| panic!("import refused: {error:?}\n{document}"));
    let block = model
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some(&format!("{BASE}.blk")[..]))
        .expect("the block under test");
    let local = |ids: &[oce_model::ConnectorId]| -> Vec<String> {
        ids.iter()
            .map(|id| {
                let iri = model.connectors[id.0 as usize]
                    .iri
                    .as_deref()
                    .expect("authored");
                iri.strip_prefix(&format!("{BASE}.blk."))
                    .expect("own connector")
                    .to_owned()
            })
            .collect()
    };
    Imported {
        params: block
            .params
            .values
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect(),
        inputs: local(&block.inputs),
        outputs: local(&block.outputs),
    }
}

/// Import the hand-written document, then require the bound connectors, the hand-written lists
/// and `class_ports` on the imported parameters to agree, index by index.
fn check(class: &str, params: &[Param], inputs: &[(&str, &str)], outputs: &[(&str, &str)]) {
    let imported = import(&document(class, params, inputs, outputs));
    let written = |side: &[(&str, &str)]| -> Vec<String> {
        side.iter().map(|(name, _)| (*name).to_owned()).collect()
    };
    assert_eq!(
        imported.inputs,
        written(inputs),
        "{class}: import input order"
    );
    assert_eq!(
        imported.outputs,
        written(outputs),
        "{class}: import output order"
    );
    let values: Vec<(&str, Value)> = imported
        .params
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let ports = class_ports(class, &values).unwrap_or_else(|error| panic!("{class}: {error}"));
    let names = |side: &[oce_api::ClassPort]| -> Vec<String> {
        side.iter().map(|port| port.name.clone()).collect()
    };
    assert_eq!(names(&ports.inputs), imported.inputs, "{class}: inputs");
    assert_eq!(names(&ports.outputs), imported.outputs, "{class}: outputs");
}

const R: &str = "Real";
const I: &str = "Integer";
const B: &str = "Boolean";

#[test]
fn vector_replicators_name_their_output_matrix_row_major() {
    for (class, kind) in [
        ("CDL.Routing.RealVectorReplicator", R),
        ("CDL.Routing.IntegerVectorReplicator", I),
        ("CDL.Routing.BooleanVectorReplicator", B),
    ] {
        check(
            class,
            &[Param::Integer("nin", 2), Param::Integer("nout", 3)],
            &[("u1", kind), ("u2", kind)],
            &[
                ("y1_1", kind),
                ("y1_2", kind),
                ("y2_1", kind),
                ("y2_2", kind),
                ("y3_1", kind),
                ("y3_2", kind),
            ],
        );
    }
}

#[test]
fn matrix_reductions_name_their_input_matrix_and_reduced_vector() {
    let cells = [
        ("u1_1", R),
        ("u1_2", R),
        ("u1_3", R),
        ("u2_1", R),
        ("u2_2", R),
        ("u2_3", R),
    ];
    for (class, flag) in [
        ("CDL.Reals.MatrixMax", "rowMax"),
        ("CDL.Reals.MatrixMin", "rowMin"),
    ] {
        let shape = |by_row| {
            vec![
                Param::Integer("nRow", 2),
                Param::Integer("nCol", 3),
                Param::Boolean(flag, by_row),
            ]
        };
        check(class, &shape(true), &cells, &[("y1", R), ("y2", R)]);
        check(
            class,
            &shape(false),
            &cells,
            &[("y1", R), ("y2", R), ("y3", R)],
        );
    }
}

#[test]
fn sort_names_both_output_vectors() {
    check(
        "CDL.Reals.Sort",
        &[Param::Integer("nin", 3)],
        &[("u1", R), ("u2", R), ("u3", R)],
        &[
            ("y1", R),
            ("y2", R),
            ("y3", R),
            ("yIdx1", I),
            ("yIdx2", I),
            ("yIdx3", I),
        ],
    );
}

#[test]
fn extractors_lead_with_the_index_input() {
    for (class, kind) in [
        ("CDL.Routing.RealExtractor", R),
        ("CDL.Routing.IntegerExtractor", I),
        ("CDL.Routing.BooleanExtractor", B),
    ] {
        check(
            class,
            &[Param::Integer("nin", 3)],
            &[("index", I), ("u1", kind), ("u2", kind), ("u3", kind)],
            &[("y", kind)],
        );
    }
}

#[test]
fn vector_filters_and_extract_signals_name_both_vectors() {
    for (class, kind) in [
        ("CDL.Routing.RealVectorFilter", R),
        ("CDL.Routing.IntegerVectorFilter", I),
        ("CDL.Routing.BooleanVectorFilter", B),
    ] {
        check(
            class,
            &[
                Param::Integer("nin", 3),
                Param::Integer("nout", 2),
                Param::Boolean("msk_2", false),
            ],
            &[("u1", kind), ("u2", kind), ("u3", kind)],
            &[("y1", kind), ("y2", kind)],
        );
    }
    for (class, kind) in [
        ("CDL.Routing.IntegerExtractSignal", I),
        ("CDL.Routing.BooleanExtractSignal", B),
    ] {
        check(
            class,
            &[
                Param::Integer("nin", 2),
                Param::Integer("nout", 3),
                Param::Integer("extract_1", 2),
                Param::Integer("extract_3", 1),
            ],
            &[("u1", kind), ("u2", kind)],
            &[("y1", kind), ("y2", kind), ("y3", kind)],
        );
    }
}

#[test]
fn integer_sums_and_logical_reductions_name_the_input_vector() {
    check(
        "CDL.Integers.MultiSum",
        &[Param::Integer("nin", 2)],
        &[("u1", I), ("u2", I)],
        &[("y", I)],
    );
    check(
        "CDL.Logical.MultiAnd",
        &[Param::Integer("nin", 3)],
        &[("u1", B), ("u2", B), ("u3", B)],
        &[("y", B)],
    );
    check(
        "CDL.Routing.IntegerScalarReplicator",
        &[Param::Integer("nout", 2)],
        &[("u", I)],
        &[("y1", I), ("y2", I)],
    );
}

#[test]
fn time_tables_name_one_output_per_value_column() {
    // A 2 x 3 table: a time column and two value columns, so outputs `y1`, `y2`.
    let table = |values: [f64; 4]| {
        vec![
            Param::Real("table_1_1", 0.0),
            Param::Real("table_1_2", values[0]),
            Param::Real("table_1_3", values[1]),
            Param::Real("table_2_1", 1.0),
            Param::Real("table_2_2", values[2]),
            Param::Real("table_2_3", values[3]),
        ]
    };
    check(
        "CDL.Reals.Sources.TimeTable",
        &table([0.5, 1.5, 2.5, 3.5]),
        &[],
        &[("y1", R), ("y2", R)],
    );
    for (class, kind, values) in [
        ("CDL.Integers.Sources.TimeTable", I, [1.0, 2.0, 3.0, 4.0]),
        ("CDL.Logical.Sources.TimeTable", B, [0.0, 1.0, 1.0, 0.0]),
    ] {
        let mut params = table(values);
        params.push(Param::Real("period", 10.0));
        check(class, &params, &[], &[("y1", kind), ("y2", kind)]);
    }
}

#[test]
fn preserved_arrays_import_under_the_expanded_template_names() {
    // The connectors are written by hand; the parameter names are minted by the import from the
    // preserved arrays and must equal the catalog templates' expansion, row-major.
    let k = import(&document(
        "CDL.Reals.MultiSum",
        &[
            Param::Integer("nin", 3),
            Param::Array("k[3]", "(3)", vec![0.5, 1.5, 2.5]),
        ],
        &[("u1", R), ("u2", R), ("u3", R)],
        &[("y", R)],
    ));
    let minted: Vec<&str> = k
        .params
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| *name != "nin")
        .collect();
    let spelled: Vec<String> = (1..=3)
        .map(|i| param_element_name("k_<i>", &[i]).unwrap())
        .collect();
    assert_eq!(minted, spelled);

    let gain = import(&document(
        "CDL.Reals.MatrixGain",
        &[
            Param::Integer("nout", 2),
            Param::Integer("nin", 3),
            Param::Array("K[2,3]", "(2, 3)", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ],
        &[("u1", R), ("u2", R), ("u3", R)],
        &[("y1", R), ("y2", R)],
    ));
    let minted: Vec<&str> = gain
        .params
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| name.starts_with("K_"))
        .collect();
    let mut spelled = Vec::new();
    for row in 1..=2 {
        for col in 1..=3 {
            spelled.push(param_element_name("K_<row>_<col>", &[row, col]).unwrap());
        }
    }
    assert_eq!(minted, spelled, "row-major, last index fastest");
    // And the hand-written connectors are the ones `class_ports` names for those parameters.
    check(
        "CDL.Reals.MatrixGain",
        &[
            Param::Integer("nout", 2),
            Param::Integer("nin", 3),
            Param::Array("K[2,3]", "(2, 3)", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ],
        &[("u1", R), ("u2", R), ("u3", R)],
        &[("y1", R), ("y2", R)],
    );
}
