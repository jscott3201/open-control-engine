//! What reading the upstream unit spellings changes for the host, on the vendored modelica-json
//! translations that load (issue #631; design note `docs/design/unit-spellings.md`, decision 4).
//!
//! Seven of the 44 translations under `third_party/modelica-buildings-cdl/cxf/` load. They write
//! their units as `qudt:hasUnit` and `S231:hasDisplayUnit` IRI nodes, which the loader left
//! unread: every one of their point rows had no unit. Read, 18 rows in five of them gain a unit
//! (`1` on 11, `Pa` on 7) and three of those a display unit (`Pa`), and six content ids move,
//! because those documents now carry the units they always declared.
//!
//! The rows are stated by hand from the documents. Each row below is a connector whose own node
//! carries a `qudt:hasUnit` in the vendored file (the three `conErr` members of `ReliefDamper`),
//! or whose connected cluster holds a node that does: the source of a root output that declares
//! a unit (`swi.y` drives `yRelDam`, declared `unit:UNITLESS`), and the inputs that source also
//! drives. A display unit does not propagate, so only the three declaring `ReliefDamper` rows
//! carry one. A root input's row shows the unit of the port it drives, never the unit it
//! declares, so a root input that declares a unit on a unit-free port gains nothing here, and
//! `TimeSuppression`, whose units all sit on root inputs, changes its content id and no row.
//!
//! The content ids are the engine's own output, pinned so that a change to what these documents
//! read as shows as a one-line diff. They are a determinism pin, not an oracle. They move with a
//! pin advance of the vendored tree, and with any change to the content-id encoding.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oce_api::Engine;

/// The class base every translation's `@id` shares.
const BASE: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.";

/// One translation that loads: its path under the G36 directory, its class path under
/// [`BASE`], how many point rows it has, its rows that carry a unit as
/// `(connector, unit, display unit)`, and its content id.
struct Translation {
    file: &'static str,
    class: &'static str,
    rows: usize,
    with_unit: &'static [(&'static str, &'static str, Option<&'static str>)],
    content_id: &'static str,
}

const TRANSLATIONS: [Translation; 7] = [
    Translation {
        file: "AHUs/MultiZone/VAV/SetPoints/ReliefDamper.jsonld",
        class: "AHUs.MultiZone.VAV.SetPoints.ReliefDamper",
        rows: 13,
        with_unit: &[
            ("conErr.u2", "Pa", Some("Pa")),
            ("conErr.y", "Pa", Some("Pa")),
            ("conP.u_m", "Pa", None),
            // The root input: the row shows `conErr.u1`, the port it drives.
            ("dpBui", "Pa", Some("Pa")),
            ("dpBuiSetPoi.y", "Pa", None),
            ("swi.y", "1", None),
        ],
        content_id: "oce:canon1:76639584ab7b11f0b1a284cc20ee912acfa8e3ead6fb79b5172ac9304e895454",
    },
    Translation {
        file: "AHUs/MultiZone/VAV/SetPoints/ReliefFan.jsonld",
        class: "AHUs.MultiZone.VAV.SetPoints.ReliefFan",
        rows: 46,
        with_unit: &[
            ("div1.u1", "Pa", None),
            ("movMea.y", "Pa", None),
            ("pro1.y", "1", None),
        ],
        content_id: "oce:canon1:34e3a0773acbe3af87d655024aefc014966a17a3b502e39b6331904a60fc20ea",
    },
    Translation {
        file: "AHUs/MultiZone/VAV/SetPoints/SupplySignals.jsonld",
        class: "AHUs.MultiZone.VAV.SetPoints.SupplySignals",
        rows: 24,
        with_unit: &[
            ("conSigCoo.u", "1", None),
            ("conSigCoo.y", "1", None),
            ("conSigHea.u", "1", None),
            ("conSigHea.y", "1", None),
            ("swi.y", "1", None),
        ],
        content_id: "oce:canon1:0cbd23d5a2083c234fb62688868d232cd01c67336715da1f46711d560ba61aef",
    },
    Translation {
        file: "Generic/TimeSuppression.jsonld",
        class: "Generic.TimeSuppression",
        rows: 56,
        with_unit: &[],
        content_id: "oce:canon1:1a6cba312ec0a27ffdd89c4e5507a024cab8b51e0bcb1cb10e73ebda737def8b",
    },
    Translation {
        file: "TerminalUnits/Reheat/Subsequences/Overrides.jsonld",
        class: "TerminalUnits.Reheat.Subsequences.Overrides",
        rows: 26,
        with_unit: &[("pro.y", "1", None), ("swi1.y", "1", None)],
        content_id: "oce:canon1:bdaa46333f5fe0c1dc38dddfc3d69b67f156c4c5c8ff4b4debf69bb6f8ac8df7",
    },
    Translation {
        file: "ThermalZones/ControlLoops.jsonld",
        class: "ThermalZones.ControlLoops",
        rows: 37,
        with_unit: &[("cooConSig.y", "1", None), ("heaConSig.y", "1", None)],
        content_id: "oce:canon1:f709e746dd1f53a43bab53b72aa46c20826e7b3410fb300cef4be96509f3ea50",
    },
    // The seventh carries no unit in any spelling: nothing about it moved.
    Translation {
        file: "ThermalZones/ZoneStates.jsonld",
        class: "ThermalZones.ZoneStates",
        rows: 30,
        with_unit: &[],
        content_id: "oce:canon1:ee958621896550252916b21a9ff69090522c94058c8599a08c61b49eb8dbfa24",
    },
];

fn load(translation: &Translation) -> Engine {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/modelica-buildings-cdl/cxf/Buildings/Controls/OBC/ASHRAE/G36")
        .join(translation.file);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(&bytes)
        .unwrap_or_else(|error| panic!("{} must load: {error}", translation.file));
    assert_eq!(
        report.warnings,
        Vec::new(),
        "{} newly warns",
        translation.file
    );
    engine
}

#[test]
fn eighteen_point_rows_in_five_translations_carry_the_unit_their_document_declares() {
    let mut units: BTreeMap<&str, usize> = BTreeMap::new();
    let mut display_units: BTreeMap<&str, usize> = BTreeMap::new();
    let mut documents_with_a_unit = 0;
    for translation in &TRANSLATIONS {
        let engine = load(translation);
        let mut rows = engine.point_list(None).expect("point inventory");
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(rows.len(), translation.rows, "{}", translation.file);
        let found: Vec<(String, String, Option<String>)> = rows
            .iter()
            .filter(|row| row.unit.is_some() || row.display_unit.is_some())
            .map(|row| {
                (
                    row.path.clone(),
                    row.unit.clone().expect("no row has a display unit alone"),
                    row.display_unit.clone(),
                )
            })
            .collect();
        let mut expected: Vec<(String, String, Option<String>)> = translation
            .with_unit
            .iter()
            .map(|(connector, unit, display)| {
                (
                    format!("{BASE}{}.{connector}", translation.class),
                    (*unit).to_owned(),
                    display.map(str::to_owned),
                )
            })
            .collect();
        expected.sort();
        assert_eq!(found, expected, "{}", translation.file);
        // No quantity anywhere: `qudt:hasQuantityKind` is not read.
        for row in &rows {
            assert_eq!(row.quantity, None, "{}", row.path);
        }
        documents_with_a_unit += usize::from(!translation.with_unit.is_empty());
        for (_, unit, display) in translation.with_unit {
            *units.entry(unit).or_default() += 1;
            if let Some(display) = display {
                *display_units.entry(display).or_default() += 1;
            }
        }
    }
    // The issue's totals: 18 rows in 5 documents, unit `1` on 11 and `Pa` on 7, display `Pa` on 3.
    assert_eq!(documents_with_a_unit, 5);
    assert_eq!(units, BTreeMap::from([("1", 11), ("Pa", 7)]));
    assert_eq!(display_units, BTreeMap::from([("Pa", 3)]));
}

#[test]
fn each_loading_translation_has_its_pinned_content_id() {
    for translation in &TRANSLATIONS {
        assert_eq!(
            load(translation).content_id().expect("content id").as_str(),
            translation.content_id,
            "{}",
            translation.file
        );
    }
}

#[test]
fn a_translation_exports_its_units_in_the_s231_spelling_and_reimports_to_the_same_content() {
    // Decision 3: the exporter writes bare strings under `S231:unit` and `S231:displayUnit`.
    // Before, these exports carried no unit at all.
    let mut complete = Vec::new();
    for translation in TRANSLATIONS
        .iter()
        .filter(|translation| !translation.with_unit.is_empty())
    {
        let engine = load(translation);
        let export = engine.export_cxf().expect("the translation exports");
        let text = std::str::from_utf8(&export.bytes).expect("UTF-8");
        for absent in ["qudt:hasUnit", "hasDisplayUnit", "unit:", "qudt.org"] {
            assert!(
                !text.contains(absent),
                "{} exports `{absent}`",
                translation.file
            );
        }
        // Where the export is the whole model it carries the units, and re-import reads the
        // same strings. A partial export omits the blocks the exporter defers, with a warning
        // for each; that is the exporter's subset, unchanged, and what it omits may be every
        // block that carried a unit.
        if export.complete().is_ok() {
            complete.push(translation.class);
            assert!(text.contains("\"S231:unit\":"), "{}", translation.file);
            let mut again = Engine::new();
            again
                .load_cxf(&export.bytes)
                .unwrap_or_else(|error| panic!("{} re-imports: {error}", translation.file));
            assert_eq!(
                again.content_id().expect("content id"),
                engine.content_id().expect("content id"),
                "{}",
                translation.file
            );
            assert_eq!(
                again.point_list(None).expect("points"),
                engine.point_list(None).expect("points"),
                "{}",
                translation.file
            );
        }
    }
    // Not vacuous: one of the five exports whole. The exports of the other four are partial.
    assert_eq!(complete, ["TerminalUnits.Reheat.Subsequences.Overrides"]);
}
