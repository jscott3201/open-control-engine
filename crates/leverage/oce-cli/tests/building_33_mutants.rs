//! Mutants of the Building 33 transcription (issue #755): what the evidence catches when the
//! authored `CoolingCoilValve` is wrong. Each mutant is one plausible mistake made in the
//! authored text (`oce-verify`'s `tests/fixtures/commissioning/building33/cooling_coil_valve.oce`),
//! emitted through `oce-author` as `oce emit` would, and held to the two behavioural pieces of
//! evidence:
//!
//! - **the derivation**: `derive_valve.py`'s valve signal over the synthetic frames that take
//!   every branch the valve signal can show, compared bit for bit (`valve_oracle.txt`);
//! - **the day**: the Building 33 day's receipt, whose body (the verdict, every figure and the
//!   warnings) must equal the golden's.
//!
//! The structural check in `oce-verify` (`building_33_transcription.rs`: every parameter, block,
//! modifier and connection against what `derive_declarations.py` read from the `.mo`) catches
//! every one of them by construction, since each changes the document; these tests ask what the
//! behaviour shows. Each mutant's row pins which of the two catches it, so a change in the
//! evidence's power shows here. A mutant neither catches is a survivor and is listed as one: the
//! one there is, the line's upper clamp turned off, cannot change the valve signal for any input,
//! and its own test says so.

mod support;

use oce_author::{EmitOptions, emit, parse_text};
use oce_verify::{CommissionOptions, CommissioningSpec, SpecInputs, SpecLimits, run_commissioning};

use support::building33::{TREND_FILES, example_file, matches_oracle, trend, verify_fixture};

/// A mutant: its name, the edits to the authored text, and whether the derivation and the day
/// each catch it.
struct Mutant {
    name: &'static str,
    edits: &'static [(&'static str, &'static str)],
    caught_by_derivation: bool,
    caught_by_day: bool,
}

const MUTANTS: [Mutant; 11] = [
    Mutant {
        name: "the setpoint and the measurement swapped at the PI",
        edits: &[
            ("TSupSet -> limPI.u_s", "TSupSet -> limPI.u_m"),
            ("TSup -> limPI.u_m", "TSup -> limPI.u_s"),
        ],
        caught_by_derivation: true,
        caught_by_day: true,
    },
    Mutant {
        name: "the proportional gain without the 9/5 conversion",
        edits: &[(
            "param k : Real = 0.018000000000000002 1/K",
            "param k : Real = 0.01 1/K",
        )],
        caught_by_derivation: true,
        caught_by_day: true,
    },
    Mutant {
        name: "the integral time a tenth of the model's",
        edits: &[("param Ti : Real = 30.0 s", "param Ti : Real = 3.0 s")],
        caught_by_derivation: true,
        caught_by_day: true,
    },
    Mutant {
        name: "reverse acting, as section 12.6's prose says",
        edits: &[(
            "param reverseActing : Boolean = false",
            "param reverseActing : Boolean = true",
        )],
        caught_by_derivation: true,
        caught_by_day: true,
    },
    Mutant {
        name: "the PI's lower limit below zero",
        edits: &[("  yMin = 0,", "  yMin = -0.5,")],
        caught_by_derivation: true,
        caught_by_day: true,
    },
    Mutant {
        name: "the supply-air high limit dropped",
        edits: &[("higLim.y -> min.u2", "yCooValMax.y -> min.u2")],
        caught_by_derivation: true,
        caught_by_day: false,
    },
    Mutant {
        name: "the high limit's two support points swapped",
        edits: &[
            ("TSupMax.y -> higLim.x2", "TSupMax.y -> higLim.x1"),
            ("TSupMin.y -> higLim.x1", "TSupMin.y -> higLim.x2"),
        ],
        caught_by_derivation: true,
        caught_by_day: false,
    },
    Mutant {
        name: "the enable dropped from the product",
        edits: &[("booToRea.y -> pro.u2", "yCooValMax.y -> pro.u2")],
        caught_by_derivation: true,
        caught_by_day: false,
    },
    Mutant {
        name: "TOutDelta converted as a difference, not as upstream writes it",
        edits: &[(
            "block TOutThr = CDL.Reals.Hysteresis(uLow = 26.666666666666686, uHigh = TOutCooCut)",
            "block TOutThr = CDL.Reals.Hysteresis(uLow = 282.0388888888889, uHigh = TOutCooCut)",
        )],
        caught_by_derivation: true,
        caught_by_day: false,
    },
    Mutant {
        name: "the fan feedback thresholds read as percent",
        edits: &[(
            "uLow = 0.04999999999999999, uHigh = FanFeeCut",
            "uLow = 5.0, uHigh = 15.0",
        )],
        caught_by_derivation: true,
        caught_by_day: false,
    },
    Mutant {
        name: "the reset triggered by the enable's level, not its change",
        edits: &[("cha.y -> limPI.trigger", "andIntErr.y -> limPI.trigger")],
        caught_by_derivation: true,
        caught_by_day: false,
    },
];

/// The authored text with `edits` applied; each `old` must occur exactly once.
fn mutated(edits: &[(&str, &str)]) -> String {
    let mut text = String::from_utf8(example_file("cooling_coil_valve.oce")).expect("UTF-8");
    for (old, new) in edits {
        assert_eq!(text.matches(old).count(), 1, "`{old}` occurs once");
        text = text.replace(old, new);
    }
    text
}

/// The CXF `oce emit` writes for `text`.
fn emitted(text: &str) -> Vec<u8> {
    let sequence = parse_text(text).unwrap_or_else(|error| panic!("the mutant parses: {error:?}"));
    emit(&sequence, &EmitOptions::new()).unwrap_or_else(|error| panic!("emits: {error:?}"))
}

/// The body of the day's receipt for the CXF `cxf`, as JSON: the verdict, the output's figures,
/// and the load and run-time warnings.
fn day_body(cxf: Vec<u8>) -> serde_json::Value {
    let spec =
        CommissioningSpec::from_json_bytes(&example_file("spec.json"), &SpecLimits::default())
            .expect("the spec loads");
    let mut files = vec![(
        "b33-points.json".to_owned(),
        example_file("b33-points.json"),
    )];
    files.extend(
        TREND_FILES
            .iter()
            .map(|name| (format!("data/{name}"), trend(name))),
    );
    let run = run_commissioning(
        &spec,
        0,
        &SpecInputs { cxf, files },
        &CommissionOptions::default(),
    )
    .unwrap_or_else(|error| panic!("the day runs: {} {error}", error.code()));
    let receipt: serde_json::Value =
        serde_json::from_str(&run.receipt.to_canonical_json()).expect("JSON");
    receipt["body"].clone()
}

fn golden_body() -> serde_json::Value {
    let golden: serde_json::Value = serde_json::from_slice(&verify_fixture(
        "commissioning/building33/receipts/day.receipt.json",
    ))
    .expect("JSON");
    golden["body"].clone()
}

#[test]
fn the_unmutated_transcription_passes_both_checks() {
    let cxf = emitted(&mutated(&[]));
    assert!(cxf == example_file("cooling_coil_valve.cxf.json"));
    assert!(matches_oracle(&cxf));
    assert_eq!(day_body(cxf), golden_body());
}

/// Run the mutants `range` of [`MUTANTS`] and hold each to its row.
fn check(range: std::ops::Range<usize>) {
    let golden = golden_body();
    for mutant in &MUTANTS[range] {
        let cxf = emitted(&mutated(mutant.edits));
        let by_derivation = !matches_oracle(&cxf);
        let body = day_body(cxf);
        let by_day = body != golden;
        assert_eq!(
            (by_derivation, by_day),
            (mutant.caught_by_derivation, mutant.caught_by_day),
            "{}: caught by the derivation, by the day; the day's outputs and warnings: {} {} {}",
            mutant.name,
            body["outputs"],
            body["load_warnings"],
            body["warnings"]
        );
        assert!(by_derivation || by_day, "{} survives", mutant.name);
    }
}

#[test]
fn mutants_of_the_pi_controller_are_caught() {
    check(0..5);
}

#[test]
fn mutants_of_the_high_limit_are_caught_by_the_derivation_and_the_day_cannot_see_them() {
    // The day's supply air is below 50 degF (the line's upper support point) at 56 instants,
    // 71,875 to 73,975 s, where the limit ranges from 0.85 (at 48.8 degF) to 0.99 and stays above
    // the valve by at least 0.068 (at 71,905 s): the limit never cuts the signal on this data,
    // so only the derivation's frames can show it.
    check(5..7);
}

#[test]
fn mutants_of_the_enable_are_caught_by_the_derivation_and_the_day_cannot_see_them() {
    // The fan was enabled, its feedback 60.9 to 72.5 % and the outdoor air above 48.89 degF all
    // day, so the enable is on throughout the day's data and no mistake in it shows there: the
    // derivation's synthetic frames are the evidence for that part of the sequence.
    check(7..11);
}

#[test]
fn the_line_s_upper_clamp_cannot_show_in_behaviour_and_only_the_structural_check_sees_it() {
    // `higLim(limitAbove = false)` changes the line only above its upper support point, where
    // it would rise past `f2 = 1`; the valve is the minimum of the line and the PI's product,
    // which never exceeds `yMax = 1`, so no input can make the mutant's valve differ. Neither
    // behavioural check sees it; the emitted document differs, which the structural check in
    // `oce-verify` (`building_33_transcription.rs`, every modifier against the `.mo`) refuses.
    let cxf = emitted(&mutated(&[(
        "block higLim = CDL.Reals.Line(limitBelow = true, limitAbove = true)",
        "block higLim = CDL.Reals.Line(limitBelow = true, limitAbove = false)",
    )]));
    assert!(matches_oracle(&cxf), "the derivation cannot see it");
    assert_eq!(day_body(cxf.clone()), golden_body(), "nor can the day");
    assert!(cxf != example_file("cooling_coil_valve.cxf.json"));
}
