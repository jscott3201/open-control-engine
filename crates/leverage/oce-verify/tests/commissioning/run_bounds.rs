//! The bounds of a commissioning run (issues #662 and #663; commissioning note, decisions 22
//! and 42): each complete list at its bound and one past, refused at bind; each capped list at
//! its cap and one past, with its total; and the point caps, at their bound and one past, before
//! the replay runs.

use oce_verify::{
    CommissionOptions, CommissionRun, MAX_COMPARED_OUTPUTS, MAX_UNIT_COLUMNS,
    MAX_UNVERIFIED_OUTPUTS, OutputStatus, SpecInputs, VerifyError,
};

use super::{body_of, commission, loaded, run_inputs, run_spec, run_spec_edited};

/// A sequence of one Real input `u` passed through to `outputs` Real outputs `y0`, `y1`, …,
/// beside one `CDL.Reals.Abs` block.
fn fan_out(outputs: usize) -> Vec<u8> {
    let root = "urn:verify:fan#top";
    let mut graph = vec![format!(
        r#"{{"@id": "{root}", "@type": "S231:Block", "S231:hasInput": [{{"@id": "{root}.u"}}],
         "S231:containsBlock": [{{"@id": "{root}.abs"}}],
         "S231:hasOutput": [{}]}}"#,
        (0..outputs)
            .map(|k| format!(r#"{{"@id": "{root}.y{k}"}}"#))
            .collect::<Vec<_>>()
            .join(", ")
    )];
    graph.push(format!(
        r#"{{"@id": "{root}.u", "@type": "S231:RealInput", "S231:isOfDataType": {{"@id": "S231:Real"}},
         "S231:isConnectedTo": [{{"@id": "{root}.abs.u"}}, {}]}}"#,
        (0..outputs)
            .map(|k| format!(r#"{{"@id": "{root}.y{k}"}}"#))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    // One block, so the root is a composite; its output is read by no one.
    graph.push(format!(
        r#"{{"@id": "{root}.abs", "@type": "CDL.Reals.Abs",
         "S231:hasInput": {{"@id": "{root}.abs.u"}}, "S231:hasOutput": {{"@id": "{root}.abs.y"}}}},
        {{"@id": "{root}.abs.u", "@type": "S231:RealInput", "S231:isOfDataType": {{"@id": "S231:Real"}}}},
        {{"@id": "{root}.abs.y", "@type": "S231:RealOutput", "S231:isOfDataType": {{"@id": "S231:Real"}}}}"#
    ));
    for k in 0..outputs {
        graph.push(format!(
            r#"{{"@id": "{root}.y{k}", "@type": "S231:RealOutput", "S231:isOfDataType": {{"@id": "S231:Real"}}}}"#
        ));
    }
    format!(
        r#"{{"@context": {{"S231": "http://data.ashrae.org/S231#"}}, "@graph": [{}]}}"#,
        graph.join(",\n")
    )
    .into_bytes()
}

/// A reference of [`fan_out`]`(outputs)` whose expected sources supply the first `compared`
/// outputs, in files of at most 2,000 columns; `u` reads 1 at 0 and 10 s, and so does every
/// trended output.
fn fan_out_run(outputs: usize, compared: usize) -> Result<CommissionRun, VerifyError> {
    fan_out_run_over(
        outputs,
        compared,
        r#""window": {"start": 0, "stop": 10}, "sampling": 10"#,
    )
}

/// [`fan_out_run`] with the reference's `window` (and any `sampling`) written as `window`.
fn fan_out_run_over(
    outputs: usize,
    compared: usize,
    window: &str,
) -> Result<CommissionRun, VerifyError> {
    fan_out_run_gated(outputs, compared, window, "")
}

/// [`fan_out_run_over`] with the reference's other members `members` (written with a trailing
/// comma, or empty).
fn fan_out_run_gated(
    outputs: usize,
    compared: usize,
    window: &str,
    members: &str,
) -> Result<CommissionRun, VerifyError> {
    let mut points = vec![
        r#"{"cdl": {"name": "u", "type": "float"}, "device": {"name": "U", "type": "float"}}"#
            .to_owned(),
    ];
    for k in 0..compared {
        points.push(format!(
            r#"{{"cdl": {{"name": "y{k}", "type": "float"}}, "device": {{"name": "Y{k}", "type": "float"}}}}"#
        ));
    }
    let mut files = vec![
        (
            "m.json".to_owned(),
            format!("[{}]", points.join(",\n")).into_bytes(),
        ),
        ("u.csv".to_owned(), b"time,U\n0,1\n10,1\n".to_vec()),
    ];
    let mut expected = Vec::new();
    for (index, chunk) in (0..compared).collect::<Vec<_>>().chunks(2_000).enumerate() {
        let path = format!("y{index}.csv");
        let names: Vec<String> = chunk.iter().map(|k| format!("Y{k}")).collect();
        let ones = vec!["1"; chunk.len()].join(",");
        files.push((
            path.clone(),
            format!("time,{}\n0,{ones}\n10,{ones}\n", names.join(",")).into_bytes(),
        ));
        let columns: Vec<String> = names.iter().map(|n| format!(r#""{n}": "{n}""#)).collect();
        expected.push(format!(
            r#"{{"path": "{path}", "format": "csv", "timeColumn": "time", "columns": {{{}}}}}"#,
            columns.join(", ")
        ));
    }
    let spec = loaded(&format!(
        r#"{{"references": [{{"model": "F", "sequence": "fan", "cxf": "f.json",
            "pointNameMapping": "m.json", {window}, {members}
            "inputs": [{{"path": "u.csv", "format": "csv", "timeColumn": "time", "columns": {{"U": "U"}}}}],
            "expected": [{}]}}]}}"#,
        expected.join(", ")
    ));
    let inputs = SpecInputs {
        cxf: fan_out(outputs),
        files,
    };
    oce_verify::run_commissioning(&spec, 0, &inputs, &CommissionOptions::default())
}

#[test]
fn the_compared_outputs_are_complete_up_to_4096_and_one_more_is_refused() {
    let run = fan_out_run(MAX_COMPARED_OUTPUTS, MAX_COMPARED_OUTPUTS).expect("at the bound");
    let body = body_of(&run.receipt);
    assert_eq!(body.outputs.len(), MAX_COMPARED_OUTPUTS);
    assert_eq!(body.status, OutputStatus::Pass);
    let error = fan_out_run(MAX_COMPARED_OUTPUTS + 1, MAX_COMPARED_OUTPUTS + 1)
        .expect_err("one past the bound");
    assert_eq!(error.code(), "too-many-outputs");
    assert_eq!(error.subject(), "references[0].expected");
}

#[test]
fn unverified_outputs_are_capped_at_64_with_their_total() {
    let run = fan_out_run(MAX_UNVERIFIED_OUTPUTS + 1, 1).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(body.unverified_outputs.len(), MAX_UNVERIFIED_OUTPUTS);
    assert_eq!(body.unverified_outputs_total, MAX_UNVERIFIED_OUTPUTS as u64);
    let run = fan_out_run(MAX_UNVERIFIED_OUTPUTS + 2, 1).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(body.unverified_outputs.len(), MAX_UNVERIFIED_OUTPUTS);
    assert_eq!(
        body.unverified_outputs_total,
        MAX_UNVERIFIED_OUTPUTS as u64 + 1
    );
    // Sorted, the first ones kept.
    assert!(
        body.unverified_outputs
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
}

/// The fixture with an indicator source of `points` more converted points.
fn with_converted_points(points: usize) -> Result<CommissionRun, VerifyError> {
    let mut mapping = String::from_utf8(super::run_file("points.json")).expect("UTF-8");
    let extra: Vec<String> = (0..points)
        .map(|k| {
            format!(
                r#"{{"cdl": {{"name": "i{k}", "unit": "1", "type": "float"}}, "device": {{"name": "P{k:04}", "unit": "percent", "type": "float"}}}}"#
            )
        })
        .collect();
    let end = mapping.rfind(']').expect("a list");
    mapping.replace_range(end.., &format!(",\n{}\n]\n", extra.join(",\n")));
    let names: Vec<String> = (0..points).map(|k| format!("P{k:04}")).collect();
    let zeros = vec!["0"; points].join(",");
    let csv = format!("time,{}\n0,{zeros}\n", names.join(","));
    let columns: Vec<String> = names.iter().map(|n| format!(r#""{n}": "{n}""#)).collect();
    let spec = run_spec_edited(&[(
        r#""columns": { "Fan Status": "Fan Status" } }"#,
        &format!(
            r#""columns": {{ "Fan Status": "Fan Status" }} }},
        {{ "path": "many.csv", "format": "csv", "timeColumn": "time", "columns": {{ {} }} }}"#,
            columns.join(", ")
        ),
    )]);
    commission(
        &spec,
        &run_inputs(&[
            ("points.json", mapping.as_bytes()),
            ("many.csv", csv.as_bytes()),
        ]),
        &CommissionOptions::default(),
    )
}

#[test]
fn unit_columns_are_capped_at_256_with_their_total() {
    // The fixture converts five points of its own.
    let run = with_converted_points(MAX_UNIT_COLUMNS - 5).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(
        (body.unit_columns.len(), body.unit_columns_total),
        (256, 256)
    );
    let run = with_converted_points(MAX_UNIT_COLUMNS - 4).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(
        (body.unit_columns.len(), body.unit_columns_total),
        (256, 257)
    );
    assert!(
        body.unit_columns
            .windows(2)
            .all(|pair| pair[0].device < pair[1].device)
    );
}

#[test]
fn one_source_past_256_is_too_many_sources() {
    let extra =
        vec![r#"{ "path": "fan.csv", "format": "csv", "timeColumn": "time", "columns": {} }"#; 254]
            .join(", ");
    let spec = run_spec_edited(&[(
        r#""columns": { "Fan Status": "Fan Status" } }"#,
        &format!(r#""columns": {{ "Fan Status": "Fan Status" }} }}, {extra}"#),
    )]);
    let error = commission(&spec, &run_inputs(&[]), &CommissionOptions::default())
        .expect_err("257 sources");
    assert_eq!(error.code(), "too-many-sources");
}

#[test]
fn an_outputs_points_are_held_to_the_envelopes_cap_at_its_bound_and_one_past() {
    // With `sampling` the count is the grid's, from the window and the sampling alone.
    let sampled = |stop: u32| {
        run_spec_edited(&[(
            r#""window": { "start": 0, "stop": 30 },"#,
            &format!(r#""window": {{ "start": 0, "stop": {stop} }}, "sampling": 1,"#),
        )])
    };
    let mut options = CommissionOptions::default();
    options.compare.max_funnel_points = 31;
    commission(&sampled(30), &run_inputs(&[]), &options).expect("31 points, at the cap");
    let error = commission(&sampled(31), &run_inputs(&[]), &options).expect_err("32 points");
    assert_eq!(error.code(), "too-many-points");
    assert!(error.path().is_some());

    // Under `aligned-grid` the Real outputs take its cap; the discrete ones keep the funnel's.
    let mut options = CommissionOptions::default();
    options.compare.max_aligned_grid_points = 31;
    let grid = |stop: u32| {
        run_spec_edited(&[
            (
                r#""model": "Plant","#,
                r#""model": "Plant", "envelope": "aligned-grid","#,
            ),
            (
                r#""window": { "start": 0, "stop": 30 },"#,
                &format!(r#""window": {{ "start": 0, "stop": {stop} }}, "sampling": 1,"#),
            ),
        ])
    };
    commission(&grid(30), &run_inputs(&[]), &options).expect("at the cap");
    let error = commission(&grid(31), &run_inputs(&[]), &options).expect_err("one past");
    assert_eq!(error.code(), "too-many-points");
    assert_eq!(error.path(), Some("urn:verify:plant#top.TSet"));

    // An off-grid `stop` is one more instant: 0 to 30.5 s by 1 s is 31 grid instants and 30.5.
    let off_grid = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 0, "stop": 30.5 }, "sampling": 1,"#,
    )]);
    let mut options = CommissionOptions::default();
    options.compare.max_funnel_points = 32;
    commission(&off_grid, &run_inputs(&[]), &options).expect("32 instants");
    options.compare.max_funnel_points = 31;
    let error = commission(&off_grid, &run_inputs(&[]), &options).expect_err("31 allowed");
    assert_eq!(error.code(), "too-many-points");
}

#[test]
fn the_runs_points_together_past_sixteen_mebi_points_are_refused_before_the_replay() {
    // 2^20 points per output is each output's cap exactly; sixteen outputs are the run's cap
    // exactly, and seventeen one output past it. Counted from the window and the sampling, so
    // nothing of that size is ever built.
    let window = r#""window": {"start": 0, "stop": 1048575}, "sampling": 1"#;
    let error = fan_out_run_over(17, 17, window).expect_err("17 * 2^20 points");
    assert_eq!(error.code(), "too-many-points");
    assert_eq!(error.path(), None, "the run's cap, not an output's");
    assert_eq!(error.subject(), "references[0].outputs");
    let error = fan_out_run_over(
        16,
        16,
        r#""window": {"start": 0, "stop": 1048576}, "sampling": 1"#,
    )
    .expect_err("one point past the per-output cap");
    assert!(
        error.path().is_some(),
        "the per-output cap is checked first"
    );
}

#[test]
fn indicators_count_against_the_runs_work() {
    // Seventeen outputs of 2^19 instants each are 8.5 times 2^20 points; gated each by one
    // indicator, read once per instant, they are 17 times 2^20, past the run's 16 times 2^20.
    let window = r#""window": {"start": 0, "stop": 524287}, "sampling": 1"#;
    let error = fan_out_run_gated(17, 17, window, r#""indicators": {"fan.y*": ["fan.u"]},"#)
        .expect_err("past the run's work");
    assert_eq!(error.code(), "too-many-points");
    assert_eq!(error.path(), None);
    assert!(error.detail().contains("indicators"), "{}", error.detail());
}

/// The fixture with `count` more Boolean points read from an indicator source and every one of
/// them gating `limOut`, beside the fan. Every one is true throughout, but the last, which is
/// false from 25 s.
fn with_indicators(count: usize) -> Result<CommissionRun, VerifyError> {
    let mut mapping = String::from_utf8(super::run_file("points.json")).expect("UTF-8");
    let extra: Vec<String> = (0..count)
        .map(|k| {
            format!(
                r#"{{"cdl": {{"name": "g{k}", "type": "bool"}}, "device": {{"name": "G{k:03}", "type": "bool"}}}}"#
            )
        })
        .collect();
    let end = mapping.rfind(']').expect("a list");
    mapping.replace_range(end.., &format!(",\n{}\n]\n", extra.join(",\n")));
    let names: Vec<String> = (0..count).map(|k| format!("G{k:03}")).collect();
    let trues = vec!["true"; count].join(",");
    let mut last_false = vec!["true"; count];
    if let Some(last) = last_false.last_mut() {
        *last = "false";
    }
    let csv = format!(
        "time,{}\n0,{trues}\n25,{}\n",
        names.join(","),
        last_false.join(",")
    );
    let columns: Vec<String> = names.iter().map(|n| format!(r#""{n}": "{n}""#)).collect();
    let indicators: Vec<String> = (0..count).map(|k| format!(r#""g{k}""#)).collect();
    let spec = run_spec_edited(&[
        (
            r#""columns": { "Fan Status": "Fan Status" } }"#,
            &format!(
                r#""columns": {{ "Fan Status": "Fan Status" }} }},
        {{ "path": "gates.csv", "format": "csv", "timeColumn": "time", "columns": {{ {} }} }}"#,
                columns.join(", ")
            ),
        ),
        (
            r#"[ "fanSta.y" ]"#,
            &format!(r#"[ "fanSta.y", {} ]"#, indicators.join(", ")),
        ),
    ]);
    commission(
        &spec,
        &run_inputs(&[
            ("points.json", mapping.as_bytes()),
            ("gates.csv", csv.as_bytes()),
        ]),
        &CommissionOptions::default(),
    )
}

#[test]
fn a_reference_holds_at_most_64_distinct_indicators() {
    // The fan and 63 more: at the cap. The fan masks 15 and 20 s, and the 64th indicator, the
    // last bit of the set, masks 25 and 30 s.
    let run = with_indicators(oce_verify::MAX_INDICATORS - 1).expect("64 indicators");
    let lim = super::output_named(body_of(&run.receipt), "limOut");
    assert_eq!((lim.compared, lim.masked), (3, 4));
    let error = with_indicators(oce_verify::MAX_INDICATORS).expect_err("65 indicators");
    assert_eq!(error.code(), "too-many-indicators");
    assert_eq!(error.subject(), "references[0].indicators.plant.lim*");
}

#[test]
fn the_grid_envelopes_quadratic_work_is_held_to_its_bound() {
    // Two Real outputs of 7 instants each: 2 * 7^2 = 98 units.
    let spec = run_spec_edited(&[(
        r#""model": "Plant","#,
        r#""model": "Plant", "envelope": "aligned-grid","#,
    )]);
    let mut options = CommissionOptions::default();
    options.max_total_aligned_grid_work = 98;
    commission(&spec, &run_inputs(&[]), &options).expect("at the bound");
    options.max_total_aligned_grid_work = 97;
    let error = commission(&spec, &run_inputs(&[]), &options).expect_err("one unit past");
    assert_eq!((error.code(), error.path()), ("too-many-points", None));
    // The funnel is not quadratic and is not held to it.
    options.max_total_aligned_grid_work = 0;
    commission(&run_spec(), &run_inputs(&[]), &options).expect("funnel");
    assert_eq!(oce_verify::MAX_TOTAL_ALIGNED_GRID_WORK, 1 << 32);
}

#[test]
fn a_trace_too_long_for_its_bytes_is_refused_before_the_replay() {
    // Four frames of 67 bytes: the time and status (9), four outputs (25), five retained inputs
    // (33), no substitution mask.
    let mut options = CommissionOptions::default();
    options.replay.max_trace_bytes = 4 * 67;
    commission(&run_spec(), &run_inputs(&[]), &options).expect("at the bound");
    options.replay.max_trace_bytes = 4 * 67 - 1;
    let error = commission(&run_spec(), &run_inputs(&[]), &options).expect_err("a byte short");
    assert_eq!(
        (error.code(), error.cause()),
        ("replay-failed", Some("trace-too-large"))
    );
    assert!(error.detail().contains("4 frames"), "{}", error.detail());
}

#[test]
fn instants_that_round_to_one_value_are_refused_as_a_vector_grid_is() {
    // At 1e16 s one ulp is 2 s, so a 1 s sampling repeats an instant.
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 1.0e16, "stop": 1.0000000000000008e16 }, "sampling": 1,"#,
    )]);
    let error = commission(&spec, &run_inputs(&[]), &CommissionOptions::default())
        .expect_err("repeated instants");
    assert_eq!(
        (error.code(), error.cause()),
        ("replay-failed", Some("grid-not-increasing"))
    );
    assert_eq!(error.subject(), "references[0].sampling");
    // The same window with a sampling the magnitude can hold runs.
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 1.0e16, "stop": 1.0000000000000008e16 }, "sampling": 2,"#,
    )]);
    commission(&spec, &run_inputs(&[]), &CommissionOptions::default()).expect("2 s steps");
}
