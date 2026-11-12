//! Coverage against the catalog in both directions: every authorable class, port and parameter
//! has exactly one vocabulary entry, and every entry names something the catalog has. The
//! counts of the design note's coverage table are pinned.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{CatalogPortNaming, catalog, class_metadata, class_ports};
use oce_vocab::{
    CLASS_BASE, CURATED_JSON, ClassLinks, ClassVocab, CuratedVocabulary, GENERATED_JSON,
    GeneratedVocabulary, PortElement, Side, TextOrigin, VocabError, Vocabulary, class_vocab,
    parameter_base_name, vocabulary,
};

use crate::generator::BUILDINGS_COMMIT;
use crate::support::vendored;
use crate::widths::{borrowed, shaped_classes, width_tables};

fn generated() -> GeneratedVocabulary {
    GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json")
}

fn curated() -> CuratedVocabulary {
    CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json")
}

/// The join's refusal of an edited generated file with the embedded curated file.
fn refused(edit: impl FnOnce(&mut GeneratedVocabulary)) -> VocabError {
    let mut generated = generated();
    edit(&mut generated);
    let generated = GeneratedVocabulary::from_json(&generated.to_json()).expect("still readable");
    Vocabulary::from_parts(generated, curated()).expect_err("refused")
}

fn not_in_catalog(class: &str, subject: &str) -> VocabError {
    VocabError::NotInCatalog {
        class: class.to_owned(),
        subject: subject.to_owned(),
    }
}

fn missing(class: &str, subject: &str) -> VocabError {
    VocabError::MissingFromVocabulary {
        class: class.to_owned(),
        subject: subject.to_owned(),
    }
}

#[test]
fn the_classes_are_exactly_the_authorable_catalog_classes_in_catalog_order() {
    let authorable: Vec<&str> = catalog()
        .iter()
        .filter(|entry| !entry.reserved)
        .map(|entry| entry.class_path)
        .collect();
    let ours: Vec<&str> = vocabulary()
        .classes()
        .map(|class| class.class_path)
        .collect();
    assert_eq!(ours, authorable);
    assert_eq!(ours.len(), 133);
    assert_eq!(catalog().len(), 136);
    for entry in catalog() {
        assert_eq!(
            class_vocab(entry.class_path).is_some(),
            !entry.reserved,
            "{}",
            entry.class_path
        );
    }
    for unknown in [
        "",
        "CDL.Reals.PID ",
        "cdl.reals.pid",
        "Buildings.Controls.OBC.CDL.Reals.PID",
        "CDL.Reals",
        "CDL.Reals.PID.u_s",
    ] {
        assert!(class_vocab(unknown).is_none(), "{unknown:?}");
    }
}

#[test]
fn a_class_on_one_side_only_or_out_of_order_is_refused_by_name() {
    let removed = refused(|generated| {
        generated.classes.remove(4);
    });
    assert_eq!(removed, missing("CDL.Reals.Sources.Sin", ""));
    let last_removed = refused(|generated| {
        generated.classes.pop();
    });
    assert!(matches!(
        last_removed,
        VocabError::MissingFromVocabulary { ref subject, .. } if subject.is_empty()
    ));
    let added = refused(|generated| {
        let mut extra = generated.classes[0].clone();
        extra.class_path = "CDL.Reals.Brand".to_owned();
        generated.classes.push(extra);
    });
    assert_eq!(added, not_in_catalog("CDL.Reals.Brand", ""));
    let renamed = refused(|generated| {
        generated.classes[0].class_path = "CDL.Reals.Sources.Constantly".to_owned();
    });
    assert_eq!(renamed, missing("CDL.Reals.Sources.Constant", ""));
    let reserved = refused(|generated| {
        let mut extra = generated.classes[1].clone();
        extra.class_path = "urn:oce:lowering#PassThrough.Real".to_owned();
        generated.classes.push(extra);
    });
    assert_eq!(
        reserved,
        not_in_catalog("urn:oce:lowering#PassThrough.Real", "")
    );
    let swapped = refused(|generated| generated.classes.swap(1, 2));
    assert_eq!(swapped, not_in_catalog("CDL.Reals.Sources.Pulse", ""));
}

fn generated_class<'a>(
    generated: &'a mut GeneratedVocabulary,
    class_path: &str,
) -> &'a mut oce_vocab::GeneratedClass {
    generated
        .classes
        .iter_mut()
        .find(|class| class.class_path == class_path)
        .unwrap_or_else(|| panic!("{class_path} is generated"))
}

#[test]
fn a_parameter_on_one_side_only_or_out_of_order_is_refused_by_name() {
    let pid = "CDL.Reals.PID";
    let renamed = refused(|generated| {
        let ti = generated_class(generated, pid)
            .parameters
            .iter_mut()
            .find(|p| p.name == "Ti")
            .expect("Ti");
        ti.name = "Tint".to_owned();
    });
    assert_eq!(renamed, missing(pid, "Ti"));
    let removed = refused(|generated| {
        generated_class(generated, pid).parameters.pop();
    });
    assert!(matches!(removed, VocabError::MissingFromVocabulary { ref class, .. } if class == pid));
    let added = refused(|generated| {
        let parameters = &mut generated_class(generated, pid).parameters;
        let mut extra = parameters[0].clone();
        extra.name = "gain".to_owned();
        parameters.push(extra);
    });
    assert_eq!(added, not_in_catalog(pid, "gain"));
    let swapped = refused(|generated| generated_class(generated, pid).parameters.swap(0, 1));
    assert!(matches!(swapped, VocabError::NotInCatalog { ref class, .. } if class == pid));
    // An array parameter is listed under its base name, not the catalog's template.
    let template = refused(|generated| {
        generated_class(generated, "CDL.Reals.MultiSum").parameters[1].name = "k_i".to_owned();
    });
    assert_eq!(template, missing("CDL.Reals.MultiSum", "k"));
}

#[test]
fn a_named_class_connector_on_one_side_only_or_misplaced_is_refused_by_name() {
    let pid = "CDL.Reals.PID";
    let renamed = refused(|generated| {
        generated_class(generated, pid).connectors[0].name = "u_set".to_owned();
    });
    assert_eq!(renamed, missing(pid, "u_s"));
    let removed = refused(|generated| {
        generated_class(generated, pid).connectors.remove(1);
    });
    assert_eq!(removed, missing(pid, "u_m"));
    let swapped = refused(|generated| generated_class(generated, pid).connectors.swap(0, 1));
    assert_eq!(swapped, not_in_catalog(pid, "u_m"));
    let wrong_side = refused(|generated| {
        generated_class(generated, pid).connectors[0].side = Side::Output;
    });
    assert_eq!(wrong_side, missing(pid, "u_s"));
    let vector = refused(|generated| {
        generated_class(generated, pid).connectors[0].dims = 1;
    });
    assert_eq!(vector, not_in_catalog(pid, "u_s"));
    // Inputs come before outputs in every class, width-driven ones included.
    let output_first = refused(|generated| {
        generated_class(generated, "CDL.Reals.MultiSum")
            .connectors
            .reverse();
    });
    assert_eq!(output_first, not_in_catalog("CDL.Reals.MultiSum", "u"));
}

#[test]
fn every_named_port_is_one_scalar_connector_and_every_connector_is_a_port() {
    let mut ports = 0;
    let mut named_classes = 0;
    for entry in catalog().iter().filter(|entry| !entry.reserved) {
        if entry.naming != CatalogPortNaming::Named {
            continue;
        }
        named_classes += 1;
        let class = class_vocab(entry.class_path).expect("a class");
        let mut reached = BTreeSet::new();
        for (side, side_ports) in [(Side::Input, &entry.inputs), (Side::Output, &entry.outputs)] {
            for port in side_ports {
                let name = port.name.expect("a named port");
                let (connector, element) = class
                    .connector_of_port(side, name)
                    .unwrap_or_else(|| panic!("{} {name}", entry.class_path));
                assert_eq!((connector.name, element), (name, PortElement::Scalar));
                assert!(reached.insert((side, name)), "{} {name}", entry.class_path);
                ports += 1;
            }
        }
        assert_eq!(
            reached.len(),
            class.connectors.len(),
            "{}",
            entry.class_path
        );
    }
    assert_eq!((named_classes, ports), (105, 265));
}

/// Resolve `class` at `table` and require every port to be exactly one connector's element, the
/// elements of each connector to be exactly its index range in port order, and every connector
/// to have a port. Returns the number of ports mapped.
fn ports_cover_connectors(class: &ClassVocab<'_>, table: &crate::widths::Table) -> usize {
    let path = class.class_path;
    let ports =
        class_ports(path, &borrowed(table)).unwrap_or_else(|error| panic!("{path}: {error}"));
    let mut elements: BTreeMap<(bool, &str), Vec<PortElement>> = BTreeMap::new();
    for (side, side_ports) in [(Side::Input, &ports.inputs), (Side::Output, &ports.outputs)] {
        for port in side_ports {
            let (connector, element) = class
                .connector_of_port(side, &port.name)
                .unwrap_or_else(|| panic!("{path}: port {} is not one connector's", port.name));
            elements
                .entry((side == Side::Output, connector.name))
                .or_default()
                .push(element);
        }
    }
    for connector in &class.connectors {
        let found = elements
            .get(&(connector.side == Side::Output, connector.name))
            .unwrap_or_else(|| panic!("{path}: connector {} has no port", connector.name));
        match connector.dims {
            0 => assert_eq!(found, &[PortElement::Scalar], "{path} {}", connector.name),
            1 => {
                for (at, element) in found.iter().enumerate() {
                    assert_eq!(
                        *element,
                        PortElement::Vector(at + 1),
                        "{path} {}",
                        connector.name
                    );
                }
            }
            _ => {
                let PortElement::Matrix { col: cols, .. } = found[found.len() - 1] else {
                    panic!("{path} {}: not a matrix element", connector.name);
                };
                for (at, element) in found.iter().enumerate() {
                    let expected = PortElement::Matrix {
                        row: at / cols + 1,
                        col: at % cols + 1,
                    };
                    assert_eq!(*element, expected, "{path} {}", connector.name);
                }
            }
        }
    }
    assert_eq!(elements.len(), class.connectors.len(), "{path}");
    ports.inputs.len() + ports.outputs.len()
}

#[test]
fn the_width_tables_cover_exactly_the_classes_whose_ports_the_catalog_does_not_name() {
    let unnamed: BTreeSet<&str> = catalog()
        .iter()
        .filter(|entry| !entry.reserved && entry.naming != CatalogPortNaming::Named)
        .map(|entry| entry.class_path)
        .collect();
    let shaped: BTreeSet<&str> = shaped_classes().collect();
    assert_eq!(shaped, unnamed);
    assert_eq!(shaped.len(), 28);
    assert_eq!(shaped_classes().count(), 28, "no class has two rows");
    assert!(width_tables("CDL.Reals.Add").is_none());
}

/// Run [`ports_cover_connectors`] over every width table of the classes `filter` selects.
fn resolved_ports_map_to_connectors(filter: impl Fn(&str) -> bool) -> (usize, usize) {
    let (mut tables, mut ports) = (0, 0);
    for class_path in shaped_classes().filter(|class_path| filter(class_path)) {
        let class = class_vocab(class_path).expect("a class");
        for table in width_tables(class_path).expect("a shaped class") {
            ports += ports_cover_connectors(&class, &table);
            tables += 1;
        }
    }
    (tables, ports)
}

// The maximum width is 1,048,576 ports, so the 28 classes are split by family to run in
// parallel.

#[test]
fn reals_vector_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) = resolved_ports_map_to_connectors(|class| {
        class.starts_with("CDL.Reals.") && !class.starts_with("CDL.Reals.Matrix")
    });
    assert_eq!(tables, 5 * 3);
    assert!(ports > 6 * 1_048_576, "{ports}");
}

#[test]
fn reals_matrix_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) =
        resolved_ports_map_to_connectors(|class| class.starts_with("CDL.Reals.Matrix"));
    assert_eq!(tables, 3 * 4);
    assert!(ports > 6 * 1_048_576, "{ports}");
}

#[test]
fn logical_and_integers_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) = resolved_ports_map_to_connectors(|class| {
        class.starts_with("CDL.Logical.") || class.starts_with("CDL.Integers.")
    });
    assert_eq!(tables, 5 * 3);
    assert!(ports > 3 * 1_048_576, "{ports}");
}

#[test]
fn boolean_routing_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) =
        resolved_ports_map_to_connectors(|class| class.starts_with("CDL.Routing.Boolean"));
    assert_eq!(tables, 4 * 3 + 4);
    assert!(ports > 8 * 1_048_576, "{ports}");
}

#[test]
fn integer_routing_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) =
        resolved_ports_map_to_connectors(|class| class.starts_with("CDL.Routing.Integer"));
    assert_eq!(tables, 4 * 3 + 4);
    assert!(ports > 8 * 1_048_576, "{ports}");
}

#[test]
fn real_routing_ports_map_to_one_connector_each_at_widths_one_two_and_the_maximum() {
    let (tables, ports) =
        resolved_ports_map_to_connectors(|class| class.starts_with("CDL.Routing.Real"));
    assert_eq!(tables, 4 * 3 + 4);
    assert!(ports > 8 * 1_048_576, "{ports}");
}

#[test]
fn a_port_that_is_no_connectors_element_maps_to_nothing() {
    let sum = class_vocab("CDL.Reals.MultiSum").expect("a class");
    assert_eq!(
        sum.connector_of_port(Side::Input, "u3")
            .map(|(c, e)| (c.name, e)),
        Some(("u", PortElement::Vector(3)))
    );
    for (side, port) in [
        (Side::Output, "u3"),
        (Side::Input, "u"),
        (Side::Input, "u0"),
        (Side::Input, "y"),
        (Side::Input, "k_1"),
        (Side::Input, ""),
        (Side::Output, "y1"),
    ] {
        assert!(
            sum.connector_of_port(side, port).is_none(),
            "{side:?} {port}"
        );
    }
    let sort = class_vocab("CDL.Reals.Sort").expect("a class");
    assert_eq!(
        sort.connector_of_port(Side::Output, "yIdx2")
            .map(|(c, e)| (c.name, e)),
        Some(("yIdx", PortElement::Vector(2)))
    );
    assert_eq!(
        sort.connector_of_port(Side::Output, "y2")
            .map(|(c, e)| (c.name, e)),
        Some(("y", PortElement::Vector(2)))
    );
    let replicator = class_vocab("CDL.Routing.RealVectorReplicator").expect("a class");
    assert_eq!(
        replicator
            .connector_of_port(Side::Output, "y3_2")
            .map(|(c, e)| (c.name, e)),
        Some(("y", PortElement::Matrix { row: 3, col: 2 }))
    );
    assert!(replicator.connector_of_port(Side::Output, "y3").is_none());
}

#[test]
fn every_catalog_parameter_is_one_entry_and_every_entry_is_a_catalog_parameter() {
    let mut parameters = 0;
    for metadata in class_metadata() {
        let Some(class) = class_vocab(metadata.class_path) else {
            continue;
        };
        assert_eq!(class.description, metadata.description);
        assert_eq!(class.parameters.len(), metadata.parameters.len());
        for (entry, catalog_parameter) in class.parameters.iter().zip(&metadata.parameters) {
            assert_eq!(entry.name, parameter_base_name(catalog_parameter.name));
            assert!(std::ptr::eq(entry.catalog, catalog_parameter));
            parameters += 1;
        }
        let names: BTreeSet<&str> = class.parameters.iter().map(|p| p.name).collect();
        assert_eq!(
            names.len(),
            class.parameters.len(),
            "{}",
            metadata.class_path
        );
    }
    // The design note counts 186: the catalog's parameter defaults. `class_metadata()` also
    // lists the three `TimeTable` sources' rule-only `table`, which an author must assign.
    assert_eq!(parameters, 186 + 3);
}

#[test]
fn the_coverage_counts_are_the_design_notes() {
    let classes: Vec<ClassVocab<'_>> = vocabulary().classes().collect();
    let connectors = || classes.iter().flat_map(|class| class.connectors.iter());
    let parameters = || classes.iter().flat_map(|class| class.parameters.iter());
    assert_eq!(classes.len(), 133);
    assert_eq!(classes.iter().filter(|class| class.upstream).count(), 132);
    assert_eq!(
        classes.iter().filter(|class| class.links.is_some()).count(),
        132
    );
    assert_eq!(connectors().count(), 265 + 57);
    assert_eq!(connectors().filter(|c| c.dims > 0).count(), 42);
    assert_eq!(connectors().filter(|c| c.dims == 2).count(), 5);
    assert_eq!(parameters().count(), 186 + 3);
    // Descriptions: every one present, all but six generated.
    let is_curated = |origin: TextOrigin| origin == TextOrigin::Curated;
    assert_eq!(connectors().filter(|c| is_curated(c.origin)).count(), 3);
    assert_eq!(parameters().filter(|p| is_curated(p.origin)).count(), 3);
    assert!(connectors().all(|c| !c.description.is_empty()));
    assert!(parameters().all(|p| !p.description.is_empty()));
    // Declared units and quantities.
    assert_eq!(connectors().filter(|c| !c.unit.is_empty()).count(), 21);
    assert_eq!(parameters().filter(|p| !p.unit.is_empty()).count(), 52);
    assert_eq!(connectors().filter(|c| !c.quantity.is_empty()).count(), 11);
    assert_eq!(parameters().filter(|p| !p.quantity.is_empty()).count(), 41);
    let with_units = classes
        .iter()
        .filter(|class| {
            class.connectors.iter().any(|c| !c.unit.is_empty())
                || class.parameters.iter().any(|p| !p.unit.is_empty())
        })
        .count();
    assert_eq!(with_units, 39);
    let units: BTreeSet<&str> = connectors()
        .map(|c| c.unit)
        .chain(parameters().map(|p| p.unit))
        .filter(|unit| !unit.is_empty())
        .collect();
    assert_eq!(
        units.into_iter().collect::<Vec<_>>(),
        ["1", "1/s", "Hz", "J/kg", "K", "Pa", "rad", "s"]
    );
    let quantities: BTreeSet<&str> = connectors()
        .map(|c| c.quantity)
        .chain(parameters().map(|p| p.quantity))
        .filter(|quantity| !quantity.is_empty())
        .collect();
    assert_eq!(
        quantities.into_iter().collect::<Vec<_>>(),
        [
            "Angle",
            "Frequency",
            "Pressure",
            "SpecificEnergy",
            "ThermodynamicTemperature",
            "Time"
        ]
    );
    // Curation: thirteen classes named, thirteen notes, six descriptions.
    let curated_file = curated();
    assert_eq!(curated_file.classes.len(), 13);
    assert_eq!(
        classes.iter().map(|class| class.notes.len()).sum::<usize>(),
        13
    );
    let descriptions: usize = curated_file
        .classes
        .iter()
        .map(|c| c.descriptions.len())
        .sum();
    assert_eq!(descriptions, 6);
}

#[test]
fn every_upstream_class_links_to_its_pinned_source_and_the_latest_help_page() {
    let pid = class_vocab("CDL.Reals.PID").expect("a class");
    assert_eq!(
        pid.links,
        Some(ClassLinks {
            pinned_source: "https://github.com/lbl-srg/modelica-buildings/blob/a131864e4c4df22ebcd52bb8da439de0087ac365/Buildings/Controls/OBC/CDL/Reals/PID.mo".to_owned(),
            latest_help: "https://simulationresearch.lbl.gov/modelica/releases/latest/help/Buildings_Controls_OBC_CDL_Reals.html#Buildings.Controls.OBC.CDL.Reals.PID".to_owned(),
        })
    );
    let constant = class_vocab("CDL.Reals.Sources.Constant").expect("a class");
    let links = constant.links.expect("an upstream class");
    assert!(
        links
            .pinned_source
            .ends_with("/Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo")
    );
    assert!(links.latest_help.ends_with(
        "/Buildings_Controls_OBC_CDL_Reals_Sources.html#Buildings.Controls.OBC.CDL.Reals.Sources.Constant"
    ));
    assert_eq!(
        vocabulary().generated().source.buildings_commit,
        BUILDINGS_COMMIT
    );

    let mut linked = 0;
    for class in vocabulary().classes() {
        let Some(links) = &class.links else {
            assert_eq!(class.class_path, "CDL.Logical.TrueHoldWithReset");
            continue;
        };
        linked += 1;
        // The pinned source link names the file the vendored tree holds at the same pin.
        let prefix =
            format!("https://github.com/lbl-srg/modelica-buildings/blob/{BUILDINGS_COMMIT}/");
        let file = links
            .pinned_source
            .strip_prefix(&prefix)
            .expect("the pinned prefix");
        assert!(vendored(file).is_file(), "{file}");
        assert!(
            links
                .latest_help
                .ends_with(&format!("#{CLASS_BASE}{}", class.class_path))
        );
        for link in [&links.pinned_source, &links.latest_help] {
            assert!(link.starts_with("https://") && link.is_ascii() && !link.contains(' '));
        }
    }
    assert_eq!(linked, 132);
}
