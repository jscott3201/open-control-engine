//! The block vocabulary through `oce catalog` and the scale-factor table through `oce vocab`
//! (vocabulary note decisions 12 and 14, GitHub issue #500): golden transcripts of the classes
//! the note names, and the properties over every class: a closed JSON shape, the vocabulary's
//! own text and the catalog's ranges, no `evidence`, no HTML, and the same bytes on every run.

mod support;

use std::collections::BTreeSet;

use oce_api::{CatalogDefault, ClassMetadata, ValueType, class_metadata, class_ports};
use oce_vocab::{SCALE_FACTORS, class_vocab, vocabulary};
use serde_json::{Value, json};
use support::{Run, case, oce};

// --- golden transcripts (the note's "Serving" list) ---

#[test]
fn catalog_describes_a_class_with_units_and_notes() {
    let class = ["catalog", "CDL.Logical.TrueDelay", "--param", "delayTime=5"];
    case("catalog_true_delay", 0, &class, b"");
    let json = [&class[..], &["--json"]].concat();
    case("catalog_true_delay_json", 0, &json, b"");
}

#[test]
fn catalog_describes_a_class_whose_descriptions_are_curated() {
    let class = ["catalog", "CDL.Logical.TrueHoldWithReset"];
    case("catalog_true_hold_with_reset", 0, &class, b"");
    let json = [&class[..], &["--json"]].concat();
    case("catalog_true_hold_with_reset_json", 0, &json, b"");
}

#[test]
fn catalog_describes_ranges_as_text_and_json() {
    case("catalog_pid", 0, &["catalog", "CDL.Reals.PID"], b"");
    case(
        "catalog_hysteresis_json",
        0,
        &[
            "catalog",
            "CDL.Reals.Hysteresis",
            "--json",
            "--param",
            "uLow=-0.5",
            "--param",
            "uHigh=0.5",
        ],
        b"",
    );
}

#[test]
fn catalog_describes_every_port_of_a_vector_connector() {
    case(
        "catalog_multisum",
        0,
        &["catalog", "CDL.Reals.MultiSum", "--param", "nin=3"],
        b"",
    );
}

// --- every class ---

/// `--param` arguments that give every required parameter of the class a value of its type, so
/// that the class resolves wherever such a table satisfies its rules.
fn required_params(meta: &ClassMetadata) -> Vec<String> {
    meta.parameters
        .iter()
        .filter(|parameter| parameter.default == CatalogDefault::Required)
        .filter(|parameter| !parameter.name.contains('<'))
        .filter_map(|parameter| {
            let value = match parameter.value_type {
                ValueType::Real | ValueType::Integer => "1",
                ValueType::Boolean => "true",
                _ => return None,
            };
            Some(format!("{}={value}", parameter.name))
        })
        .collect()
}

/// `oce catalog CLASS` at [`required_params`], as text or as JSON.
fn describe(meta: &ClassMetadata, json: bool) -> Run {
    let params = required_params(meta);
    let mut args = vec!["catalog", meta.class_path];
    for param in &params {
        args.extend(["--param", param.as_str()]);
    }
    if json {
        args.push("--json");
    }
    oce(&args, b"")
}

/// Every object key in `value`, at any depth.
fn keys_of(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                keys.insert(key.clone());
                keys_of(value, keys);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys_of(item, keys)),
        _ => {}
    }
}

fn object_keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect()
}

/// The classes of `oce catalog --json`, in order.
fn listed_classes() -> Vec<Value> {
    let run = oce(&["catalog", "--json"], b"");
    assert_eq!(run.code, 0);
    run.report()["classes"].as_array().expect("classes").clone()
}

const PORT_KEYS: [&str; 5] = ["description", "kind", "name", "quantity", "unit"];

/// Hold one class object to the closed shape the README documents.
fn assert_closed_shape(class: &Value) {
    assert_eq!(
        object_keys(class),
        [
            "class_path",
            "description",
            "inputs",
            "links",
            "notes",
            "outputs",
            "parameters",
            "reserved"
        ],
        "{class}"
    );
    for side in ["inputs", "outputs"] {
        // `null` where the ports do not resolve at this table.
        for port in class[side].as_array().into_iter().flatten() {
            assert_eq!(object_keys(port), PORT_KEYS, "{port}");
            for key in PORT_KEYS {
                assert!(port[key].is_string(), "{port}");
            }
        }
    }
    for parameter in class["parameters"].as_array().expect("parameters") {
        assert_eq!(
            object_keys(parameter),
            [
                "default",
                "description",
                "enumeration",
                "name",
                "quantity",
                "range",
                "settable",
                "type",
                "unit"
            ],
            "{parameter}"
        );
        for key in ["description", "quantity", "unit"] {
            assert!(parameter[key].is_string(), "{parameter}");
        }
        let range = parameter["range"].as_array().expect("range is a list");
        assert!(range.iter().all(Value::is_string), "{parameter}");
    }
    for note in class["notes"].as_array().expect("notes is a list") {
        assert_eq!(object_keys(note), ["kind", "subject", "text"], "{note}");
        assert!(note.as_object().unwrap().values().all(Value::is_string));
    }
    if !class["links"].is_null() {
        assert_eq!(
            object_keys(&class["links"]),
            ["latest_help", "pinned_source"]
        );
    }
}

#[test]
fn every_class_has_the_closed_shape_in_the_list_and_alone() {
    let classes = listed_classes();
    assert_eq!(classes.len(), class_metadata().len());
    for (class, meta) in classes.iter().zip(class_metadata()) {
        assert_closed_shape(class);
        let alone = describe(meta, true).report();
        assert_closed_shape(&alone["class"]);
        // The vocabulary of a class does not depend on the table its ports resolve at.
        for key in ["class_path", "description", "links", "notes", "parameters"] {
            assert_eq!(alone["class"][key], class[key], "{} {key}", meta.class_path);
        }
    }
}

#[test]
fn every_class_carries_the_vocabularys_text_and_the_catalogs_ranges() {
    let (mut ports, mut parameters, mut notes, mut linked, mut ranged) = (0, 0, 0, 0, 0);
    for (class, meta) in listed_classes().iter().zip(class_metadata()) {
        let path = meta.class_path;
        let Some(vocab) = class_vocab(path) else {
            // A reserved lowering identity: no vocabulary, and the shape still holds.
            assert_eq!(class["reserved"], true, "{path}");
            assert_eq!(class["links"], Value::Null, "{path}");
            assert_eq!(class["notes"], json!([]), "{path}");
            for parameter in class["parameters"].as_array().unwrap() {
                assert_eq!(parameter["description"], "", "{path}");
                assert_eq!(parameter["range"], json!([]), "{path}");
            }
            continue;
        };
        assert_eq!(class["reserved"], false, "{path}");
        let expected_links = vocab.links.as_ref().map_or(Value::Null, |links| {
            json!({ "latest_help": links.latest_help, "pinned_source": links.pinned_source })
        });
        assert_eq!(class["links"], expected_links, "{path}");
        linked += usize::from(vocab.links.is_some());
        let expected_notes: Vec<Value> = vocab
            .notes
            .iter()
            .map(|note| {
                json!({ "kind": note.kind.as_str(), "subject": note.subject, "text": note.text })
            })
            .collect();
        assert_eq!(class["notes"], json!(expected_notes), "{path}");
        notes += expected_notes.len();

        let listed = class["parameters"].as_array().unwrap();
        assert_eq!(listed.len(), vocab.parameters.len(), "{path}");
        for (parameter, expected) in listed.iter().zip(&vocab.parameters) {
            let name = expected.catalog.name;
            assert_eq!(parameter["name"], name, "{path}");
            assert_eq!(
                parameter["description"], expected.description,
                "{path} {name}"
            );
            assert!(!expected.description.is_empty(), "{path} {name}");
            assert_eq!(parameter["unit"], expected.unit, "{path} {name}");
            assert_eq!(parameter["quantity"], expected.quantity, "{path} {name}");
            assert_eq!(parameter["range"], json!(expected.range), "{path} {name}");
            ranged += usize::from(!expected.range.is_empty());
            parameters += 1;
        }

        // The ports of the list are resolved at the default table; a class with a required
        // parameter resolves alone, at a table that gives it one.
        let alone = describe(meta, true).report();
        for (side, vocab_side) in [
            ("inputs", oce_vocab::Side::Input),
            ("outputs", oce_vocab::Side::Output),
        ] {
            for resolved in [&class[side], &alone["class"][side]] {
                for port in resolved.as_array().into_iter().flatten() {
                    let name = port["name"].as_str().unwrap();
                    let (connector, _) = vocab
                        .connector_of_port(vocab_side, name)
                        .unwrap_or_else(|| panic!("{path}: port {name} has no connector"));
                    assert!(!connector.description.is_empty(), "{path} {name}");
                    assert_eq!(port["description"], connector.description, "{path} {name}");
                    assert_eq!(port["unit"], connector.unit, "{path} {name}");
                    assert_eq!(port["quantity"], connector.quantity, "{path} {name}");
                    ports += 1;
                }
            }
        }
    }
    // Volume pins: a join that silently found nothing must not leave this green.
    assert_eq!(vocabulary().classes().count(), 133);
    assert_eq!((linked, notes, parameters, ranged), (132, 13, 189, 101));
    assert!(ports > 500, "only {ports} ports were compared");
}

/// Every `evidence` string of `curated.json`: what reviewers read and agents must not.
fn evidence_strings() -> BTreeSet<&'static str> {
    let curated = vocabulary().curated();
    let all: BTreeSet<&str> = curated
        .classes
        .iter()
        .flat_map(|class| {
            let descriptions = class.descriptions.iter().map(|d| d.evidence.as_str());
            descriptions.chain(class.notes.iter().map(|note| note.evidence.as_str()))
        })
        .collect();
    assert!(all.len() >= 5, "the curated seed carries evidence");
    all
}

#[track_caller]
fn assert_no_evidence(run: &Run, evidence: &BTreeSet<&str>, what: &str) {
    let printed = run.transcript();
    assert!(!printed.contains("evidence"), "{what}: prints `evidence`");
    for text in evidence {
        assert!(!printed.contains(text), "{what}: prints evidence {text:?}");
    }
}

#[test]
fn evidence_is_never_printed_as_text_or_json() {
    let evidence = evidence_strings();
    // The whole catalog as JSON and as the text list, and the vocabulary command.
    for args in [
        &["catalog", "--json"][..],
        &["catalog"][..],
        &["vocab"][..],
        &["vocab", "--json"][..],
    ] {
        let run = oce(args, b"");
        assert_eq!(run.code, 0, "{args:?}");
        assert_no_evidence(&run, &evidence, &args.join(" "));
        if args.contains(&"--json") {
            let mut keys = BTreeSet::new();
            keys_of(&run.report(), &mut keys);
            assert!(!keys.contains("evidence"), "{args:?}");
        }
    }
    // Every curated class alone, as text and as JSON, with its notes and descriptions shown.
    let curated = &vocabulary().curated().classes;
    assert!(curated.len() >= 10);
    for class in curated {
        let meta = class_metadata()
            .iter()
            .find(|meta| meta.class_path == class.class_path)
            .expect("a curated class is a catalog class");
        for json in [false, true] {
            let run = describe(meta, json);
            assert_eq!(run.code, 0, "{}", run.transcript());
            assert_no_evidence(&run, &evidence, &class.class_path);
            // The run did print the class's curated text, so the absence is not vacuous.
            for note in &class.notes {
                let text = serde_json::to_string(&note.text).unwrap();
                let shown = if json {
                    text.trim_matches('"')
                } else {
                    &note.text
                };
                assert!(run.stdout_text().contains(shown), "{}", class.class_path);
            }
            for description in &class.descriptions {
                assert!(
                    run.stdout_text().contains(&description.text),
                    "{}",
                    class.class_path
                );
            }
        }
    }
}

/// The HTML the upstream documentation strings use, and its link scheme (the vocabulary note's
/// closed list). `<i>` is absent on purpose: the catalog spells an array template `k_<i>`.
const MARKUP: &[&str] = &[
    "<html",
    "<p>",
    "<code>",
    "<a ",
    "<br",
    "<img",
    "</",
    "modelica://",
];

#[test]
fn every_class_prints_plain_text_without_markup_or_host_paths() {
    let mut described = 0;
    let mut runs = vec![oce(&["catalog", "--json"], b""), oce(&["catalog"], b"")];
    for meta in class_metadata() {
        let text = describe(meta, false);
        // Text is the product only where the ports resolve; the report is printed otherwise.
        described += usize::from(text.code == 0);
        runs.push(text);
        runs.push(describe(meta, true));
    }
    assert!(
        described >= 120,
        "only {described} classes resolved as text"
    );
    for run in &runs {
        let printed = run.transcript();
        for needle in MARKUP {
            assert!(!printed.contains(needle), "{needle} in\n{printed}");
        }
        for needle in ["/Users/", "/home/", "C:\\", "/tmp/", "/private/", "file://"] {
            assert!(!printed.contains(needle), "{needle} in\n{printed}");
        }
        assert!(!printed.contains('\r') && !printed.contains('\t'));
        assert!(run.stderr.is_empty(), "{printed}");
    }
}

#[test]
fn the_text_and_the_json_of_a_class_say_the_same_things() {
    for meta in class_metadata() {
        let text = describe(meta, false);
        if text.code != 0 {
            continue;
        }
        let text = text.stdout_text().to_owned();
        let class = describe(meta, true).report()["class"].clone();
        let path = meta.class_path;
        assert!(text.starts_with(&format!("{path}\n  {}\n", meta.description)));
        for parameter in class["parameters"].as_array().unwrap() {
            let name = parameter["name"].as_str().unwrap();
            assert!(text.contains(&format!("\n  {name} : ")), "{path} {name}");
            for key in ["description", "unit", "quantity"] {
                let value = parameter[key].as_str().unwrap();
                assert!(text.contains(value), "{path} {name} {key}");
            }
            let range: Vec<&str> = parameter["range"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line.as_str().unwrap())
                .collect();
            if !range.is_empty() {
                let line = format!("\n      range: {}\n", range.join("; "));
                assert!(text.contains(&line), "{path} {name}: {line}");
            }
        }
        for side in ["inputs", "outputs"] {
            for port in class[side].as_array().unwrap() {
                let (name, kind) = (
                    port["name"].as_str().unwrap(),
                    port["kind"].as_str().unwrap(),
                );
                let mut entry = format!("\n  {name} : {kind}\n");
                let description = port["description"].as_str().unwrap();
                if !description.is_empty() {
                    entry.push_str(&format!("      {description}\n"));
                }
                assert!(text.contains(&entry), "{path} {name}");
            }
        }
        for note in class["notes"].as_array().unwrap() {
            let (kind, subject) = (
                note["kind"].as_str().unwrap(),
                note["subject"].as_str().unwrap(),
            );
            let label = if subject.is_empty() {
                kind.to_owned()
            } else {
                format!("{kind} ({subject})")
            };
            let line = format!("\n  {label}: {}\n", note["text"].as_str().unwrap());
            assert!(text.contains(&line), "{path}: {line}");
        }
        if class["notes"].as_array().unwrap().is_empty() {
            assert!(text.contains("\nnotes:\n  (none)\n"), "{path}");
        }
        match class["links"].as_object() {
            Some(links) => {
                for link in links.values() {
                    assert!(text.contains(link.as_str().unwrap()), "{path}");
                }
                assert!(text.contains("(may drift from the pin)"), "{path}");
            }
            None => assert!(text.ends_with("\nlinks:\n  (none)\n"), "{path}"),
        }
    }
}

#[test]
fn the_class_list_stays_one_line_per_class() {
    let run = oce(&["catalog"], b"");
    assert_eq!(run.code, 0);
    let lines: Vec<&str> = run.stdout_text().lines().collect();
    assert_eq!(lines.len(), class_metadata().len());
    for (line, meta) in lines.iter().zip(class_metadata()) {
        assert!(line.starts_with(meta.class_path), "{line}");
        assert!(line.ends_with(meta.description), "{line}");
    }
}

#[test]
fn a_port_of_a_vector_connector_carries_the_connectors_meaning_at_any_width() {
    for width in [1usize, 2, 3, 64] {
        let param = format!("nin={width}");
        let run = oce(
            &["catalog", "CDL.Reals.MultiSum", "--json", "--param", &param],
            b"",
        );
        assert_eq!(run.code, 0);
        let inputs = run.report()["class"]["inputs"].as_array().unwrap().clone();
        assert_eq!(inputs.len(), width);
        let ports = class_ports(
            "CDL.Reals.MultiSum",
            &[("nin", oce_api::Value::Integer(width as i64))],
        )
        .expect("the table resolves");
        for (index, (input, port)) in inputs.iter().zip(&ports.inputs).enumerate() {
            assert_eq!(input["name"], port.name);
            assert_eq!(input["name"], format!("u{}", index + 1));
            assert_eq!(input["description"], inputs[0]["description"]);
            assert_ne!(input["description"], "");
        }
    }
    // Width zero: no input ports, and the class is still described.
    let run = oce(&["catalog", "CDL.Reals.MultiSum", "--json"], b"");
    assert_eq!(run.report()["class"]["inputs"], json!([]));
    assert_ne!(run.report()["class"]["parameters"][0]["description"], "");
}

// --- the scale-factor table ---

#[test]
fn vocab_lists_the_scale_factor_table_exactly() {
    let report = oce(&["vocab", "--json"], b"").report();
    assert_eq!(
        report["scale_factors"],
        json!([
            { "base": "s", "factor": 60, "symbol": "min" },
            { "base": "s", "factor": 3600, "symbol": "h" },
            { "base": "s", "factor": 86400, "symbol": "d" },
        ])
    );
    let rows = report["scale_factors"].as_array().unwrap();
    assert_eq!(rows.len(), SCALE_FACTORS.len());
    let base_units: Vec<&str> = report["base_units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| unit.as_str().unwrap())
        .collect();
    for (row, expected) in rows.iter().zip(SCALE_FACTORS) {
        assert_eq!(object_keys(row), ["base", "factor", "symbol"]);
        assert_eq!(row["symbol"], expected.symbol);
        assert_eq!(row["base"], expected.base);
        // An exact integer, never a float spelling.
        assert_eq!(row["factor"].as_u64(), Some(u64::from(expected.factor)));
        assert!(row["factor"].is_u64(), "{row}");
        // A scaled symbol multiplies a base unit and is not one itself.
        assert!(base_units.contains(&expected.base), "{row}");
        assert!(!base_units.contains(&expected.symbol), "{row}");
    }

    let text = oce(&["vocab"], b"");
    assert_eq!(text.code, 0);
    assert!(text.stdout_text().contains(
        "\nscale factors (time only; one symbol is exactly this many base units):\n  \
         min  60 s\n  h    3600 s\n  d    86400 s\n\n"
    ));
}

// --- determinism ---

#[test]
fn every_vocabulary_surface_is_the_same_bytes_on_a_second_run() {
    let surfaces: &[&[&str]] = &[
        &["catalog"],
        &["catalog", "--json"],
        &["catalog", "CDL.Logical.TrueDelay", "--param", "delayTime=5"],
        &[
            "catalog",
            "CDL.Logical.TrueDelay",
            "--param",
            "delayTime=5",
            "--json",
        ],
        &["catalog", "CDL.Logical.TrueHoldWithReset"],
        &["catalog", "CDL.Logical.TrueHoldWithReset", "--json"],
        &[
            "catalog",
            "CDL.Reals.MultiSum",
            "--param",
            "nin=3",
            "--json",
        ],
        &["vocab"],
        &["vocab", "--json"],
    ];
    for args in surfaces {
        let (first, second) = (oce(args, b""), oce(args, b""));
        assert_eq!(first.code, 0, "{args:?}");
        assert!(!first.stdout.is_empty(), "{args:?}");
        assert_eq!(first.stdout, second.stdout, "{args:?}");
        assert_eq!(first.stderr, second.stderr, "{args:?}");
        assert_eq!(first.code, second.code, "{args:?}");
    }
}
