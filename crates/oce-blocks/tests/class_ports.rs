//! Resolved port names, class descriptions and the settable flag (GitHub #440).
//!
//! `tests/fixtures/class_ports.golden.txt` pins, byte for byte:
//!
//! 1. the contract port names of every registered class at the empty parameter table;
//! 2. the names of every width-driven class at widths 1, 2 and the class maximum
//!    ([`MAX_RESOLVED_PORT_WIDTH`], or the widest product a two-parameter class accepts), with the
//!    middle of a long port list elided to its count;
//! 3. the three `TimeTable` shapes, whose output arity follows the table's column count;
//! 4. every class's one-sentence description and every parameter's settable flag.
//!
//! Regenerate deliberately with `OCE_BLESS_CLASS_PORTS=1 cargo nextest run -p oce-blocks --test
//! class_ports`, rerun without the switch, and review the diff. Every assertion except the byte
//! comparison runs on freshly resolved ports, so a stale or wrong re-bless cannot hide a defect
//! the property tests catch.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use oce_blocks::{DefaultLiteral, DefaultSource};
use oce_blocks::{
    MAX_RESOLVED_PORT_WIDTH, PortKind, PortNaming, PortSide, ResolvedPorts, ResolvedPortsError,
    catalog, class_description, class_parameters, resolved_ports,
};
use oce_model::{ParamTable, Value, ValueType, enum_descriptor};

const MAX: i64 = MAX_RESOLVED_PORT_WIDTH as i64;

/// Port lists longer than this print their first and last [`EDGE`] names around a count.
const LISTED: usize = 8;
const EDGE: usize = 3;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/class_ports.golden.txt")
}

fn table(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn int(name: &str, value: i64) -> (&str, Value) {
    (name, Value::Integer(value))
}

/// One width scenario: a class, its parameters, and the arity they must resolve to.
struct Scenario {
    class_path: &'static str,
    params: Vec<(&'static str, Value)>,
    inputs: usize,
    outputs: usize,
}

fn scenario(
    class_path: &'static str,
    params: Vec<(&'static str, Value)>,
    (inputs, outputs): (i64, i64),
) -> Scenario {
    Scenario {
        class_path,
        params,
        inputs: inputs as usize,
        outputs: outputs as usize,
    }
}

/// The five width-driven `CDL.Routing` classes of each signal kind.
const ROUTING_FAMILIES: [[&str; 5]; 3] = [
    [
        "CDL.Routing.BooleanExtractSignal",
        "CDL.Routing.BooleanExtractor",
        "CDL.Routing.BooleanScalarReplicator",
        "CDL.Routing.BooleanVectorFilter",
        "CDL.Routing.BooleanVectorReplicator",
    ],
    [
        "CDL.Routing.IntegerExtractSignal",
        "CDL.Routing.IntegerExtractor",
        "CDL.Routing.IntegerScalarReplicator",
        "CDL.Routing.IntegerVectorFilter",
        "CDL.Routing.IntegerVectorReplicator",
    ],
    [
        "CDL.Routing.RealExtractSignal",
        "CDL.Routing.RealExtractor",
        "CDL.Routing.RealScalarReplicator",
        "CDL.Routing.RealVectorFilter",
        "CDL.Routing.RealVectorReplicator",
    ],
];

/// Widths 1, 2 and the maximum for every width-driven class, with the arity each must resolve to,
/// derived from the class's CDL declaration rather than from the engine.
fn width_scenarios() -> Vec<Scenario> {
    let mut all = Vec::new();
    for class in [
        "CDL.Reals.MultiMax",
        "CDL.Reals.MultiMin",
        "CDL.Reals.MultiSum",
        "CDL.Integers.MultiSum",
        "CDL.Logical.MultiAnd",
        "CDL.Logical.MultiOr",
    ] {
        for n in [1, 2, MAX] {
            all.push(scenario(class, vec![int("nin", n)], (n, 1)));
        }
    }
    for (nin, nout) in [(1, 1), (2, 2), (MAX, 1), (1, MAX)] {
        let params = vec![int("nin", nin), int("nout", nout)];
        all.push(scenario("CDL.Reals.MatrixGain", params, (nin, nout)));
    }
    for (class, flag) in [
        ("CDL.Reals.MatrixMax", "rowMax"),
        ("CDL.Reals.MatrixMin", "rowMin"),
    ] {
        for (rows, cols) in [(1, 1), (2, 3), (MAX, 1), (1, MAX)] {
            for by_row in [true, false] {
                let params = vec![
                    int("nRow", rows),
                    int("nCol", cols),
                    (flag, Value::Boolean(by_row)),
                ];
                let outputs = if by_row { rows } else { cols };
                all.push(scenario(class, params, (rows * cols, outputs)));
            }
        }
    }
    for n in [1, 2, MAX] {
        all.push(scenario("CDL.Reals.Sort", vec![int("nin", n)], (n, 2 * n)));
    }
    for [
        extract_signal,
        extractor,
        scalar_replicator,
        vector_filter,
        vector_replicator,
    ] in ROUTING_FAMILIES
    {
        for n in [1, 2, MAX] {
            let params = vec![int("nin", n), int("nout", n)];
            all.push(scenario(extract_signal, params, (n, n)));
            all.push(scenario(extractor, vec![int("nin", n)], (n + 1, 1)));
            all.push(scenario(scalar_replicator, vec![int("nout", n)], (1, n)));
            let params = vec![int("nin", n), int("nout", n)];
            all.push(scenario(vector_filter, params, (n, n)));
        }
        // A mask that drops an element: three inputs, two outputs.
        let params = vec![
            int("nin", 3),
            int("nout", 2),
            ("msk_2", Value::Boolean(false)),
        ];
        all.push(scenario(vector_filter, params, (3, 2)));
        for (nin, nout) in [(1, 1), (2, 2), (2, 3), (MAX, 1), (1, MAX)] {
            let params = vec![int("nin", nin), int("nout", nout)];
            all.push(scenario(vector_replicator, params, (nin, nin * nout)));
        }
    }
    all
}

/// A `rows x cols` time table whose value cells hold `cell(row, col)`.
fn time_table(rows: usize, cols: usize, cell: fn(usize, usize) -> f64) -> Vec<(String, Value)> {
    let mut values = Vec::new();
    for row in 1..=rows {
        for col in 1..=cols {
            let value = if col == 1 {
                (row - 1) as f64
            } else {
                cell(row, col)
            };
            values.push((format!("table_{row}_{col}"), Value::Real(value)));
        }
    }
    values
}

/// A `TimeTable` class, its flattened table parameters, and its expected output count.
type TimeTableScenario = (&'static str, Vec<(String, Value)>, usize);

/// The three `TimeTable` classes at one, two and three output columns. Positional: the output
/// arity is the table's column count minus the time column.
fn time_table_scenarios() -> Vec<TimeTableScenario> {
    let mut all = Vec::new();
    for (class, cell, period) in [
        (
            "CDL.Reals.Sources.TimeTable",
            (|row, col| (row * 10 + col) as f64) as fn(usize, usize) -> f64,
            false,
        ),
        (
            "CDL.Integers.Sources.TimeTable",
            |row, col| (row + col) as f64,
            true,
        ),
        (
            "CDL.Logical.Sources.TimeTable",
            |row, col| ((row + col) % 2) as f64,
            true,
        ),
    ] {
        for (rows, cols) in [(2, 2), (3, 3), (2, 4)] {
            let mut values = time_table(rows, cols, cell);
            if period {
                values.push(("period".to_owned(), Value::Real(10.0)));
            }
            all.push((class, values, cols - 1));
        }
    }
    all
}

fn kind_label(kind: PortKind) -> &'static str {
    match kind {
        PortKind::Real => "Real",
        PortKind::Integer => "Integer",
        PortKind::Boolean => "Boolean",
    }
}

fn render_side(out: &mut String, label: &str, ports: &[oce_blocks::ResolvedPort]) {
    let item = |port: &oce_blocks::ResolvedPort| format!("{}:{}", port.name, kind_label(port.kind));
    let _ = write!(out, "  {label} ({})", ports.len());
    if ports.len() <= LISTED {
        for port in ports {
            let _ = write!(out, " {}", item(port));
        }
    } else {
        for port in &ports[..EDGE] {
            let _ = write!(out, " {}", item(port));
        }
        let _ = write!(out, " ...");
        for port in &ports[ports.len() - EDGE..] {
            let _ = write!(out, " {}", item(port));
        }
    }
    out.push('\n');
}

fn render_ports(out: &mut String, heading: &str, ports: &ResolvedPorts) {
    let _ = writeln!(out, "{heading}");
    render_side(out, "in ", &ports.inputs);
    render_side(out, "out", &ports.outputs);
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Integer(v) => v.to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::Real(v) => format!("{v:?}"),
        other => format!("{other:?}"),
    }
}

fn render_type(value_type: ValueType) -> String {
    match value_type {
        ValueType::Real => "Real".to_owned(),
        ValueType::Integer => "Integer".to_owned(),
        ValueType::Boolean => "Boolean".to_owned(),
        ValueType::String => "String".to_owned(),
        ValueType::Enum(class) => enum_descriptor(class)
            .map_or_else(|| format!("Enum#{}", class.0), |d| d.class_path.to_owned()),
    }
}

fn render_default(default: DefaultSource) -> String {
    match default {
        DefaultSource::Literal(DefaultLiteral::Real(v)) => format!("{v:?}"),
        DefaultSource::Literal(DefaultLiteral::Integer(v)) => v.to_string(),
        DefaultSource::Literal(DefaultLiteral::Boolean(v)) => v.to_string(),
        DefaultSource::Literal(DefaultLiteral::Str(v)) => format!("{v:?}"),
        DefaultSource::Literal(DefaultLiteral::EnumMember(v)) => v.to_owned(),
        DefaultSource::Derived { formula } => format!("derived `{formula}`"),
        DefaultSource::Required => "required".to_owned(),
    }
}

fn render() -> String {
    let mut out = String::from(
        "# Contract port names, descriptions and settable flags (GitHub #440).\n\
         # Regenerate: OCE_BLESS_CLASS_PORTS=1 cargo nextest run -p oce-blocks --test class_ports\n",
    );
    out.push_str("\n## Every class at the empty parameter table\n\n");
    for entry in catalog() {
        let ports = resolved_ports(entry.class_path, &ParamTable::default()).expect("resolves");
        render_ports(&mut out, entry.class_path, &ports);
    }
    out.push_str("\n## Width-driven classes at widths 1, 2 and the maximum\n\n");
    for scenario in width_scenarios() {
        let ports = resolved_ports(scenario.class_path, &table(&scenario.params)).expect("ok");
        let params: Vec<String> = scenario
            .params
            .iter()
            .map(|(name, value)| format!("{name}={}", render_value(value)))
            .collect();
        let heading = format!("{} {}", scenario.class_path, params.join(" "));
        render_ports(&mut out, &heading, &ports);
    }
    out.push_str("\n## TimeTable shapes (positional; one output per value column)\n\n");
    for (class, values, _) in time_table_scenarios() {
        let params = ParamTable {
            values: values
                .iter()
                .map(|(name, value)| (Arc::from(name.as_str()), value.clone()))
                .collect(),
        };
        let ports = resolved_ports(class, &params).expect("ok");
        let columns = values
            .iter()
            .filter(|(name, _)| name.starts_with("table_1_"))
            .count();
        let rows = values
            .iter()
            .filter(|(name, _)| name.starts_with("table_") && name.ends_with("_1"))
            .count();
        render_ports(&mut out, &format!("{class} table={rows}x{columns}"), &ports);
    }
    out.push_str(
        "\n## Descriptions and parameters (`name: type = default`; `-` marks a parameter an author \
         may not set)\n\n",
    );
    for entry in catalog() {
        let description = class_description(entry.class_path).expect("described");
        let _ = writeln!(out, "{}\n  \"{description}\"", entry.class_path);
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let mark = if parameter.settable { '+' } else { '-' };
            let _ = writeln!(
                out,
                "  {mark} {}: {} = {}",
                parameter.name,
                render_type(parameter.value_type),
                render_default(parameter.default)
            );
        }
    }
    out
}

#[test]
fn checked_in_golden_matches_resolved_ports_and_metadata() {
    let generated = render();
    assert_eq!(generated, render(), "rendering is deterministic");
    if oce_bless::enabled("OCE_BLESS_CLASS_PORTS") {
        std::fs::create_dir_all(golden_path().parent().expect("parent")).expect("mkdir");
        std::fs::write(golden_path(), &generated).expect("write golden");
        return;
    }
    let checked_in = std::fs::read_to_string(golden_path()).expect("golden is checked in");
    if generated != checked_in {
        let line = generated
            .lines()
            .zip(checked_in.lines())
            .position(|(fresh, old)| fresh != old);
        panic!(
            "tests/fixtures/class_ports.golden.txt is stale (first differing line index \
             {line:?}). Re-bless deliberately with `OCE_BLESS_CLASS_PORTS=1 cargo nextest run -p \
             oce-blocks --test class_ports` and review the diff."
        );
    }
}

#[test]
fn every_registered_class_resolves_ports_for_its_default_table() {
    assert_eq!(catalog().len(), 136);
    for entry in catalog() {
        let ports = resolved_ports(entry.class_path, &ParamTable::default())
            .unwrap_or_else(|error| panic!("{}: {error}", entry.class_path));
        assert_eq!(ports.class_path, entry.class_path);
        for (named, listed) in [
            (&ports.inputs, &entry.inputs),
            (&ports.outputs, &entry.outputs),
        ] {
            let kinds: Vec<PortKind> = named.iter().map(|port| port.kind).collect();
            let catalog_kinds: Vec<PortKind> = listed.iter().map(|port| port.kind).collect();
            assert_eq!(kinds, catalog_kinds, "{}", entry.class_path);
            if entry.naming == PortNaming::Named {
                // A named class's contract names are exactly its declared CDL names.
                let names: Vec<&str> = named.iter().map(|port| port.name.as_str()).collect();
                let declared: Vec<&str> = listed.iter().map(|port| port.name.unwrap()).collect();
                assert_eq!(names, declared, "{}", entry.class_path);
            }
        }
    }
}

#[test]
fn width_driven_classes_resolve_the_expected_count_and_names_at_every_width() {
    let scenarios = width_scenarios();
    let mut classes: Vec<&str> = scenarios.iter().map(|s| s.class_path).collect();
    classes.sort_unstable();
    classes.dedup();
    let mut width_driven: Vec<&str> = catalog()
        .iter()
        .filter(|entry| entry.naming == PortNaming::WidthDriven)
        .map(|entry| entry.class_path)
        .collect();
    width_driven.sort_unstable();
    assert_eq!(
        classes, width_driven,
        "every width-driven class has scenarios"
    );
    assert_eq!(width_driven.len(), 25);
    let mut at_max = 0;
    for scenario in &scenarios {
        let ports = resolved_ports(scenario.class_path, &table(&scenario.params)).expect("ok");
        assert_eq!(
            (ports.inputs.len(), ports.outputs.len()),
            (scenario.inputs, scenario.outputs),
            "{} {:?}",
            scenario.class_path,
            scenario.params
        );
        let widest = scenario.inputs.max(scenario.outputs);
        if widest >= MAX_RESOLVED_PORT_WIDTH {
            at_max += 1;
        }
    }
    // Every width-driven class reaches the port-width maximum in at least one scenario.
    assert!(at_max >= 25, "{at_max} scenarios at the maximum");
}

#[test]
fn vector_and_matrix_names_follow_the_corpus_spelling() {
    let names = |class: &str, params: &[(&str, Value)], side: PortSide| -> Vec<String> {
        let ports = resolved_ports(class, &table(params)).expect("ok");
        let side = match side {
            PortSide::Input => ports.inputs,
            PortSide::Output => ports.outputs,
        };
        side.into_iter().map(|port| port.name).collect()
    };
    // Vectors: base, then the 1-based index with no separator (the corpus's `mulSum.u1`).
    assert_eq!(
        names("CDL.Reals.MultiSum", &[int("nin", 3)], PortSide::Input),
        ["u1", "u2", "u3"]
    );
    assert_eq!(
        names(
            "CDL.Routing.BooleanScalarReplicator",
            &[int("nout", 4)],
            PortSide::Output
        ),
        ["y1", "y2", "y3", "y4"]
    );
    // The scalar `index` leads, as upstream declares it before `u[nin]`.
    assert_eq!(
        names(
            "CDL.Routing.RealExtractor",
            &[int("nin", 2)],
            PortSide::Input
        ),
        ["index", "u1", "u2"]
    );
    // Sort: both output vectors share the input width, `y` before `yIdx`.
    assert_eq!(
        names("CDL.Reals.Sort", &[int("nin", 2)], PortSide::Output),
        ["y1", "y2", "yIdx1", "yIdx2"]
    );
    // Matrices: row-major, last index fastest.
    assert_eq!(
        names(
            "CDL.Routing.IntegerVectorReplicator",
            &[int("nin", 2), int("nout", 3)],
            PortSide::Output
        ),
        ["y1_1", "y1_2", "y2_1", "y2_2", "y3_1", "y3_2"]
    );
    let matrix_max = |by_row: bool| {
        let params = [
            int("nRow", 2),
            int("nCol", 3),
            ("rowMax", Value::Boolean(by_row)),
        ];
        (
            names("CDL.Reals.MatrixMax", &params, PortSide::Input),
            names("CDL.Reals.MatrixMax", &params, PortSide::Output),
        )
    };
    let cells = ["u1_1", "u1_2", "u1_3", "u2_1", "u2_2", "u2_3"];
    assert_eq!(
        matrix_max(true),
        (
            cells.map(String::from).to_vec(),
            vec!["y1".into(), "y2".into()]
        )
    );
    assert_eq!(
        matrix_max(false),
        (
            cells.map(String::from).to_vec(),
            vec!["y1".into(), "y2".into(), "y3".into()]
        )
    );
    // The reduction flag defaults to the catalog's literal (rows) when unbound or mistyped.
    for flag in [None, Some(Value::Integer(0)), Some(Value::Real(0.0))] {
        let mut params = vec![int("nRow", 2), int("nCol", 3)];
        params.extend(flag.map(|value| ("rowMin", value)));
        assert_eq!(
            names("CDL.Reals.MatrixMin", &params, PortSide::Output),
            ["y1", "y2"]
        );
    }
}

#[test]
fn time_table_output_arity_follows_the_table_shape() {
    for (class, values, outputs) in time_table_scenarios() {
        let params = ParamTable {
            values: values
                .iter()
                .map(|(name, value)| (Arc::from(name.as_str()), value.clone()))
                .collect(),
        };
        let ports = resolved_ports(class, &params).expect("ok");
        assert!(ports.inputs.is_empty(), "{class}");
        let names: Vec<String> = ports.outputs.iter().map(|port| port.name.clone()).collect();
        let expected: Vec<String> = (1..=outputs).map(|i| format!("y{i}")).collect();
        assert_eq!(names, expected, "{class}");
    }
    // No table at all: the constructor's one-output fallback, named `y1`.
    for class in [
        "CDL.Reals.Sources.TimeTable",
        "CDL.Integers.Sources.TimeTable",
        "CDL.Logical.Sources.TimeTable",
    ] {
        let ports = resolved_ports(class, &ParamTable::default()).expect("ok");
        assert_eq!(ports.outputs.len(), 1);
        assert_eq!(ports.outputs[0].name, "y1");
    }
}

#[test]
fn widths_the_load_gate_refuses_resolve_to_what_the_constructor_builds() {
    // The load gate refuses these tables; the pure function reports the constructor's result
    // instead of inventing names for ports that do not exist: a negative width builds no vector
    // ports, and a width past the cap is clamped to it.
    for (n, width) in [
        (-1, 0),
        (i64::MIN, 0),
        (MAX + 1, MAX_RESOLVED_PORT_WIDTH),
        (i64::MAX, MAX_RESOLVED_PORT_WIDTH),
    ] {
        let ports = resolved_ports("CDL.Reals.MultiSum", &table(&[int("nin", n)])).expect("ok");
        assert_eq!(ports.inputs.len(), width, "nin={n}");
        assert_eq!(ports.outputs.len(), 1);
    }
    // A replicator product above the width cap keeps its input vector and drops every output.
    let ports = resolved_ports(
        "CDL.Routing.RealVectorReplicator",
        &table(&[int("nin", 2), int("nout", MAX)]),
    )
    .expect("ok");
    assert_eq!((ports.inputs.len(), ports.outputs.len()), (2, 0));
}

#[test]
fn unknown_classes_are_a_typed_error() {
    for class in [
        "",
        "CDL.Reals.NoSuchBlock",
        "Buildings.Controls.OBC.CDL.Reals.MultiSum",
        "cdl.reals.multisum",
        "CDL.Reals.MultiSum\0",
        "urn:oce:lowering#PassThrough.String",
    ] {
        assert_eq!(
            resolved_ports(class, &ParamTable::default()),
            Err(ResolvedPortsError::UnknownClass),
            "{class:?}"
        );
    }
}

#[test]
fn hostile_parameter_values_never_panic_and_never_misname() {
    let hostile = [
        Value::Real(f64::NAN),
        Value::Real(f64::INFINITY),
        Value::Real(-0.0),
        Value::Real(2.5),
        Value::Integer(0),
        Value::Integer(-1),
        Value::Boolean(true),
        Value::String(Arc::from("2")),
    ];
    for entry in catalog() {
        for name in [
            "nin",
            "nout",
            "nRow",
            "nCol",
            "rowMax",
            "rowMin",
            "table_1_1",
            "period",
        ] {
            for value in &hostile {
                let params = table(&[(name, value.clone())]);
                let ports = resolved_ports(entry.class_path, &params).expect("nameable");
                let block = (oce_blocks::lookup(entry.class_path).unwrap().make)(&params);
                let signature = block.resolved_signature();
                assert_eq!(ports.inputs.len(), signature.inputs.len());
                assert_eq!(ports.outputs.len(), signature.outputs.len());
            }
        }
        // A duplicated width binding: the constructor reads the first, so the names do too.
        let params = table(&[int("nin", 2), int("nin", 5)]);
        let ports = resolved_ports(entry.class_path, &params).expect("nameable");
        let block = (oce_blocks::lookup(entry.class_path).unwrap().make)(&params);
        assert_eq!(ports.inputs.len(), block.resolved_signature().inputs.len());
    }
}

#[test]
fn errors_render_their_cause() {
    assert_eq!(
        ResolvedPortsError::UnknownClass.to_string(),
        "the class path is not registered"
    );
    assert_eq!(
        ResolvedPortsError::NoLayout.to_string(),
        "the class declares no port layout"
    );
    assert_eq!(
        ResolvedPortsError::LayoutMismatch {
            side: PortSide::Output,
            resolved: 3
        }
        .to_string(),
        "3 resolved output port(s) do not fit the class's declared connectors"
    );
}

/// The 1-based indices a contract name carries after its base: `u2_3` is `[2, 3]`, `y4` is `[4]`.
fn indices(name: &str, base: &str) -> Vec<usize> {
    name.strip_prefix(base)
        .unwrap_or_else(|| panic!("{name} does not start with {base}"))
        .split('_')
        .map(|part| part.parse().unwrap_or_else(|_| panic!("{name}: bad index")))
        .collect()
}

/// For every (input, output) pair, the constructed block's own feedthrough must be what the two
/// names say it is. A divergence in how the naming and the constructor read a parameter (the
/// reduction flag, the selector array) shows up here even where port counts agree.
fn assert_wiring(class: &str, params: &[(&str, Value)], expect: impl Fn(&str, &str) -> bool) {
    let params = table(params);
    let ports = resolved_ports(class, &params).expect("ok");
    let block = (oce_blocks::lookup(class).unwrap().make)(&params);
    for (i, input) in ports.inputs.iter().enumerate() {
        for (j, output) in ports.outputs.iter().enumerate() {
            assert_eq!(
                block.feeds_through(i, j),
                expect(&input.name, &output.name),
                "{class} {:?}: {} -> {}",
                params.values,
                input.name,
                output.name
            );
        }
    }
}

#[test]
fn names_agree_with_the_constructed_blocks_wiring_under_every_flag_spelling() {
    for (class, flag) in [
        ("CDL.Reals.MatrixMax", "rowMax"),
        ("CDL.Reals.MatrixMin", "rowMin"),
    ] {
        // Square matrices give the same count either way, so only the wiring can tell the
        // reduction axis apart.
        for (rows, cols) in [(2, 2), (3, 3), (2, 3), (3, 1)] {
            for value in [
                None,
                Some(Value::Boolean(true)),
                Some(Value::Boolean(false)),
                Some(Value::Integer(0)),
                Some(Value::Real(0.0)),
                Some(Value::String(Arc::from("false"))),
            ] {
                let by_row = !matches!(value, Some(Value::Boolean(false)));
                let mut params = vec![int("nRow", rows), int("nCol", cols)];
                params.extend(value.map(|v| (flag, v)));
                assert_wiring(class, &params, |input, output| {
                    let cell = indices(input, "u");
                    let k = indices(output, "y")[0];
                    if by_row { cell[0] == k } else { cell[1] == k }
                });
            }
        }
    }
    for class in ROUTING_FAMILIES.iter().map(|family| family[4]) {
        for (nin, nout) in [(1, 1), (2, 3), (3, 2)] {
            assert_wiring(
                class,
                &[int("nin", nin), int("nout", nout)],
                |input, output| indices(input, "u")[0] == indices(output, "y")[1],
            );
        }
    }
    for class in ROUTING_FAMILIES.iter().map(|family| family[0]) {
        let selectors = [3_i64, 1];
        let mut params = vec![int("nin", 3), int("nout", 2)];
        params.push(("extract_1", Value::Integer(selectors[0])));
        params.push(("extract_2", Value::Integer(selectors[1])));
        assert_wiring(class, &params, |input, output| {
            let k = indices(output, "y")[0];
            indices(input, "u")[0] as i64 == selectors[k - 1]
        });
    }
    for class in ROUTING_FAMILIES.iter().map(|family| family[2]) {
        assert_wiring(class, &[int("nout", 3)], |input, _| input == "u");
    }
}
