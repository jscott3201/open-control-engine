//! The generated units and quantities are the ones the vendored Modelica sources declare, the
//! inverse tables are total over the vendored document and injective, and the recorded pins are
//! the vendored tree's.
//!
//! The `.mo` sources are read as text, never parsed: a declaration is found by its description
//! string, and the text from the previous `;` up to that string must name the connector or
//! parameter and carry `unit="…"` (and `quantity="…"`), the same kind of data read as
//! `oce-api`'s `class_descriptions_match_vendored_sources`.

use std::collections::BTreeSet;

use oce_vocab::vocabulary;
use serde_json::Value;

use crate::generator::core::Core;
use crate::generator::units::{
    QUANTITY_KINDS, QUDT_UNITS, RESOLVED_QUANTITIES, modelica_quantities, modelica_unit,
};
use crate::generator::{BUILDINGS_COMMIT, DOCUMENT, MODELICA_JSON_COMMIT};
use crate::support::{core_bytes, modelica_source, repository, vendored};

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `window` names `name` as a whole identifier.
fn names(window: &str, name: &str) -> bool {
    let bytes = window.as_bytes();
    window.match_indices(name).any(|(at, _)| {
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + name.len()).copied();
        !before.is_some_and(is_identifier) && !after.is_some_and(is_identifier)
    })
}

/// Where the declaration that ends just before byte `at` starts: after the nearest earlier
/// `;` that is outside every bracket, so the row separator of a matrix literal
/// (`K[:,:]=[1, 0; 0, 1]`) does not cut the declaration short.
fn declaration_start(source: &str, at: usize) -> usize {
    let mut depth = 0usize;
    for (index, byte) in source.as_bytes()[..at].iter().enumerate().rev() {
        match byte {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => return index + 1,
            _ => {}
        }
    }
    0
}

/// Whether `source` has a declaration that ends with the description string `description`,
/// names `name` and carries every string of `attributes`.
fn declares(source: &str, name: &str, description: &str, attributes: &[String]) -> bool {
    let quoted = format!("\"{description}\"");
    source.match_indices(&quoted).any(|(at, _)| {
        let window = &source[declaration_start(source, at)..at];
        names(window, name)
            && attributes
                .iter()
                .all(|attribute| window.contains(attribute))
    })
}

#[test]
fn the_declaration_scan_is_scoped_to_one_declaration() {
    let source = "block B \"A block\"\n  parameter Real k(final unit=\"s\")=1 \"Gain\";\n  \
                  Interfaces.RealInput u \"Input\";\n  Interfaces.RealOutput y(unit=\"K\") \"Output\";\n";
    let unit = |unit: &str| vec![format!("unit=\"{unit}\"")];
    assert!(declares(source, "k", "Gain", &unit("s")));
    assert!(declares(source, "y", "Output", &unit("K")));
    // The unit of another declaration does not count, nor a name that only contains the name.
    assert!(!declares(source, "u", "Input", &unit("s")));
    assert!(!declares(source, "u", "Input", &unit("K")));
    assert!(!declares(source, "k", "Gain", &unit("K")));
    assert!(!declares(source, "y", "Gain", &unit("s")));
    assert!(!declares(source, "Inpu", "Input", &[]));
    assert!(!declares(source, "k", "Missing", &[]));
    assert!(declares(source, "u", "Input", &[]));
    // A `;` inside a matrix literal is not a declaration boundary; the one before it is.
    let matrix =
        "  Real a \"First\";\n  parameter Real K[:,:]=[1, 0; 0, 1]\n    \"Gain matrix\";\n";
    assert!(declares(matrix, "K", "Gain matrix", &[]));
    assert!(!declares(matrix, "a", "Gain matrix", &[]));
}

#[test]
fn every_generated_unit_and_quantity_is_declared_in_the_vendored_modelica_source() {
    let (mut units, mut quantities, mut described) = (0, 0, 0);
    for class in vocabulary()
        .generated()
        .classes
        .iter()
        .filter(|c| c.upstream)
    {
        let source = modelica_source(&class.class_path);
        let connectors = class
            .connectors
            .iter()
            .map(|c| (&c.name, &c.description, &c.unit, &c.quantity));
        let parameters = class
            .parameters
            .iter()
            .map(|p| (&p.name, &p.description, &p.unit, &p.quantity));
        for (name, description, unit, quantity) in connectors.chain(parameters) {
            if description.is_empty() {
                continue;
            }
            let mut attributes = Vec::new();
            if !unit.is_empty() {
                attributes.push(format!("unit=\"{unit}\""));
                units += 1;
            }
            if !quantity.is_empty() {
                attributes.push(format!("quantity=\"{quantity}\""));
                quantities += 1;
            }
            assert!(
                declares(&source, name, description, &attributes),
                "{} {name}: no declaration with {attributes:?} and \"{description}\"",
                class.class_path
            );
            described += 1;
        }
    }
    // Every generated description is found in its source, and so is every unit and quantity.
    assert_eq!((units, quantities), (73, 52));
    assert_eq!(described, 265 - 3 + 57 + 189 - 3);
}

/// The public declarations of a class source: everything before its `protected` section or its
/// equations, whichever comes first.
fn public_section(source: &str) -> &str {
    let end = [
        "\nprotected",
        "\nequation",
        "\ninitial equation",
        "\nalgorithm",
    ]
    .iter()
    .filter_map(|keyword| source.find(keyword))
    .min()
    .unwrap_or(source.len());
    &source[..end]
}

#[test]
fn every_unit_and_quantity_a_public_declaration_carries_is_generated() {
    // The other direction: the generator drops no declared unit. Counted per class, so a unit
    // lost on one declaration cannot hide behind one gained on another class.
    let (mut units, mut quantities) = (0, 0);
    for class in vocabulary()
        .generated()
        .classes
        .iter()
        .filter(|c| c.upstream)
    {
        let source = modelica_source(&class.class_path);
        let public = public_section(&source);
        assert!(
            public.len() < source.len(),
            "{}: no equation section",
            class.class_path
        );
        let members = || {
            let connectors = class.connectors.iter().map(|c| (&c.unit, &c.quantity));
            connectors.chain(class.parameters.iter().map(|p| (&p.unit, &p.quantity)))
        };
        let generated_units = members().filter(|(unit, _)| !unit.is_empty()).count();
        let generated_quantities = members()
            .filter(|(_, quantity)| !quantity.is_empty())
            .count();
        assert_eq!(
            public.matches(" unit=\"").count() + public.matches("(unit=\"").count(),
            generated_units,
            "{}: declared units",
            class.class_path
        );
        assert_eq!(
            public.matches(" quantity=\"").count() + public.matches("(quantity=\"").count(),
            generated_quantities,
            "{}: declared quantities",
            class.class_path
        );
        units += generated_units;
        quantities += generated_quantities;
    }
    assert_eq!((units, quantities), (73, 52));
    assert_eq!(
        public_section("block B\n  Real x;\nprotected\n  Real y;\nequation\n"),
        "block B\n  Real x;"
    );
}

#[test]
fn a_wrong_unit_or_quantity_would_not_be_found_in_the_source() {
    let source = modelica_source("CDL.Logical.TrueDelay");
    let attribute = |text: &str| vec![text.to_owned()];
    assert!(declares(
        &source,
        "delayTime",
        "Delay time",
        &attribute("unit=\"s\"")
    ));
    assert!(!declares(
        &source,
        "delayTime",
        "Delay time",
        &attribute("unit=\"min\"")
    ));
    assert!(!declares(
        &source,
        "delayTime",
        "Delay time",
        &attribute("quantity=\"Angle\"")
    ));
    assert!(!declares(
        &source,
        "delayOnInit",
        "Set to true to delay initial true input",
        &attribute("unit=\"s\"")
    ));
}

#[test]
fn the_unit_table_is_injective_and_total_over_the_vendored_document() {
    let qudt: BTreeSet<&str> = QUDT_UNITS.iter().map(|(qudt, _)| *qudt).collect();
    let modelica: BTreeSet<&str> = QUDT_UNITS.iter().map(|(_, unit)| *unit).collect();
    assert_eq!(qudt.len(), QUDT_UNITS.len(), "a QUDT unit is listed twice");
    assert_eq!(
        modelica.len(),
        QUDT_UNITS.len(),
        "two QUDT units invert to one string"
    );
    assert_eq!(QUDT_UNITS.len(), 27);

    let core = Core::parse(core_bytes()).expect("the vendored document parses");
    let identifiers = core.identifiers("qudt:hasUnit");
    assert_eq!(identifiers.len(), 73);
    let mut fallbacks = BTreeSet::new();
    for (node, identifier) in &identifiers {
        let unit = modelica_unit(identifier)
            .unwrap_or_else(|| panic!("{node}: {identifier} has no inverse"));
        if !identifier.starts_with("unit:") {
            fallbacks.insert(unit);
        }
    }
    // The one unit modelica-json's table lacks, carried as the original string.
    assert_eq!(fallbacks.into_iter().collect::<Vec<_>>(), ["1/s"]);
    assert_eq!(modelica_unit("unit:SEC"), Some("s"));
    assert_eq!(modelica_unit("unit:UNITLESS"), Some("1"));
    assert_eq!(modelica_unit("S231:1/s"), Some("1/s"));
    for identifier in [
        "unit:sec",
        "unit:",
        "SEC",
        "",
        "S231:",
        "q:Time",
        "http://qudt.org/vocab/unit#SEC",
    ] {
        assert_eq!(modelica_unit(identifier), None, "{identifier}");
    }
}

#[test]
fn the_quantity_table_is_total_and_ambiguous_only_for_pressure() {
    let forward: BTreeSet<&str> = QUANTITY_KINDS
        .iter()
        .map(|(quantity, _)| *quantity)
        .collect();
    assert_eq!(
        forward.len(),
        QUANTITY_KINDS.len(),
        "a Modelica quantity is listed twice"
    );
    assert_eq!(QUANTITY_KINDS.len(), 18);
    let kinds: BTreeSet<&str> = QUANTITY_KINDS.iter().map(|(_, kind)| *kind).collect();
    for kind in &kinds {
        let inverses = modelica_quantities(&format!("q:{kind}"));
        if *kind == "Pressure" {
            assert_eq!(inverses, ["Pressure", "PressureDifference"]);
        } else {
            assert_eq!(inverses, [*kind], "{kind}");
        }
    }
    for identifier in ["q:", "q:time", "Time", "", "unit:SEC", "q:Luminosity"] {
        assert!(modelica_quantities(identifier).is_empty(), "{identifier}");
    }

    let core = Core::parse(core_bytes()).expect("the vendored document parses");
    let identifiers = core.identifiers("qudt:hasQuantityKind");
    assert_eq!(identifiers.len(), 52);
    let ambiguous: Vec<&str> = identifiers
        .iter()
        .filter(|(_, identifier)| {
            let inverses = modelica_quantities(identifier);
            assert!(!inverses.is_empty(), "{identifier} has no inverse");
            inverses.len() > 1
        })
        .map(|(node, _)| *node)
        .collect();
    // Each ambiguous node is one pinned resolution, and each resolution is in use.
    let resolved: Vec<String> = RESOLVED_QUANTITIES
        .iter()
        .map(|(class, subject, _)| format!("S231:Buildings.Controls.OBC.{class}.{subject}"))
        .collect();
    assert_eq!(ambiguous, resolved);
    for (class, subject, quantity) in RESOLVED_QUANTITIES {
        let generated = vocabulary()
            .class(class)
            .and_then(|class| {
                class
                    .parameters
                    .iter()
                    .find(|parameter| parameter.name == *subject)
                    .map(|parameter| parameter.quantity.to_owned())
            })
            .unwrap_or_else(|| panic!("{class} {subject} is not a generated parameter"));
        assert_eq!(generated, *quantity);
    }
}

#[test]
fn the_recorded_pins_and_digest_are_the_vendored_trees() {
    let source = &vocabulary().generated().source;
    assert_eq!(source.buildings_commit, BUILDINGS_COMMIT);
    assert_eq!(source.modelica_json_commit, MODELICA_JSON_COMMIT);
    assert_eq!(source.document, DOCUMENT);
    assert!(repository(&source.document).is_file());
    let core = Core::parse(core_bytes()).expect("the vendored document parses");
    assert_eq!(source.sha256, core.sha256());

    // The hash manifest of the vendored tree records the same pins and the same digest.
    let manifest = repository("tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).expect("read the manifest"))
            .expect("the manifest is JSON");
    assert_eq!(manifest["buildings_commit"], BUILDINGS_COMMIT);
    assert_eq!(manifest["modelica_json_commit"], MODELICA_JSON_COMMIT);
    let entry = manifest["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["path"] == "cxf-core/CXF-Core.jsonld")
        .expect("the manifest lists the document");
    assert_eq!(entry["sha256"], source.sha256.as_str());
    assert_eq!(entry["origin"], "generated-cxf-core");
    assert_eq!(entry["size_bytes"], core_bytes().len());

    // The vendored README records both pins and the digest beside the regeneration command.
    let readme = std::fs::read_to_string(vendored("README.md")).expect("read the vendored README");
    for recorded in [
        BUILDINGS_COMMIT,
        MODELICA_JSON_COMMIT,
        source.sha256.as_str(),
    ] {
        assert!(
            readme.contains(recorded),
            "the vendored README lacks {recorded}"
        );
    }
    assert!(readme.contains("--elementary --cxfCore"));
}
