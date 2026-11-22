//! The blob decoder's shape check agrees with the load gate (issue #772): over random parameter
//! tables of both `MultiSum` classes, an `ExtractSignal`, a `VectorFilter`, `MatrixGain` and the
//! three `TimeTable` sources, every table the load gate refuses for an element outside its
//! extent, a matrix its cells do not fill or one wider than the port width,
//! `oce_blocks::element_outside_extent` refuses too; and every table that check refuses, the load
//! gate refuses (for that, or for the length that set the extent: a required length left out, a
//! negative one). So a blob carries no element a loaded document could not.

use super::common::*;

/// xorshift64*: deterministic, no dependency.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// The phrases of the load gate's extent and completeness findings.
const EXTENT_PHRASES: [&str; 8] = [
    "is outside resolved",
    "is outside its matrix",
    "leaves",
    "must use positive one-based indexes",
    "is missing required cell",
    "required cells of its",
    "more than the maximum flattened cell count",
    "exceeds maximum flattened cell count",
];

/// A random table for `class`: its length parameters (sometimes absent, negative or zero) and
/// elements indexed from 0 to 5 (`base_<i>` or `base_<r>_<c>`), values of the element's kind.
fn table(rng: &mut XorShift, class: &str) -> Vec<(Arc<str>, Value)> {
    let mut out: Vec<(Arc<str>, Value)> = Vec::new();
    let len = |rng: &mut XorShift, name: &str, out: &mut Vec<(Arc<str>, Value)>| {
        if rng.below(4) != 0 {
            let n = i64::try_from(rng.below(6)).unwrap() - 1;
            out.push((Arc::from(name), Value::Integer(n)));
        }
    };
    let vector =
        |rng: &mut XorShift, base: &str, value: &dyn Fn(u64) -> Value, out: &mut Vec<_>| {
            for i in 0..6u64 {
                if rng.below(2) == 0 {
                    out.push((Arc::from(format!("{base}_{i}")), value(i)));
                }
            }
        };
    let matrix = |rng: &mut XorShift, base: &str, out: &mut Vec<_>| {
        let dense = rng.below(2) == 0;
        let (rows, cols) = (rng.below(4) + 1, rng.below(4) + 1);
        for row in 0..=rows {
            for col in 0..=cols {
                let keep = if dense {
                    row > 0 && col > 0
                } else {
                    rng.below(3) == 0
                };
                if keep || (row > 0 && col > 0 && rng.below(10) == 0) {
                    let value = Value::Real(if col == 1 { row as f64 } else { 0.5 });
                    out.push((Arc::from(format!("{base}_{row}_{col}")), value));
                }
            }
        }
    };
    match class {
        "CDL.Reals.MultiSum" => {
            len(rng, "nin", &mut out);
            vector(rng, "k", &|_| Value::Real(1.0), &mut out);
        }
        "CDL.Integers.MultiSum" => {
            len(rng, "nin", &mut out);
            vector(rng, "k", &|_| Value::Integer(1), &mut out);
        }
        "CDL.Routing.RealExtractSignal" => {
            len(rng, "nin", &mut out);
            len(rng, "nout", &mut out);
            vector(rng, "extract", &|_| Value::Integer(1), &mut out);
        }
        "CDL.Routing.RealVectorFilter" => {
            len(rng, "nin", &mut out);
            len(rng, "nout", &mut out);
            vector(rng, "msk", &|_| Value::Boolean(true), &mut out);
        }
        "CDL.Reals.MatrixGain" => matrix(rng, "K", &mut out),
        _ => {
            matrix(rng, "table", &mut out);
            vector(rng, "offset", &|_| Value::Real(0.0), &mut out);
        }
    }
    out
}

#[test]
fn the_decoders_shape_check_refuses_exactly_what_the_load_gate_refuses_for_shape() {
    let classes = [
        "CDL.Reals.MultiSum",
        "CDL.Integers.MultiSum",
        "CDL.Routing.RealExtractSignal",
        "CDL.Routing.RealVectorFilter",
        "CDL.Reals.MatrixGain",
        "CDL.Reals.Sources.TimeTable",
        "CDL.Integers.Sources.TimeTable",
        "CDL.Logical.Sources.TimeTable",
    ];
    let mut rng = XorShift(0x0772_0772_2026_1122);
    let (mut agreed_refused, mut agreed_clean) = (0, 0);
    for case in 0..8_000 {
        let class = classes[usize::try_from(rng.below(classes.len() as u64)).unwrap()];
        let params = table(&mut rng, class);
        let model = one_block_model(class, &[], &[], params.clone());
        let findings: Vec<String> = match validate(&model) {
            Ok(warnings) => warnings.iter().map(|d| d.message.clone()).collect(),
            Err(error) => error
                .diagnostics
                .iter()
                .map(|d| d.message.clone())
                .collect(),
        };
        let refused = validate(&model).is_err();
        let extent = findings
            .iter()
            .any(|message| EXTENT_PHRASES.iter().any(|phrase| message.contains(phrase)));
        let table = ParamTable { values: params };
        let decoder = oce_blocks::element_outside_extent(class, &table);
        if extent {
            assert!(
                decoder.is_some(),
                "case {case}: the gate refuses {class} {table:?} for shape: {findings:?}"
            );
            agreed_refused += 1;
        }
        if decoder.is_some() {
            assert!(
                refused,
                "case {case}: the decoder refuses {class} {table:?}, the gate loads it"
            );
        } else {
            agreed_clean += 1;
        }
    }
    assert!(agreed_refused > 500, "{agreed_refused} refused for shape");
    assert!(agreed_clean > 500, "{agreed_clean} inside their shape");
}
