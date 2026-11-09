//! Hostile parameter tables through every registered constructor (GitHub #400, design note
//! decision 12).
//!
//! A compiled-schedule blob is trusted for semantics but hostile for memory safety: on a target,
//! `oce-validate`'s parameter rules are not linked, so every registry constructor receives the
//! blob's parameter table as written. This fuzz feeds every class seeded random tables — the
//! class's own parameter names (rules, defaults and their flattened `_i` / `_row_col` members)
//! mixed with junk names, carrying values of every kind at their edges (NaN, infinities, signed
//! zero, subnormals, `i64` extremes, widths around [`MAX_RESOLVED_PORT_WIDTH`], unknown enum
//! classes and ordinals, arbitrary strings) — and requires construction and every structural query
//! to return without a panic: the resolved signature, kind, state length and revision, and, while
//! the resolved shape stays small enough to enumerate, `feeds_through` over every port pair,
//! `init_state`, one emit and state update at two times with typed zero inputs, and the
//! contract port names (`resolved_ports`, GitHub #440), which must name every resolved port
//! exactly once with the signature's kind.
//!
//! A seeded xorshift64* generator, no dependency: `OCE_PARAM_FUZZ_CASES` overrides the per-class
//! case count (default 200) and `OCE_PARAM_FUZZ_SEED` the seed.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use oce_blocks::{
    BlockKind, Ctx, MAX_RESOLVED_ARITY, MAX_RESOLVED_PORT_WIDTH, NoopDiagnostics,
    ParamRuleProjection, PortKind, TimeTableValues, catalog, lookup, resolved_ports,
};
use oce_model::{EnumClassId, ParamTable, Value, ValueType};

/// Shapes above these are constructed and queried for signature and state length only.
const MAX_ENUMERATED_PORTS: usize = 256;
const MAX_INITIALIZED_WORDS: usize = 1 << 16;
/// Shapes up to this many ports per side also have their contract names synthesized and checked.
const MAX_NAMED_PORTS: usize = 4096;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// One rule's variant name, its scalar parameter names and its flattened array or matrix bases.
type RuleNames = (&'static str, Vec<&'static str>, Vec<&'static str>);

/// The parameter names each rule mentions, through the owner's exhaustive projection: a new
/// `ParamRule` variant adds a required method here, so it cannot be skipped silently.
struct Names;

impl ParamRuleProjection for Names {
    type Output = RuleNames;
    fn required(name: &'static str, _: ValueType) -> RuleNames {
        ("Required", vec![name], vec![])
    }
    fn structural(name: &'static str) -> RuleNames {
        ("Structural", vec![name], vec![])
    }
    fn structural_array_elements(base: &'static str) -> RuleNames {
        ("StructuralArrayElements", vec![], vec![base])
    }
    fn boolean(name: &'static str) -> RuleNames {
        ("Boolean", vec![name], vec![])
    }
    fn real(name: &'static str) -> RuleNames {
        ("Real", vec![name], vec![])
    }
    fn real_finite(name: &'static str) -> RuleNames {
        ("RealFinite", vec![name], vec![])
    }
    fn real_greater_than(name: &'static str, _: f64) -> RuleNames {
        ("RealGreaterThan", vec![name], vec![])
    }
    fn real_finite_greater_than(name: &'static str, _: f64) -> RuleNames {
        ("RealFiniteGreaterThan", vec![name], vec![])
    }
    fn integer_greater_or_equal(name: &'static str, _: i64) -> RuleNames {
        ("IntegerGreaterOrEqual", vec![name], vec![])
    }
    fn integer_less_or_equal_constant(name: &'static str, _: i64) -> RuleNames {
        ("IntegerLessOrEqualConstant", vec![name], vec![])
    }
    fn integer_array_elements(base: &'static str, len: &'static str) -> RuleNames {
        ("IntegerArrayElements", vec![len], vec![base])
    }
    fn integer_array_elements_in_range(
        base: &'static str,
        len: &'static str,
        _: i64,
        _: i64,
        max: &'static str,
        _: i64,
        _: bool,
    ) -> RuleNames {
        ("IntegerArrayElementsInRange", vec![len, max], vec![base])
    }
    fn real_array_elements(base: &'static str, len: &'static str) -> RuleNames {
        ("RealArrayElements", vec![len], vec![base])
    }
    fn real_matrix_elements(
        base: &'static str,
        rows: &'static str,
        _: i64,
        cols: &'static str,
        _: i64,
    ) -> RuleNames {
        ("RealMatrixElements", vec![rows, cols], vec![base])
    }
    fn time_table_matrix(
        base: &'static str,
        _: TimeTableValues,
        time_scale: &'static str,
        period: Option<&'static str>,
        extrapolation: Option<&'static str>,
    ) -> RuleNames {
        let mut scalars = vec![time_scale];
        scalars.extend(period);
        scalars.extend(extrapolation);
        ("TimeTableMatrix", scalars, vec![base])
    }
    fn time_table_offset(base: &'static str, table: &'static str) -> RuleNames {
        ("TimeTableOffset", vec![], vec![base, table])
    }
    fn boolean_array_elements(base: &'static str, len: &'static str) -> RuleNames {
        ("BooleanArrayElements", vec![len], vec![base])
    }
    fn boolean_array_true_count_equals(
        base: &'static str,
        len: &'static str,
        count: &'static str,
        _: bool,
    ) -> RuleNames {
        ("BooleanArrayTrueCountEquals", vec![len, count], vec![base])
    }
    fn enum_members(name: &'static str, _: &'static [&'static str]) -> RuleNames {
        ("EnumMembers", vec![name], vec![])
    }
    fn real_greater_or_equal(name: &'static str, _: f64) -> RuleNames {
        ("RealGreaterOrEqual", vec![name], vec![])
    }
    fn real_less_or_equal_constant(name: &'static str, _: f64) -> RuleNames {
        ("RealLessOrEqualConstant", vec![name], vec![])
    }
    fn real_times_integer_inclusive_range(
        real: &'static str,
        integer: &'static str,
        _: f64,
        _: f64,
    ) -> RuleNames {
        (
            "RealTimesIntegerInclusiveRange",
            vec![real, integer],
            vec![],
        )
    }
    fn integer_product_less_or_equal_constant(
        left: &'static str,
        right: &'static str,
        _: i64,
    ) -> RuleNames {
        (
            "IntegerProductLessOrEqualConstant",
            vec![left, right],
            vec![],
        )
    }
    fn real_less_or_equal(lower: &'static str, upper: &'static str) -> RuleNames {
        ("RealLessOrEqual", vec![lower, upper], vec![])
    }
    fn real_less_or_equal_warning(lower: &'static str, upper: &'static str) -> RuleNames {
        ("RealLessOrEqualWarning", vec![lower, upper], vec![])
    }
    fn real_greater_or_equal_scaled_warning(
        left: &'static str,
        right: &'static str,
        _: f64,
    ) -> RuleNames {
        ("RealGreaterOrEqualScaledWarning", vec![left, right], vec![])
    }
    fn real_equal_warning(left: &'static str, right: &'static str) -> RuleNames {
        ("RealEqualWarning", vec![left, right], vec![])
    }
}

/// Every `ParamRule` variant the projection names. A new variant fails to compile in `Names`
/// first; this list then fails `every_rule_variant_is_projected_and_used_by_the_registry`.
const RULE_VARIANTS: [&str; 27] = [
    "Required",
    "Structural",
    "StructuralArrayElements",
    "Boolean",
    "Real",
    "RealFinite",
    "RealGreaterThan",
    "RealFiniteGreaterThan",
    "IntegerGreaterOrEqual",
    "IntegerLessOrEqualConstant",
    "IntegerArrayElements",
    "IntegerArrayElementsInRange",
    "RealArrayElements",
    "RealMatrixElements",
    "TimeTableMatrix",
    "TimeTableOffset",
    "BooleanArrayElements",
    "BooleanArrayTrueCountEquals",
    "EnumMembers",
    "RealGreaterOrEqual",
    "RealLessOrEqualConstant",
    "RealTimesIntegerInclusiveRange",
    "IntegerProductLessOrEqualConstant",
    "RealLessOrEqual",
    "RealLessOrEqualWarning",
    "RealGreaterOrEqualScaledWarning",
    "RealEqualWarning",
];

/// Every parameter name a class's rules and defaults mention, with array and matrix members.
fn names_of(class_path: &str) -> Vec<String> {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == class_path)
        .unwrap();
    let mut bases: Vec<&str> = Vec::new();
    let mut scalars: Vec<String> = Vec::new();
    for rule in entry.param_rules {
        let (_, names, rule_bases) = rule.project::<Names>();
        scalars.extend(names.into_iter().map(String::from));
        bases.extend(rule_bases);
    }
    for default in entry.param_defaults {
        match default.name.split_once('<') {
            Some((base, _)) => bases.push(base.trim_end_matches('_')),
            None => scalars.push(default.name.into()),
        }
    }
    let mut names = scalars;
    for base in bases {
        for i in [0usize, 1, 2, 3, 7, 64] {
            names.push(format!("{base}_{i}"));
            for j in [1usize, 2, 5] {
                names.push(format!("{base}_{i}_{j}"));
            }
        }
        names.push(format!("{base}_"));
        names.push(format!("{base}_1_"));
        names.push(format!("{base}_18446744073709551616"));
    }
    names.extend(["".into(), "_".into(), "x".into(), "\u{1F525}".into()]);
    names.sort_unstable();
    names.dedup();
    names
}

const REALS: [f64; 14] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::MAX,
    f64::MIN,
    f64::MIN_POSITIVE,
    5e-324,
    1e-300,
    4_294_967_296.0,
];

fn integers() -> [i64; 16] {
    let width = MAX_RESOLVED_PORT_WIDTH as i64;
    [
        0,
        1,
        -1,
        2,
        3,
        8,
        65,
        width - 1,
        width,
        width + 1,
        i64::from(i32::MAX),
        i64::from(i32::MIN),
        i64::MAX,
        i64::MIN,
        1 << 32,
        -(1 << 40),
    ]
}

fn random_value(rng: &mut XorShift) -> Value {
    match rng.below(6) {
        0 => Value::Real(*rng.pick(&REALS)),
        1 => Value::Real(f64::from_bits(rng.next())),
        2 => Value::Integer(*rng.pick(&integers())),
        3 => Value::Boolean(rng.below(2) == 0),
        4 => {
            let strings = [
                "",
                "PID",
                "Buildings.Controls.OBC.CDL.Types.SimpleController.PI",
                "Buildings.Controls.OBC.CDL.Types.Extrapolation.Periodic",
                "CDL.Types.Smoothness.ConstantSegments",
                ".",
                "Periodic.",
                "\u{0}",
            ];
            Value::String(Arc::from(*rng.pick(&strings)))
        }
        _ => Value::Enum {
            class: EnumClassId(*rng.pick(&[0u32, 1, 2, 3, 4, 101, 110, 999, u32::MAX])),
            ordinal: *rng.pick(&[0u32, 1, 2, 3, 4, 44, 45, u32::MAX]),
        },
    }
}

fn zero(kind: PortKind) -> Value {
    match kind {
        PortKind::Real => Value::Real(0.0),
        PortKind::Integer => Value::Integer(0),
        PortKind::Boolean => Value::Boolean(false),
    }
}

/// Construct and query one block; returns nothing, panics on a defect.
fn exercise(class_path: &str, params: &ParamTable) {
    let entry = lookup(class_path).unwrap();
    let block = (entry.make)(params);
    let signature = block.resolved_signature();
    let kind = block.kind();
    let state_len = block.state_len();
    let _ = block.state_contract_revision();
    let _ = block.signature();
    if kind == BlockKind::Algebraic {
        assert_eq!(state_len, 0, "an algebraic block reports state");
    }
    let (inputs, outputs) = (signature.inputs.len(), signature.outputs.len());
    // The compiled-schedule decoder refuses longer port lists before allocating for them.
    assert!(inputs <= MAX_RESOLVED_ARITY && outputs <= MAX_RESOLVED_ARITY);
    if inputs <= MAX_NAMED_PORTS && outputs <= MAX_NAMED_PORTS {
        check_names(class_path, params, &signature.inputs, &signature.outputs);
    }
    if inputs > MAX_ENUMERATED_PORTS
        || outputs > MAX_ENUMERATED_PORTS
        || state_len > MAX_INITIALIZED_WORDS
    {
        return;
    }
    for i in 0..inputs {
        for j in 0..outputs {
            let _ = block.feeds_through(i, j);
        }
    }
    let mut region = vec![0u64; state_len];
    block.init_state(&mut region, params);
    let values: Vec<Value> = signature.inputs.iter().map(|k| zero(*k)).collect();
    let diagnostics = NoopDiagnostics;
    for t in [0.0, 1.0] {
        let _ = block.time_is_representable(t, &region);
        let ctx = Ctx::new(t, &diagnostics);
        let mut emitted = 0usize;
        let mut emit = |port: usize, _value: Value| {
            assert!(port < outputs, "emitted port {port} of {outputs}");
            emitted += 1;
        };
        match kind {
            BlockKind::Algebraic => block.step_algebraic(&ctx, &values, &mut emit),
            BlockKind::Stateful => {
                block.emit_from_state(&ctx, &values, &region, &mut emit);
                block.update_state(&ctx, &values, &mut region);
            }
        }
    }
}

/// The contract names cover the constructed block exactly: one unique name per resolved port, in
/// signature order, with the signature's kind. Any table a constructor accepts must be nameable.
fn check_names(class_path: &str, params: &ParamTable, inputs: &[PortKind], outputs: &[PortKind]) {
    let ports = resolved_ports(class_path, params)
        .unwrap_or_else(|error| panic!("{class_path}: ports not nameable: {error}"));
    for (named, kinds) in [(&ports.inputs, inputs), (&ports.outputs, outputs)] {
        let named_kinds: Vec<PortKind> = named.iter().map(|port| port.kind).collect();
        assert_eq!(
            named_kinds, kinds,
            "{class_path}: names disagree with the signature"
        );
        let mut names: Vec<&str> = named.iter().map(|port| port.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            named.len(),
            "{class_path}: duplicate port name"
        );
    }
}

#[test]
fn every_constructor_survives_hostile_parameter_tables() {
    let cases = env_u64("OCE_PARAM_FUZZ_CASES", 200);
    let mut rng = XorShift(env_u64("OCE_PARAM_FUZZ_SEED", 0x0CEB_0400_7A2A_3500).max(1));
    let mut failures = Vec::new();
    for entry in catalog() {
        let names = names_of(entry.class_path);
        // The default table first, then every known name alone with each edge value kind.
        let mut tables = vec![ParamTable::default()];
        for name in &names {
            for value in [
                Value::Real(f64::NAN),
                Value::Real(-0.0),
                Value::Integer(i64::MAX),
                Value::Integer(i64::MIN),
                Value::Integer(MAX_RESOLVED_PORT_WIDTH as i64 + 1),
                Value::Boolean(true),
                Value::String(Arc::from("")),
            ] {
                tables.push(ParamTable {
                    values: vec![(Arc::from(name.as_str()), value)],
                });
            }
        }
        for _ in 0..cases {
            let count = rng.below(9);
            let values = (0..count)
                .map(|_| (Arc::from(rng.pick(&names).as_str()), random_value(&mut rng)))
                .collect();
            tables.push(ParamTable { values });
        }
        for params in &tables {
            let outcome = catch_unwind(AssertUnwindSafe(|| exercise(entry.class_path, params)));
            if outcome.is_err() {
                failures.push(format!("{}: {:?}", entry.class_path, params.values));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} hostile table(s) panicked; first: {:?}",
        failures.len(),
        failures.first()
    );
}

#[test]
fn every_rule_variant_is_projected_and_used_by_the_registry() {
    let mut used: Vec<&str> = catalog()
        .iter()
        .flat_map(|entry| entry.param_rules)
        .map(|rule| rule.project::<Names>().0)
        .collect();
    used.sort_unstable();
    used.dedup();
    let mut all = RULE_VARIANTS.to_vec();
    all.sort_unstable();
    assert_eq!(
        used, all,
        "every ParamRule variant the fuzz derives names from is in use"
    );
}

/// The arity bound is the true maximum: `CDL.Reals.Sort` reaches it at `nin` equal to the port
/// width, and every width-driven class stays within it at and past its widest parameters.
#[test]
fn the_resolved_arity_bound_is_reached_and_never_exceeded() {
    let width = MAX_RESOLVED_PORT_WIDTH as i64;
    let sort = (lookup("CDL.Reals.Sort").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(width))],
    });
    assert_eq!(sort.resolved_signature().outputs.len(), MAX_RESOLVED_ARITY);
    let mut widest = 0usize;
    for entry in catalog().iter().filter(|entry| entry.width_driven) {
        for n in [width, width + 1, i64::MAX] {
            let values = names_of(entry.class_path)
                .into_iter()
                .filter(|name| !name.contains('_'))
                .map(|name| (Arc::from(name.as_str()), Value::Integer(n)))
                .collect();
            let block = (lookup(entry.class_path).unwrap().make)(&ParamTable { values });
            let signature = block.resolved_signature();
            widest = widest
                .max(signature.inputs.len())
                .max(signature.outputs.len());
        }
    }
    assert_eq!(widest, MAX_RESOLVED_ARITY);
}
