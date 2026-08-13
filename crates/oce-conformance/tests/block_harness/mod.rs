//! Shared facade-bound exact conformance harness for one-block CDL fixtures.

#![allow(dead_code)]

use std::path::PathBuf;

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriverOptions, OutputPattern,
    PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, VerifyConfig,
    drive_trace_with_options, escape_regex,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SignalKind {
    Real,
    Integer,
    Boolean,
}

impl SignalKind {
    fn cxf_type(self, dir: &str) -> &'static str {
        match (self, dir) {
            (SignalKind::Real, "input") => "S231:RealInput",
            (SignalKind::Real, "output") => "S231:RealOutput",
            (SignalKind::Integer, "input") => "S231:IntegerInput",
            (SignalKind::Integer, "output") => "S231:IntegerOutput",
            (SignalKind::Boolean, "input") => "S231:BooleanInput",
            (SignalKind::Boolean, "output") => "S231:BooleanOutput",
            _ => unreachable!("test builder passes input/output only"),
        }
    }

    fn cxf_data_type(self) -> &'static str {
        match self {
            SignalKind::Real => "S231:Real",
            SignalKind::Integer => "S231:Integer",
            SignalKind::Boolean => "S231:Boolean",
        }
    }

    fn config_type(self) -> &'static str {
        match self {
            SignalKind::Real => "Real",
            SignalKind::Integer => "Integer",
            SignalKind::Boolean => "Boolean",
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct Port {
    pub(crate) name: &'static str,
    pub(crate) kind: SignalKind,
}

#[derive(Copy, Clone)]
pub(crate) struct Param {
    pub(crate) name: &'static str,
    pub(crate) value: ParamValue,
}

#[derive(Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum ParamValue {
    Real(&'static str),
    Integer(&'static str),
    Boolean(&'static str),
}

#[derive(Copy, Clone)]
pub(crate) struct BlockCase {
    pub(crate) slug: &'static str,
    pub(crate) class_path: &'static str,
    reference_path: &'static str,
    pub(crate) inputs: &'static [Port],
    params: &'static [Param],
    pub(crate) outputs: &'static [Port],
}

#[allow(dead_code)]
pub(crate) const R: SignalKind = SignalKind::Real;
#[allow(dead_code)]
pub(crate) const I: SignalKind = SignalKind::Integer;
#[allow(dead_code)]
pub(crate) const B: SignalKind = SignalKind::Boolean;

pub(crate) const fn case(
    slug: &'static str,
    class_path: &'static str,
    reference_path: &'static str,
    inputs: &'static [Port],
    params: &'static [Param],
    outputs: &'static [Port],
) -> BlockCase {
    BlockCase {
        slug,
        class_path,
        reference_path,
        inputs,
        params,
        outputs,
    }
}

pub(crate) fn assert_cases_match_exact_oracle(
    cases: &[BlockCase],
    family_dir: &str,
    sequence: &str,
) {
    for case in cases {
        let reference = read_reference(case, family_dir);
        let cxf = build_cxf(case);
        let run = drive_case(case, sequence, &cxf, &reference);

        assert_eq!(run.comparisons.len(), case.outputs.len(), "{}", case.slug);
        for (idx, (comparison, output)) in
            run.comparisons.iter().zip(case.outputs.iter()).enumerate()
        {
            assert_eq!(comparison.output, output_point(case, idx));
            assert_eq!(comparison.reference_column, output.name);
            assert!(!comparison.masked, "exact per-block comparison is unmasked");

            let ComparisonResult::Exact(result) = &comparison.result else {
                panic!("{} {} did not use exact comparison", case.slug, output.name);
            };
            assert!(
                result.passed,
                "{} {} mismatch: {:?}",
                case.slug, output.name, result
            );
            assert_eq!(
                result.compared_points, reference.n_rows,
                "{} {}",
                case.slug, output.name
            );
            assert_eq!(result.first_mismatch, None, "{} {}", case.slug, output.name);
        }
    }
}

pub(crate) fn assert_cases_match_aligned_tolerance_oracle(
    cases: &[BlockCase],
    family_dir: &str,
    sequence: &str,
) {
    let tolerances = aligned_real_tolerances();
    for case in cases {
        let reference = read_reference(case, family_dir);
        let cxf = build_cxf(case);
        let run = drive_case_with_mode(
            case,
            sequence,
            &cxf,
            &reference,
            ComparisonMode::AlignedTolerance,
            tolerances,
        );

        assert_eq!(run.comparisons.len(), case.outputs.len(), "{}", case.slug);
        for (idx, (comparison, output)) in
            run.comparisons.iter().zip(case.outputs.iter()).enumerate()
        {
            assert_eq!(comparison.output, output_point(case, idx));
            assert_eq!(comparison.reference_column, output.name);
            assert!(
                !comparison.masked,
                "aligned per-block comparison is unmasked"
            );
            assert_eq!(comparison.tolerance, tolerances);

            let ComparisonResult::AlignedTolerance(result) = &comparison.result else {
                panic!(
                    "{} {} did not use aligned tolerance comparison",
                    case.slug, output.name
                );
            };
            assert!(
                result.passed,
                "{} {} mismatch: {:?}",
                case.slug, output.name, result
            );
            assert_eq!(
                result.compared_points, reference.n_rows,
                "{} {}",
                case.slug, output.name
            );
            assert_eq!(result.first_mismatch, None, "{} {}", case.slug, output.name);
        }
    }
}

pub(crate) fn assert_cases_are_deterministic(
    cases: &[BlockCase],
    slugs: &[&str],
    family_dir: &str,
    sequence: &str,
) {
    for slug in slugs {
        let case = cases
            .iter()
            .find(|case| case.slug == *slug)
            .unwrap_or_else(|| panic!("{slug} case"));
        let reference = read_reference(case, family_dir);
        let cxf = build_cxf(case);
        let first = drive_case(case, sequence, &cxf, &reference);
        let second = drive_case(case, sequence, &cxf, &reference);

        assert_trace_bit_eq(&first, &second);
        assert_eq!(first.comparisons, second.comparisons, "{slug}");
    }
}

fn read_reference(case: &BlockCase, family_dir: &str) -> CombiTimeTable {
    let path = reference_path(case, family_dir);
    CombiTimeTable::read(&path)
        .unwrap_or_else(|err| panic!("{} reference should parse: {err:?}", case.slug))
}

fn reference_path(case: &BlockCase, family_dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/golden-gen/goldens")
        .join(family_dir)
        .join(case.reference_path)
        .join("reference.csv")
}

fn drive_case(
    case: &BlockCase,
    sequence: &str,
    cxf: &str,
    reference: &CombiTimeTable,
) -> oce_conformance::DriverRun {
    drive_case_with_mode(
        case,
        sequence,
        cxf,
        reference,
        ComparisonMode::Exact,
        zero_tolerances(),
    )
}

pub(crate) fn drive_case_for_audit(
    case: &BlockCase,
    family_dir: &str,
    sequence: &str,
) -> (CombiTimeTable, oce_conformance::DriverRun) {
    let reference = read_reference(case, family_dir);
    let cxf = build_cxf(case);
    let run = drive_case(case, sequence, &cxf, &reference);
    (reference, run)
}

pub(crate) fn drive_case_with_external_reference(
    case: &BlockCase,
    sequence: &str,
    reference: &CombiTimeTable,
) -> oce_conformance::DriverRun {
    let cxf = build_cxf(case);
    drive_case(case, sequence, &cxf, reference)
}

fn drive_case_with_mode(
    case: &BlockCase,
    sequence: &str,
    cxf: &str,
    reference: &CombiTimeTable,
    comparison: ComparisonMode,
    tolerances: Tolerances,
) -> oce_conformance::DriverRun {
    drive_trace_with_options(
        cxf.as_bytes(),
        &config(case, sequence, tolerances),
        reference,
        &DriverOptions {
            comparison,
            ..DriverOptions::default()
        },
    )
    .unwrap_or_else(|err| {
        panic!(
            "{} should drive through {:?}: {err:?}",
            case.slug, comparison
        )
    })
}

fn config(case: &BlockCase, sequence: &str, tolerances: Tolerances) -> VerifyConfig {
    let mut point_name_mapping = Vec::with_capacity(case.inputs.len() + case.outputs.len());
    for input in case.inputs {
        point_name_mapping.push(map(input.name, &input_point(case, input), input.kind));
    }
    for (idx, output) in case.outputs.iter().enumerate() {
        point_name_mapping.push(map(output.name, &output_point(case, idx), output.kind));
    }

    VerifyConfig {
        references: vec![ReferenceSpec {
            model: case.slug.to_string(),
            sequence: sequence.to_string(),
            point_name_mapping,
        }],
        tolerances,
        outputs: case
            .outputs
            .iter()
            .enumerate()
            .map(|(idx, _)| OutputPattern {
                // Anchored and escaped: the authored path's dots are regex wildcards.
                pattern: format!("^{}$", escape_regex(&output_point(case, idx))),
                tolerances: PartialTolerances::default(),
            })
            .collect(),
        indicators: Vec::new(),
        sampling: None,
        run_controller: true,
    }
}

fn zero_tolerances() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn aligned_real_tolerances() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 1.0e-12,
        rtolx: 0.0,
        rtoly: 1.0e-12,
        ltolx: 0.0,
        ltoly: 1.0e-12,
    }
}

fn map(device: &str, cdl: &str, kind: SignalKind) -> PointMapEntry {
    let kind = Some(kind.config_type().to_string());
    PointMapEntry {
        cdl: PointEnd {
            name: cdl.to_string(),
            unit: None,
            kind: kind.clone(),
        },
        device: PointEnd {
            name: device.to_string(),
            unit: None,
            kind,
        },
    }
}

fn input_point(case: &BlockCase, input: &Port) -> String {
    format!("http://example.org#{}.{}", case.slug, input.name)
}

pub(crate) fn output_point(case: &BlockCase, output_idx: usize) -> String {
    // The authored `@id` this harness emits for the block's output connector in `build_cxf`;
    // the facade point path is that subject IRI.
    format!(
        "http://example.org#{}.block.{}",
        case.slug, case.outputs[output_idx].name
    )
}

fn build_cxf(case: &BlockCase) -> String {
    let model_id = format!("http://example.org#{}", case.slug);
    let block_id = format!("{model_id}.block");
    let output_ids: Vec<String> = case
        .outputs
        .iter()
        .map(|output| format!("{block_id}.{}", output.name))
        .collect();
    let model_output_ids: Vec<String> = case
        .outputs
        .iter()
        .map(|output| format!("{model_id}.{}", output.name))
        .collect();
    let mut nodes = Vec::new();

    nodes.push(model_node(case, &model_id, &block_id, &model_output_ids));
    nodes.push(block_node(case, &block_id, &output_ids));
    nodes.extend(case.params.iter().map(|param| param_node(&block_id, param)));
    for input in case.inputs {
        nodes.push(block_input_node(&block_id, input));
        nodes.push(model_input_node(case, &model_id, &block_id, input));
    }
    for (output, (output_id, model_output_id)) in case
        .outputs
        .iter()
        .zip(output_ids.iter().zip(model_output_ids.iter()))
    {
        nodes.push(block_output_node(output, output_id, model_output_id));
        nodes.push(model_output_node(output, model_output_id));
    }

    format!(
        r#"{{
  "@context": {{
    "S231": "http://data.ashrae.org/S231P#",
    "base": "http://example.org#"
  }},
  "@graph": [
{}
  ]
}}
"#,
        nodes.join(",\n")
    )
}

fn model_node(
    case: &BlockCase,
    model_id: &str,
    block_id: &str,
    model_output_ids: &[String],
) -> String {
    let mut fields = vec![
        format!(r#""@id": "{model_id}""#),
        r#""@type": "S231:Block""#.to_string(),
        format!(r#""S231:label": "{}""#, case.slug),
        format!(r#""S231:containsBlock": {{ "@id": "{block_id}" }}"#),
    ];
    if !case.inputs.is_empty() {
        fields.push(format!(
            r#""S231:hasInput": {}"#,
            id_refs(
                &case
                    .inputs
                    .iter()
                    .map(|input| input_point(case, input))
                    .collect::<Vec<_>>()
            )
        ));
    }
    fields.push(format!(
        r#""S231:hasOutput": {}"#,
        id_refs(model_output_ids)
    ));
    object(fields)
}

fn block_node(case: &BlockCase, block_id: &str, output_ids: &[String]) -> String {
    let mut fields = vec![
        format!(r#""@id": "{block_id}""#),
        format!(
            r#""@type": "http://example.org#Buildings.Controls.OBC.{}""#,
            case.class_path
        ),
        r#""S231:label": "block""#.to_string(),
    ];
    if !case.params.is_empty() {
        fields.push(format!(
            r#""S231:hasParameter": {}"#,
            id_refs(
                &case
                    .params
                    .iter()
                    .map(|param| format!("{block_id}.{}", param.name))
                    .collect::<Vec<_>>()
            )
        ));
    }
    if !case.inputs.is_empty() {
        fields.push(format!(
            r#""S231:hasInput": {}"#,
            id_refs(
                &case
                    .inputs
                    .iter()
                    .map(|input| format!("{block_id}.{}", input.name))
                    .collect::<Vec<_>>()
            )
        ));
    }
    fields.push(format!(r#""S231:hasOutput": {}"#, id_refs(output_ids)));
    object(fields)
}

fn param_node(block_id: &str, param: &Param) -> String {
    let (value, ty) = match param.value {
        ParamValue::Real(value) => (value, "http://www.w3.org/2001/XMLSchema#double"),
        ParamValue::Integer(value) => (value, "http://www.w3.org/2001/XMLSchema#integer"),
        ParamValue::Boolean(value) => (value, "http://www.w3.org/2001/XMLSchema#boolean"),
    };
    object(vec![
        format!(r#""@id": "{block_id}.{}""#, param.name),
        format!(r#""S231:value": {{ "@value": "{value}", "@type": "{ty}" }}"#),
    ])
}

fn block_input_node(block_id: &str, input: &Port) -> String {
    object(vec![
        format!(r#""@id": "{block_id}.{}""#, input.name),
        format!(r#""@type": "{}""#, input.kind.cxf_type("input")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            input.kind.cxf_data_type()
        ),
    ])
}

fn model_input_node(case: &BlockCase, model_id: &str, block_id: &str, input: &Port) -> String {
    object(vec![
        format!(r#""@id": "{}""#, input_point(case, input)),
        format!(r#""@type": "{}""#, input.kind.cxf_type("input")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            input.kind.cxf_data_type()
        ),
        format!(
            r#""S231:isConnectedTo": {{ "@id": "{block_id}.{}" }}"#,
            input.name
        ),
        format!(
            r#""S231:label": "{}.{}""#,
            model_id.rsplit('#').next().unwrap(),
            input.name
        ),
    ])
}

fn block_output_node(output: &Port, output_id: &str, model_output_id: &str) -> String {
    object(vec![
        format!(r#""@id": "{output_id}""#),
        format!(r#""@type": "{}""#, output.kind.cxf_type("output")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            output.kind.cxf_data_type()
        ),
        format!(r#""S231:isConnectedTo": {{ "@id": "{model_output_id}" }}"#),
    ])
}

fn model_output_node(output: &Port, model_output_id: &str) -> String {
    object(vec![
        format!(r#""@id": "{model_output_id}""#),
        format!(r#""@type": "{}""#, output.kind.cxf_type("output")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            output.kind.cxf_data_type()
        ),
    ])
}

fn id_refs(ids: &[String]) -> String {
    if ids.len() == 1 {
        format!(r#"{{ "@id": "{}" }}"#, ids[0])
    } else {
        format!(
            "[{}]",
            ids.iter()
                .map(|id| format!(r#"{{ "@id": "{id}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn object(fields: Vec<String>) -> String {
    format!("    {{\n      {}\n    }}", fields.join(",\n      "))
}

fn assert_trace_bit_eq(left: &oce_conformance::DriverRun, right: &oce_conformance::DriverRun) {
    assert_eq!(bits(&left.trace.times), bits(&right.trace.times));
    assert_eq!(left.trace.columns.len(), right.trace.columns.len());
    for (left, right) in left.trace.columns.iter().zip(&right.trace.columns) {
        assert_eq!(left.name, right.name);
        assert_eq!(left.kind, right.kind);
        assert_eq!(bits(&left.values), bits(&right.values));
    }
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|value| value.to_bits()).collect()
}
