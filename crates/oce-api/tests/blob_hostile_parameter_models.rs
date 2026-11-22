//! Whole models of hostile parameter tables through the blob decoder (issue #700): seeded
//! models of up to twelve blocks, each a registered class with one of the parameter fuzz's
//! hostile tables (`oce-blocks`' `tests/support/hostile_tables.rs`: a class's own names with
//! index-sized members such as `table_524288_2`, junk names, edge values), written as a blob
//! that is correct in every other respect.
//!
//! For every case the test predicts the decoder's outcome from the blocks it constructed
//! itself: the first block that takes the constructed total past
//! `oce_graph::MAX_CONSTRUCTED_WORDS` is a typed `ConstructedTooLarge` naming that block and
//! the words requested; a block too wide to wire is the arity refusal it was written to be;
//! otherwise the blob decodes. `decode` must agree, `inspect` must carry the same verdict, and
//! the peak held must stay within the documented bound: the linear term, the words of the
//! blocks kept, one block's construction, and the state words.
//!
//! `OCE_MODEL_FUZZ_CASES` overrides the case count (default 100) and `OCE_MODEL_FUZZ_SEED` the
//! seed.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_image;
use crate::blob_models;
use crate::blob_peak_allocation::{LINEAR_PEAK_RATIO, documented_peak_bound};
use crate::hostile_tables;

use std::sync::Arc;

use allocation_counter::measure;
use blob_image::{Image, In, Inst, VType, Val, words_crc};
use blob_models::class;
use hostile_tables::{XorShift, env_u64, hostile_tables_sized_by_index as tables_of};
use oce_blocks::{Block, BlockKind, MAX_CONSTRUCTION_PEAK_BYTES, PortKind, catalog, lookup};
use oce_graph::MAX_CONSTRUCTED_WORDS;
use oce_graph::blob::{BlobError, InstanceField, decode, inspect};
use oce_model::{ParamTable, Value, enum_descriptor};

/// A block with more ports or state than this is written without them, so the decoder refuses
/// it at its arity or state length: wiring a million connectors is the linear term, which
/// `blob_peak_allocation.rs` pins.
const MAX_WIRED_PORTS: usize = 2048;
const MAX_WIRED_STATE_WORDS: usize = 1 << 16;

/// The table as the decoder would construct from it: one binding per name, no non-finite Real,
/// no Enum the wire form cannot carry and no value its parameter's catalog type does not admit
/// (issue #734). The decoder refuses each of those before it constructs the block, which is not
/// what this fuzz is about.
fn constructible(class_path: &str, table: &ParamTable) -> ParamTable {
    let mut values: Vec<(Arc<str>, Value)> = Vec::new();
    for (name, value) in &table.values {
        let carried = match value {
            Value::Real(x) => x.is_finite(),
            Value::Enum { class, ordinal } => enum_descriptor(*class).is_some_and(|descriptor| {
                *ordinal >= 1 && *ordinal as usize <= descriptor.members.len()
            }),
            _ => true,
        } && oce_blocks::parameter_kind_fault(class_path, name, value).is_none();
        if carried && !values.iter().any(|(seen, _)| seen == name) {
            values.push((Arc::clone(name), value.clone()));
        }
    }
    ParamTable { values }
}

/// One block of a case: what the test learned by constructing it.
struct Planned {
    class_path: &'static str,
    params: ParamTable,
    block: Box<dyn Block>,
    words: usize,
    /// The refusal this block's record earns when it is too wide to wire, if any.
    unwired: Option<InstanceField>,
}

/// The image of `plan`, every wired block's ports claimed in order and every input driven by a
/// boundary input of its kind, and the outcome the decoder must report for it.
fn image_of(plan: &[Planned]) -> (Image, Result<(), BlobError>) {
    let mut paths: Vec<&str> = plan.iter().map(|block| block.class_path).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut image = Image::new();
    image.classes = paths.iter().map(|path| class(path)).collect();
    let mut connector = 0u32;
    let mut claim = |count: usize| -> Vec<u32> {
        let ids: Vec<u32> = (connector..connector + u32::try_from(count).unwrap()).collect();
        connector += u32::try_from(count).unwrap();
        ids
    };
    for planned in plan {
        let block = &planned.block;
        let stateful = block.kind() == BlockKind::Stateful;
        let signature = block.resolved_signature();
        let (inputs, outputs, state_len, state_crc) = if planned.unwired.is_some() {
            (Vec::new(), Vec::new(), 0, 0)
        } else {
            let mut region = vec![0u64; block.state_len()];
            block.init_state(&mut region, &planned.params);
            let inputs = claim(signature.inputs.len());
            for (id, kind) in inputs.iter().zip(signature.inputs.iter()) {
                image.inputs.push(In {
                    path: format!("urn:fuzz:{id:08}").into_bytes(),
                    vtype: match kind {
                        PortKind::Real => VType::Real,
                        PortKind::Integer => VType::Integer,
                        PortKind::Boolean => VType::Boolean,
                    },
                    min: None,
                    max: None,
                    targets: vec![*id],
                    payload: Vec::new(),
                });
            }
            (
                inputs,
                claim(signature.outputs.len()),
                u32::try_from(region.len()).unwrap(),
                if stateful { words_crc(&region) } else { 0 },
            )
        };
        image.instances.push(Inst {
            class: u32::try_from(
                paths
                    .iter()
                    .position(|path| *path == planned.class_path)
                    .unwrap(),
            )
            .unwrap(),
            kind: u8::from(stateful),
            state_len,
            state_crc,
            inputs,
            outputs,
            params: planned
                .params
                .values
                .iter()
                .map(|(name, value)| (name.as_bytes().to_vec(), Val::from_value(value)))
                .collect(),
        });
    }
    image.connectors = connector;
    image.schedule = (0..u32::try_from(plan.len()).unwrap()).collect();

    // The oracle: the decoder charges a block the moment it is constructed, then compares it.
    let mut total = 0usize;
    for (index, planned) in plan.iter().enumerate() {
        let block = u32::try_from(index).unwrap();
        total += planned.words;
        if total > MAX_CONSTRUCTED_WORDS {
            let requested = total as u64;
            return (
                image,
                Err(BlobError::ConstructedTooLarge { block, requested }),
            );
        }
        if let Some(field) = planned.unwired {
            return (image, Err(BlobError::InstanceMismatch { block, field }));
        }
    }
    (image, Ok(()))
}

fn plan_block(class_path: &'static str, table: &ParamTable) -> Planned {
    let params = constructible(class_path, table);
    let block = (lookup(class_path).unwrap().make)(&params);
    let signature = block.resolved_signature();
    let (inputs, outputs) = (signature.inputs.len(), signature.outputs.len());
    let wired = inputs + outputs <= MAX_WIRED_PORTS && block.state_len() <= MAX_WIRED_STATE_WORDS;
    // An unwired record carries no state and no ports, so the first of the block's own state
    // length, inputs and outputs that is not zero is the field the decoder names.
    let unwired = if wired {
        None
    } else if block.state_len() != 0 {
        Some(InstanceField::StateLen)
    } else if inputs != 0 {
        Some(InstanceField::InputArity)
    } else {
        Some(InstanceField::OutputArity)
    };
    drop(signature);
    Planned {
        class_path,
        words: block.constructed_words(),
        params,
        block,
        unwired,
    }
}

/// The shapes of issue #700 at seeded sizes: a table sized by one cell name, or a dense gain
/// with no cell bound. Each holds up to 8 MiB behind a few ports, so it can be wired and a
/// model of them accumulates until the budget refuses it.
fn narrow_and_heavy(rng: &mut XorShift) -> (&'static str, ParamTable) {
    let integer = |name: String, value: i64| (Arc::from(name.as_str()), Value::Integer(value));
    if rng.below(8) == 0 {
        let (nin, nout) = *rng.pick(&[(1024, 1024), (1024, 1000), (512, 512), (2000, 48)]);
        let values = vec![integer("nin".into(), nin), integer("nout".into(), nout)];
        return ("CDL.Reals.MatrixGain", ParamTable { values });
    }
    let class_path = *rng.pick(&[
        "CDL.Reals.Sources.TimeTable",
        "CDL.Integers.Sources.TimeTable",
        "CDL.Logical.Sources.TimeTable",
    ]);
    let rows = *rng.pick(&[
        1usize, 1000, 262_143, 349_525, 349_525, 524_287, 524_287, 524_288,
    ]);
    let cols = *rng.pick(&[2usize, 2, 3]);
    let values = vec![integer(format!("table_{rows}_{cols}"), 0)];
    (class_path, ParamTable { values })
}

/// Every size parameter of `class_path` set to 5,000: more ports than this fuzz wires, for
/// every class whose width a parameter sets (a table keeps its one default output).
fn wide(class_path: &str) -> ParamTable {
    let values = hostile_tables::names_of(class_path)
        .into_iter()
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic()))
        .map(|name| (Arc::from(name.as_str()), Value::Integer(5_000)))
        .collect();
    ParamTable { values }
}

#[test]
fn hostile_models_decode_or_refuse_typed_within_the_documented_peak() {
    let positive = measure(|| drop(std::hint::black_box(vec![0_u8; 64])));
    assert_eq!(positive.count_total, 1, "the counter observes allocations");

    let cases = env_u64("OCE_MODEL_FUZZ_CASES", 100);
    let mut rng = XorShift(env_u64("OCE_MODEL_FUZZ_SEED", 0x0CEB_0700_B10B_F00D).max(1));
    // Three blocks in four come from the classes whose shape a parameter sets.
    let sized: Vec<&'static str> = catalog()
        .iter()
        .filter(|entry| entry.width_driven || entry.class_path.ends_with(".Sources.TimeTable"))
        .map(|entry| entry.class_path)
        .collect();
    assert_eq!(sized.len(), 28, "the width-driven classes and the tables");
    let every: Vec<&'static str> = catalog().iter().map(|entry| entry.class_path).collect();

    let (mut accepted, mut crossed, mut unwired, mut largest) = (0u64, 0u64, 0u64, 0u64);
    for case in 0..cases {
        // One case in three takes its blocks as they come, so a block too wide to wire ends
        // it at that block. The others redraw such a block, so the model is refused only by
        // the budget, if at all.
        let any_width = rng.below(3) == 0;
        let plan: Vec<Planned> = (0..2 + rng.below(12))
            .map(|_| {
                loop {
                    let planned = match rng.below(6) {
                        0..=3 => {
                            let (class_path, table) = narrow_and_heavy(&mut rng);
                            plan_block(class_path, &table)
                        }
                        4 => {
                            let class_path = *rng.pick(&every);
                            let tables = tables_of(class_path, 8, &mut rng);
                            plan_block(class_path, rng.pick(&tables))
                        }
                        _ if any_width => {
                            let class_path = *rng.pick(&sized);
                            plan_block(class_path, &wide(class_path))
                        }
                        _ => {
                            let class_path = *rng.pick(&sized);
                            let tables = tables_of(class_path, 8, &mut rng);
                            plan_block(class_path, rng.pick(&tables))
                        }
                    };
                    if any_width || planned.unwired.is_none() {
                        break planned;
                    }
                }
            })
            .collect();
        let (image, expected) = image_of(&plan);
        let bytes = image.to_bytes();
        let describe = || {
            plan.iter()
                .map(|block| format!("{} {:?}", block.class_path, block.params.values))
                .collect::<Vec<_>>()
        };

        let mut outcome = Ok(());
        let info = measure(|| outcome = decode(&bytes).map(|_| ()));
        assert_eq!(outcome, expected, "case {case}: {:?}", describe());
        let verdict = inspect(&bytes)
            .unwrap_or_else(|error| panic!("case {case}: no summary: {error:?}"))
            .decode_refusal;
        assert_eq!(verdict, expected.err(), "case {case}: inspect");

        // The blocks kept when the decoder stopped, one construction on top, and the state the
        // wired blocks own (at most `MAX_WIRED_STATE_WORDS` each, far inside the state budget).
        let stopped_at = match expected {
            Ok(()) => plan.len(),
            Err(BlobError::ConstructedTooLarge { block, .. })
            | Err(BlobError::InstanceMismatch { block, .. }) => block as usize,
            Err(other) => panic!("case {case}: unpredicted {other:?}"),
        };
        let kept: usize = plan[..stopped_at].iter().map(|block| block.words).sum();
        assert!(kept <= MAX_CONSTRUCTED_WORDS, "case {case}: kept {kept}");
        let state = 8 * (plan.len() * MAX_WIRED_STATE_WORDS) as u64;
        let bound = LINEAR_PEAK_RATIO * bytes.len() as u64
            + 8 * kept as u64
            + MAX_CONSTRUCTION_PEAK_BYTES as u64
            + state;
        assert!(
            info.bytes_max <= bound && info.bytes_max <= documented_peak_bound(bytes.len()),
            "case {case}: peak {} over {bound}: {:?}",
            info.bytes_max,
            describe()
        );
        largest = largest.max(info.bytes_max);
        match expected {
            Ok(()) => accepted += 1,
            Err(BlobError::ConstructedTooLarge { .. }) => crossed += 1,
            Err(_) => unwired += 1,
        }
    }
    // The seeded default reaches all three outcomes, and models that hold most of the budget.
    if cases >= 100 {
        assert!(accepted >= 10, "{accepted} accepted");
        assert!(crossed >= 10, "{crossed} past the budget");
        assert!(unwired >= 10, "{unwired} too wide to wire");
        assert!(largest > 24 << 20, "largest peak {largest}");
    }
}
