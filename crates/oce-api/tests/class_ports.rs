//! `class_ports`, `class_metadata` and `param_element_name` through the facade (GitHub #440).
//!
//! The port and parameter names are cross-checked against the CXF import itself, not against a
//! copy of the naming rule: every block of every checked-in G36 document is loaded, and the names
//! `class_ports` gives for its loaded parameters must equal the local names of the connectors the
//! import bound at each port index; and preserved array parameters written in CXF array form must
//! load under exactly the names `param_element_name` spells. Class descriptions are held to the
//! vendored Modelica sources. The per-class values are pinned by `oce-blocks`'
//! `tests/fixtures/class_ports.golden.txt`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use oce_api::{
    CatalogDefault, CatalogPortKind, CatalogPortNaming, CatalogRule, CatalogValueKind,
    ClassPortsError, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER, Value,
    catalog, class_metadata, class_ports, param_element_name,
};

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Every checked-in CXF document a host-visible topology can be read from: the G36 corpus and the
/// facade's own extract-signal loop.
fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = workspace_path("crates/oce-cxf/tests/fixtures/g36");
    let mut documents: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("g36 fixtures")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).expect("read fixture"))
        })
        .collect();
    documents.sort();
    let extract =
        workspace_path("crates/oce-api/src/tests/fixtures/extract_signal_block_loop.jsonld");
    documents.push((
        "extract_signal_block_loop.jsonld".to_owned(),
        std::fs::read(extract).expect("read extract fixture"),
    ));
    documents
}

fn borrowed(params: &[(String, Value)]) -> Vec<(&str, Value)> {
    params
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect()
}

#[test]
fn port_names_equal_the_connectors_the_import_bound_across_the_corpus() {
    let documents = corpus();
    assert!(documents.len() >= 47, "{} documents", documents.len());
    let mut compared = 0usize;
    let mut parameter_shaped: BTreeSet<String> = BTreeSet::new();
    for (name, bytes) in &documents {
        let (model, _) = oce_cxf::import_cxf(bytes, &oce_cxf::ResolveOptions::default())
            .unwrap_or_else(|error| panic!("{name}: import: {error:?}"));
        for block in &model.blocks {
            if block.class_iri.starts_with("urn:oce:lowering#") {
                continue;
            }
            let params: Vec<(&str, Value)> = block
                .params
                .values
                .iter()
                .map(|(name, value)| (name.as_ref(), value.clone()))
                .collect();
            let instance = block.instance_iri.as_deref().unwrap_or_default();
            let ports = class_ports(&block.class_iri, &params)
                .unwrap_or_else(|error| panic!("{name}: {instance}: {error}"));
            assert_eq!(ports.inputs.len(), block.inputs.len(), "{name}: {instance}");
            assert_eq!(
                ports.outputs.len(),
                block.outputs.len(),
                "{name}: {instance}"
            );
            let prefix = format!("{instance}.");
            let sides = [
                (&ports.inputs, &block.inputs),
                (&ports.outputs, &block.outputs),
            ];
            for (named, bound) in sides {
                for (port, id) in named.iter().zip(bound) {
                    let connector = &model.connectors[id.0 as usize];
                    // A port whose own node was elided into a boundary connector carries the
                    // boundary's identity instead; only the block's own connectors name a port.
                    let Some(local) = connector
                        .iri
                        .as_deref()
                        .and_then(|iri| iri.strip_prefix(&prefix))
                    else {
                        continue;
                    };
                    assert_eq!(port.name, local, "{name}: {instance}");
                    compared += 1;
                    let naming = catalog()
                        .iter()
                        .find(|entry| entry.class_path == block.class_iri.as_ref())
                        .expect("registered")
                        .naming;
                    if naming != CatalogPortNaming::Named {
                        parameter_shaped.insert(block.class_iri.to_string());
                    }
                }
            }
        }
    }
    assert!(compared > 1000, "only {compared} ports compared");
    // The corpus exercises flattened vector connectors on both sides.
    for class in [
        "CDL.Reals.MultiSum",
        "CDL.Reals.MultiMax",
        "CDL.Logical.MultiOr",
        "CDL.Routing.BooleanScalarReplicator",
        "CDL.Routing.RealExtractSignal",
    ] {
        assert!(
            parameter_shaped.contains(class),
            "{class} not compared: {parameter_shaped:?}"
        );
    }
}

/// A one-block CXF document: `class` with `scalars` as Integer parameters, `arrays` as preserved
/// array parameters `(label, sizeOfDimensions, element literals in row-major order)`, and the
/// ports `class_ports` names for the same flattened parameters, as connector nodes.
fn array_document(
    class: &str,
    scalars: &[(&str, i64)],
    arrays: &[(&str, &str, Vec<f64>)],
    ports: &oce_api::ClassPorts,
) -> String {
    let base = "http://example.org#arr";
    let mut parameter_ids = Vec::new();
    let mut nodes = Vec::new();
    for (name, value) in scalars {
        parameter_ids.push(format!("{{ \"@id\": \"{base}.blk.{name}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{base}.blk.{name}\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Integer\" }}, \"S231:value\": {value} }}"
        ));
    }
    for (label, size, elements) in arrays {
        let name = label.split('[').next().unwrap();
        let rank = size.matches(',').count() + 1;
        let literals: Vec<String> = elements
            .iter()
            .map(|value| {
                format!(
                    "{{ \"@value\": \"{value:?}\", \
                     \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}"
                )
            })
            .collect();
        parameter_ids.push(format!("{{ \"@id\": \"{base}.blk.{name}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{base}.blk.{name}\", \"S231:label\": \"{label}\", \
             \"S231:isArray\": true, \"S231:numberDimensions\": {rank}, \
             \"S231:sizeOfDimensions\": \"{size}\", \"S231:value\": [{}] }}",
            literals.join(", ")
        ));
    }
    // Every input is driven by its own `Sources.Constant`, so the import's single-assignment
    // check passes without a boundary connector replacing the port's own identity.
    let mut port_ids = [Vec::new(), Vec::new()];
    let mut drivers = Vec::new();
    for (side, (named, direction)) in [(&ports.inputs, "Input"), (&ports.outputs, "Output")]
        .into_iter()
        .enumerate()
    {
        for port in named {
            assert_eq!(
                port.kind,
                CatalogPortKind::Real,
                "the driver blocks are Real"
            );
            port_ids[side].push(format!("{{ \"@id\": \"{base}.blk.{}\" }}", port.name));
            nodes.push(format!(
                "{{ \"@id\": \"{base}.blk.{}\", \"@type\": \"S231:Real{direction}\", \
                 \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}",
                port.name
            ));
            if direction == "Input" {
                let driver = format!("{base}.drv_{}", port.name);
                drivers.push(format!("{{ \"@id\": \"{driver}\" }}"));
                nodes.push(format!(
                    "{{ \"@id\": \"{driver}\", \
                     \"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
                     \"S231:hasParameter\": [{{ \"@id\": \"{driver}.k\" }}], \
                     \"S231:hasOutput\": [{{ \"@id\": \"{driver}.y\" }}] }}, \
                     {{ \"@id\": \"{driver}.k\", \"@type\": \"S231:Parameter\", \
                     \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}, \"S231:value\": 1.0 }}, \
                     {{ \"@id\": \"{driver}.y\", \"@type\": \"S231:RealOutput\", \
                     \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}, \
                     \"S231:isConnectedTo\": {{ \"@id\": \"{base}.blk.{}\" }} }}",
                    port.name
                ));
            }
        }
    }
    drivers.push(format!("{{ \"@id\": \"{base}.blk\" }}"));
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231P#\" }}, \"@graph\": [\
         {{ \"@id\": \"{base}\", \"@type\": \"S231:Block\", \
            \"S231:containsBlock\": [{}] }},\
         {{ \"@id\": \"{base}.blk\", \
            \"@type\": \"http://example.org#Buildings.Controls.OBC.{class}\", \
            \"S231:hasParameter\": [{}], \"S231:hasInput\": [{}], \"S231:hasOutput\": [{}] }}, \
         {} ] }}",
        drivers.join(", "),
        parameter_ids.join(", "),
        port_ids[0].join(", "),
        port_ids[1].join(", "),
        nodes.join(", ")
    )
}

/// Resolve `document` with the CXF import and return the block under test's flattened parameter
/// names in the import's order, and the local names of the connectors bound at each port index.
fn imported_names(document: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let (model, _) = oce_cxf::import_cxf(document.as_bytes(), &oce_cxf::ResolveOptions::default())
        .unwrap_or_else(|error| panic!("import refused: {error:?}\n{document}"));
    let block = model
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some("http://example.org#arr.blk"))
        .expect("the block under test");
    let local = |ids: &[oce_model::ConnectorId]| -> Vec<String> {
        ids.iter()
            .map(|id| {
                let iri = model.connectors[id.0 as usize]
                    .iri
                    .as_deref()
                    .expect("authored");
                iri.rsplit('.').next().unwrap().to_owned()
            })
            .collect()
    };
    (
        block
            .params
            .values
            .iter()
            .map(|(name, _)| name.to_string())
            .collect(),
        local(&block.inputs),
        local(&block.outputs),
    )
}

#[test]
fn param_element_names_equal_the_names_the_import_mints_for_preserved_arrays() {
    // (class, scalar widths, preserved arrays, the catalog template and its index space)
    struct Case {
        class: &'static str,
        scalars: Vec<(&'static str, i64)>,
        array: (&'static str, &'static str, Vec<f64>),
        template: &'static str,
        indices: Vec<Vec<usize>>,
    }
    let cases = [
        Case {
            class: "CDL.Reals.MultiSum",
            scalars: vec![("nin", 3)],
            array: ("k[3]", "(3)", vec![0.5, 1.5, 2.5]),
            template: "k_<i>",
            indices: (1..=3).map(|i| vec![i]).collect(),
        },
        Case {
            class: "CDL.Reals.MatrixGain",
            scalars: vec![("nout", 2), ("nin", 3)],
            array: ("K[2,3]", "(2, 3)", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            template: "K_<row>_<col>",
            indices: (1..=2)
                .flat_map(|r| (1..=3).map(move |c| vec![r, c]))
                .collect(),
        },
    ];
    for case in cases {
        let spelled: Vec<String> = case
            .indices
            .iter()
            .map(|indices| param_element_name(case.template, indices).unwrap())
            .collect();
        let mut params: Vec<(&str, Value)> = case
            .scalars
            .iter()
            .map(|(name, value)| (*name, Value::Integer(*value)))
            .collect();
        params.extend(
            spelled
                .iter()
                .zip(&case.array.2)
                .map(|(name, v)| (name.as_str(), Value::Real(*v))),
        );
        let ports = class_ports(case.class, &params).expect("valid");
        let document = array_document(
            case.class,
            &case.scalars,
            std::slice::from_ref(&case.array),
            &ports,
        );
        let (parameters, inputs, outputs) = imported_names(&document);
        let flattened: Vec<String> = parameters
            .into_iter()
            .filter(|name| !case.scalars.iter().any(|(scalar, _)| scalar == name))
            .collect();
        // The import mints exactly the spelled names, row-major, last index fastest.
        assert_eq!(flattened, spelled, "{}", case.class);
        // And it binds the named ports at the indices `class_ports` gives them.
        let names = |side: &[oce_api::ClassPort]| -> Vec<String> {
            side.iter().map(|port| port.name.clone()).collect()
        };
        assert_eq!(inputs, names(&ports.inputs), "{}", case.class);
        assert_eq!(outputs, names(&ports.outputs), "{}", case.class);
    }
}

#[test]
fn param_element_name_spells_scalars_vectors_and_matrices_and_refuses_bad_indices() {
    assert_eq!(PARAM_INDEX_PLACEHOLDER, "<i>");
    assert_eq!(PARAM_ROW_PLACEHOLDER, "<row>");
    assert_eq!(PARAM_COL_PLACEHOLDER, "<col>");
    assert_eq!(param_element_name("nin", &[]).as_deref(), Some("nin"));
    assert_eq!(
        param_element_name("msk_<i>", &[7]).as_deref(),
        Some("msk_7")
    );
    assert_eq!(
        param_element_name("table_<row>_<col>", &[3, 2]).as_deref(),
        Some("table_3_2")
    );
    for (template, indices) in [
        ("nin", &[1][..]),
        ("k_<i>", &[][..]),
        ("k_<i>", &[0][..]),
        ("k_<i>", &[1, 1][..]),
        ("K_<row>_<col>", &[1][..]),
        ("K_<row>_<col>", &[1, 0][..]),
    ] {
        assert_eq!(
            param_element_name(template, indices),
            None,
            "{template} {indices:?}"
        );
    }
}

/// A minimal valid binding for every parameter a class requires: one value per `Required` rule
/// by kind, a two-row table for the `TimeTable` sources, and the few cross-parameter orderings
/// the rules impose on the generic values.
fn required_parameters(entry: &oce_api::CatalogEntry) -> Vec<(String, Value)> {
    let mut values: Vec<(String, Value)> = Vec::new();
    for rule in &entry.param_rules {
        match rule {
            CatalogRule::Required { name, kind } => {
                let value = match kind {
                    CatalogValueKind::Real => Value::Real(1.0),
                    CatalogValueKind::Integer => Value::Integer(2),
                    CatalogValueKind::Boolean => Value::Boolean(true),
                    CatalogValueKind::String => Value::String("message".into()),
                    CatalogValueKind::Enum {
                        class_path,
                        members,
                    } => Value::String(
                        format!("Buildings.Controls.OBC.{class_path}.{}", members[0]).into(),
                    ),
                };
                values.push(((*name).to_owned(), value));
            }
            CatalogRule::TimeTableMatrix { base, .. } => {
                for row in 1..=2usize {
                    for col in 1..=2usize {
                        let cell = if col == 1 { (row - 1) as f64 } else { 1.0 };
                        values.push((format!("{base}_{row}_{col}"), Value::Real(cell)));
                    }
                }
            }
            _ => {}
        }
    }
    // Orderings the generic values would tie or cross.
    for (name, value) in &mut values {
        match (entry.class_path, name.as_str()) {
            ("CDL.Reals.Hysteresis", "uHigh") | ("CDL.Reals.Limiter", "uMax") => {
                *value = Value::Real(2.0);
            }
            // A periodic table's last time stamp must precede its period.
            (_, "period") => *value = Value::Real(10.0),
            _ => {}
        }
    }
    values
}

#[test]
fn every_registered_class_resolves_its_default_table_or_names_what_it_needs() {
    let mut needing = Vec::new();
    for entry in catalog() {
        let expect_refusal = entry.param_rules.iter().any(|rule| {
            matches!(
                rule,
                CatalogRule::Required { .. } | CatalogRule::TimeTableMatrix { .. }
            )
        });
        let ports = match class_ports(entry.class_path, &[]) {
            Ok(ports) => ports,
            Err(ClassPortsError::InvalidParameters {
                class_path,
                diagnostics,
            }) => {
                assert_eq!(class_path, entry.class_path);
                assert!(!diagnostics.is_empty());
                needing.push(entry.class_path);
                let params = required_parameters(entry);
                class_ports(entry.class_path, &borrowed(&params))
                    .unwrap_or_else(|error| panic!("{}: {error:?}", entry.class_path))
            }
            Err(error) => panic!("{}: {error}", entry.class_path),
        };
        assert_eq!(
            needing.last() == Some(&entry.class_path),
            expect_refusal,
            "{}: refused at the empty table iff it requires a parameter",
            entry.class_path
        );
        assert_eq!(ports.class_path, entry.class_path);
        if !expect_refusal {
            for (named, listed) in [
                (&ports.inputs, &entry.inputs),
                (&ports.outputs, &entry.outputs),
            ] {
                let kinds: Vec<CatalogPortKind> = named.iter().map(|port| port.kind).collect();
                let listed_kinds: Vec<CatalogPortKind> =
                    listed.iter().map(|port| port.kind).collect();
                assert_eq!(kinds, listed_kinds, "{}", entry.class_path);
            }
        }
        if entry.naming == CatalogPortNaming::Named {
            for (named, listed) in [
                (&ports.inputs, &entry.inputs),
                (&ports.outputs, &entry.outputs),
            ] {
                let names: Vec<&str> = named.iter().map(|port| port.name.as_str()).collect();
                let declared: Vec<&str> = listed.iter().map(|port| port.name.unwrap()).collect();
                assert_eq!(names, declared, "{}", entry.class_path);
            }
        }
    }
    assert_eq!(needing.len(), 39, "{needing:?}");
}

#[test]
fn a_width_at_the_maximum_resolves_and_one_past_it_is_refused() {
    let max = oce_api::catalog()
        .iter()
        .find(|entry| entry.class_path == "CDL.Logical.MultiAnd")
        .and_then(|entry| {
            entry.param_rules.iter().find_map(|rule| match rule {
                oce_api::CatalogRule::IntegerLessOrEqualConstant { name: "nin", max } => Some(*max),
                _ => None,
            })
        })
        .expect("MultiAnd publishes its width cap");
    assert_eq!(max, 1 << 20);
    let ports = class_ports("CDL.Logical.MultiAnd", &[("nin", Value::Integer(max))]).expect("ok");
    assert_eq!(ports.inputs.len(), max as usize);
    assert_eq!(ports.inputs.last().unwrap().name, format!("u{max}"));
    for nin in [max + 1, -1, i64::MAX, i64::MIN] {
        let error =
            class_ports("CDL.Logical.MultiAnd", &[("nin", Value::Integer(nin))]).unwrap_err();
        let ClassPortsError::InvalidParameters { diagnostics, .. } = error else {
            panic!("nin={nin}: {error:?}");
        };
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code.as_str() == "parameter-out-of-range"),
            "nin={nin}: {diagnostics:?}"
        );
    }
}

#[test]
fn hostile_tables_are_typed_refusals() {
    let refused = |class: &str, params: &[(&str, Value)]| -> Vec<&'static str> {
        match class_ports(class, params) {
            Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
                diagnostics.iter().map(|diag| diag.code.as_str()).collect()
            }
            other => panic!("{class} {params:?}: {other:?}"),
        }
    };
    assert!(
        refused(
            "CDL.Reals.MultiSum",
            &[("k_1", Value::Real(f64::NAN)), ("nin", Value::Integer(1))]
        )
        .contains(&"non-finite-parameter")
    );
    assert!(
        refused(
            "CDL.Reals.MultiSum",
            &[("nin", Value::Integer(1)), ("nin", Value::Integer(2))]
        )
        .contains(&"duplicate-parameter")
    );
    assert!(
        refused("CDL.Reals.MatrixMax", &[("nRow", Value::Integer(2))])
            .contains(&"missing-required-parameter")
    );
    assert!(
        refused(
            "CDL.Routing.RealVectorFilter",
            &[
                ("nin", Value::Integer(3)),
                ("nout", Value::Integer(3)),
                ("msk_2", Value::Boolean(false))
            ]
        )
        .contains(&"parameter-out-of-range")
    );
    assert!(
        refused(
            "CDL.Routing.RealVectorReplicator",
            &[
                ("nin", Value::Integer(2)),
                ("nout", Value::Integer(1 << 20))
            ]
        )
        .contains(&"parameter-out-of-range")
    );
    for class in [
        "",
        "CDL.Reals.NoSuchBlock",
        "Buildings.Controls.OBC.CDL.Reals.Add",
        "cdl.reals.add",
    ] {
        assert_eq!(
            class_ports(class, &[]),
            Err(ClassPortsError::UnknownClass {
                class_path: class.to_owned()
            })
        );
    }
    let message = class_ports("CDL.Reals.NoSuchBlock", &[])
        .unwrap_err()
        .to_string();
    assert_eq!(message, "unknown block class `CDL.Reals.NoSuchBlock`");
}

#[test]
fn class_metadata_covers_every_class_in_catalog_order() {
    let metadata = class_metadata();
    let catalog = catalog();
    assert_eq!(metadata.len(), catalog.len());
    for (meta, entry) in metadata.iter().zip(catalog) {
        assert_eq!(meta.class_path, entry.class_path);
        assert!(!meta.description.is_empty(), "{}", meta.class_path);
        // Every authored default is a parameter, in order, settable unless derived or reserved.
        for (parameter, default) in meta.parameters.iter().zip(&entry.param_defaults) {
            assert_eq!(parameter.name, default.name);
            let derived = matches!(default.default, CatalogDefault::Derived(_));
            assert_eq!(
                parameter.settable,
                !derived && !entry.reserved,
                "{}",
                parameter.name
            );
        }
        assert!(meta.parameters.len() >= entry.param_defaults.len());
    }
    assert!(std::ptr::eq(class_metadata(), metadata), "allocated once");
}

#[test]
fn class_descriptions_match_vendored_sources() {
    let root = workspace_path("third_party/modelica-buildings-cdl/Buildings/Controls/OBC");
    let mut checked = 0;
    for meta in class_metadata() {
        let source = root.join(format!("{}.mo", meta.class_path.replace('.', "/")));
        let Ok(text) = std::fs::read_to_string(&source) else {
            continue;
        };
        let class = meta.class_path.rsplit('.').next().unwrap();
        // `block <Name>` (or `model`), then the description string.
        let declaration = ["block ", "model "]
            .iter()
            .find_map(|keyword| {
                text.find(&format!("{keyword}{class}"))
                    .map(|at| at + keyword.len() + class.len())
            })
            .unwrap_or_else(|| panic!("{}: no class declaration", meta.class_path));
        let rest = text[declaration..].trim_start();
        let description = rest
            .strip_prefix('"')
            .and_then(|tail| tail.split_once('"'))
            .map(|(description, _)| description)
            .unwrap_or_else(|| panic!("{}: no description string", meta.class_path));
        assert_eq!(meta.description, description, "{}", meta.class_path);
        checked += 1;
    }
    assert_eq!(
        checked, 132,
        "every class with a vendored source is checked"
    );
}
