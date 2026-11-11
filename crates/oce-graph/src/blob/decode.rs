//! The building pass: refusal steps 7 to 10 over a blob whose structure is already proven.
//!
//! Every allocation here is sized by a count the structural pass checked against the input
//! (`sections::check_structure`), or by a constructed block whose arity has just been compared with
//! the blob's own port lists. No step panics on any input.

use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};

use oce_blocks::{Block, BlockKind, RegistryEntry, class_is_target_bound, lookup};
use oce_model::{
    BlockId, BlockInstance, Connection, ConnectorId, Dir, Model, ParamTable, Value, ValueType,
    enum_descriptor_by_path,
};

use super::connectors::Owners;
use super::error::{
    BlobError, ConnectionFault, ConnectorFault, InputFault, InstanceField, OutputFault,
    ScheduleFault,
};
use super::sections::{
    self, Counts, Layout, class_count, class_record, connection_count, input_count, input_record,
    instance_counts, instance_record, output_count, output_record, parameter,
};
use super::wire::{RawType, RawValue, state_crc};
use super::{BlobInput, BlobOutput, Decoded};
use crate::{MAX_STATE_WORDS, Schedule, allocate_state, build_driver_of};

/// Decode and check one `OCEB` blob into a ready-to-tick executable.
///
/// The checks run in this order, and the first failure is returned:
///
/// 1. the length is at most [`MAX_BLOB_BYTES`](super::MAX_BLOB_BYTES) ([`BlobError::TooLarge`]);
/// 2. a header and trailer fit and the magic is `OCEB` ([`BlobError::Malformed`]);
/// 3. the format version is [`FORMAT_VERSION`](super::FORMAT_VERSION)
///    ([`BlobError::UnsupportedFormat`]), then the engine version is
///    [`ENGINE_VERSION`](super::ENGINE_VERSION) ([`BlobError::EngineVersionMismatch`]): nothing
///    after the version field is checked in a blob another engine version wrote, whose length,
///    trailer and body may be laid out differently under the same format number;
/// 4. the header length equals the input length ([`BlobError::Malformed`]), then the CRC-32C
///    trailer matches ([`BlobError::IntegrityMismatch`]);
/// 5. the execution profile is this build's and the reserved flags are zero;
/// 6. structure, each with a byte offset: section order and lengths, bounded counts, UTF-8, tags,
///    canonical list order and exact consumption ([`BlobError::Malformed`]);
/// 7. classes (registered, same signature hash, each used) and instances (class index, unique
///    parameter names, finite Reals, known enum classes and ordinals, then the constructed block's
///    kind, state length and arity, and every connector id exactly once);
/// 8. boundary inputs, connections (direction, kind, distinct blocks, not an input target, one
///    driver), undriven inputs and boundary outputs;
/// 9. the schedule: a permutation that respects emit-before under this build's `feeds_through`;
/// 10. the state budget, then each stateful block's initial-state CRC.
///
/// I/O paths and `SemanticPoint` payloads borrow from `bytes`. The allocations are listed in the
/// [module documentation](super#allocation).
///
/// # Errors
/// A typed [`BlobError`] naming the first failed check; never a panic.
pub fn decode(bytes: &[u8]) -> Result<Decoded<'_>, BlobError> {
    let layout = sections::frame(bytes)?;
    let counts = sections::check_structure(&layout)?;

    let Instances {
        mut model,
        blocks,
        classes,
        mut owners,
    } = build_instances(&layout, &counts)?;
    let target_bound = classes
        .iter()
        .any(|(_, path, _)| class_is_target_bound(path));
    drop(classes);

    let inputs = build_inputs(&layout, &mut owners)?;
    model.connections = build_connections(&layout, &owners)?;
    let driver_of = build_driver_of(&model);
    for (id, driver) in driver_of.iter().enumerate() {
        let id = index_u32(id);
        let undriven = owners
            .get(id)
            .is_some_and(|port| port.dir == Dir::In && !port.target && driver.0 == id);
        if undriven {
            return Err(BlobError::UndrivenInput { connector: id });
        }
    }
    let outputs = build_outputs(&layout, &owners)?;

    let order = build_order(&layout, &model, &blocks, &owners)?;
    drop(owners);
    let schedule = Schedule {
        order,
        connector_order: Vec::new(),
        driver_of,
    };

    let state = build_state(&layout, &model, &blocks)?;
    let content_id = (layout.content_id != [0u8; 32]).then_some(layout.content_id);
    Ok(Decoded {
        model,
        schedule,
        blocks,
        state,
        inputs,
        outputs,
        content_id,
        target_bound,
    })
}

/// The class table: registry entry, shared class path, and whether an instance used it.
type ClassTable = Vec<(&'static RegistryEntry, Arc<str>, bool)>;

/// What step 7 builds: the model arena, the constructed blocks, the class table and the
/// per-connector ownership table.
struct Instances {
    model: Model,
    blocks: Vec<Box<dyn Block>>,
    classes: ClassTable,
    owners: Owners,
}

/// Step 7: classes, then every instance with its constructed block and its connectors.
fn build_instances(layout: &Layout<'_>, counts: &Counts) -> Result<Instances, BlobError> {
    let mut r = layout.classes.reader();
    let class_total = class_count(&mut r)?;
    let mut classes: ClassTable = Vec::with_capacity(class_total);
    for index in 0..class_total {
        let record = class_record(&mut r)?;
        let class = index_u32(index);
        let entry = lookup(record.path).ok_or(BlobError::UnknownClass { class })?;
        let expected = oce_blocks::class_signature_hash(record.path)
            .ok_or(BlobError::UnknownClass { class })?;
        if expected != record.hash {
            return Err(BlobError::ClassSignatureMismatch {
                class,
                expected,
                found: record.hash,
            });
        }
        classes.push((entry, Arc::from(record.path), false));
    }

    let mut r = layout.instances.reader();
    let (block_total, connector_total) = instance_counts(&mut r)?;
    let mut model = Model::new();
    model.blocks = Vec::with_capacity(block_total);
    // Eight bytes per connector id; the arena is built from it once every id is claimed.
    let mut owners = Owners::new(connector_total);
    let mut blocks: Vec<Box<dyn Block>> = Vec::with_capacity(block_total);
    let mut used = 0usize;
    let mut names_scratch: Vec<&str> = Vec::with_capacity(counts.largest_parameter_list);

    for index in 0..block_total {
        let block = index_u32(index);
        let record = instance_record(&mut r)?;
        let class_index = usize::try_from(record.class).unwrap_or(usize::MAX);
        let Some((entry, class_path, class_used)) = classes.get_mut(class_index) else {
            return Err(BlobError::InstanceMismatch {
                block,
                field: InstanceField::ClassIndex,
            });
        };
        if !*class_used {
            *class_used = true;
            used += 1;
        }
        let (entry, class_path) = (*entry, Arc::clone(class_path));

        // Parameters: unique names, finite Reals, resolvable enums.
        let mut names = record.params.reader();
        names_scratch.clear();
        for _ in 0..record.params.count {
            let (name, _, _) = parameter(&mut names)?;
            names_scratch.push(name);
        }
        names_scratch.sort_unstable();
        if let Some(pair) = names_scratch.windows(2).find(|pair| pair[0] == pair[1]) {
            // Report the second occurrence of the first repeated name in sorted order: one more
            // linear pass, so a hostile table cannot make this quadratic.
            let repeated = pair[0];
            let mut seen = record.params.reader();
            let mut occurrences = 0;
            for position in 0..record.params.count {
                if parameter(&mut seen)?.0 == repeated {
                    occurrences += 1;
                    if occurrences == 2 {
                        return Err(BlobError::DuplicateParameter {
                            block,
                            parameter: index_u32(position),
                        });
                    }
                }
            }
        }
        let mut values = record.params.reader();
        let mut params = Vec::with_capacity(record.params.count);
        for parameter_index in 0..record.params.count {
            let (name, raw, at) = parameter(&mut values)?;
            if let RawValue::Real(bits) = raw
                && !f64::from_bits(bits).is_finite()
            {
                return Err(BlobError::NonFiniteParameter {
                    block,
                    parameter: index_u32(parameter_index),
                });
            }
            params.push((Arc::from(name), value(raw, at)?));
        }
        let params = ParamTable { values: params };

        let instance = (entry.make)(&params);
        let signature = instance.resolved_signature();
        let mismatch = |field| Err(BlobError::InstanceMismatch { block, field });
        if instance.kind() != record.kind {
            return mismatch(InstanceField::Kind);
        }
        if u64::try_from(instance.state_len()).ok() != Some(u64::from(record.state_len)) {
            return mismatch(InstanceField::StateLen);
        }
        if record.kind == BlockKind::Algebraic && record.state_crc != 0 {
            return mismatch(InstanceField::StateCrc);
        }
        if signature.inputs.len() != record.inputs.len() {
            return mismatch(InstanceField::InputArity);
        }
        if signature.outputs.len() != record.outputs.len() {
            return mismatch(InstanceField::OutputArity);
        }
        let mut inputs = Vec::with_capacity(record.inputs.len());
        for (port, (id, kind)) in record
            .inputs
            .iter()
            .zip(signature.inputs.iter())
            .enumerate()
        {
            owners.claim(id, block, port, Dir::In, *kind)?;
            inputs.push(ConnectorId(id));
        }
        let mut outputs = Vec::with_capacity(record.outputs.len());
        for (port, (id, kind)) in record
            .outputs
            .iter()
            .zip(signature.outputs.iter())
            .enumerate()
        {
            owners.claim(id, block, port, Dir::Out, *kind)?;
            outputs.push(ConnectorId(id));
        }
        drop(signature);
        model.blocks.push(BlockInstance {
            id: BlockId(block),
            class_iri: class_path,
            inputs,
            outputs,
            params,
            decl_order: block,
            instance_iri: None,
        });
        blocks.push(instance);
    }
    if used != class_total {
        let class = classes.iter().position(|(_, _, used)| !used).unwrap_or(0);
        return Err(BlobError::UnusedClass {
            class: index_u32(class),
        });
    }
    if let Some(missing) = owners.first_unclaimed() {
        return Err(BlobError::InvalidConnector {
            connector: missing,
            fault: ConnectorFault::Missing,
        });
    }
    model.connectors = owners.connectors();
    Ok(Instances {
        model,
        blocks,
        classes,
        owners,
    })
}

/// A raw value as a model value, resolving an Enum's class path and range-checking its ordinal.
fn value(raw: RawValue<'_>, at: u32) -> Result<Value, BlobError> {
    Ok(match raw {
        RawValue::Real(bits) => Value::Real(f64::from_bits(bits)),
        RawValue::Integer(n) => Value::Integer(n),
        RawValue::Boolean(b) => Value::Boolean(b),
        RawValue::String(s) => Value::String(Arc::from(s)),
        RawValue::Enum {
            class_path,
            ordinal,
        } => {
            let descriptor = enum_descriptor_by_path(class_path)
                .ok_or(BlobError::UnknownEnumClass { offset: at })?;
            let members = u32::try_from(descriptor.members.len()).unwrap_or(u32::MAX);
            if ordinal == 0 || ordinal > members {
                return Err(BlobError::EnumOrdinalOutOfRange {
                    offset: at,
                    ordinal,
                });
            }
            Value::Enum {
                class: descriptor.id,
                ordinal,
            }
        }
    })
}

/// Step 8, inputs: type, targets and bounds of every boundary input; marks each target.
fn build_inputs<'a>(
    layout: &Layout<'a>,
    owners: &mut Owners,
) -> Result<Vec<BlobInput<'a>>, BlobError> {
    let mut r = layout.io.reader();
    let count = input_count(&mut r)?;
    let mut inputs = Vec::with_capacity(count);
    for index in 0..count {
        let input = index_u32(index);
        let fault = |fault| Err(BlobError::InvalidInput { input, fault });
        let record = input_record(&mut r)?;
        let value_type = match record.value_type {
            RawType::Real => ValueType::Real,
            RawType::Integer => ValueType::Integer,
            RawType::Boolean => ValueType::Boolean,
            RawType::String => ValueType::String,
            RawType::Enum => return fault(InputFault::EnumType),
        };
        if record.targets.len() == 0 {
            return fault(InputFault::NoTargets);
        }
        let mut targets = Vec::with_capacity(record.targets.len());
        for target in record.targets.iter() {
            let Some(connector) = owners.get(target) else {
                return fault(InputFault::TargetOutOfRange);
            };
            if connector.dir != Dir::In {
                return fault(InputFault::TargetNotInput);
            }
            if connector.value_type != value_type {
                return fault(InputFault::TargetTypeMismatch);
            }
            if connector.target {
                return fault(InputFault::TargetShared);
            }
            owners.mark_target(target);
            targets.push(ConnectorId(target));
        }
        let bound = |raw: Option<(RawValue<'_>, u32)>| -> Result<Option<Value>, BlobError> {
            let Some((raw, at)) = raw else {
                return Ok(None);
            };
            let typed = matches!(
                (value_type, raw),
                (ValueType::Real, RawValue::Real(_)) | (ValueType::Integer, RawValue::Integer(_))
            );
            if !typed {
                return Err(BlobError::InvalidInput {
                    input,
                    fault: InputFault::BoundTypeMismatch,
                });
            }
            value(raw, at).map(Some)
        };
        let min = bound(record.min)?;
        let max = bound(record.max)?;
        if !bounds_are_valid(min.as_ref(), max.as_ref()) {
            return fault(InputFault::InvalidBound);
        }
        inputs.push(BlobInput {
            path: record.path,
            value_type,
            min,
            max,
            targets,
            semantic_point: record.semantic_point,
        });
    }
    Ok(inputs)
}

/// Step 8, connections: well-typed output-to-input edges between distinct blocks, one driver per
/// input, never into a boundary input target.
fn build_connections(layout: &Layout<'_>, owners: &Owners) -> Result<Vec<Connection>, BlobError> {
    let mut r = layout.connections.reader();
    let count = connection_count(&mut r)?;
    let mut connections = Vec::with_capacity(count);
    let mut previous: Option<u32> = None;
    for index in 0..count {
        let connection = index_u32(index);
        let fault = |fault| Err(BlobError::InvalidConnection { connection, fault });
        let from = r.u32()?;
        let to = r.u32()?;
        let (Some(producer), Some(consumer)) = (owners.get(from), owners.get(to)) else {
            return fault(ConnectionFault::OutOfRange);
        };
        if producer.dir != Dir::Out {
            return fault(ConnectionFault::ProducerNotOutput);
        }
        if consumer.dir != Dir::In {
            return fault(ConnectionFault::ConsumerNotInput);
        }
        if producer.value_type != consumer.value_type {
            return fault(ConnectionFault::KindMismatch);
        }
        if producer.block == consumer.block {
            return fault(ConnectionFault::SameBlock);
        }
        if consumer.target {
            return fault(ConnectionFault::ConsumerIsInputTarget);
        }
        if previous == Some(to) {
            return fault(ConnectionFault::MultipleDrivers);
        }
        previous = Some(to);
        connections.push(Connection {
            from: ConnectorId(from),
            to: ConnectorId(to),
        });
    }
    Ok(connections)
}

/// Step 8, outputs: each source an output connector, no path twice.
fn build_outputs<'a>(
    layout: &Layout<'a>,
    owners: &Owners,
) -> Result<Vec<BlobOutput<'a>>, BlobError> {
    let mut r = layout.io.reader();
    let inputs = input_count(&mut r)?;
    for _ in 0..inputs {
        input_record(&mut r)?;
    }
    let count = output_count(&mut r)?;
    let mut outputs: Vec<BlobOutput<'a>> = Vec::with_capacity(count);
    for index in 0..count {
        let output = index_u32(index);
        let record = output_record(&mut r)?;
        let fault = |fault| Err(BlobError::InvalidOutput { output, fault });
        match owners.get(record.source) {
            None => return fault(OutputFault::SourceOutOfRange),
            Some(connector) if connector.dir != Dir::Out => {
                return fault(OutputFault::SourceNotOutput);
            }
            Some(_) => {}
        }
        if outputs.last().is_some_and(|last| last.path == record.path) {
            return fault(OutputFault::DuplicatePath);
        }
        outputs.push(BlobOutput {
            path: record.path,
            source: ConnectorId(record.source),
            semantic_point: record.semantic_point,
        });
    }
    Ok(outputs)
}

/// Step 9: the carried order is a permutation of the blocks and respects emit-before: for every
/// connection into an input its consumer reads at emit time, the producer is scheduled strictly
/// earlier. Any such order gives the host's outputs; the host's tie-break need not be reproduced.
fn build_order(
    layout: &Layout<'_>,
    model: &Model,
    blocks: &[Box<dyn Block>],
    owners: &Owners,
) -> Result<Vec<BlockId>, BlobError> {
    let mut r = layout.schedule.reader();
    let list = r.u32_list()?;
    let total = model.blocks.len();
    if list.len() != total {
        return Err(BlobError::InvalidSchedule {
            position: index_u32(list.len().min(total)),
            fault: ScheduleFault::CountMismatch,
        });
    }
    let mut position_of = vec![u32::MAX; total];
    let mut order = Vec::with_capacity(total);
    for (position, block) in list.iter().enumerate() {
        let position = index_u32(position);
        let Some(slot) = position_of.get_mut(usize::try_from(block).unwrap_or(usize::MAX)) else {
            return Err(BlobError::InvalidSchedule {
                position,
                fault: ScheduleFault::BlockOutOfRange,
            });
        };
        if *slot != u32::MAX {
            return Err(BlobError::InvalidSchedule {
                position,
                fault: ScheduleFault::Duplicate,
            });
        }
        *slot = position;
        order.push(BlockId(block));
    }
    for connection in &model.connections {
        // Both endpoints were range-checked when the connection was decoded.
        let (Some(from), Some(to)) = (owners.get(connection.from.0), owners.get(connection.to.0))
        else {
            continue;
        };
        let (producer, consumer) = (from.block as usize, to.block as usize);
        let instance = blocks[consumer].as_ref();
        let reads_at_emit = instance.kind() == BlockKind::Algebraic || {
            let port = to.port as usize;
            (0..model.blocks[consumer].outputs.len()).any(|out| instance.feeds_through(port, out))
        };
        if reads_at_emit && position_of[producer] >= position_of[consumer] {
            return Err(BlobError::InvalidSchedule {
                position: position_of[consumer],
                fault: ScheduleFault::EmitBefore,
            });
        }
    }
    Ok(order)
}

/// Step 10: the state budget (checked before `allocate_state` reserves anything), the allocation,
/// then each stateful block's initial-state CRC against the carried one.
fn build_state(
    layout: &Layout<'_>,
    model: &Model,
    blocks: &[Box<dyn Block>],
) -> Result<crate::RunState, BlobError> {
    let mut total = 0usize;
    let mut last_stateful = 0u32;
    for (index, block) in blocks.iter().enumerate() {
        if block.kind() != BlockKind::Stateful {
            continue;
        }
        last_stateful = index_u32(index);
        match total.checked_add(block.state_len()) {
            Some(next) if next <= MAX_STATE_WORDS => total = next,
            over => {
                return Err(BlobError::StateTooLarge {
                    block: last_stateful,
                    requested: over.map_or(u64::MAX, |n| u64::try_from(n).unwrap_or(u64::MAX)),
                });
            }
        }
    }
    let state = allocate_state(model, blocks).map_err(|_| BlobError::StateTooLarge {
        block: last_stateful,
        requested: u64::try_from(total).unwrap_or(u64::MAX),
    })?;

    let mut r = layout.instances.reader();
    let (block_total, _) = instance_counts(&mut r)?;
    for index in 0..block_total {
        let record = instance_record(&mut r)?;
        if record.kind != BlockKind::Stateful {
            continue;
        }
        let slot = state.slots[state.slot_of[index]];
        let found = state_crc(&state.words[slot.offset..slot.offset + slot.len]);
        if found != record.state_crc {
            return Err(BlobError::InitialStateMismatch {
                block: index_u32(index),
                expected: record.state_crc,
                found,
            });
        }
    }
    Ok(state)
}

/// Whether an input's acceptance domain is well formed: Real bounds finite, and `min <= max`
/// when both are present. A NaN bound would admit or refuse every value depending on how a
/// frame driver compares it, so it is refused rather than interpreted.
pub(super) fn bounds_are_valid(min: Option<&Value>, max: Option<&Value>) -> bool {
    let finite = |bound: Option<&Value>| match bound {
        Some(Value::Real(x)) => x.is_finite(),
        _ => true,
    };
    let ordered = match (min, max) {
        (Some(Value::Real(lo)), Some(Value::Real(hi))) => lo <= hi,
        (Some(Value::Integer(lo)), Some(Value::Integer(hi))) => lo <= hi,
        _ => true,
    };
    finite(min) && finite(max) && ordered
}

/// An index into a list whose length the structural pass bounded by a 64 MiB input.
fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
