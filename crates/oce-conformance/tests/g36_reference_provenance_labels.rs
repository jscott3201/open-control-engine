//! Label guards for the G36 Tier-A sequence reference provenance records.
//!
//! Every `tools/golden-gen/goldens/G36/<scenario>/<signal>.prov.json` carries three label fields
//! alongside its source citation:
//!
//! - `sequence_derivation` — `"g36-source-rederivation"` when the reference re-derives the pinned
//!   G36 sequence graph, or `"fixture-local-composition"` when the fixture is a hand-authored
//!   composition of CDL elementary blocks with fixture-local inputs. A fixture-local reference is
//!   not an answer key for any G36 sequence.
//! - `diverges_from_g36_intent` — a note when the reference faithfully reproduces pinned Buildings
//!   behaviour that contradicts the stated G36 intent, otherwise `null`.
//! - `frame_rate_dependence` — a note when a signal's values follow a fixed-step discretization
//!   that the engine shares, otherwise `null`.
//!
//! These tests keep the labels present, well formed, consistent with the G36 reference catalog,
//! and tied to the vendored sources that justify them: if a source pin advance removes the
//! upstream behaviour a note describes, the matching test fails and the note must be revisited.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

const DERIVATION_G36_SOURCE: &str = "g36-source-rederivation";
const DERIVATION_FIXTURE_LOCAL: &str = "fixture-local-composition";
const LABEL_FIELDS: [&str; 3] = [
    "sequence_derivation",
    "diverges_from_g36_intent",
    "frame_rate_dependence",
];
const CDL_SOURCE_PREFIX: &str = "Buildings/Controls/OBC/CDL/";
const G36_SOURCE_PREFIX: &str = "Buildings/Controls/OBC/ASHRAE/G36/";
const VENDORED_BUILDINGS: &str = "third_party/modelica-buildings-cdl";
const G36_CATALOG: &str = "tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.catalog.json";

/// Scenarios whose references reproduce pinned behaviour that diverges from G36 intent.
const REVIEWED_G36_INTENT_DIVERGENCES: [&str; 3] = [
    "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
    "multizone_vav_economizer_enable",
    "multizone_vav_supply_temperature",
];

struct Record {
    path: PathBuf,
    scenario: String,
    value: Value,
}

impl Record {
    fn text(&self, field: &str) -> &str {
        self.value[field]
            .as_str()
            .unwrap_or_else(|| panic!("{} field {field} is not a string", self.path.display()))
    }

    fn source_files(&self) -> Vec<&str> {
        self.text("source_files")
            .split(';')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .collect()
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root resolves")
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read {} failed: {err}", path.display()))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&read_text(path))
        .unwrap_or_else(|err| panic!("parse JSON {} failed: {err}", path.display()))
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("enumerate {} failed: {err}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn g36_records() -> Vec<Record> {
    let root = repository_root().join("tools/golden-gen/goldens/G36");
    let mut records = Vec::new();
    for scenario_dir in sorted_entries(&root) {
        if !scenario_dir.is_dir() {
            continue;
        }
        for path in sorted_entries(&scenario_dir) {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(signal) = name.strip_suffix(".prov.json").map(str::to_string) else {
                continue;
            };
            let value = read_json(&path);
            assert_eq!(value["class_path"], "G36", "{} class_path", path.display());
            let scenario = value["scenario"]
                .as_str()
                .unwrap_or_else(|| panic!("{} has no scenario", path.display()))
                .to_string();
            assert_eq!(
                Some(scenario.as_str()),
                scenario_dir.file_name().and_then(|name| name.to_str()),
                "{} scenario must name its directory",
                path.display()
            );
            assert_eq!(value["signal"], signal, "{} signal", path.display());
            records.push(Record {
                path,
                scenario,
                value,
            });
        }
    }
    assert!(
        records.len() >= 100,
        "expected the full G36 reference corpus, found {} records",
        records.len()
    );
    records
}

fn optional_note<'a>(record: &'a Record, field: &str) -> Option<&'a str> {
    match &record.value[field] {
        Value::Null => None,
        Value::String(note) => {
            assert!(
                !note.trim().is_empty(),
                "{} field {field} must be null rather than an empty note",
                record.path.display()
            );
            Some(note)
        }
        other => panic!(
            "{} field {field} must be a string or null, found {other}",
            record.path.display()
        ),
    }
}

/// Collapse whitespace so multi-line Modelica declarations compare as single-line text.
fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn vendored_source(relative: &str) -> String {
    normalized(&read_text(
        &repository_root().join(VENDORED_BUILDINGS).join(relative),
    ))
}

#[test]
fn every_g36_reference_record_carries_each_label_field_exactly_once() {
    for record in g36_records() {
        let text = read_text(&record.path);
        for field in LABEL_FIELDS {
            assert_eq!(
                text.matches(&format!("\"{field}\":")).count(),
                1,
                "{} must carry {field} exactly once",
                record.path.display()
            );
        }
        let derivation = record.text("sequence_derivation");
        assert!(
            [DERIVATION_G36_SOURCE, DERIVATION_FIXTURE_LOCAL].contains(&derivation),
            "{} has unknown sequence_derivation {derivation}",
            record.path.display()
        );
        optional_note(&record, "diverges_from_g36_intent");
        optional_note(&record, "frame_rate_dependence");
    }
}

#[test]
fn fixture_local_compositions_are_exactly_the_catalog_fixture_only_sequences() {
    let catalog = read_json(&repository_root().join(G36_CATALOG));
    let catalog_names = catalog["fixture_only_sequences"]
        .as_array()
        .expect("catalog lists fixture-only sequences")
        .iter()
        .map(|entry| {
            entry["name"]
                .as_str()
                .expect("fixture-only sequence name")
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    let labelled = g36_records()
        .into_iter()
        .filter(|record| record.text("sequence_derivation") == DERIVATION_FIXTURE_LOCAL)
        .map(|record| record.scenario)
        .collect::<BTreeSet<_>>();
    assert!(
        !catalog_names.is_empty(),
        "catalog has no fixture-only sequences"
    );
    assert_eq!(
        labelled, catalog_names,
        "fixture-local G36 references must be exactly the catalog's fixture-only sequences"
    );
}

#[test]
fn fixture_local_compositions_cite_only_vendored_cdl_block_sources() {
    let root = repository_root();
    for record in g36_records()
        .into_iter()
        .filter(|record| record.text("sequence_derivation") == DERIVATION_FIXTURE_LOCAL)
    {
        let sources = record.source_files();
        assert!(
            !sources.is_empty(),
            "{} cites no source files",
            record.path.display()
        );
        for source in sources {
            assert!(
                source.starts_with(CDL_SOURCE_PREFIX),
                "{} is a fixture-local composition but cites non-CDL source {source}",
                record.path.display()
            );
            assert!(
                root.join(VENDORED_BUILDINGS).join(source).is_file(),
                "{} cites {source}, which is not vendored",
                record.path.display()
            );
        }
        let source = record.text("source");
        assert!(
            source.contains("not derived from") && !source.contains("independent re-derivation"),
            "{} source text must disclaim G36 derivation: {source}",
            record.path.display()
        );
        assert_eq!(
            record.text("fixture_status"),
            "supported-fixture-only fixture-local composition",
            "{} fixture status",
            record.path.display()
        );
    }
}

#[test]
fn source_rederivations_cite_a_g36_sequence_source() {
    for record in g36_records()
        .into_iter()
        .filter(|record| record.text("sequence_derivation") == DERIVATION_G36_SOURCE)
    {
        assert!(
            record
                .source_files()
                .iter()
                .any(|source| source.starts_with(G36_SOURCE_PREFIX)),
            "{} claims a G36 source re-derivation but cites no G36 source",
            record.path.display()
        );
    }
}

#[test]
fn recorded_divergences_are_exactly_the_reviewed_set() {
    let records = g36_records();
    let divergent = records
        .iter()
        .filter(|record| optional_note(record, "diverges_from_g36_intent").is_some())
        .map(|record| record.scenario.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        divergent,
        BTreeSet::from(REVIEWED_G36_INTENT_DIVERGENCES),
        "G36-intent divergence labels changed; review the notes and this list together"
    );
    for scenario in REVIEWED_G36_INTENT_DIVERGENCES {
        assert!(
            records
                .iter()
                .filter(|record| record.scenario == scenario)
                .all(|record| optional_note(record, "diverges_from_g36_intent").is_some()),
            "every {scenario} signal must carry its divergence note"
        );
    }
}

#[test]
fn economizer_enable_divergence_matches_the_vendored_or_wiring() {
    let enable = vendored_source(
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo",
    );
    for expected in [
        "Buildings.Controls.OBC.CDL.Logical.Or or2",
        "connect(hysOutTem.y, or2.u1)",
        "connect(or2.y, truFalHol.u)",
        "connect(truFalHol.y, and1.u1)",
        "connect(and1.y, andEnaDis.u1)",
        "The economizer is disabled whenever the outdoor air conditions exceed the economizer high limit setpoint.",
    ] {
        assert!(
            enable.contains(expected),
            "vendored Enable.mo no longer contains `{expected}`; revisit the \
             diverges_from_g36_intent notes for the economizer enable and controller references"
        );
    }
    assert!(
        !enable.contains("Logical.Nor"),
        "vendored Enable.mo now uses Nor; revisit the economizer divergence notes"
    );
}

#[test]
fn supply_temperature_divergence_matches_the_vendored_mode_constants() {
    let supply_temperature = vendored_source(
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
    );
    for expected in [
        "Buildings.Controls.OBC.CDL.Integers.Sources.Constant cooDowMod( final k=3)",
        "Buildings.Controls.OBC.CDL.Integers.LessThreshold intLesThr2( final t=3)",
        "Buildings.Controls.OBC.CDL.Integers.GreaterThreshold intGreThr2( final t=0)",
        "connect(cooDowMod.y, intEqu.u2)",
    ] {
        assert!(
            supply_temperature.contains(expected),
            "vendored SupplyTemperature.mo no longer contains `{expected}`; revisit the \
             diverges_from_g36_intent note for multizone_vav_supply_temperature"
        );
    }

    let catalog = read_json(&repository_root().join(G36_CATALOG));
    let operation_modes = catalog["g36_types"]["integer_constant_packages"]
        .as_array()
        .expect("catalog lists integer constant packages")
        .iter()
        .find(|entry| {
            entry["class_path"] == "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes"
        })
        .expect("catalog pins OperationModes");
    assert_eq!(operation_modes["constants"]["coolDown"], 2);
    assert_eq!(operation_modes["constants"]["setUp"], 3);
}

#[test]
fn supply_temperature_nested_trim_and_respond_literals_are_a_fixture_deviation() {
    // The .mo binds the nested parameters to the outer ones; the CXF fixture writes literals.
    // The divergence note says so, and that the literals equal the outer values today.
    let supply_temperature = vendored_source(
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
    );
    let fixture = read_json(
        &repository_root()
            .join("crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld"),
    );
    let root = "http://example.org#g36.source.multizone_vav_supply_temperature";
    let parameter = |id: String| -> String {
        let node = fixture["@graph"]
            .as_array()
            .expect("fixture graph")
            .iter()
            .find(|node| node["@id"] == id.as_str())
            .unwrap_or_else(|| panic!("fixture has no parameter {id}"));
        match &node["S231:value"] {
            Value::String(text) => text.clone(),
            value => value["@value"]
                .as_str()
                .unwrap_or_else(|| panic!("{id} has no literal value"))
                .to_string(),
        }
    };
    for (name, outer) in [
        ("delTim", "delTim"),
        ("samplePeriod", "samplePeriod"),
        ("numIgnReq", "numIgnReq"),
        ("triAmo", "triAmo"),
        ("resAmo", "resAmo"),
        ("maxRes", "maxRes"),
        ("iniSet", "TSupCoo_max"),
        ("minSet", "TSupCoo_min"),
        ("maxSet", "TSupCoo_max"),
    ] {
        assert!(
            supply_temperature.contains(&format!("final {name}={name},"))
                || supply_temperature.contains(&format!("final {name}={name})")),
            "vendored SupplyTemperature.mo no longer binds maxSupTemRes.{name}; revisit the \
             multizone_vav_supply_temperature divergence note"
        );
        let nested = parameter(format!("{root}.maxSupTemRes.{name}"));
        assert!(
            nested.parse::<f64>().is_ok(),
            "fixture maxSupTemRes.{name} is now `{nested}`, not a literal; revisit the note"
        );
        assert_eq!(
            nested,
            parameter(format!("{root}.{outer}")),
            "fixture maxSupTemRes.{name} literal no longer equals the outer {outer}; the note's \
             claim that the trace is unaffected is stale"
        );
    }
    let note = g36_records()
        .into_iter()
        .find(|record| record.scenario == "multizone_vav_supply_temperature")
        .and_then(|record| optional_note(&record, "diverges_from_g36_intent").map(str::to_string))
        .expect("supply temperature divergence note");
    assert!(
        note.contains("final delTim=delTim") && note.contains("CXF fixture sets them as literal"),
        "the supply temperature note must attribute the nested literals to the fixture: {note}"
    );
}
