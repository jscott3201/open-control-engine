//! Masking by indicators (issues #662 and #663; commissioning note, "Indicators", decisions 2 and
//! 23): masked instants zeroed in both series before the envelope is built, counted apart, listed
//! as intervals, and an error there reported and never counted.

use oce_verify::{CommissionOptions, OutputStatus};

use super::{
    body_of, commission, commissioned, curves_named, output_named, refused_run, run_file_edited,
    run_inputs, run_spec, run_spec_edited,
};

#[test]
fn masked_instants_are_zeroed_in_both_series_before_the_envelope_is_built() {
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let run = commission(&run_spec(), &run_inputs(&[]), &options).expect("runs");
    let lim = output_named(body_of(&run.receipt), "limOut");
    assert_eq!((lim.compared, lim.masked), (5, 2));
    assert_eq!(lim.masked_intervals_total, 1);
    assert_eq!(
        (lim.masked_intervals[0].from, lim.masked_intervals[0].to),
        (15.0, 20.0)
    );
    let bounds = curves_named(&run, "limOut").bounds.clone().expect("kept");
    // The fan is false from 15 s to 25 s: both series read zero at 15 and 20 s, and their own
    // value elsewhere.
    for curve in [&bounds.reference, &bounds.test] {
        assert_eq!(curve.x(), [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0]);
        assert_eq!(curve.y()[3].to_bits(), 0.0_f64.to_bits());
        assert_eq!(curve.y()[4].to_bits(), 0.0_f64.to_bits());
        assert_eq!((curve.y()[2], curve.y()[5]), (0.5, 0.4));
    }
    // A Boolean or Integer output has no envelope and no bound curves.
    assert!(curves_named(&run, "nOut").bounds.is_none());
}

#[test]
fn an_output_masked_at_every_instant_is_no_data() {
    let fan = b"time,Fan Status\n0,false\n";
    let run = commissioned(&run_spec(), &run_inputs(&[("fan.csv", fan)]));
    let body = body_of(&run.receipt);
    let lim = output_named(body, "limOut");
    assert_eq!(
        (lim.status, lim.compared, lim.masked),
        (OutputStatus::NoData, 0, 7)
    );
    assert!(
        lim.half_width.is_none(),
        "no envelope is built with nothing compared"
    );
    assert_eq!(body.status, OutputStatus::NoData);
    assert!(!run.receipt.header.passed);
}

#[test]
fn an_indicator_is_false_before_its_first_sample() {
    let fan = b"time,Fan Status\n5,true\n";
    let run = commissioned(&run_spec(), &run_inputs(&[("fan.csv", fan)]));
    let lim = output_named(body_of(&run.receipt), "limOut");
    assert_eq!((lim.compared, lim.masked), (6, 1));
    assert_eq!(
        (lim.masked_intervals[0].from, lim.masked_intervals[0].to),
        (0.0, 0.0)
    );
}

#[test]
fn an_indicator_at_one_half_is_refused() {
    let mapping = run_file_edited(
        "points.json",
        &[(
            r#"{ "cdl": { "name": "fanSta.y", "type": "bool" },
    "device": { "name": "Fan Status", "type": "bool" } }"#,
            r#"{ "cdl": { "name": "fanSta.y", "type": "float" },
    "device": { "name": "Fan Status", "type": "float" } }"#,
        )],
    );
    let fan = b"time,Fan Status\n0,1\n15,0.5\n";
    let error = refused_run(
        &run_spec(),
        &run_inputs(&[("points.json", &mapping), ("fan.csv", fan)]),
    );
    assert_eq!(error.code(), "indicator-not-boolean");
    assert!(error.detail().contains("15.0"), "{}", error.detail());
    // Exactly 0 and 1 in a float column are Boolean values.
    let fan = b"time,Fan Status\n0,1\n15,0\n25,1\n";
    let run = commissioned(
        &run_spec(),
        &run_inputs(&[("points.json", &mapping), ("fan.csv", fan)]),
    );
    assert_eq!(output_named(body_of(&run.receipt), "limOut").masked, 2);
}

#[test]
fn both_series_zeroed_at_the_same_instants_leave_no_masked_exceedance() {
    // A mask edge between two trended samples: with both series on one grid and zeroed at the
    // same instants, the band at a masked instant is the zeroed reference's own, which the
    // zeroed test meets.
    let outputs = run_file_edited(
        "outputs.csv",
        &[("20,24,false,3,40", "15,22,true,2,50\n20,24,false,3,40")],
    );
    let run = commissioned(&run_spec(), &run_inputs(&[("outputs.csv", &outputs)]));
    let lim = output_named(body_of(&run.receipt), "limOut");
    assert_eq!(lim.status, OutputStatus::Pass);
    assert_eq!((lim.compared, lim.masked), (5, 2));
    assert_eq!(lim.masked_exceedance_total, 0);
    assert!(lim.masked_exceedance.is_empty());
    let curve = super::curves_named(&run, "limOut");
    assert_eq!((curve.errors.y()[3], curve.errors.y()[4]), (0.0, 0.0));
}

#[test]
fn masked_intervals_are_capped_with_their_total() {
    // A fan toggling every second over 200 s, compared every half second: 100 masked runs.
    let mut fan = String::from("time,Fan Status\n");
    for t in 0..200 {
        fan.push_str(&format!("{t},{}\n", t % 2 == 0));
    }
    let mut inputs = String::from("time,Supply Temp,Offset,Enable,Stage,Limit\n");
    let mut outputs = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for k in 0..=400 {
        let t = f64::from(k) / 2.0;
        if k % 4 == 0 {
            // The limit alternates 50 % and 40 % every two seconds.
            let limit = if k % 8 == 0 { 50 } else { 40 };
            inputs.push_str(&format!("{t},20,1,true,1,{limit}\n"));
        }
        let limit = if (k / 4) % 2 == 0 { 50 } else { 40 };
        outputs.push_str(&format!("{t},21,true,1,{limit}\n"));
    }
    let spec = run_spec_edited(&[(r#""stop": 30 },"#, r#""stop": 200 }, "sampling": 0.5,"#)]);
    let run = commissioned(
        &spec,
        &run_inputs(&[
            ("fan.csv", fan.as_bytes()),
            ("inputs.csv", inputs.as_bytes()),
            ("outputs.csv", outputs.as_bytes()),
        ]),
    );
    let lim = output_named(body_of(&run.receipt), "limOut");
    assert_eq!(lim.masked_intervals.len(), oce_verify::MAX_MASKED_INTERVALS);
    assert_eq!(lim.masked_intervals_total, 100);
    assert_eq!(lim.masked_exceedance_total, 0);
    // Capped lists keep the first entries, in time order.
    assert!(
        lim.masked_intervals
            .windows(2)
            .all(|pair| pair[0].to < pair[1].from)
    );
}
