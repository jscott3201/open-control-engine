//! Trigger timing of the five event-triggered CDL classes against OpenModelica (issue #670).
//!
//! `OnCounter`, `TriggeredSampler`, `TriggeredMax`, `IntegratorWithReset` and `PIDWithReset`
//! (PI) each ran in the pinned OpenModelica 1.25.1 container from a pilot model under
//! `tools/openmodelica-event-reference/events/`, in two scenarios at 1 s frames with every input
//! held: the trigger true from the initial instant, falling and rising again
//! (`initially_true`), and the control, the trigger false at the initial instant and rising at
//! the first frame after it (`rising_after_start`). `canonicalize.py` kept, at each frame
//! instant, the last row OpenModelica wrote within 1e-6 s of it: the values after any event
//! there. The fixtures are under `fixtures/open_modelica/<family>_<class>/`; no container runs
//! here.
//!
//! Every output is exact in both tools, so every comparison is exact: the counts, samples and
//! maxima are discrete, the integrator's input is 0, and the PI's integral time is 1e20 s, so
//! its integral moves by less than half an ulp per frame and its output is the proportional
//! part plus the held or reset integral. What that leaves out: `IntegratorWithReset`'s
//! integration is `open_modelica_integrator_ramp.rs`, `PIDWithReset`'s the Building 33
//! comparison of `oce-verify` (issue #787), and a trigger fed by a pulse that is true on
//! consecutive frames, where the engine and OpenModelica disagree, is
//! `open_modelica_event_pulses.rs` (issue #800).
//!
//! - The engine, driven through the facade at the ten frame instants with the reference's
//!   inputs, equals OpenModelica at every frame of the ten scenarios, twice over.
//! - An independent recurrence of each class under the rule of `docs/execution-profile.md`
//!   (no event at frame 0; an event shows at the frame where its condition rises) equals
//!   OpenModelica's output at every frame, and each of the two readings the rule replaced
//!   (an event at frame 0 for a trigger already true; an event shown one frame late) differs
//!   from it at pinned frames: the evidence tells the rules apart in every class.
//! - The provenance record of each class binds the pilot, the canonical tables, the raw result
//!   and the simulation script by SHA-256, names the pinned image, `omc` version and library
//!   commits and trees, and records the digest of the class source it simulated, which equals
//!   the vendored copy's.

use oce_conformance::{CombiTimeTable, ComparisonResult, DriveMode};
use sha2::{Digest, Sha256};

use crate::block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, R, case,
    drive_case_with_external_reference_at_instants,
};

pub(crate) const IMAGE: &str = "openmodelica/openmodelica@sha256:\
79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864";
const FRAMES: usize = 10;

const fn port(name: &'static str, kind: crate::block_harness::SignalKind) -> Port {
    Port { name, kind }
}
const fn real(name: &'static str, value: &'static str) -> Param {
    Param {
        name,
        value: ParamValue::Real(value),
    }
}

const ON_COUNTER: BlockCase = case(
    "openmodelica_integers_on_counter",
    "CDL.Integers.OnCounter",
    "external",
    &[port("trigger", B), port("reset", B)],
    &[Param {
        name: "y_start",
        value: ParamValue::Integer("3"),
    }],
    &[port("y", I)],
);
const TRIGGERED_SAMPLER: BlockCase = case(
    "openmodelica_discrete_triggered_sampler",
    "CDL.Discrete.TriggeredSampler",
    "external",
    &[port("u", R), port("trigger", B)],
    &[real("y_start", "7.0")],
    &[port("y", R)],
);
const TRIGGERED_MAX: BlockCase = case(
    "openmodelica_discrete_triggered_max",
    "CDL.Discrete.TriggeredMax",
    "external",
    &[port("u", R), port("trigger", B)],
    &[],
    &[port("y", R)],
);
const INTEGRATOR_WITH_RESET: BlockCase = case(
    "openmodelica_reals_integrator_with_reset",
    "CDL.Reals.IntegratorWithReset",
    "external",
    &[port("u", R), port("y_reset_in", R), port("trigger", B)],
    &[real("k", "1.0"), real("y_start", "1.5")],
    &[port("y", R)],
);
const PID_WITH_RESET: BlockCase = case(
    "openmodelica_reals_pid_with_reset",
    "CDL.Reals.PIDWithReset",
    "external",
    &[port("u_s", R), port("u_m", R), port("trigger", B)],
    &[
        real("k", "1.0"),
        real("Ti", "1e20"),
        real("xi_start", "0.125"),
        real("y_reset", "0.5"),
        real("yMax", "10.0"),
        real("yMin", "-10.0"),
    ],
    &[port("y", R)],
);

/// One class's evidence: its case, its fixture directory and the bytes the tests read.
struct Evidence {
    case: BlockCase,
    directory: &'static str,
    pilot: &'static [u8],
    vendored: &'static [u8],
    raw: &'static [u8],
    provenance: &'static str,
    initially_true: &'static [u8],
    rising_after_start: &'static [u8],
}

macro_rules! evidence {
    ($case:expr, $dir:literal, $pilot:literal, $source:literal, $raw:literal) => {
        Evidence {
            case: $case,
            directory: $dir,
            pilot: include_bytes!(concat!(
                "../../../tools/openmodelica-event-reference/events/",
                $pilot
            )),
            vendored: include_bytes!(concat!(
                "../../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/",
                $source
            )),
            raw: include_bytes!(concat!("fixtures/open_modelica/", $dir, "/", $raw)),
            provenance: include_str!(concat!("fixtures/open_modelica/", $dir, "/provenance.json")),
            initially_true: include_bytes!(concat!(
                "fixtures/open_modelica/",
                $dir,
                "/initially_true.canonical.csv"
            )),
            rising_after_start: include_bytes!(concat!(
                "fixtures/open_modelica/",
                $dir,
                "/rising_after_start.canonical.csv"
            )),
        }
    };
}

fn classes() -> [Evidence; 5] {
    [
        evidence!(
            ON_COUNTER,
            "integers_on_counter",
            "OnCounterPilot.mo",
            "Integers/OnCounter.mo",
            "on_counter.raw.csv"
        ),
        evidence!(
            TRIGGERED_SAMPLER,
            "discrete_triggered_sampler",
            "TriggeredSamplerPilot.mo",
            "Discrete/TriggeredSampler.mo",
            "triggered_sampler.raw.csv"
        ),
        evidence!(
            TRIGGERED_MAX,
            "discrete_triggered_max",
            "TriggeredMaxPilot.mo",
            "Discrete/TriggeredMax.mo",
            "triggered_max.raw.csv"
        ),
        evidence!(
            INTEGRATOR_WITH_RESET,
            "reals_integrator_with_reset",
            "IntegratorWithResetPilot.mo",
            "Reals/IntegratorWithReset.mo",
            "integrator_with_reset.raw.csv"
        ),
        evidence!(
            PID_WITH_RESET,
            "reals_pid_with_reset",
            "PIDWithResetPilot.mo",
            "Reals/PIDWithReset.mo",
            "pid_with_reset.raw.csv"
        ),
    ]
}

const SCENARIOS: [&str; 2] = ["initially_true", "rising_after_start"];

impl Evidence {
    fn canonical(&self, scenario: &str) -> &'static [u8] {
        match scenario {
            "initially_true" => self.initially_true,
            "rising_after_start" => self.rising_after_start,
            other => panic!("no scenario {other}"),
        }
    }

    fn reference(&self, scenario: &str) -> CombiTimeTable {
        let text = std::str::from_utf8(self.canonical(scenario)).expect("UTF-8");
        CombiTimeTable::parse(text).expect("a canonical table")
    }

    fn provenance(&self) -> serde_json::Value {
        serde_json::from_str(self.provenance).expect("provenance JSON")
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The reference's rows as `(column name -> value)` lookups, in frame order.
pub(crate) fn columns(reference: &CombiTimeTable) -> (Vec<String>, Vec<Vec<f64>>) {
    let names = reference.col_names.clone().expect("named columns");
    let rows = reference
        .data
        .chunks(reference.n_cols)
        .map(<[f64]>::to_vec)
        .collect();
    (names, rows)
}

/// The engine's `y` over the reference's frames, and the exact comparison's outcome.
fn drive(evidence: &Evidence, scenario: &str) -> (Vec<f64>, ComparisonResult, DriveMode) {
    drive_reference(evidence, scenario, evidence.reference(scenario))
}

/// [`drive`] against `reference` instead of the committed table.
fn drive_reference(
    evidence: &Evidence,
    scenario: &str,
    reference: CombiTimeTable,
) -> (Vec<f64>, ComparisonResult, DriveMode) {
    let instants: Vec<f64> = columns(&reference).1.iter().map(|row| row[0]).collect();
    let run = drive_case_with_external_reference_at_instants(
        &evidence.case,
        scenario,
        &reference,
        instants,
    );
    assert_eq!(run.comparisons.len(), 1);
    assert_eq!(run.load_warnings.len(), 0);
    let output = run
        .trace
        .columns
        .iter()
        .find(|column| column.name.ends_with(".block.y"))
        .expect("the captured output");
    (
        output.values.clone(),
        run.comparisons[0].result.clone(),
        run.drive_mode,
    )
}

#[test]
fn the_engine_equals_openmodelica_at_every_frame_of_every_scenario() {
    for evidence in classes() {
        for scenario in SCENARIOS {
            let label = format!("{} {scenario}", evidence.case.class_path);
            let (engine, comparison, mode) = drive(&evidence, scenario);
            let ComparisonResult::Exact(exact) = &comparison else {
                panic!("{label}: the comparison must be exact");
            };
            assert!(exact.passed, "{label}: {exact:?}");
            assert_eq!(exact.compared_points, FRAMES, "{label}");
            assert_eq!(exact.first_mismatch, None, "{label}");
            let DriveMode::EventAligned { instants } = &mode else {
                panic!("{label}: event-aligned drive");
            };
            let frames: Vec<f64> = (0..FRAMES).map(|k| k as f64).collect();
            assert_eq!(instants, &frames, "{label}");
            // Determinism: a second drive repeats every output bit.
            let (again, _, again_mode) = drive(&evidence, scenario);
            let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&engine), bits(&again), "{label}");
            assert_eq!(mode, again_mode, "{label}");
        }
    }
}

/// Which reading of a Boolean event the recurrence applies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    /// HostTick v1 (`docs/execution-profile.md`): no event at frame 0; an event shows at the frame
    /// where its condition rises.
    AtTheFrame,
    /// A trigger already true at frame 0 is an event there (`pre(trigger)` taken as false).
    EventAtFrameZero,
    /// The event is applied after the frame emits, so it shows one frame late.
    ShownOneFrameLate,
}

/// The output `y` of `class` over `reference`'s inputs under `rule`, written from the class's
/// `.mo` (`when` clauses and initial equations) with this file's parameters; nothing here reads
/// the reference's `y` column.
fn recurrence(class: &str, reference: &CombiTimeTable, rule: Rule) -> Vec<f64> {
    let (names, rows) = columns(reference);
    let at = |row: &[f64], name: &str| row[names.iter().position(|n| n == name).expect(name)];
    let mut previous: Option<Vec<bool>> = None;
    let mut state = f64::NAN;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let conditions: Vec<bool> = match class {
            "CDL.Integers.OnCounter" => vec![at(row, "trigger") == 1.0, at(row, "reset") == 1.0],
            _ => vec![at(row, "trigger") == 1.0],
        };
        let rising = match &previous {
            None => rule == Rule::EventAtFrameZero && conditions.iter().any(|c| *c),
            Some(before) => conditions.iter().zip(before).any(|(now, was)| *now && !was),
        };
        if previous.is_none() {
            state = match class {
                "CDL.Integers.OnCounter" => 3.0,
                "CDL.Discrete.TriggeredSampler" => 7.0,
                "CDL.Discrete.TriggeredMax" => at(row, "u"),
                "CDL.Reals.IntegratorWithReset" => 1.5,
                "CDL.Reals.PIDWithReset" => 0.125,
                other => panic!("no recurrence for {other}"),
            };
        }
        previous = Some(conditions.clone());
        let event = |held: f64| match class {
            "CDL.Integers.OnCounter" if conditions[1] => 3.0,
            "CDL.Integers.OnCounter" => held + 1.0,
            "CDL.Discrete.TriggeredSampler" => at(row, "u"),
            "CDL.Discrete.TriggeredMax" => held.max(at(row, "u").abs()),
            "CDL.Reals.IntegratorWithReset" => at(row, "y_reset_in"),
            // `I := y_reset - (P + D)` with `k = 1`, `r = 1`, reverse acting, no D.
            _ => 0.5 - (at(row, "u_s") - at(row, "u_m")),
        };
        let emit = |held: f64| match class {
            "CDL.Reals.PIDWithReset" => {
                ((at(row, "u_s") - at(row, "u_m")) + held).clamp(-10.0, 10.0)
            }
            _ => held,
        };
        if rule == Rule::ShownOneFrameLate {
            out.push(emit(state));
            if rising {
                state = event(state);
            }
        } else {
            if rising {
                state = event(state);
            }
            out.push(emit(state));
        }
    }
    out
}

fn mismatch_rows(computed: &[f64], reference: &CombiTimeTable) -> Vec<usize> {
    let (_, rows) = columns(reference);
    computed
        .iter()
        .zip(&rows)
        .enumerate()
        .filter(|(_, (value, row))| value.to_bits() != row[row.len() - 1].to_bits())
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn openmodelica_fires_no_event_at_frame_zero_and_shows_each_event_at_its_own_frame() {
    // (class, scenario, frames an event at frame 0 misses, frames a late event misses).
    let expected: [(&str, &str, &[usize], &[usize]); 10] = [
        (
            "CDL.Integers.OnCounter",
            "initially_true",
            &[0, 1, 2, 3, 4, 5, 6],
            &[3, 6, 7, 9],
        ),
        (
            "CDL.Integers.OnCounter",
            "rising_after_start",
            &[],
            &[1, 4, 6, 9],
        ),
        (
            "CDL.Discrete.TriggeredSampler",
            "initially_true",
            &[0, 1, 2],
            &[3, 6, 9],
        ),
        (
            "CDL.Discrete.TriggeredSampler",
            "rising_after_start",
            &[],
            &[1, 4, 8],
        ),
        (
            "CDL.Discrete.TriggeredMax",
            "initially_true",
            &[0, 1, 2],
            &[3, 8],
        ),
        (
            "CDL.Discrete.TriggeredMax",
            "rising_after_start",
            &[],
            &[1, 4, 9],
        ),
        (
            "CDL.Reals.IntegratorWithReset",
            "initially_true",
            &[0, 1, 2],
            &[3, 7],
        ),
        (
            "CDL.Reals.IntegratorWithReset",
            "rising_after_start",
            &[],
            &[1, 4, 6, 9],
        ),
        (
            "CDL.Reals.PIDWithReset",
            "initially_true",
            &[0, 1, 2],
            &[3, 6],
        ),
        (
            "CDL.Reals.PIDWithReset",
            "rising_after_start",
            &[],
            &[1, 4, 8],
        ),
    ];
    let classes = classes();
    for (class, scenario, at_frame_zero, late) in expected {
        let evidence = classes
            .iter()
            .find(|evidence| evidence.case.class_path == class)
            .expect("a class");
        let reference = evidence.reference(scenario);
        let label = format!("{class} {scenario}");
        let rule = recurrence(class, &reference, Rule::AtTheFrame);
        assert_eq!(mismatch_rows(&rule, &reference), [0_usize; 0], "{label}");
        let zero = recurrence(class, &reference, Rule::EventAtFrameZero);
        assert_eq!(mismatch_rows(&zero, &reference), at_frame_zero, "{label}");
        let shown_late = recurrence(class, &reference, Rule::ShownOneFrameLate);
        assert_eq!(mismatch_rows(&shown_late, &reference), late, "{label}");
        // The facade comparison is not vacuous: a reference rewritten under either replaced
        // reading fails against the engine at the first frame that reading changes.
        for (wrong, rows) in [(zero, at_frame_zero), (shown_late, late)] {
            let Some(first) = rows.first() else { continue };
            let (_, comparison, _) =
                drive_reference(evidence, scenario, with_y(&reference, &wrong));
            let ComparisonResult::Exact(exact) = comparison else {
                panic!("{label}: exact");
            };
            assert!(!exact.passed, "{label}");
            assert_eq!(
                exact.first_mismatch.map(|m| m.index),
                Some(*first),
                "{label}"
            );
        }
    }
}

/// `reference` with its `y` column replaced by `values`.
fn with_y(reference: &CombiTimeTable, values: &[f64]) -> CombiTimeTable {
    let mut table = reference.clone();
    let width = table.n_cols;
    for (row, value) in table.data.chunks_mut(width).zip(values) {
        row[width - 1] = *value;
    }
    table
}

#[test]
fn each_initially_true_scenario_starts_with_the_trigger_true_and_each_control_with_it_false() {
    for evidence in classes() {
        for (scenario, first) in [("initially_true", 1.0), ("rising_after_start", 0.0)] {
            let reference = evidence.reference(scenario);
            let (names, rows) = columns(&reference);
            let trigger = names.iter().position(|n| n == "trigger").expect("trigger");
            assert_eq!(rows.len(), FRAMES);
            assert_eq!(rows[0][trigger], first, "{} {scenario}", evidence.directory);
            // The control rises at the first frame after the initial instant.
            if first == 0.0 {
                assert_eq!(rows[1][trigger], 1.0, "{}", evidence.directory);
            } else {
                // The initially true trigger falls and rises again inside the run.
                let falls = rows
                    .iter()
                    .position(|row| row[trigger] == 0.0)
                    .expect("falls");
                assert!(rows[falls..].iter().any(|row| row[trigger] == 1.0));
            }
        }
    }
    // The integrator's input is 0 throughout, so its output is exact in both tools.
    let integrator = &classes()[3];
    let (names, rows) = columns(&integrator.reference("initially_true"));
    let u = names.iter().position(|n| n == "u").expect("u");
    assert!(rows.iter().all(|row| row[u] == 0.0));
}

#[test]
fn each_class_s_provenance_binds_its_pilot_tables_and_raw_result() {
    for evidence in classes() {
        let record = evidence.provenance();
        let field = |key: &str| record[key].as_str().expect(key).to_owned();
        let label = evidence.directory;
        assert_eq!(field("class"), evidence.case.class_path, "{label}");
        assert_run_record(
            &record,
            label,
            evidence.pilot,
            evidence.vendored,
            evidence.raw,
        );
        for scenario in SCENARIOS {
            assert_eq!(
                field(&format!("{scenario}_sha256")),
                sha256(evidence.canonical(scenario)),
                "{label} {scenario}"
            );
        }
        // The pilot instantiates the class it is the evidence for.
        let pilot = std::str::from_utf8(evidence.pilot).expect("UTF-8");
        assert!(
            pilot.contains(&format!(
                "Buildings.Controls.OBC.{} initiallyTrue",
                evidence.case.class_path
            )),
            "{label}"
        );
    }
}

/// The simulation script every pilot ran, with its `@PILOT@` placeholder.
const SIMULATE_TEMPLATE: &[u8] =
    include_bytes!("../../../tools/openmodelica-event-reference/events/simulate.mos");

/// Assert the fields every pilot's provenance record shares: the producer, the pinned image and
/// architecture, the library commits and trees every other OpenModelica case of the repository
/// records, the simulation script, the pilot and raw result by digest, and the class source
/// OpenModelica simulated, whose digest must equal the vendored copy's.
pub(crate) fn assert_run_record(
    record: &serde_json::Value,
    label: &str,
    pilot: &[u8],
    vendored: &[u8],
    raw: &[u8],
) {
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
        assert_eq!(field(key), value, "{label} {key}");
    }
    assert_eq!(
        field("simulate_template_sha256"),
        sha256(SIMULATE_TEMPLATE),
        "{label}"
    );
    assert_eq!(field("pilot_sha256"), sha256(pilot), "{label}");
    assert_eq!(field("raw_sha256"), sha256(raw), "{label}");
    assert_eq!(field("class_source_sha256"), sha256(vendored), "{label}");
}

/// The vendored source of `class` (`CDL.<family>.<name>`), for the other OpenModelica event
/// modules: this module is the one place the `conformance` binary's event evidence reads the
/// vendored tree.
pub(crate) fn vendored_source(class: &str) -> &'static [u8] {
    classes()
        .into_iter()
        .find(|evidence| evidence.case.class_path == class)
        .map(|evidence| evidence.vendored)
        .unwrap_or_else(|| panic!("no vendored source for {class}"))
}
