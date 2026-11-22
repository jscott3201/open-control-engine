//! The Building 33 worked example (issue #755; commissioning note, "Worked spec", "Building 33
//! data" and their "As landed"): one day, 0 to 86,400 s at 5 s, of the six vendored trends of
//! LBNL Building 33 (`third_party/modelica-buildings-cdl/Buildings/Resources/Data/...`, read in
//! place, never copied) against `CoolingCoilValve` **authored from the vendored model, not
//! vendored** (`tests/fixtures/commissioning/building33/`). Every verdict here is about that
//! transcription, not about upstream's translation, which does not load (issue #760).
//!
//! - **Golden**: the receipt, byte for byte, blessed only with `OCE_BLESS_VERIFY=1`; and its
//!   verdict figures as literals, so a reader of `docs/verification-evidence.md` can find them.
//! - **Determinism**: one receipt for replay chunks of 1, 7 and the whole run.
//! - **Oracle link**: the run's five funnel curves print, under `%.16g`, as the pinned pyfunnel
//!   printed them for the Building 33 day of the funnel differential
//!   (`tests/fixtures/funnel/recorded/building_33_day/`); the case's two inputs are this run's
//!   series, written so that they read back to the same bits.
//! - **The rulings, held**: `uFanFee` mapped with a unit on one side is refused; a corrected
//!   `TOutDelta` changes no figure of the day.

use oce_verify::{CommissionOptions, CommissionRun, Curve, OutputStatus, SpecInputs, TrendData};

use super::support;
use super::{body_of, loaded};

/// The vendored trend directory, relative to this crate.
const TRENDS: &str = "../../../third_party/modelica-buildings-cdl/Buildings/Resources/Data/\
                      Utilities/Plotters/Examples/ControlsVerification_CoolingCoilValve";

/// The six trend files the spec names under `data/`.
pub(super) const TREND_FILES: [&str; 6] = [
    "Supply_Air_Temp.mos",
    "SA_Clg_Stpt.mos",
    "OA_Temp.mos",
    "VFD_Fan_Feedback.mos",
    "VFD_Fan_Enable.mos",
    "Clg_Coil_Valve.mos",
];

/// The bytes of a file of the worked example's fixture directory.
pub(super) fn example_file(name: &str) -> Vec<u8> {
    support::fixture_bytes(&format!("commissioning/building33/{name}"))
}

/// The bytes of a vendored trend file.
pub(super) fn trend(name: &str) -> Vec<u8> {
    let path = support::crate_dir().join(TRENDS).join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The worked example's spec with each `(old, new)` of `edits` replaced in its text.
pub(super) fn example_spec(edits: &[(&str, &str)]) -> oce_verify::CommissioningSpec {
    let text =
        String::from_utf8(support::edited(&example_file("spec.json"), edits)).expect("UTF-8");
    loaded(&text)
}

/// The files the spec names, under the paths it writes: the mapping, then the six trends; the
/// CXF is `cxf`.
pub(super) fn example_inputs(cxf: Vec<u8>, mapping: Vec<u8>) -> SpecInputs {
    let mut files = vec![("b33-points.json".to_owned(), mapping)];
    files.extend(
        TREND_FILES
            .iter()
            .map(|name| (format!("data/{name}"), trend(name))),
    );
    SpecInputs { cxf, files }
}

/// Run the day under `options`.
fn run_day(options: &CommissionOptions) -> CommissionRun {
    run_day_of(&example_spec(&[]), options)
}

fn run_day_of(spec: &oce_verify::CommissioningSpec, options: &CommissionOptions) -> CommissionRun {
    let inputs = example_inputs(
        example_file("cooling_coil_valve.cxf.json"),
        example_file("b33-points.json"),
    );
    oce_verify::run_commissioning(spec, 0, &inputs, options).unwrap_or_else(|error| {
        panic!(
            "the day gives a receipt: {} {:?}: {error}",
            error.code(),
            error.cause()
        )
    })
}

/// The worked example's `spec_id`, derived by `derive_canonical.py`.
const EXAMPLE_SPEC_ID: &str = "spec:fnv1a128:9b422287934f07ba1470d7adcbf423fb";

/// The day's `receipt_id`, derived by `derive_run.py`'s rule from the golden's text.
const DAY_RECEIPT_ID: &str = "receipt:fnv1a128:4d025f7ac401d69d617b42b587e3eef2";

/// The receipt golden's path.
const GOLDEN: &str = "commissioning/building33/receipts/day.receipt.json";

#[test]
fn the_day_receipt_matches_its_golden_byte_for_byte_and_is_labelled_trended() {
    let run = run_day(&CommissionOptions::default().with_data(TrendData::Trended));
    let bytes = run.receipt.to_canonical_json();
    support::compare_or_bless(&support::fixture(GOLDEN), &bytes);
    assert!(bytes.contains("\"data\": \"trended\""), "{bytes}");
    // Both ids recomputed outside the crate: the spec's by `../derive_canonical.py
    // building33/spec.json`, the receipt's by `run/derive_run.py`'s `receipt_id` rule.
    assert_eq!(run.receipt.header.spec_id, EXAMPLE_SPEC_ID);
    assert_eq!(run.receipt.header.receipt_id, DAY_RECEIPT_ID);
    assert_eq!(run.receipt.computed_receipt_id(), DAY_RECEIPT_ID);
    // The default label is the same claim.
    let plain = run_day(&CommissionOptions::default());
    assert_eq!(plain.receipt.to_canonical_json(), bytes);
}

#[test]
fn the_day_fails_with_the_worst_exceedance_of_0_0209_at_65_965_s_exactly_as_recorded() {
    // The verdict is whatever resulted on the first run; nothing was tuned to reach it. Every
    // figure is the receipt's, pinned here as the literal the evidence doc quotes.
    let run = run_day(&CommissionOptions::default());
    let body = body_of(&run.receipt);
    assert_eq!(body.status, OutputStatus::Fail);
    assert!(!run.receipt.header.passed);
    assert_eq!(body.outputs.len(), 1);
    let output = &body.outputs[0];
    assert_eq!(output.output, "urn:oce:author:CoolingCoilValve.yCooVal");
    assert_eq!(output.status, OutputStatus::Fail);
    assert_eq!(
        (output.compared, output.masked, output.uncovered),
        (17_281, 0, 0)
    );
    let worst = output.worst.expect("a failing output has a worst instant");
    assert_eq!(worst.t, 65_965.0);
    assert_eq!(
        worst.exceedance.to_bits(),
        0.020_864_709_745_257_892_f64.to_bits()
    );
    assert_eq!(output.first_exceedance_t, Some(61_770.0));
    assert_eq!(output.exceedance_time_s, 4_085.0);
    let (width, height) = (
        output.half_width.expect("an envelope was built"),
        output.half_height.expect("an envelope was built"),
    );
    assert_eq!((width.min, width.max), (1.0, 1.0));
    assert_eq!((height.min, height.max), (0.01, 0.01));
    // The fan was enabled all day, so the indicator masked nothing.
    assert!(output.masked_intervals.is_empty());
    // The frames are the whole trend's, not the window's: the replay runs every frame the input
    // files hold (75,583, to 493,800 s), and the instants past the window are not compared.
    assert_eq!(
        (
            body.frames.executed,
            body.frames.substituted,
            body.frames.refused
        ),
        (2, 75_566, 15)
    );
}

#[test]
fn the_day_receipt_id_is_one_for_replay_chunks_of_1_7_and_the_whole_run() {
    let reference = run_day(&CommissionOptions::default());
    let bytes = reference.receipt.to_canonical_json();
    for chunk in [1, 7, usize::MAX] {
        let mut options = CommissionOptions::default();
        options.chunk_frames = chunk;
        let run = run_day(&options);
        assert_eq!(
            run.receipt.header.receipt_id, reference.receipt.header.receipt_id,
            "chunk {chunk}"
        );
        assert_eq!(run.receipt.to_canonical_json(), bytes, "chunk {chunk}");
        assert_eq!(run.curves, reference.curves, "chunk {chunk}");
    }
}

/// A series as the funnel case's input: `x,y`, then each point in Rust's shortest spelling
/// that reads back to the same bits (Python's `float` reads it so too).
fn exact_csv(curve: &Curve) -> String {
    let mut text = String::from("x,y\n");
    for (x, y) in curve.points() {
        text.push_str(&format!("{x:?},{y:?}\n"));
    }
    text
}

#[test]
fn the_day_run_prints_the_five_curves_pyfunnel_printed_for_the_building_33_day() {
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let run = run_day(&options);
    let bounds = run.curves[0]
        .bounds
        .as_ref()
        .expect("an envelope was built");
    let case = support::fixture("funnel/recorded/building_33_day");
    // The case's inputs are this run's two series: the engine's around which the envelope is
    // built (`funnelReference: "engine"`), and the trend's. Blessed with the receipt.
    support::compare_or_bless(&case.join("reference.csv"), &exact_csv(&bounds.reference));
    support::compare_or_bless(&case.join("test.csv"), &exact_csv(&bounds.test));
    // What pyfunnel made of them, captured once and never blessed.
    let expected = support::fixture("funnel/expected/building_33_day");
    for (file, curve) in [
        ("reference.csv", &bounds.reference),
        ("test.csv", &bounds.test),
        ("lowerBound.csv", &bounds.lower),
        ("upperBound.csv", &bounds.upper),
        ("errors.csv", &run.curves[0].errors),
    ] {
        let theirs = std::fs::read_to_string(expected.join(file))
            .unwrap_or_else(|error| panic!("read expected/building_33_day/{file}: {error}"));
        assert!(
            curve.to_csv() == theirs,
            "{file}: the run's curve does not print as pyfunnel's"
        );
    }
}

#[test]
fn a_unit_on_one_side_of_the_fan_feedback_is_refused_as_the_ruling_says() {
    // `uFanFee` declares no unit, so `cdl.unit` must be absent too, and `percent` to `1` cannot
    // bind: refused at bind as `unit-refused`, the port's unit disagreeing.
    let mapping = support::edited(
        &example_file("b33-points.json"),
        &[(
            r#"{ "cdl": { "name": "uFanFee", "type": "float" },
    "device": { "name": "VFD Fan Feedback", "type": "float" } }"#,
            r#"{ "cdl": { "name": "uFanFee", "unit": "1", "type": "float" },
    "device": { "name": "VFD Fan Feedback", "unit": "percent", "type": "float" } }"#,
        )],
    );
    let inputs = example_inputs(example_file("cooling_coil_valve.cxf.json"), mapping);
    let error = oce_verify::run_commissioning(
        &example_spec(&[]),
        0,
        &inputs,
        &CommissionOptions::default(),
    )
    .expect_err("a unit the port does not declare is refused");
    assert_eq!(
        (error.code(), error.cause()),
        ("unit-refused", Some("port-unit-mismatch"))
    );
}

#[test]
fn a_corrected_t_out_delta_changes_no_figure_of_the_day() {
    // Upstream converts the 2 degF hysteresis delta as an absolute temperature, so `TOutThr`'s
    // `uLow` is 26.67 K and the cut-off, once on, never turns off. A delta converted as a
    // difference (1.11 K) puts `uLow` at 282.04 K, 48 degF; the day's outdoor air never falls
    // below 48.89 degF, so the enable is on all day either way and every figure is the same.
    let corrected = run_day_of(
        &example_spec(&[(
            r#""parameters": {}"#,
            r#""parameters": { "TOutThr.uLow": 282.0388888888889 }"#,
        )]),
        &CommissionOptions::default(),
    );
    let upstream = run_day(&CommissionOptions::default());
    let (ours, theirs) = (body_of(&corrected.receipt), body_of(&upstream.receipt));
    assert_eq!(ours.parameters.len(), 1, "the edit was applied");
    let (a, b) = (&ours.outputs[0], &theirs.outputs[0]);
    assert_eq!((a.status, a.compared), (b.status, b.compared));
    assert_eq!(a.worst, b.worst);
    assert_eq!(a.first_exceedance_t, b.first_exceedance_t);
    assert_eq!(a.exceedance_time_s.to_bits(), b.exceedance_time_s.to_bits());
    assert_eq!(corrected.curves, upstream.curves);
}
