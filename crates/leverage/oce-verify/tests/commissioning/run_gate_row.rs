//! The M3 gate row "Tolerance mode checked" (issue #663; commissioning note, decision 39, the
//! owner's ruling of 2026-11-20): on a synthetic trend generated from the engine and perturbed
//! by a known amount, zero tolerances give `fail` and the stated band gives `pass`, both golden
//! receipts labelled `data: synthetic`.

use oce_replay::TraceColumn;
use oce_verify::{CommissionOptions, OutputStatus, TrendData};

use super::support;
use super::{
    body_of, commission, commissioned, output_named, run_inputs, run_spec, run_spec_edited,
};

/// The perturbation of the trended setpoint, in kelvin (and degrees Celsius).
const PERTURBATION: f64 = 0.1;

/// The expected outputs the engine computes for the fixture's inputs, written back in the
/// device's units, with the setpoint raised by [`PERTURBATION`].
fn synthetic_outputs() -> Vec<u8> {
    let run = commissioned(&run_spec(), &run_inputs(&[]));
    let trace = &run.trace;
    let column = |name: &str| {
        &trace
            .outputs()
            .iter()
            .find(|(path, _)| path.ends_with(&format!(".{name}")))
            .expect("an output")
            .1
    };
    let (
        TraceColumn::Real(set),
        TraceColumn::Boolean(en),
        TraceColumn::Integer(n),
        TraceColumn::Real(lim),
    ) = (
        column("TSet"),
        column("enOut"),
        column("nOut"),
        column("limOut"),
    )
    else {
        panic!("the plant's output types");
    };
    let mut csv = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for (row, t) in trace.times().iter().enumerate() {
        csv.push_str(&format!(
            "{t:?},{:?},{},{},{:?}\n",
            set[row] - 273.15 + PERTURBATION,
            en[row] == 1,
            n[row],
            lim[row] * 100.0
        ));
    }
    csv.into_bytes()
}

#[test]
fn a_perturbed_synthetic_trend_fails_with_zero_tolerances_and_passes_within_the_stated_band() {
    let outputs = synthetic_outputs();
    let options = CommissionOptions::default().with_data(TrendData::Synthetic);
    let zero = run_spec_edited(&[
        (r#""tolerances": { "atolx": 1, "atoly": 0.01 },"#, ""),
        (r#""outputs": { "plant.TSet": { "atoly": 0.05 } },"#, ""),
    ]);
    let band = run_spec_edited(&[(
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } },"#,
        r#""outputs": { "plant.TSet": { "atoly": 0.2 } },"#,
    )]);
    for (name, spec, status) in [
        ("gate_zero", zero, OutputStatus::Fail),
        ("gate_band", band, OutputStatus::Pass),
    ] {
        let run =
            commission(&spec, &run_inputs(&[("outputs.csv", &outputs)]), &options).expect("runs");
        let body = body_of(&run.receipt);
        assert_eq!(body.data, TrendData::Synthetic);
        let set = output_named(body, "TSet");
        assert_eq!(set.status, status, "{name}");
        if status == OutputStatus::Fail {
            // With no tolerance the band is funnel's 1e-10 floor, so the error is the
            // perturbation less that floor, within the rounding of the round trip through degC.
            let worst = set.worst.expect("failed").exceedance;
            assert!((worst - PERTURBATION).abs() < 1e-9, "{worst}");
            assert_eq!(set.compared, 7);
        }
        // Every other output is the engine's own and passes either way.
        for other in ["enOut", "nOut", "limOut"] {
            assert_eq!(
                output_named(body, other).status,
                OutputStatus::Pass,
                "{name} {other}"
            );
        }
        assert_eq!(body.status, status);
        support::compare_or_bless(
            &support::fixture(&format!("commissioning/run/receipts/{name}.receipt.json")),
            &run.receipt.to_canonical_json(),
        );
    }
}
