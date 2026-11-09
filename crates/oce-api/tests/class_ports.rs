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
    ValueType, catalog, class_metadata, class_ports, param_element_name,
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
        // Every authored default is a parameter, in order, with the catalog's own default
        // payload; settable is the reservation alone (GitHub #440 amendment, 2026-11-08), so a
        // derived default such as `MatrixGain.K` is settable.
        for (parameter, default) in meta.parameters.iter().zip(&entry.param_defaults) {
            assert_eq!(parameter.name, default.name);
            assert_eq!(parameter.default, default.default, "{}", parameter.name);
        }
        for parameter in &meta.parameters {
            assert_eq!(parameter.settable, !entry.reserved, "{}", parameter.name);
        }
        // A `Required` rule's kind is the parameter's type.
        for rule in &entry.param_rules {
            if let CatalogRule::Required { name, kind } = rule {
                let parameter = meta.parameters.iter().find(|p| p.name == *name).unwrap();
                let expected = match kind {
                    CatalogValueKind::Real => ValueType::Real,
                    CatalogValueKind::Integer => ValueType::Integer,
                    CatalogValueKind::Boolean => ValueType::Boolean,
                    CatalogValueKind::String => ValueType::String,
                    CatalogValueKind::Enum { .. } => parameter.value_type,
                };
                assert_eq!(
                    parameter.value_type, expected,
                    "{}.{name}",
                    entry.class_path
                );
            }
        }
        assert!(meta.parameters.len() >= entry.param_defaults.len());
    }
    // A derived default stays settable: the corpus itself assigns `MatrixGain.K`.
    let gain = metadata
        .iter()
        .find(|meta| meta.class_path == "CDL.Reals.MatrixGain")
        .unwrap();
    let k = gain
        .parameters
        .iter()
        .find(|p| p.name == "K_<row>_<col>")
        .unwrap();
    assert!(matches!(k.default, CatalogDefault::Derived(_)));
    assert!(k.settable);
    assert_eq!(k.value_type, ValueType::Real);
    assert!(std::ptr::eq(class_metadata(), metadata), "allocated once");
}

/// The body of a Modelica string literal whose opening quote is already consumed, with its
/// escapes (`\"`, `\\`, `\n`, …) decoded; `None` if the literal never closes.
fn modelica_string(tail: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = tail.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}

#[test]
fn the_modelica_string_reader_decodes_escaped_quotes() {
    assert_eq!(modelica_string(r#"plain" rest"#).as_deref(), Some("plain"));
    assert_eq!(
        modelica_string(r#"say \"hi\" now" rest"#).as_deref(),
        Some(r#"say "hi" now"#)
    );
    assert_eq!(modelica_string(r#"a \\ b""#).as_deref(), Some(r"a \ b"));
    assert_eq!(modelica_string("never closes"), None);
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
            .and_then(modelica_string)
            .unwrap_or_else(|| panic!("{}: no description string", meta.class_path));
        assert_eq!(meta.description, description, "{}", meta.class_path);
        checked += 1;
    }
    assert_eq!(
        checked, 132,
        "every class with a vendored source is checked"
    );
}
