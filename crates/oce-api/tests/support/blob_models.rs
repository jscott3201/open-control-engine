//! Small hand-built compiled-schedule blob models shared by the hostile-model test binaries: each
//! decodes when correct, so a test edits one field and observes one typed refusal.

#![allow(dead_code)]

use std::sync::Arc;

use oce_api::Value;
use oce_graph::blob::{BlobError, decode};
use oce_model::ParamTable;

use super::blob_image::{Image, In, Inst, Out, VType, Val, words_crc};

pub(crate) fn class(path: &str) -> (Vec<u8>, u64) {
    (
        path.as_bytes().to_vec(),
        oce_blocks::class_signature_hash(path).unwrap(),
    )
}

/// One instance of `path` with `params`, its kind, state length and initial-state CRC computed
/// from the registry.
pub(crate) fn inst(
    class: u32,
    path: &str,
    params: &[(&str, Value)],
    inputs: &[u32],
    outputs: &[u32],
) -> Inst {
    let table = ParamTable {
        values: params
            .iter()
            .map(|(n, v)| (Arc::from(*n), v.clone()))
            .collect(),
    };
    let block = (oce_blocks::lookup(path).unwrap().make)(&table);
    let stateful = block.kind() == oce_blocks::BlockKind::Stateful;
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &table);
    Inst {
        class,
        kind: u8::from(stateful),
        state_len: u32::try_from(block.state_len()).unwrap(),
        state_crc: if stateful { words_crc(&region) } else { 0 },
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
        params: params
            .iter()
            .map(|(n, v)| (n.as_bytes().to_vec(), Val::from_value(v)))
            .collect(),
    }
}

pub(crate) fn out(path: &str, source: u32) -> Out {
    Out {
        path: path.as_bytes().to_vec(),
        source,
        payload: Vec::new(),
    }
}

pub(crate) fn input(path: &str, vtype: VType, targets: &[u32]) -> In {
    In {
        path: path.as_bytes().to_vec(),
        vtype,
        min: None,
        max: None,
        targets: targets.to_vec(),
        payload: Vec::new(),
    }
}

/// `Constant(k = 1.5).y` (c0) → `Abs.u` (c1); `Abs.y` (c2) is the boundary output.
pub(crate) fn chain() -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Abs"), class("CDL.Reals.Sources.Constant")];
    image.connectors = 3;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(-1.5))],
            &[],
            &[0],
        ),
        inst(0, "CDL.Reals.Abs", &[], &[1], &[2]),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 1)];
    image.outputs = vec![out("urn:test:y", 2)];
    image
}

/// `Add(u1 = boundary input a, u2 = Constant)`: c0 const y, c1 u1, c2 u2, c3 y.
pub(crate) fn adder() -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Add"), class("CDL.Reals.Sources.Constant")];
    image.connectors = 4;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(2.0))],
            &[],
            &[0],
        ),
        inst(0, "CDL.Reals.Add", &[], &[1, 2], &[3]),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 2)];
    image.inputs = vec![input("urn:test:a", VType::Real, &[1])];
    image.outputs = vec![out("urn:test:y", 3)];
    image
}

/// `Constant.y` (c0) → `UnitDelay.u` (c1); `UnitDelay.y` (c2) is the output.
pub(crate) fn delay() -> Image {
    let mut image = Image::new();
    image.classes = vec![
        class("CDL.Discrete.UnitDelay"),
        class("CDL.Reals.Sources.Constant"),
    ];
    image.connectors = 3;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(4.0))],
            &[],
            &[0],
        ),
        inst(
            0,
            "CDL.Discrete.UnitDelay",
            &[
                ("samplePeriod", Value::Real(1.0)),
                ("y_start", Value::Real(3.0)),
            ],
            &[1],
            &[2],
        ),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 1)];
    image.outputs = vec![out("urn:test:y", 2)];
    image
}

pub(crate) fn refusal(image: &Image) -> BlobError {
    match decode(&image.to_bytes()) {
        Err(error) => error,
        Ok(_) => panic!("expected a refusal"),
    }
}

/// `TriggeredMovingMean(n)` owns `state_len(n)` words; its inputs come from two boundary inputs.
pub(crate) fn moving_mean(n: i64) -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Discrete.TriggeredMovingMean")];
    image.connectors = 3;
    image.instances = vec![inst(
        0,
        "CDL.Discrete.TriggeredMovingMean",
        &[("n", Value::Integer(n))],
        &[0, 1],
        &[2],
    )];
    image.schedule = vec![0];
    image.inputs = vec![
        input("urn:test:trigger", VType::Boolean, &[1]),
        input("urn:test:u", VType::Real, &[0]),
    ];
    image
}

/// The class whose table is sized by its parameter names alone.
pub(crate) const REAL_TIME_TABLE: &str = "CDL.Reals.Sources.TimeTable";

/// What a `CDL.Reals.Sources.TimeTable` whose one parameter is `table_<rows>_2` reports as
/// constructed words: the time column and the value column, one offset and one output kind.
pub(crate) fn time_table_words(rows: u64) -> u64 {
    2 * rows + 2
}

/// One `CDL.Reals.Sources.TimeTable` per entry of `cells`, each with that one parameter (a
/// `table_<row>_<col>` name, which alone sizes the block's table) and one output connector, all
/// scheduled: a blob that decodes whenever the blocks fit the constructed budget (issue #700).
pub(crate) fn time_tables(cells: &[&str]) -> Image {
    let count = u32::try_from(cells.len()).unwrap();
    let mut image = Image::new();
    image.classes = vec![class(REAL_TIME_TABLE)];
    image.connectors = count;
    image.instances = cells
        .iter()
        .zip(0..)
        .map(|(cell, block)| Inst {
            class: 0,
            kind: 0,
            state_len: 0,
            state_crc: 0,
            inputs: Vec::new(),
            outputs: vec![block],
            params: vec![(cell.as_bytes().to_vec(), Val::real(0.0))],
        })
        .collect();
    image.schedule = (0..count).collect();
    image
}

/// One `CDL.Reals.MatrixGain` whose only `K` cell is the corner `K_<rows>_<cols>`, its `rows`
/// outputs and `cols` inputs listed: before issue #772 the corner alone sized a `rows x cols`
/// matrix of identity gains from one record; it is now refused before the block is built.
pub(crate) fn lone_gain_corner(rows: u32, cols: u32) -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.MatrixGain")];
    image.connectors = cols + rows;
    image.instances = vec![Inst {
        class: 0,
        kind: 0,
        state_len: 0,
        state_crc: 0,
        inputs: (0..cols).collect(),
        outputs: (cols..cols + rows).collect(),
        params: vec![(format!("K_{rows}_{cols}").into_bytes(), Val::real(1.0))],
    }];
    image.schedule = vec![0];
    image.inputs = vec![input(
        "urn:t:u",
        VType::Real,
        &(0..cols).collect::<Vec<_>>(),
    )];
    image
}

/// The class whose blocks hold most words per connector among the classes a scalar parameter
/// sizes: `nin` gains and `nin` input kinds behind `nin + 1` connectors.
pub(crate) const REAL_MULTI_SUM: &str = "CDL.Reals.MultiSum";

/// What a `CDL.Reals.MultiSum` of `nin` inputs reports as constructed words: its `nin` gains and
/// its `nin` input kinds, eight to a word.
pub(crate) fn multi_sum_words(nin: u64) -> u64 {
    nin + nin.div_ceil(8)
}

/// The widest `MultiSum` (`nin = 2^20`, `MAX_RESOLVED_PORT_WIDTH`): 1,179,648 words.
pub(crate) const WIDEST_SUM: u32 = 1 << 20;

/// A `MultiSum` width that tops three [`WIDEST_SUM`] blocks up to exactly the constructed budget:
/// `3 x 1,179,648 + 655,360 = 2^22` words.
pub(crate) const BUDGET_TOP_UP_SUM: u32 = 582_542;

/// Blocks that are shape-valid, as every blob the engine writes is (issue #772: the decoder
/// refuses an element outside its class's shape before it builds the block, so a lone table
/// corner cell no longer sizes a block). One `CDL.Reals.MultiSum` per entry of `widths`, `nin` set
/// to it and every input a target of the one Real boundary input, then one complete one-row
/// `CDL.Reals.Sources.TimeTable` (`table_1_1`, `table_1_2`: four words) when `table` is set; every
/// block scheduled in id order. The connectors are numbered block by block: each sum's inputs then
/// its output, then the table's output. Built in the test, never a tracked file.
pub(crate) fn sums_then_table(widths: &[u32], table: bool) -> Image {
    let mut image = Image::new();
    image.classes = vec![class(REAL_MULTI_SUM)];
    let mut next = 0u32;
    let mut targets = Vec::new();
    for &width in widths {
        let inputs: Vec<u32> = (next..next + width).collect();
        targets.extend_from_slice(&inputs);
        image.instances.push(Inst {
            class: 0,
            kind: 0,
            state_len: 0,
            state_crc: 0,
            inputs,
            outputs: vec![next + width],
            params: vec![(b"nin".to_vec(), Val::Integer(i64::from(width)))],
        });
        next += width + 1;
    }
    if table {
        image.classes.push(class(REAL_TIME_TABLE));
        image.instances.push(Inst {
            class: 1,
            kind: 0,
            state_len: 0,
            state_crc: 0,
            inputs: Vec::new(),
            outputs: vec![next],
            params: vec![
                (b"table_1_1".to_vec(), Val::real(0.0)),
                (b"table_1_2".to_vec(), Val::real(0.0)),
            ],
        });
        next += 1;
    }
    image.connectors = next;
    image.schedule = (0..u32::try_from(image.instances.len()).unwrap()).collect();
    if !targets.is_empty() {
        image.inputs = vec![input("urn:t:u", VType::Real, &targets)];
    }
    image
}

/// Three [`WIDEST_SUM`] blocks and one [`BUDGET_TOP_UP_SUM`] block: exactly the constructed
/// budget, 2^22 words, behind 3,728,274 connectors.
pub(crate) const AT_BUDGET_SUMS: [u32; 4] = [WIDEST_SUM, WIDEST_SUM, WIDEST_SUM, BUDGET_TOP_UP_SUM];
