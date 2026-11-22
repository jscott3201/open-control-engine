//! Binding a reference to its sequence (issue #662; commissioning note, decisions 17, 27, 29, 34,
//! 40 and the owner rulings of 2026-11-21 on units and quantities): every refusal before any
//! frame, each by its code, its place and, where it has one, its path or cause.

use oce_verify::{CommissionOptions, OutputStatus, SpecInputs};

use super::support;
use super::{
    body_of, commissioned, loaded, output_named, refused_run, run_file, run_file_edited,
    run_inputs, run_spec, run_spec_edited,
};

const PLANT: &str = "urn:verify:plant#top.";

#[test]
fn listing_12_2_names_with_no_prefix_bind_and_the_fixture_passes() {
    let run = commissioned(&run_spec(), &run_inputs(&[]));
    let body = body_of(&run.receipt);
    assert_eq!(body.status, OutputStatus::Pass);
    let outputs: Vec<&str> = body.outputs.iter().map(|o| o.output.as_str()).collect();
    assert_eq!(
        outputs,
        [
            "urn:verify:plant#top.TSet",
            "urn:verify:plant#top.enOut",
            "urn:verify:plant#top.limOut",
            "urn:verify:plant#top.nOut",
        ],
        "sorted by canonical path bytes"
    );
    assert!(body.unverified_outputs.is_empty());
}

#[test]
fn an_input_no_source_reads_is_input_not_supplied() {
    let spec = run_spec_edited(&[(r#""Supply Temp": "Supply Temp", "#, "")]);
    let error = refused_run(&spec, &run_inputs(&[]));
    assert_eq!(error.code(), "input-not-supplied");
    assert_eq!(error.path(), Some(&format!("{PLANT}T")[..]));
    assert_eq!(error.subject(), "references[0].pointNameMapping");
}

#[test]
fn a_mapped_name_the_sequence_lacks_is_unknown_on_its_side() {
    let mapping = run_file_edited("points.json", &[(r#""name": "T""#, r#""name": "Tx""#)]);
    let error = refused_run(&run_spec(), &run_inputs(&[("points.json", &mapping)]));
    assert_eq!(error.code(), "unknown-input");
    assert_eq!(error.subject(), "references[0].pointNameMapping[Tx]");

    let mapping = run_file_edited("points.json", &[(r#""name": "TSet""#, r#""name": "TSup""#)]);
    let error = refused_run(&run_spec(), &run_inputs(&[("points.json", &mapping)]));
    assert_eq!(error.code(), "unknown-output");
}

/// The fixture with one more point mapped to `cdl`, read from a column `Extra` of the source
/// `file` (`inputs.csv` or `outputs.csv`) that repeats the column `copied`.
fn with_extra_point(
    cdl: &str,
    file: &str,
    copied: usize,
    role: &str,
) -> (oce_verify::CommissioningSpec, SpecInputs) {
    let mapping = run_file_edited(
        "points.json",
        &[(
            "\n]",
            &format!(
                r#",
  {{ "cdl": {{ "name": "{cdl}", "unit": "K", "type": "float" }},
    "device": {{ "name": "Extra", "unit": "degC", "type": "float" }} }}
]"#
            ),
        )],
    );
    let text = String::from_utf8(run_file(file)).expect("UTF-8");
    let mut csv = String::new();
    for (index, line) in text.lines().enumerate() {
        let cells: Vec<&str> = line.split(',').collect();
        let extra = if index == 0 { "Extra" } else { cells[copied] };
        csv.push_str(&format!("{line},{extra}\n"));
    }
    let anchor = match role {
        "inputs" => r#""Limit": "Limit" }"#,
        _ => r#""Limited": "Limited" }"#,
    };
    let replacement = match role {
        "inputs" => r#""Limit": "Limit", "Extra": "Extra" }"#,
        _ => r#""Limited": "Limited", "Extra": "Extra" }"#,
    };
    let spec = run_spec_edited(&[(anchor, replacement)]);
    let inputs = run_inputs(&[("points.json", &mapping), (file, csv.as_bytes())]);
    (spec, inputs)
}

#[test]
fn two_names_landing_on_one_port_are_a_duplicate_on_either_side() {
    // A whole canonical path is an exact name too.
    let (spec, inputs) = with_extra_point("urn:verify:plant#top.T", "inputs.csv", 1, "inputs");
    let error = refused_run(&spec, &inputs);
    assert_eq!(error.code(), "duplicate-input");
    assert_eq!(error.path(), Some(&format!("{PLANT}T")[..]));

    let (spec, inputs) =
        with_extra_point("urn:verify:plant#top.TSet", "outputs.csv", 1, "expected");
    let error = refused_run(&spec, &inputs);
    assert_eq!(error.code(), "duplicate-output");
    assert_eq!(error.path(), Some(&format!("{PLANT}TSet")[..]));
}

#[test]
fn a_name_two_ports_end_with_is_ambiguous_with_its_candidates() {
    // The Integer input renamed `a.T`, so the mapping's `T` names two inputs.
    let subject = support::edited(&support::subject("plant"), &[("#top.n\"", "#top.a.T\"")]);
    let mut inputs = run_inputs(&[]);
    inputs.cxf = subject;
    let error = refused_run(&run_spec(), &inputs);
    assert_eq!(error.code(), "ambiguous-input");
    assert_eq!(
        error.candidates(),
        [format!("{PLANT}T"), format!("{PLANT}a.T")]
    );
}

#[test]
fn an_exact_key_whose_rest_names_two_outputs_is_refused_as_the_spec_readers_ambiguous_output() {
    let subject = support::edited(
        &support::subject("plant"),
        &[("#top.limOut\"", "#top.b.TSet\"")],
    );
    // The mapping names both by their whole paths, so only the key is ambiguous.
    let mapping = run_file_edited(
        "points.json",
        &[
            (
                r#""name": "limOut""#,
                r#""name": "urn:verify:plant#top.b.TSet""#,
            ),
            (
                r#""name": "TSet""#,
                r#""name": "urn:verify:plant#top.TSet""#,
            ),
        ],
    );
    let mut inputs = run_inputs(&[("points.json", &mapping)]);
    inputs.cxf = subject;
    let spec = run_spec_edited(&[(r#""plant.lim*""#, r#""plant.b.TSet""#)]);
    let error = refused_run(&spec, &inputs);
    assert_eq!(
        (error.code(), error.cause()),
        ("spec-refused", Some("ambiguous-output"))
    );
    assert_eq!(error.subject(), "references[0].outputs");
}

#[test]
fn a_key_without_the_sequence_prefix_a_glob_matching_nothing_and_a_bare_stem_are_unmatched() {
    for (old, new, map) in [
        (r#""plant.TSet": { "#, r#""TSet": { "#, "outputs"),
        (r#""plant.TSet": { "#, r#""plant.zz*": { "#, "outputs"),
        // `lim` selects only an output named `lim`, never `limOut`.
        (r#""plant.lim*""#, r#""plant.lim""#, "indicators"),
    ] {
        let error = refused_run(&run_spec_edited(&[(old, new)]), &run_inputs(&[]));
        assert_eq!(
            (error.code(), error.cause()),
            ("spec-refused", Some("unmatched-key")),
            "{new}"
        );
        assert_eq!(error.subject(), format!("references[0].{map}"));
    }
}

#[test]
fn an_exact_key_beats_a_glob_field_by_field_and_two_globs_that_disagree_are_refused() {
    let spec = run_spec_edited(&[(
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } }"#,
        r#""outputs": { "plant.T*": { "atoly": 0.5, "rtoly": 0.1 }, "plant.TSet": { "atoly": 0.05 } }"#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let tolerances = output_named(body_of(&run.receipt), "TSet")
        .tolerances
        .expect("a Real output's tolerances");
    assert_eq!(tolerances.atoly(), Some(0.05));
    assert_eq!(tolerances.rtoly(), Some(0.1));
    assert_eq!(
        tolerances.atolx(),
        Some(1.0),
        "the top level lies under every key"
    );

    let spec = run_spec_edited(&[(
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } }"#,
        r#""outputs": { "plant.T*": { "atoly": 0.5 }, "plant.TS?t": { "atoly": 0.4 } }"#,
    )]);
    let error = refused_run(&spec, &run_inputs(&[]));
    assert_eq!(
        (error.code(), error.cause()),
        ("spec-refused", Some("conflicting-override"))
    );
}

#[test]
fn an_indicator_name_is_a_sequence_signal_with_the_prefix_or_an_indicator_source_point_without() {
    // Listing 12.1's `fanSta.y` is a column of `fan.csv`; `plant.en` is the sequence's input.
    let spec = run_spec_edited(&[(r#"[ "fanSta.y" ]"#, r#"[ "fanSta.y", "plant.en" ]"#)]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let lim = output_named(body_of(&run.receipt), "limOut");
    // en is false from 20 s; the fan from 15 s to 25 s: of 0, 5, …, 30 s only 0, 5 and 10 are
    // compared.
    assert_eq!((lim.compared, lim.masked), (3, 4));

    for (name, why) in [
        ("fanSta.z", "no such point"),
        ("plant.nope", "no such signal"),
    ] {
        let spec = run_spec_edited(&[(r#""fanSta.y" ]"#, &format!(r#""{name}" ]"#))]);
        let error = refused_run(&spec, &run_inputs(&[]));
        assert_eq!(error.code(), "unknown-indicator", "{why}");
        assert_eq!(error.subject(), "references[0].indicators.plant.lim*");
    }
}

#[test]
fn a_point_typed_otherwise_than_its_port_is_a_value_type_mismatch() {
    let mapping = run_file_edited(
        "points.json",
        &[(
            r#"{ "cdl": { "name": "enOut", "type": "bool" },
    "device": { "name": "Enabled", "type": "bool" } }"#,
            r#"{ "cdl": { "name": "enOut", "type": "int" },
    "device": { "name": "Enabled", "type": "bool" } }"#,
        )],
    );
    let error = refused_run(&run_spec(), &run_inputs(&[("points.json", &mapping)]));
    assert_eq!(error.code(), "value-type-mismatch");
    assert_eq!(error.path(), Some(&format!("{PLANT}enOut")[..]));
}

#[test]
fn a_unit_that_does_not_fit_its_port_is_refused_with_the_unit_tables_word() {
    for (old, new, cause, port) in [
        // A reading converted without its offset onto a ThermodynamicTemperature port.
        (
            r#""name": "Supply Temp", "unit": "degC""#,
            r#""name": "Supply Temp", "unit": "degC_delta""#,
            "difference-for-absolute",
            "T",
        ),
        // A difference converted with the offset onto a TemperatureDifference port.
        (
            r#""name": "Offset", "unit": "degC_delta""#,
            r#""name": "Offset", "unit": "degC""#,
            "absolute-for-difference",
            "dT",
        ),
        // `cdl.unit` is not the port's.
        (
            r#""name": "TSet", "unit": "K""#,
            r#""name": "TSet", "unit": "degC""#,
            "port-unit-mismatch",
            "TSet",
        ),
    ] {
        let mapping = run_file_edited("points.json", &[(old, new)]);
        let error = refused_run(&run_spec(), &run_inputs(&[("points.json", &mapping)]));
        assert_eq!(
            (error.code(), error.cause()),
            ("unit-refused", Some(cause)),
            "{new}"
        );
        assert_eq!(error.path(), Some(&format!("{PLANT}{port}")[..]));
    }
    // No unit on either side of a point whose port declares one disagrees too.
    let mapping = run_file_edited(
        "points.json",
        &[
            (r#""name": "limOut", "unit": "1""#, r#""name": "limOut""#),
            (
                r#""name": "Limited", "unit": "percent""#,
                r#""name": "Limited""#,
            ),
        ],
    );
    let error = refused_run(&run_spec(), &run_inputs(&[("points.json", &mapping)]));
    assert_eq!(
        (error.code(), error.cause()),
        ("unit-refused", Some("port-unit-mismatch"))
    );
}

#[test]
fn one_mapping_file_serves_two_references() {
    let text = String::from_utf8(run_file("spec.json")).expect("UTF-8");
    let start = text.find("    {\n      \"model\"").expect("the reference");
    let end = text.rfind("\n  ]").expect("the list's end");
    let reference = &text[start..end];
    let second = reference.replace("\"plant", "\"plant2");
    let spec = loaded(&format!(
        "{}{reference},\n{second}{}",
        &text[..start],
        &text[end..]
    ));
    assert_eq!(spec.references().len(), 2);
    for index in [0, 1] {
        let run = oce_verify::run_commissioning(
            &spec,
            index,
            &run_inputs(&[]),
            &CommissionOptions::default(),
        )
        .expect("each reference runs on the one mapping");
        let body = body_of(&run.receipt);
        assert_eq!(body.status, OutputStatus::Pass, "{index}");
        assert_eq!(body.reference, index as u64);
    }
}

#[test]
fn a_reference_past_the_end_or_without_cxf_or_window_refuses_to_run() {
    let error = oce_verify::run_commissioning(
        &run_spec(),
        1,
        &run_inputs(&[]),
        &CommissionOptions::default(),
    )
    .expect_err("one reference");
    assert_eq!(error.code(), "reference-out-of-range");

    let spec = run_spec_edited(&[(r#""cxf": "plant.cxf.json","#, "")]);
    let error = refused_run(&spec, &run_inputs(&[]));
    assert_eq!(
        (error.code(), error.subject()),
        ("reference-incomplete", "references[0].cxf")
    );

    let spec = run_spec_edited(&[(r#""window": { "start": 0, "stop": 30 },"#, "")]);
    let error = refused_run(&spec, &run_inputs(&[]));
    assert_eq!(
        (error.code(), error.subject()),
        ("reference-incomplete", "references[0].window")
    );

    // Both missing: `cxf` is named first.
    let spec = run_spec_edited(&[
        (r#""cxf": "plant.cxf.json","#, ""),
        (r#""window": { "start": 0, "stop": 30 },"#, ""),
    ]);
    assert_eq!(
        refused_run(&spec, &run_inputs(&[])).subject(),
        "references[0].cxf"
    );
}

#[test]
fn a_cxf_the_engine_refuses_is_subject_load_failed_at_the_references_cxf() {
    let mut inputs = run_inputs(&[]);
    inputs.cxf = b"{}".to_vec();
    let error = refused_run(&run_spec(), &inputs);
    assert_eq!(
        (error.code(), error.subject()),
        ("subject-load-failed", "references[0].cxf")
    );
}

#[test]
fn a_refused_source_keeps_the_readers_code_and_too_many_sources_is_the_runs_own() {
    // `maybe` is no Boolean text (`true`, `false`, `active`, `inactive`, `1`, `0`).
    let csv = run_file_edited("inputs.csv", &[("10,21,1,true,2,50", "10,21,1,maybe,2,50")]);
    let error = refused_run(&run_spec(), &run_inputs(&[("inputs.csv", &csv)]));
    assert_eq!(
        (error.code(), error.cause()),
        ("source-refused", Some("cell-refused"))
    );
    assert_eq!(error.subject(), "inputs.csv");

    let mut options = CommissionOptions::default();
    options.trends.max_sources = 2;
    let error = oce_verify::run_commissioning(&run_spec(), 0, &run_inputs(&[]), &options)
        .expect_err("three sources");
    assert_eq!((error.code(), error.cause()), ("too-many-sources", None));
}

#[test]
fn two_rules_selecting_one_output_gate_it_with_both_their_indicators() {
    // A glob gives the fan (false from 15 to 25 s) and an exact name gives `en` (false from
    // 20 s): every instant either masks is masked, 15 to 30 s.
    let spec = run_spec_edited(&[(
        r#""indicators": { "plant.lim*": [ "fanSta.y" ] }"#,
        r#""indicators": { "plant.lim*": [ "fanSta.y" ], "plant.limOut": [ "plant.en" ] }"#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let lim = output_named(body_of(&run.receipt), "limOut");
    assert_eq!((lim.compared, lim.masked), (3, 4));
    assert_eq!(
        (lim.masked_intervals[0].from, lim.masked_intervals[0].to),
        (15.0, 30.0)
    );
}
