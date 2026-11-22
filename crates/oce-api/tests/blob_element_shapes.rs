//! The compiled-schedule blob decoder applies each class's shape rules (issue #772): an array
//! element past its vector's resolved length, an index of zero, a table offset past the table's
//! outputs, and a matrix cell that leaves the rectangle its largest row and column span unfilled
//! are each refused with `BlobError::ElementOutsideShape`, before the block is built. Before, the
//! decoder judged a name only by whether its class declares it, so `table_3_2` on a two-row
//! `TimeTable` reshaped its output and `k_3` on a `MultiSum` of `nin = 2` was dropped.
//!
//! And `MatrixGain`'s `nin`/`nout` are no parameters of the class (issue #773): a blob carrying
//! either is `UndeclaredParameter`.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_image::{Image, VType, Val};
use crate::blob_models::{class, input, inst};

use oce_api::{BlobError, Value};
use oce_graph::blob::decode;

/// One block of `path` with `params` (in name order), `ins` Real inputs each driven by a boundary
/// input and `outs` outputs: a blob that decodes when `params` is shape-valid.
fn single(path: &str, params: &[(&str, Value)], ins: u32, outs: u32) -> Image {
    let mut sorted = params.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut image = Image::new();
    image.classes = vec![class(path)];
    image.connectors = ins + outs;
    let inputs: Vec<u32> = (0..ins).collect();
    let outputs: Vec<u32> = (ins..ins + outs).collect();
    image.instances = vec![inst(0, path, &sorted, &inputs, &outputs)];
    image.schedule = vec![0];
    if ins > 0 {
        image.inputs = vec![input("urn:t:u", VType::Real, &inputs)];
    }
    image
}

/// `image` with `name = value` added to block 0's parameters, kept in name order; the position
/// it takes.
fn with(mut image: Image, name: &str, value: Val) -> (Image, u32) {
    let params = &mut image.instances[0].params;
    params.push((name.as_bytes().to_vec(), value));
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let at = params
        .iter()
        .position(|(n, _)| n == name.as_bytes())
        .unwrap();
    (image, u32::try_from(at).unwrap())
}

fn refusal(image: &Image) -> Option<BlobError> {
    decode(&image.to_bytes()).err()
}

fn outside(parameter: u32) -> Option<BlobError> {
    Some(BlobError::ElementOutsideShape {
        block: 0,
        parameter,
    })
}

fn real(x: f64) -> Value {
    Value::Real(x)
}

fn int(n: i64) -> Value {
    Value::Integer(n)
}

/// A 2x2 `TimeTable`: `[[0, 2], [2, 3]]`.
fn two_row_table() -> Image {
    single(
        "CDL.Reals.Sources.TimeTable",
        &[
            ("table_1_1", real(0.0)),
            ("table_1_2", real(2.0)),
            ("table_2_1", real(2.0)),
            ("table_2_2", real(3.0)),
        ],
        0,
        1,
    )
}

#[test]
fn the_issue_reproductions_are_refused_before_the_block_is_built() {
    // `table_3_2` on a two-row table: its cells no longer fill their 3x2 rectangle.
    assert_eq!(refusal(&two_row_table()), None);
    let (image, at) = with(two_row_table(), "table_3_2", Val::real(9.0));
    assert_eq!(refusal(&image), outside(at));
    // With `table_3_1` too the table is a complete, different 3x2 table: it decodes, as the
    // load gate accepts it (a wrong value of the right kind executes as written).
    let (image, _) = with(image, "table_3_1", Val::real(4.0));
    assert_eq!(refusal(&image), None);

    // `k_3` on a `MultiSum` of `nin = 2`.
    let sum = single(
        "CDL.Reals.MultiSum",
        &[("nin", int(2)), ("k_1", real(1.0)), ("k_2", real(1.0))],
        2,
        1,
    );
    assert_eq!(refusal(&sum), None);
    let (image, at) = with(sum.clone(), "k_3", Val::real(5.0));
    assert_eq!(refusal(&image), outside(at));
    // An index too large for any width, or for `usize`.
    for name in [
        "k_4294967296",
        "k_18446744073709551616",
        "k_99999999999999999999999",
    ] {
        let (image, at) = with(sum.clone(), name, Val::real(5.0));
        assert_eq!(refusal(&image), outside(at), "{name}");
    }
    // An index of zero names no element of the class (issue #748's check refuses it first).
    let (image, at) = with(sum.clone(), "k_0", Val::real(5.0));
    assert_eq!(
        refusal(&image),
        Some(BlobError::UndeclaredParameter {
            block: 0,
            parameter: at
        })
    );

    // `extract_2` on a `RealExtractSignal` of one output.
    let extract = single(
        "CDL.Routing.RealExtractSignal",
        &[("nin", int(2)), ("nout", int(1)), ("extract_1", int(2))],
        2,
        1,
    );
    assert_eq!(refusal(&extract), None);
    let (image, at) = with(extract, "extract_2", Val::Integer(1));
    assert_eq!(refusal(&image), outside(at));

    // `K_2_1` on a 1x1 `MatrixGain`: since issue #773 the cells set the shape, so `K_1_1` and
    // `K_2_1` are a complete 2x1 matrix of two outputs, refused for the one output listed (never
    // built with the cell dropped); `K_2_2` leaves its 2x2 rectangle unfilled.
    let gain = single("CDL.Reals.MatrixGain", &[("K_1_1", real(2.0))], 1, 1);
    assert_eq!(refusal(&gain), None);
    let (image, _) = with(gain.clone(), "K_2_1", Val::real(7.0));
    assert!(matches!(
        refusal(&image),
        Some(BlobError::InstanceMismatch { block: 0, .. })
    ));
    let (image, at) = with(gain, "K_2_2", Val::real(7.0));
    assert_eq!(refusal(&image), outside(at));

    // `offset_2` on a table of one output column.
    let (image, at) = with(two_row_table(), "offset_2", Val::real(1.0));
    assert_eq!(refusal(&image), outside(at));
    let (image, _) = with(two_row_table(), "offset_1", Val::real(1.0));
    assert_eq!(refusal(&image), None);
}

#[test]
fn the_shape_check_names_the_first_element_outside_and_comes_before_the_build() {
    // Two elements outside: the first in the block's parameter order is named.
    let sum = single(
        "CDL.Reals.MultiSum",
        &[("nin", int(2)), ("k_1", real(1.0)), ("k_2", real(1.0))],
        2,
        1,
    );
    let (image, _) = with(sum, "k_4", Val::real(5.0));
    let (image, at) = with(image, "k_3", Val::real(5.0));
    assert_eq!(refusal(&image), outside(at));
    // A name the class does not declare is refused first, by its own variant.
    let (image, _) = with(image, "k_x", Val::real(5.0));
    assert!(matches!(
        refusal(&image),
        Some(BlobError::UndeclaredParameter { .. })
    ));
    // A negative length admits no element (the load gate refuses the length itself; a blob
    // that carries one never builds the block with its elements dropped).
    let negative = single("CDL.Reals.MultiSum", &[("nin", int(-1))], 0, 1);
    let (image, at) = with(negative, "k_1", Val::real(5.0));
    assert_eq!(refusal(&image), outside(at));
    // A length that is no Integer leaves the elements unjudged here; the kind check refuses it.
    let wide = single("CDL.Reals.MultiSum", &[("nin", int(2))], 2, 1);
    let (image, _) = with(wide, "k_1", Val::real(1.0));
    assert_eq!(refusal(&image), None);
}

#[test]
fn matrix_gain_nin_and_nout_are_undeclared_in_a_blob() {
    let gain = single(
        "CDL.Reals.MatrixGain",
        &[
            ("K_1_1", real(1.0)),
            ("K_1_2", real(0.0)),
            ("K_2_1", real(0.0)),
            ("K_2_2", real(1.0)),
        ],
        2,
        2,
    );
    assert_eq!(refusal(&gain), None);
    for name in ["nin", "nout"] {
        let (image, at) = with(gain.clone(), name, Val::Integer(2));
        assert_eq!(
            refusal(&image),
            Some(BlobError::UndeclaredParameter {
                block: 0,
                parameter: at
            }),
            "{name}"
        );
    }
}

#[test]
fn a_blob_holding_a_matrix_gain_written_before_its_counts_came_from_k_is_refused() {
    // The relief fan group's blob as this engine version wrote it before issue #773: its
    // `MatrixGain` carried `nin` and `nout`, and the class's signature hash was the old one.
    let before = include_bytes!("fixtures/blob_matrix_gain_with_counts.oceb");
    assert!(matches!(
        decode(before).err(),
        Some(BlobError::ClassSignatureMismatch { .. })
    ));
}

/// xorshift64*: deterministic, no dependency.
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
}

/// Valid blobs whose parameter names a mutation can push outside their shape.
fn shapes() -> Vec<Image> {
    let mut table = Vec::new();
    for row in 1..=3 {
        for col in 1..=3 {
            table.push((format!("table_{row}_{col}"), real(f64::from(row))));
        }
    }
    let table: Vec<(&str, Value)> = table.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
    vec![
        single("CDL.Reals.Sources.TimeTable", &table, 0, 2),
        single(
            "CDL.Reals.MultiSum",
            &[
                ("nin", int(3)),
                ("k_1", real(1.0)),
                ("k_2", real(1.0)),
                ("k_3", real(1.0)),
            ],
            3,
            1,
        ),
        single(
            "CDL.Routing.RealExtractSignal",
            &[
                ("nin", int(3)),
                ("nout", int(2)),
                ("extract_1", int(3)),
                ("extract_2", int(1)),
            ],
            3,
            2,
        ),
        single(
            "CDL.Reals.MatrixGain",
            &[
                ("K_1_1", real(1.0)),
                ("K_1_2", real(0.0)),
                ("K_2_1", real(0.0)),
                ("K_2_2", real(1.0)),
            ],
            2,
            2,
        ),
        single(
            "CDL.Routing.RealVectorFilter",
            &[
                ("nin", int(2)),
                ("nout", int(2)),
                ("msk_1", Value::Boolean(true)),
                ("msk_2", Value::Boolean(true)),
            ],
            2,
            2,
        ),
    ]
}

/// Block 0's parameters as the decoder reads them.
fn table_of(image: &Image) -> oce_model::ParamTable {
    oce_model::ParamTable {
        values: image.instances[0]
            .params
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    Val::Real(bits) => Value::Real(f64::from_bits(*bits)),
                    Val::Integer(n) => Value::Integer(*n),
                    Val::Boolean(b) => Value::Boolean(*b != 0),
                    other => panic!("not written here: {other:?}"),
                };
                (
                    std::sync::Arc::from(String::from_utf8_lossy(name).as_ref()),
                    value,
                )
            })
            .collect(),
    }
}

/// Thousands of random edits of the parameter names of valid blobs: digits replaced, inserted
/// and removed, indices grown past every width. Every outcome is a decode or a typed refusal,
/// never a panic, and a shape refusal is exactly what the class's shape rules predict.
#[test]
fn every_random_edit_of_a_parameter_name_decodes_or_is_a_typed_refusal() {
    const DIGITS: &[u8] = b"0123456789_";
    let shapes = shapes();
    for image in &shapes {
        assert_eq!(refusal(image), None, "the base decodes");
    }
    let mut rng = XorShift(0x0772_0773_2026_1122);
    let (mut decoded, mut outside_shape, mut other) = (0, 0, 0);
    for case in 0..4_000 {
        let mut image = shapes[rng.below(shapes.len())].clone();
        let params = &mut image.instances[0].params;
        let pick = rng.below(params.len());
        let mut name = params[pick].0.clone();
        for _ in 0..=rng.below(3) {
            let at = rng.below(name.len() + 1);
            let digit = DIGITS[rng.below(DIGITS.len())];
            match rng.below(4) {
                0 if at < name.len() => name[at] = digit,
                1 if at < name.len() && name.len() > 1 => {
                    name.remove(at);
                }
                2 => name.extend_from_slice(b"0000000000000000000000"),
                _ => name.insert(at, digit),
            }
        }
        params[pick].0 = name;
        params.sort_by(|a, b| a.0.cmp(&b.0));
        let outcome = decode(&image.to_bytes()).map(|_| ());
        let class_path = String::from_utf8(image.classes[0].0.clone()).unwrap();
        let table = table_of(&image);
        match outcome {
            Ok(()) => {
                assert_eq!(
                    oce_blocks::element_outside_extent(&class_path, &table),
                    None,
                    "case {case}: decoded with an element outside its shape: {table:?}"
                );
                decoded += 1;
            }
            Err(BlobError::ElementOutsideShape { block, parameter }) => {
                assert_eq!(block, 0, "case {case}");
                assert_eq!(
                    oce_blocks::element_outside_extent(&class_path, &table),
                    Some(usize::try_from(parameter).unwrap()),
                    "case {case}: {table:?}"
                );
                outside_shape += 1;
            }
            Err(_) => other += 1,
        }
    }
    assert!(decoded > 20, "{decoded} decoded");
    assert!(outside_shape > 100, "{outside_shape} outside their shape");
    assert!(other > 100, "{other} refused otherwise");
}
