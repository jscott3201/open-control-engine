//! The text listing of `oce catalog CLASS` held to its JSON report, line for line, over every
//! class (vocabulary note decision 12, GitHub issue #500).
//!
//! The two are separate renderings of one view, so each checks the other: the test writes the
//! text it expects from the JSON alone and requires the binary's text to be exactly that. A
//! line the text drops, adds, reorders or spells differently (a port's `unit:` line, a range, a
//! note) fails here for the class that shows it. Golden transcripts pin a class whose ports
//! declare units, as text and as JSON.

mod support;

use oce_api::class_metadata;
use serde_json::Value;
use support::{case, describe};

#[test]
fn catalog_describes_ports_with_declared_units_as_text_and_json() {
    let class = ["catalog", "CDL.Psychrometrics.DewPoint_TDryBulPhi"];
    case("catalog_dew_point", 0, &class, b"");
    let json = [&class[..], &["--json"]].concat();
    case("catalog_dew_point_json", 0, &json, b"");
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is a string in {value}"))
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` is a list in {value}"))
}

/// The indented lines under a parameter or a port: its description, then the declared unit and
/// quantity on one line. Nothing for an empty field.
fn meaning_lines(entry: &Value, out: &mut String) {
    let description = string(entry, "description");
    if !description.is_empty() {
        out.push_str(&format!("      {description}\n"));
    }
    let declared: Vec<String> = ["unit", "quantity"]
        .iter()
        .filter(|key| !string(entry, key).is_empty())
        .map(|key| format!("{key}: {}", string(entry, key)))
        .collect();
    if !declared.is_empty() {
        out.push_str(&format!("      {}\n", declared.join("; ")));
    }
}

/// A parameter's default as the text listing spells it, from the report's `{kind, value}`.
fn default_text(default: &Value) -> String {
    let value = &default["value"];
    match string(default, "kind") {
        "real" => format!("= {:?}", value.as_f64().expect("a real default")),
        "integer" => format!("= {}", value.as_i64().expect("an integer default")),
        "boolean" => format!("= {}", value.as_bool().expect("a boolean default")),
        "string" => format!("= {:?}", value.as_str().expect("a string default")),
        "member" => format!("= {}", value.as_str().expect("a member default")),
        "derived" => format!("(derived: {})", value.as_str().expect("a formula")),
        "required" => "(required)".to_owned(),
        other => panic!("unknown default kind {other}"),
    }
}

/// The whole text listing of a class, written from its JSON report alone.
fn text_of(class: &Value) -> String {
    let mut out = format!(
        "{}\n  {}\n",
        string(class, "class_path"),
        string(class, "description")
    );
    out.push_str("\nparameters:\n");
    if array(class, "parameters").is_empty() {
        out.push_str("  (none)\n");
    }
    for parameter in array(class, "parameters") {
        // An enumeration is spelled by its type name, the last segment of its class path.
        let type_name = parameter["enumeration"].as_str().map_or_else(
            || string(parameter, "type"),
            |path| path.rsplit('.').next().unwrap_or(path),
        );
        let settable = parameter["settable"].as_bool().expect("settable");
        out.push_str(&format!(
            "  {} : {type_name} {}{}\n",
            string(parameter, "name"),
            default_text(&parameter["default"]),
            if settable { "" } else { " [not settable]" }
        ));
        meaning_lines(parameter, &mut out);
        let range: Vec<&str> = array(parameter, "range")
            .iter()
            .map(|line| line.as_str().expect("a range line"))
            .collect();
        if !range.is_empty() {
            out.push_str(&format!("      range: {}\n", range.join("; ")));
        }
    }
    for side in ["inputs", "outputs"] {
        out.push_str(&format!("\n{side}:\n"));
        if array(class, side).is_empty() {
            out.push_str("  (none)\n");
        }
        for port in array(class, side) {
            out.push_str(&format!(
                "  {} : {}\n",
                string(port, "name"),
                string(port, "kind")
            ));
            meaning_lines(port, &mut out);
        }
    }
    out.push_str("\nnotes:\n");
    if array(class, "notes").is_empty() {
        out.push_str("  (none)\n");
    }
    for note in array(class, "notes") {
        let (kind, subject, text) = (
            string(note, "kind"),
            string(note, "subject"),
            string(note, "text"),
        );
        if subject.is_empty() {
            out.push_str(&format!("  {kind}: {text}\n"));
        } else {
            out.push_str(&format!("  {kind} ({subject}): {text}\n"));
        }
    }
    out.push_str("\nlinks:\n");
    if class["links"].is_null() {
        out.push_str("  (none)\n");
    } else {
        out.push_str(&format!(
            "  source at the pinned Buildings commit: {}\n",
            string(&class["links"], "pinned_source")
        ));
        out.push_str(&format!(
            "  help in the latest Buildings release (may drift from the pin): {}\n",
            string(&class["links"], "latest_help")
        ));
    }
    out
}

/// How many entries of `entries` declare a unit, and how many a quantity.
fn declared(entries: &[Value]) -> (usize, usize) {
    let count = |key| {
        entries
            .iter()
            .filter(|entry| !string(entry, key).is_empty())
            .count()
    };
    (count("unit"), count("quantity"))
}

/// The classes `support::describe` does not resolve, in catalog order.
const UNRESOLVED: [&str; 3] = [
    "CDL.Reals.Sources.TimeTable",
    "CDL.Logical.Sources.TimeTable",
    "CDL.Integers.Sources.TimeTable",
];

#[test]
fn the_text_of_every_class_is_exactly_what_its_json_says() {
    let (mut classes, mut unresolved) = (0, Vec::new());
    let (mut parameter_units, mut port_units) = ((0, 0), (0, 0));
    let (mut ranges, mut notes, mut unlinked) = (0, 0, 0);
    for meta in class_metadata() {
        let text = describe(meta, false);
        if text.code != 0 {
            // The ports do not resolve at this table: the report is printed, not the listing.
            unresolved.push(meta.class_path);
            continue;
        }
        let class = describe(meta, true).report()["class"].clone();
        assert_eq!(text.stdout_text(), text_of(&class), "{}", meta.class_path);
        classes += 1;

        let add = |total: &mut (usize, usize), (units, quantities): (usize, usize)| {
            total.0 += units;
            total.1 += quantities;
        };
        add(&mut parameter_units, declared(array(&class, "parameters")));
        add(&mut port_units, declared(array(&class, "inputs")));
        add(&mut port_units, declared(array(&class, "outputs")));
        ranges += array(&class, "parameters")
            .iter()
            .filter(|parameter| !array(parameter, "range").is_empty())
            .count();
        notes += array(&class, "notes").len();
        unlinked += usize::from(class["links"].is_null());
    }
    // The three `TimeTable` sources need a whole table, which the shared parameter table does
    // not write. Their JSON is still held to the vocabulary in `vocabulary.rs`.
    assert_eq!(unresolved, UNRESOLVED);
    assert_eq!(classes, 133);
    // Volume pins: the comparison must have had every kind of line to compare, ports with a
    // declared unit above all. All 21 connectors that declare a unit and all 11 that declare a
    // quantity (the `oce-vocab` README's coverage table) are ports of the classes compared; so
    // are 47 of the 52 parameters with a unit and 39 of the 41 with a quantity (the rest are
    // the `TimeTable` sources' `timeScale` and `period`).
    assert_eq!(port_units, (21, 11));
    assert_eq!(parameter_units, (47, 39));
    // 96 of the 101 parameters with a range, all 13 notes, and the four classes without links
    // (the three reserved identities and `TrueHoldWithReset`).
    assert_eq!((ranges, notes, unlinked), (96, 13, 4));
}

#[test]
fn the_expected_text_spells_each_kind_of_line() {
    // The writer the comparison rests on, on a report written here: every optional line present
    // in one entry and absent in the next, so a line it forgot cannot hide behind the binary.
    let class = serde_json::json!({
        "class_path": "P.C",
        "description": "A class",
        "inputs": [
            { "description": "In", "kind": "Real", "name": "u", "quantity": "Time", "unit": "s" },
            { "description": "", "kind": "Boolean", "name": "v", "quantity": "", "unit": "1" },
            { "description": "", "kind": "Integer", "name": "w", "quantity": "Q", "unit": "" },
        ],
        "links": null,
        "notes": [
            { "kind": "loop", "subject": "", "text": "About the class." },
            { "kind": "pitfall", "subject": "k", "text": "About k." },
        ],
        "outputs": [],
        "parameters": [
            { "default": { "kind": "real", "value": 1e-13 }, "description": "Gain",
              "enumeration": null, "name": "k", "quantity": "", "range": ["k >= 0.0", "k <= m"],
              "settable": true, "type": "Real", "unit": "1" },
            { "default": { "kind": "member", "value": "PI" }, "description": "",
              "enumeration": "CDL.Types.SimpleController", "name": "t", "quantity": "",
              "range": [], "settable": false, "type": "Enum", "unit": "" },
            { "default": { "kind": "derived", "value": "size(K, 2)" }, "description": "",
              "enumeration": null, "name": "n", "quantity": "", "range": [], "settable": true,
              "type": "Integer", "unit": "" },
            { "default": { "kind": "required", "value": null }, "description": "",
              "enumeration": null, "name": "r", "quantity": "", "range": [], "settable": true,
              "type": "Boolean", "unit": "" },
        ],
        "reserved": false,
    });
    assert_eq!(
        text_of(&class),
        "P.C\n  A class\n\
         \nparameters:\n\
         \x20 k : Real = 1e-13\n      Gain\n      unit: 1\n      range: k >= 0.0; k <= m\n\
         \x20 t : SimpleController = PI [not settable]\n\
         \x20 n : Integer (derived: size(K, 2))\n\
         \x20 r : Boolean (required)\n\
         \ninputs:\n\
         \x20 u : Real\n      In\n      unit: s; quantity: Time\n\
         \x20 v : Boolean\n      unit: 1\n\
         \x20 w : Integer\n      quantity: Q\n\
         \noutputs:\n  (none)\n\
         \nnotes:\n  loop: About the class.\n  pitfall (k): About k.\n\
         \nlinks:\n  (none)\n"
    );
}
