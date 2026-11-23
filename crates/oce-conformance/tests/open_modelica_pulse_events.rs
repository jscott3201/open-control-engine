//! The settled-value rule for event inputs fed by one-frame pulses (issue #800, design note
//! `docs/design/pulse-event-inputs.md`, option (g)) against OpenModelica 1.25.1 running the
//! pinned Buildings classes (not transcriptions).
//!
//! Three pilots in `tools/openmodelica-event-reference/events/` (`regenerate.sh ... pulses`):
//! `PulseConsumersPilot` (every event consumer behind `Change(u)` with `u` changing at 2, 3, 5,
//! 7, 8 and 9 s, `Not(Change)`, `Or(Change, level)`, the fallback `Or(Change, Pre(b))`, and
//! `Latch`/`Toggle` behind `Change` of a signal true from the start), `SampleTriggerPilot`
//! (`SampleTrigger` at 0.5, 1, 2, 1.4 s and 1 s shifted half a frame, from an instant at t = 0, into
//! every consumer class, `Not(SampleTrigger)` among them) and `SampleTriggerOffGridPilot` (frames
//! at 0.25 + k s, so the start is not an instant; `Pre(SampleTrigger)` as a fallback cone). The
//! engine runs the same blocks wired the same way (`pulse_event_documents.rs`) at the pilots'
//! frames.
//!
//! Every compared signal equals OpenModelica bit for bit at every frame, except: `tim` of the
//! consumers pilot, within 1e-6 (OpenModelica's table changes land about 1e-10 s after each frame,
//! so its timer reads that much off a whole second); `mean`, within 1e-12 (OpenModelica writes 16
//! significant digits, and a mean of three samples needs 17); and the four registered known divergences
//! (`DVG-000003` to `DVG-000006`, issue #800's ruling 2): a fallback cone `Or(Change(u), Pre(b))`,
//! several sample instants in one frame (0.5 s at 1 s frames), an instant between frames sampled
//! with the frame's value (1.4 s), and the fallback `Pre(SampleTrigger)`. Those columns equal the
//! committed engine tables (`<stem>.engine.csv`, the register's engine evidence) instead. This file
//! is in the `conformance` binary, which no per-PR job runs (nightly `release-gate.yml` and
//! `gate.sh full`).

use std::collections::BTreeMap;

use oce_api::{Engine, Value};
use oce_conformance::CombiTimeTable;

use crate::open_modelica_event_triggers::{IMAGE, columns, sha256};
use crate::pulse_event_documents::{self, Doc};
use crate::pulse_event_init_documents;

const DIR: &str = "fixtures/open_modelica/pulse_events";
const REGISTER: &str = include_str!("fixtures/known_divergence/register.json");
const TEMPLATE: &[u8] =
    include_bytes!("../../../tools/openmodelica-event-reference/events/simulate_pulses.mos");

/// How one compared column must relate to OpenModelica.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Expect {
    /// Bit for bit at every frame.
    Exact,
    /// Within this absolute difference at every frame.
    Within(f64),
    /// A registered known divergence: the engine equals its committed table, and differs from
    /// OpenModelica at these frames.
    Registered(&'static str, &'static [usize]),
}

struct Pilot {
    stem: &'static str,
    doc: fn() -> Doc,
    canonical: &'static str,
    engine: &'static str,
    raw: &'static [u8],
    provenance: &'static str,
    source: &'static [u8],
    expect: &'static [(&'static str, Expect)],
}

macro_rules! fixture {
    ($stem:literal, $ext:literal) => {
        include_str!(concat!("fixtures/open_modelica/pulse_events/", $stem, $ext))
    };
}

/// A pilot of the review round of GitHub #805, its files named by `stem` and its `.mo` by `mo`.
macro_rules! init_pilot {
    ($stem:literal, $doc:expr, $mo:literal, $expect:expr) => {
        Pilot {
            stem: $stem,
            doc: $doc,
            canonical: fixture!($stem, ".canonical.csv"),
            engine: fixture!($stem, ".engine.csv"),
            raw: include_bytes!(concat!(
                "fixtures/open_modelica/pulse_events/",
                $stem,
                ".raw.csv"
            )),
            provenance: fixture!($stem, ".provenance.json"),
            source: include_bytes!(concat!(
                "../../../tools/openmodelica-event-reference/events/",
                $mo,
                ".mo"
            )),
            expect: $expect,
        }
    };
}

fn pilots() -> [Pilot; 9] {
    use Expect::{Exact, Registered, Within};
    [
        Pilot {
            stem: "consumers",
            doc: pulse_event_documents::consumers,
            canonical: fixture!("consumers", ".canonical.csv"),
            engine: fixture!("consumers", ".engine.csv"),
            raw: include_bytes!("fixtures/open_modelica/pulse_events/consumers.raw.csv"),
            provenance: fixture!("consumers", ".provenance.json"),
            source: include_bytes!(
                "../../../tools/openmodelica-event-reference/events/PulseConsumersPilot.mo"
            ),
            expect: &[
                ("sampler", Exact),
                ("maxer", Exact),
                ("mean", Within(1e-12)),
                ("integ", Exact),
                ("pid", Exact),
                ("lat", Exact),
                ("tog", Exact),
                ("tim", Within(1e-6)),
                ("cntNot", Exact),
                ("cntOr", Exact),
                ("cntPre", Registered("DVG-000003", &[8, 9])),
                ("latX", Exact),
                ("togX", Exact),
            ],
        },
        Pilot {
            stem: "sample_triggers",
            doc: pulse_event_documents::sample_triggers,
            canonical: fixture!("sample_triggers", ".canonical.csv"),
            engine: fixture!("sample_triggers", ".engine.csv"),
            raw: include_bytes!("fixtures/open_modelica/pulse_events/sample_triggers.raw.csv"),
            provenance: fixture!("sample_triggers", ".provenance.json"),
            source: include_bytes!(
                "../../../tools/openmodelica-event-reference/events/SampleTriggerPilot.mo"
            ),
            expect: &[
                (
                    "c05",
                    Registered("DVG-000004", &[1, 2, 3, 4, 5, 6, 7, 8, 9]),
                ),
                ("c1", Exact),
                ("c2", Exact),
                ("c14", Exact),
                ("c1h", Exact),
                ("t05", Exact),
                ("t1", Exact),
                ("t2", Exact),
                ("t14", Registered("DVG-000005", &[2, 5, 9])),
                ("tNot", Exact),
                ("lat", Exact),
                ("mean", Within(1e-12)),
                ("maxer", Exact),
                ("integ", Exact),
                ("pid", Exact),
                ("tim", Exact),
                ("tog", Exact),
                ("latHol", Exact),
            ],
        },
        Pilot {
            stem: "sample_trigger_off_grid",
            doc: pulse_event_documents::sample_trigger_off_grid,
            canonical: fixture!("sample_trigger_off_grid", ".canonical.csv"),
            engine: fixture!("sample_trigger_off_grid", ".engine.csv"),
            raw: include_bytes!(
                "fixtures/open_modelica/pulse_events/sample_trigger_off_grid.raw.csv"
            ),
            provenance: fixture!("sample_trigger_off_grid", ".provenance.json"),
            source: include_bytes!(
                "../../../tools/openmodelica-event-reference/events/SampleTriggerOffGridPilot.mo"
            ),
            expect: &[
                ("c1", Exact),
                ("t1", Exact),
                ("lat", Exact),
                (
                    "cPre",
                    Registered("DVG-000006", &[1, 2, 3, 4, 5, 6, 7, 8, 9]),
                ),
            ],
        },
        init_pilot!(
            "init_negated",
            pulse_event_init_documents::init_negated,
            "InitNegatedPilot",
            &[
                ("timC1", Within(1e-6)),
                ("timC0", Within(1e-6)),
                ("timS", Within(1e-6)),
                ("meanC1", Within(1e-12)),
                ("meanC0", Within(1e-12)),
                ("meanS", Within(1e-12)),
                (
                    "integS",
                    Registered("DVG-000010", &[1, 2, 3, 4, 5, 6, 7, 8, 9])
                ),
                ("pidS", Registered("DVG-000010", &[1, 2, 3, 4, 5, 6, 8])),
            ]
        ),
        init_pilot!(
            "init_negated_off_grid",
            pulse_event_init_documents::init_negated,
            "InitNegatedOffGridPilot",
            &[
                ("timC1", Registered("DVG-000011", &[3, 4, 5, 6, 7, 8, 9])),
                ("timC0", Registered("DVG-000011", &[3, 4, 5, 6, 7, 8, 9])),
                ("timS", Within(1e-6)),
                ("meanC1", Within(1e-12)),
                ("meanC0", Within(1e-12)),
                ("meanS", Within(1e-12)),
                (
                    "integS",
                    Registered("DVG-000010", &[1, 2, 3, 4, 5, 6, 7, 8, 9])
                ),
                ("pidS", Registered("DVG-000010", &[1, 2, 3, 4, 5, 6, 8])),
            ]
        ),
        init_pilot!(
            "init_mixtures",
            pulse_event_init_documents::init_mixtures,
            "InitMixturesPilot",
            &[
                ("cntNotPre", Registered("DVG-000007", &[0])),
                ("latNotPre", Registered("DVG-000007", &[0])),
                ("togNotPre", Registered("DVG-000007", &[0])),
            ]
        ),
        init_pilot!(
            "init_mixtures_off_grid",
            pulse_event_init_documents::init_mixtures,
            "InitMixturesOffGridPilot",
            &[
                ("cntNotPre", Registered("DVG-000007", &[0])),
                ("latNotPre", Registered("DVG-000007", &[0])),
                ("togNotPre", Registered("DVG-000007", &[0])),
            ]
        ),
        init_pilot!(
            "latch_clear",
            pulse_event_init_documents::latch_clear,
            "LatchClearPilot",
            &[
                ("meanD", Within(1e-12)),
                ("latD", Registered("DVG-000008", &[1, 3, 5, 7, 9])),
                ("togD", Registered("DVG-000008", &[1, 2, 5, 6, 9])),
                ("samD", Registered("DVG-000009", &[2])),
                ("integD", Registered("DVG-000009", &[2])),
                ("pidD", Registered("DVG-000009", &[2, 5])),
            ]
        ),
        init_pilot!(
            "mixed_fallback",
            pulse_event_init_documents::mixed_fallback,
            "MixedFallbackPilot",
            &[
                ("togC", Registered("DVG-000012", &[1])),
                ("togD", Registered("DVG-000008", &[1, 7])),
            ]
        ),
    ]
}

fn number(value: &Value) -> f64 {
    match value {
        Value::Real(x) => *x,
        Value::Integer(n) => *n as f64,
        Value::Boolean(b) => f64::from(u8::from(*b)),
        other => panic!("unexpected output {other:?}"),
    }
}

/// The engine's outputs at the reference's frames, fed its input columns: one map per frame.
fn run(doc: &Doc, reference: &CombiTimeTable) -> Vec<BTreeMap<String, f64>> {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(doc.cxf().as_bytes())
        .expect("the pilot's model loads");
    assert!(
        report.warnings.is_empty(),
        "{}: {:?}",
        doc.model,
        report.warnings
    );
    let (names, rows) = columns(reference);
    let prefix = format!("{}.", doc.id());
    let output = format!("{prefix}y_");
    rows.iter()
        .map(|row| {
            let points: Vec<(String, Value)> = doc
                .inputs
                .iter()
                .map(|(name, kind)| {
                    let x = row[names.iter().position(|n| n == name).expect(name)];
                    let value = if *kind == "Boolean" {
                        Value::Boolean(x == 1.0)
                    } else {
                        Value::Real(x)
                    };
                    (format!("{prefix}{name}"), value)
                })
                .collect();
            let entries: Vec<(&str, Value)> = points
                .iter()
                .map(|(p, v)| (p.as_str(), v.clone()))
                .collect();
            let frame = engine.prepare_frame(row[0], &entries).expect("prepares");
            let done = engine.execute_frame(frame).expect("executes");
            done.outputs()
                .iter()
                .map(|(path, value)| (path.trim_start_matches(&output).to_owned(), number(value)))
                .collect()
        })
        .collect()
}

/// The committed engine table text for `pilot`'s outputs.
fn engine_table(doc: &Doc, frames: &[f64], outputs: &[BTreeMap<String, f64>]) -> String {
    let names: Vec<&str> = doc.outputs.iter().map(|(n, _, _)| *n).collect();
    let mut text = format!("time,{}\n", names.join(","));
    for (t, row) in frames.iter().zip(outputs) {
        let cells: Vec<String> = names.iter().map(|n| format!("{:?}", row[*n])).collect();
        text += &format!("{t:?},{}\n", cells.join(","));
    }
    text
}

#[test]
fn the_engine_equals_openmodelica_on_every_shape_the_settled_value_rule_claims() {
    for pilot in pilots() {
        let reference = CombiTimeTable::parse(pilot.canonical).expect("canonical table");
        let (names, rows) = columns(&reference);
        let doc = (pilot.doc)();
        let engine = run(&doc, &reference);
        let frames: Vec<f64> = rows.iter().map(|r| r[0]).collect();
        let table = engine_table(&doc, &frames, &engine);
        if std::env::var_os("OCE_BLESS_PULSE_EVENTS").is_some() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join(DIR)
                .join(format!("{}.engine.csv", pilot.stem));
            std::fs::write(path, &table).expect("bless");
            continue;
        } else {
            assert_eq!(
                table, pilot.engine,
                "{}: the engine table moved",
                pilot.stem
            );
        }
        // Every compared output: `Exact` unless the pilot lists it.
        for (listed, _) in pilot.expect {
            assert!(
                doc.outputs.iter().any(|(n, _, _)| n == listed),
                "{}: {listed} is no output",
                pilot.stem
            );
        }
        for (column, _, _) in &doc.outputs {
            let expect = pilot
                .expect
                .iter()
                .find(|(n, _)| n == column)
                .map_or(&Expect::Exact, |(_, e)| e);
            let om = names.iter().position(|n| n == column).expect(column);
            let differ: Vec<usize> = (0..rows.len())
                .filter(|&k| {
                    let (a, b) = (engine[k][*column], rows[k][om]);
                    match expect {
                        Expect::Within(tol) => (a - b).abs() > *tol,
                        _ => a.to_bits() != b.to_bits(),
                    }
                })
                .collect();
            match expect {
                Expect::Exact | Expect::Within(_) => assert!(
                    differ.is_empty(),
                    "{} {column}: the engine parts from OpenModelica at frames {differ:?}: \
                     engine {:?}, OpenModelica {:?}",
                    pilot.stem,
                    engine.iter().map(|r| r[*column]).collect::<Vec<_>>(),
                    rows.iter().map(|r| r[om]).collect::<Vec<_>>()
                ),
                Expect::Registered(id, frames) => {
                    assert_eq!(&differ, frames, "{} {column} ({id})", pilot.stem);
                    assert!(REGISTER.contains(&format!(r#""id": "{id}""#)), "{id}");
                }
            }
        }
        // A second run repeats every value bit for bit.
        assert_eq!(engine_table(&doc, &frames, &run(&doc, &reference)), table);
    }
}

#[test]
fn every_registered_pulse_divergence_binds_both_tables_by_digest() {
    for pilot in pilots() {
        if pilot
            .expect
            .iter()
            .any(|(_, e)| matches!(e, Expect::Registered(..)))
        {
            assert!(
                REGISTER.contains(&sha256(pilot.engine.as_bytes())),
                "{}",
                pilot.stem
            );
            assert!(
                REGISTER.contains(&sha256(pilot.canonical.as_bytes())),
                "{}",
                pilot.stem
            );
        }
    }
}

/// The vendored copy of a class source a pilot's record names.
fn vendored(path: &str) -> &'static [u8] {
    macro_rules! cdl {
        ($($rel:literal),* $(,)?) => {
            match path {
                $(concat!("Buildings/Controls/OBC/CDL/", $rel) => include_bytes!(concat!(
                    "../../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/",
                    $rel
                )) as &[u8],)*
                other => panic!("no vendored copy of {other}"),
            }
        };
    }
    cdl!(
        "Logical/Change.mo",
        "Discrete/TriggeredSampler.mo",
        "Discrete/TriggeredMax.mo",
        "Discrete/TriggeredMovingMean.mo",
        "Reals/IntegratorWithReset.mo",
        "Reals/PIDWithReset.mo",
        "Logical/Latch.mo",
        "Logical/Toggle.mo",
        "Logical/TimerAccumulating.mo",
        "Integers/OnCounter.mo",
        "Logical/Not.mo",
        "Logical/Or.mo",
        "Logical/Pre.mo",
        "Logical/Sources/SampleTrigger.mo",
        "Logical/And.mo",
        "Logical/Edge.mo",
        "Logical/Xor.mo",
        "Conversions/BooleanToReal.mo",
        "Logical/Nor.mo",
    )
}

#[test]
fn each_pulse_pilot_s_record_binds_the_pinned_run_the_classes_and_its_tables() {
    for pilot in pilots() {
        let record: serde_json::Value =
            serde_json::from_str(pilot.provenance).expect("provenance JSON");
        let field = |key: &str| record[key].as_str().expect(key).to_owned();
        for (key, value) in [
            ("omc", "OpenModelica 1.25.1"),
            ("image", IMAGE),
            ("architecture", "arm64"),
            (
                "buildings_commit",
                "a131864e4c4df22ebcd52bb8da439de0087ac365",
            ),
            ("buildings_tree", "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09"),
            ("msl_commit", "7a4bf7de77a3986e8eb1e88cbb515d646f78f834"),
            ("msl_tree", "43d7d8fc1a991358e9e5e91976e27cdc4280173f"),
        ] {
            assert_eq!(field(key), value, "{} {key}", pilot.stem);
        }
        assert_eq!(
            field("simulate_template_sha256"),
            sha256(TEMPLATE),
            "{}",
            pilot.stem
        );
        assert_eq!(
            field("pilot_sha256"),
            sha256(pilot.source),
            "{}",
            pilot.stem
        );
        assert_eq!(field("raw_sha256"), sha256(pilot.raw), "{}", pilot.stem);
        assert_eq!(
            field("canonical_sha256"),
            sha256(pilot.canonical.as_bytes()),
            "{}",
            pilot.stem
        );
        let sources = record["class_sources"].as_object().expect("class_sources");
        assert!(!sources.is_empty(), "{}", pilot.stem);
        for (path, digest) in sources {
            assert_eq!(
                digest.as_str(),
                Some(sha256(vendored(path)).as_str()),
                "{path}"
            );
        }
        // The pilot loads the Buildings library, not a transcription.
        let text = std::str::from_utf8(pilot.source).expect("UTF-8");
        assert!(
            text.contains("Buildings.Controls.OBC.CDL."),
            "{}",
            pilot.stem
        );
        assert!(!text.contains("equation\n    y = "), "{}", pilot.stem);
    }
}
