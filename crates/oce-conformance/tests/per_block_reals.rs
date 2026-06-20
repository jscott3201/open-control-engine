//! Tier-1 exact conformance for CDL.Reals blocks through the frozen facade.

use std::path::PathBuf;

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriverOptions, OutputPattern,
    PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, VerifyConfig,
    drive_trace_with_options,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SignalKind {
    Real,
    Boolean,
}

impl SignalKind {
    fn cxf_type(self, dir: &str) -> &'static str {
        match (self, dir) {
            (SignalKind::Real, "input") => "S231:RealInput",
            (SignalKind::Real, "output") => "S231:RealOutput",
            (SignalKind::Boolean, "input") => "S231:BooleanInput",
            (SignalKind::Boolean, "output") => "S231:BooleanOutput",
            _ => unreachable!("test builder passes input/output only"),
        }
    }

    fn cxf_data_type(self) -> &'static str {
        match self {
            SignalKind::Real => "S231:Real",
            SignalKind::Boolean => "S231:Boolean",
        }
    }

    fn config_type(self) -> &'static str {
        match self {
            SignalKind::Real => "Real",
            SignalKind::Boolean => "Boolean",
        }
    }
}

#[derive(Copy, Clone)]
struct Port {
    name: &'static str,
    kind: SignalKind,
}

#[derive(Copy, Clone)]
struct Param {
    name: &'static str,
    value: ParamValue,
}

#[derive(Copy, Clone)]
enum ParamValue {
    Real(&'static str),
}

#[derive(Copy, Clone)]
struct BlockCase {
    slug: &'static str,
    class_path: &'static str,
    reference_path: &'static str,
    inputs: &'static [Port],
    params: &'static [Param],
    output: Port,
}

const R: SignalKind = SignalKind::Real;
const B: SignalKind = SignalKind::Boolean;

const U: &[Port] = &[Port { name: "u", kind: R }];
const U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: R,
    },
];
const LINE_INPUTS: &[Port] = &[
    Port {
        name: "x1",
        kind: R,
    },
    Port {
        name: "f1",
        kind: R,
    },
    Port {
        name: "x2",
        kind: R,
    },
    Port {
        name: "f2",
        kind: R,
    },
    Port { name: "u", kind: R },
];
const SWITCH_INPUTS: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: B,
    },
    Port {
        name: "u3",
        kind: R,
    },
];

const REAL_Y: Port = Port { name: "y", kind: R };
const BOOL_Y: Port = Port { name: "y", kind: B };

const P_02: &[Param] = &[Param {
    name: "p",
    value: ParamValue::Real("0.2"),
}];
const K_01: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Real("0.1"),
}];
const CONSTANT_K: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Real("21.5"),
}];
const LIMITER_PARAMS: &[Param] = &[
    Param {
        name: "uMin",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "uMax",
        value: ParamValue::Real("5.5"),
    },
];
const THRESHOLD_PARAMS: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Real("4.5"),
}];
const HYSTERESIS_PARAMS: &[Param] = &[
    Param {
        name: "uLow",
        value: ParamValue::Real("1.5"),
    },
    Param {
        name: "uHigh",
        value: ParamValue::Real("3.5"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "reals_constant",
        "CDL.Reals.Sources.Constant",
        "Sources/Constant",
        &[],
        CONSTANT_K,
        REAL_Y,
    ),
    case("reals_add", "CDL.Reals.Add", "Add", U1_U2, &[], REAL_Y),
    case(
        "reals_subtract",
        "CDL.Reals.Subtract",
        "Subtract",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_multiply",
        "CDL.Reals.Multiply",
        "Multiply",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_divide",
        "CDL.Reals.Divide",
        "Divide",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_add_parameter",
        "CDL.Reals.AddParameter",
        "AddParameter",
        U,
        P_02,
        REAL_Y,
    ),
    case(
        "reals_multiply_by_parameter",
        "CDL.Reals.MultiplyByParameter",
        "MultiplyByParameter",
        U,
        K_01,
        REAL_Y,
    ),
    case("reals_abs", "CDL.Reals.Abs", "Abs", U, &[], REAL_Y),
    case("reals_min", "CDL.Reals.Min", "Min", U1_U2, &[], REAL_Y),
    case("reals_max", "CDL.Reals.Max", "Max", U1_U2, &[], REAL_Y),
    case(
        "reals_limiter",
        "CDL.Reals.Limiter",
        "Limiter",
        U,
        LIMITER_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_line",
        "CDL.Reals.Line",
        "Line",
        LINE_INPUTS,
        &[],
        REAL_Y,
    ),
    case(
        "reals_greater",
        "CDL.Reals.Greater",
        "Greater",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "reals_greater_threshold",
        "CDL.Reals.GreaterThreshold",
        "GreaterThreshold",
        U,
        THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_hysteresis",
        "CDL.Reals.Hysteresis",
        "Hysteresis",
        U,
        HYSTERESIS_PARAMS,
        BOOL_Y,
    ),
    case("reals_less", "CDL.Reals.Less", "Less", U1_U2, &[], BOOL_Y),
    case(
        "reals_less_threshold",
        "CDL.Reals.LessThreshold",
        "LessThreshold",
        U,
        THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_switch",
        "CDL.Reals.Switch",
        "Switch",
        SWITCH_INPUTS,
        &[],
        REAL_Y,
    ),
];

const fn case(
    slug: &'static str,
    class_path: &'static str,
    reference_path: &'static str,
    inputs: &'static [Port],
    params: &'static [Param],
    output: Port,
) -> BlockCase {
    BlockCase {
        slug,
        class_path,
        reference_path,
        inputs,
        params,
        output,
    }
}

#[test]
fn reals_reference_blocks_match_exact_oracle() {
    for case in CASES {
        let reference = read_reference(case);
        let cxf = build_cxf(case);
        let run = drive_case(case, &cxf, &reference);

        assert_eq!(run.comparisons.len(), 1, "{}", case.slug);
        let comparison = &run.comparisons[0];
        assert_eq!(comparison.output, output_point(case));
        assert_eq!(comparison.reference_column, "y");
        assert!(!comparison.masked, "exact per-block comparison is unmasked");

        let ComparisonResult::Exact(result) = &comparison.result else {
            panic!("{} did not use exact comparison", case.slug);
        };
        assert!(result.passed, "{} mismatch: {:?}", case.slug, result);
        assert_eq!(result.compared_points, reference.n_rows, "{}", case.slug);
        assert_eq!(result.first_mismatch, None, "{}", case.slug);
    }
}

#[test]
fn stateful_reals_block_exact_run_is_deterministic() {
    let case = CASES
        .iter()
        .find(|case| case.slug == "reals_hysteresis")
        .expect("hysteresis case");
    let reference = read_reference(case);
    let cxf = build_cxf(case);
    let first = drive_case(case, &cxf, &reference);
    let second = drive_case(case, &cxf, &reference);

    assert_trace_bit_eq(&first, &second);
    assert_eq!(first.comparisons, second.comparisons);
}

fn read_reference(case: &BlockCase) -> CombiTimeTable {
    let path = reference_path(case);
    CombiTimeTable::read(&path)
        .unwrap_or_else(|err| panic!("{} reference should parse: {err:?}", case.slug))
}

fn reference_path(case: &BlockCase) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/golden-gen/goldens/CDL/Reals")
        .join(case.reference_path)
        .join("reference.csv")
}

fn drive_case(
    case: &BlockCase,
    cxf: &str,
    reference: &CombiTimeTable,
) -> oce_conformance::DriverRun {
    drive_trace_with_options(
        cxf.as_bytes(),
        &config(case),
        reference,
        &DriverOptions {
            comparison: ComparisonMode::Exact,
            ..DriverOptions::default()
        },
    )
    .unwrap_or_else(|err| panic!("{} should drive through exact mode: {err:?}", case.slug))
}

fn config(case: &BlockCase) -> VerifyConfig {
    let mut point_name_mapping = Vec::with_capacity(case.inputs.len() + 1);
    for input in case.inputs {
        point_name_mapping.push(map(input.name, &input_point(case, input), input.kind));
    }
    point_name_mapping.push(map("y", &output_point(case), case.output.kind));

    VerifyConfig {
        references: vec![ReferenceSpec {
            model: case.slug.to_string(),
            sequence: "single-block-reals".to_string(),
            point_name_mapping,
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: vec![OutputPattern {
            pattern: output_point(case),
            tolerances: PartialTolerances::default(),
        }],
        indicators: Vec::new(),
        sampling: None,
        run_controller: true,
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

fn output_point(case: &BlockCase) -> String {
    format!("conn#{}", case.inputs.len())
}

fn build_cxf(case: &BlockCase) -> String {
    let model_id = format!("http://example.org#{}", case.slug);
    let block_id = format!("{model_id}.block");
    let output_id = format!("{block_id}.y");
    let model_output_id = format!("{model_id}.y");
    let mut nodes = Vec::new();

    nodes.push(model_node(case, &model_id, &block_id, &model_output_id));
    nodes.push(block_node(case, &block_id, &output_id));
    nodes.extend(case.params.iter().map(|param| param_node(&block_id, param)));
    for input in case.inputs {
        nodes.push(block_input_node(&block_id, input));
        nodes.push(model_input_node(case, &model_id, &block_id, input));
    }
    nodes.push(block_output_node(case, &output_id, &model_output_id));
    nodes.push(model_output_node(case, &model_output_id));

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

fn model_node(case: &BlockCase, model_id: &str, block_id: &str, model_output_id: &str) -> String {
    let mut fields = vec![
        format!(r#""@id": "{model_id}""#),
        r#""@type": "S231:Block""#.to_string(),
        format!(r#""S231:label": "{}""#, case.slug),
        format!(r#""S231:containsBlock": {{ "@id": "{block_id}" }}"#),
    ];
    if !case.inputs.is_empty() {
        fields.push(format!(
            r#""S231:hasInput": [{}]"#,
            case.inputs
                .iter()
                .map(|input| format!(r#"{{ "@id": "{}" }}"#, input_point(case, input)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    fields.push(format!(
        r#""S231:hasOutput": {{ "@id": "{model_output_id}" }}"#
    ));
    object(fields)
}

fn block_node(case: &BlockCase, block_id: &str, output_id: &str) -> String {
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
            r#""S231:hasParameter": [{}]"#,
            case.params
                .iter()
                .map(|param| format!(r#"{{ "@id": "{block_id}.{}" }}"#, param.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !case.inputs.is_empty() {
        fields.push(format!(
            r#""S231:hasInput": [{}]"#,
            case.inputs
                .iter()
                .map(|input| format!(r#"{{ "@id": "{block_id}.{}" }}"#, input.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    fields.push(format!(r#""S231:hasOutput": {{ "@id": "{output_id}" }}"#));
    object(fields)
}

fn param_node(block_id: &str, param: &Param) -> String {
    let (value, ty) = match param.value {
        ParamValue::Real(value) => (value, "http://www.w3.org/2001/XMLSchema#double"),
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

fn block_output_node(case: &BlockCase, output_id: &str, model_output_id: &str) -> String {
    object(vec![
        format!(r#""@id": "{output_id}""#),
        format!(r#""@type": "{}""#, case.output.kind.cxf_type("output")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            case.output.kind.cxf_data_type()
        ),
        format!(r#""S231:isConnectedTo": {{ "@id": "{model_output_id}" }}"#),
    ])
}

fn model_output_node(case: &BlockCase, model_output_id: &str) -> String {
    object(vec![
        format!(r#""@id": "{model_output_id}""#),
        format!(r#""@type": "{}""#, case.output.kind.cxf_type("output")),
        format!(
            r#""S231:isOfDataType": {{ "@id": "{}" }}"#,
            case.output.kind.cxf_data_type()
        ),
    ])
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
