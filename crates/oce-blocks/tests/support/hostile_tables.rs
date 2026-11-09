//! The shared hostile parameter-table generator (GitHub #400, #440).
//!
//! `oce-blocks`' constructor fuzz (`tests/hostile_parameters.rs`) and `oce-validate`'s
//! standalone-parameter agreement test both draw their tables from here, so the two cover the
//! same inputs: every parameter name a class's rules and defaults mention, with in- and
//! out-of-range array members (`k_0`, `K_3_1`, `base_`, overflowing indices) and junk names,
//! carrying values of every kind at their edges. A seeded xorshift64* generator, no dependency.

use std::sync::Arc;

use oce_blocks::{MAX_RESOLVED_PORT_WIDTH, ParamRuleProjection, TimeTableValues, catalog};
use oce_model::{EnumClassId, ParamTable, Value, ValueType};

pub struct XorShift(pub u64);

impl XorShift {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

pub fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// One rule's variant name, its scalar parameter names and its flattened array or matrix bases.
pub type RuleNames = (&'static str, Vec<&'static str>, Vec<&'static str>);

/// The parameter names each rule mentions, through the owner's exhaustive projection: a new
/// `ParamRule` variant adds a required method here, so it cannot be skipped silently.
pub struct Names;

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
pub const RULE_VARIANTS: [&str; 27] = [
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
pub fn names_of(class_path: &str) -> Vec<String> {
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

pub const REALS: [f64; 14] = [
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

pub fn integers() -> [i64; 16] {
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

pub fn random_value(rng: &mut XorShift) -> Value {
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

/// The fuzz's tables for one class: the empty table, every known name alone with each edge value
/// kind, then `cases` seeded random tables of up to eight entries (duplicates included).
pub fn hostile_tables(class_path: &str, cases: u64, rng: &mut XorShift) -> Vec<ParamTable> {
    let names = names_of(class_path);
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
            .map(|_| (Arc::from(rng.pick(&names).as_str()), random_value(rng)))
            .collect();
        tables.push(ParamTable { values });
    }
    tables
}
