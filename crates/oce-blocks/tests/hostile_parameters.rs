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
//! `init_state`, and one emit and state update at two times with typed zero inputs.
//!
//! A seeded xorshift64* generator, no dependency: `OCE_PARAM_FUZZ_CASES` overrides the per-class
//! case count (default 200) and `OCE_PARAM_FUZZ_SEED` the seed.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use oce_blocks::{
    BlockKind, Ctx, MAX_RESOLVED_PORT_WIDTH, NoopDiagnostics, ParamRule, PortKind, catalog, lookup,
};
use oce_model::{EnumClassId, ParamTable, Value};

/// Shapes above these are constructed and queried for signature and state length only.
const MAX_ENUMERATED_PORTS: usize = 256;
const MAX_INITIALIZED_WORDS: usize = 1 << 16;

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

/// Every parameter name a class's rules and defaults mention, with array and matrix members.
fn names_of(class_path: &str) -> Vec<String> {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == class_path)
        .unwrap();
    let mut bases: Vec<&str> = Vec::new();
    let mut scalars: Vec<String> = Vec::new();
    for rule in entry.param_rules {
        match *rule {
            ParamRule::Required { name, .. }
            | ParamRule::Structural { name }
            | ParamRule::Boolean { name }
            | ParamRule::Real { name }
            | ParamRule::RealFinite { name }
            | ParamRule::RealGreaterThan { name, .. }
            | ParamRule::RealFiniteGreaterThan { name, .. }
            | ParamRule::IntegerGreaterOrEqual { name, .. }
            | ParamRule::IntegerLessOrEqualConstant { name, .. }
            | ParamRule::EnumMembers { name, .. }
            | ParamRule::RealGreaterOrEqual { name, .. }
            | ParamRule::RealLessOrEqualConstant { name, .. } => scalars.push(name.into()),
            ParamRule::StructuralArrayElements { base } => bases.push(base),
            ParamRule::IntegerArrayElements { base, len }
            | ParamRule::RealArrayElements { base, len }
            | ParamRule::BooleanArrayElements { base, len } => {
                bases.push(base);
                scalars.push(len.into());
            }
            ParamRule::IntegerArrayElementsInRange { base, len, max, .. } => {
                bases.push(base);
                scalars.push(len.into());
                scalars.push(max.into());
            }
            ParamRule::RealMatrixElements {
                base, rows, cols, ..
            } => {
                bases.push(base);
                scalars.push(rows.into());
                scalars.push(cols.into());
            }
            ParamRule::TimeTableMatrix {
                base,
                time_scale,
                period,
                extrapolation,
                ..
            } => {
                bases.push(base);
                scalars.push(time_scale.into());
                scalars.extend(period.map(Into::into));
                scalars.extend(extrapolation.map(Into::into));
            }
            ParamRule::TimeTableOffset { base, .. } => bases.push(base),
            ParamRule::BooleanArrayTrueCountEquals {
                base, len, count, ..
            } => {
                bases.push(base);
                scalars.push(len.into());
                scalars.push(count.into());
            }
            ParamRule::RealTimesIntegerInclusiveRange { real, integer, .. } => {
                scalars.push(real.into());
                scalars.push(integer.into());
            }
            ParamRule::IntegerProductLessOrEqualConstant { left, right, .. } => {
                scalars.push(left.into());
                scalars.push(right.into());
            }
            ParamRule::RealLessOrEqual { lower, upper }
            | ParamRule::RealLessOrEqualWarning { lower, upper } => {
                scalars.push(lower.into());
                scalars.push(upper.into());
            }
            ParamRule::RealGreaterOrEqualScaledWarning { left, right, .. }
            | ParamRule::RealEqualWarning { left, right } => {
                scalars.push(left.into());
                scalars.push(right.into());
            }
            _ => {}
        }
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
    assert!(inputs <= MAX_RESOLVED_PORT_WIDTH * 2 && outputs <= MAX_RESOLVED_PORT_WIDTH * 2);
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
