//! Registry sweep for the `set_param` schedule-shape guard (issue #768): for every registered
//! class and every settable, non-structural scalar parameter, each candidate value from a small grid
//! is staged on a one-block engine. A candidate that changes the block's schedule shape (port
//! counts, feedthrough, reads at emit, event roles) must be refused with `ParamStructural`; every
//! other candidate must not be refused for structure.
//!
//! Grid: every member of an enumeration, both Booleans, the integers 0 to 3 and the reals 0, 1, 2,
//! -1, 0.5 and 100. Array parameters (`k_<i>`, `table_<row>_<col>`) are not swept: their element
//! count is `Structural` (or `StructuralArrayElements`) everywhere, and an element's value feeds no
//! feedthrough rule in the registry. A class whose one-block model does not build from its scalar
//! defaults is skipped and pinned by name (the three TimeTables, which have no input, and
//! `Assert`, which has no output), so a change shows.

use std::collections::BTreeSet;
use std::sync::Arc;

use oce_blocks::{
    Block, BlockKind, DefaultLiteral, DefaultSource, ParamNameTemplate, ParamRule, catalog,
    class_parameters, lookup,
};
use oce_model::{ParamTable, Value, ValueType, enum_descriptor, enum_member_ordinal};

use super::state_family_tests::model;
use crate::{Engine, OcError};

fn shape(block: &dyn Block) -> (usize, usize, Vec<bool>, Vec<bool>, Vec<String>) {
    let signature = block.resolved_signature();
    let (ins, outs) = (signature.inputs.len(), signature.outputs.len());
    let algebraic = block.kind() == BlockKind::Algebraic;
    let mut feeds = Vec::new();
    let mut reads = Vec::new();
    let mut roles = Vec::new();
    for i in 0..ins {
        let mut any = false;
        for j in 0..outs {
            let f = block.feeds_through(i, j);
            any |= f;
            feeds.push(f);
        }
        reads.push(algebraic || any);
        roles.push(format!("{:?}", block.event_input_role(i)));
    }
    (ins, outs, feeds, reads, roles)
}

fn literal(kind: ValueType, default: DefaultLiteral) -> Option<Value> {
    match (kind, default) {
        (_, DefaultLiteral::Real(x)) => Some(Value::Real(x)),
        (_, DefaultLiteral::Integer(x)) => Some(Value::Integer(x)),
        (_, DefaultLiteral::Boolean(x)) => Some(Value::Boolean(x)),
        (ValueType::Enum(class), DefaultLiteral::EnumMember(member)) => {
            enum_member_ordinal(class, member).map(|ordinal| Value::Enum { class, ordinal })
        }
        _ => None,
    }
}

fn filler(kind: ValueType) -> Option<Value> {
    match kind {
        ValueType::Real => Some(Value::Real(1.0)),
        ValueType::Integer => Some(Value::Integer(1)),
        ValueType::Boolean => Some(Value::Boolean(false)),
        ValueType::Enum(class) => Some(Value::Enum { class, ordinal: 1 }),
        ValueType::String => None,
    }
}

fn grid(kind: ValueType) -> Vec<Value> {
    match kind {
        ValueType::Real => [0.0, 1.0, 2.0, -1.0, 0.5, 100.0]
            .into_iter()
            .map(Value::Real)
            .collect(),
        ValueType::Integer => (0..=3).map(Value::Integer).collect(),
        ValueType::Boolean => vec![Value::Boolean(false), Value::Boolean(true)],
        ValueType::Enum(class) => (1..=enum_descriptor(class).map_or(0, |d| d.members.len()))
            .map(|ordinal| Value::Enum {
                class,
                ordinal: u32::try_from(ordinal).expect("small"),
            })
            .collect(),
        ValueType::String => Vec::new(),
    }
}

fn structural(class_path: &str, name: &str) -> bool {
    lookup(class_path).is_some_and(|entry| {
        entry.param_rules().iter().any(|rule| match *rule {
            ParamRule::Structural { name: ruled } => ruled == name,
            ParamRule::StructuralArrayElements { base } => {
                ParamNameTemplate::Vector { base }.names(name)
            }
            _ => false,
        })
    })
}

#[test]
fn every_value_that_changes_a_block_s_schedule_shape_is_refused_by_set_param() {
    let mut changing: BTreeSet<(String, String)> = BTreeSet::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut swept = 0usize;
    for entry in catalog() {
        let class_path = entry.class_path;
        let Some(parameters) = class_parameters(class_path) else {
            continue;
        };
        if parameters.iter().any(|p| !p.settable) {
            continue; // an engine-reserved lowering identity: no author can spell its parameters
        }
        // The base table: every scalar parameter at its literal default, a filler for a
        // required one; derived and array parameters left to the constructor.
        let mut base: Vec<(Arc<str>, Value)> = Vec::new();
        for parameter in &parameters {
            if ParamNameTemplate::parse(parameter.name).rank() != 0 {
                continue;
            }
            let value = match parameter.default {
                DefaultSource::Literal(default) => literal(parameter.value_type, default),
                DefaultSource::Required => filler(parameter.value_type),
                DefaultSource::Derived { .. } => None,
            };
            if let Some(value) = value {
                base.push((Arc::from(parameter.name), value));
            }
        }
        let base_table = ParamTable {
            values: base.clone(),
        };
        let reference = shape((lookup(class_path).unwrap().make)(&base_table).as_ref());
        let fresh = || {
            let mut engine = Engine::new();
            engine
                .build_model_in_memory(model(class_path, base_table.clone()), Some("urn:sweep"))
                .ok()
                .map(|()| engine)
        };
        if fresh().is_none() {
            skipped.push(class_path.to_owned());
            continue;
        }
        for (name, _) in &base {
            if structural(class_path, name) {
                continue;
            }
            let kind = base
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.value_type())
                .unwrap();
            for candidate in grid(kind) {
                let mut table = base_table.clone();
                for slot in &mut table.values {
                    if slot.0 == *name {
                        slot.1 = candidate.clone();
                    }
                }
                let changes =
                    shape((lookup(class_path).unwrap().make)(&table).as_ref()) != reference;
                let mut engine = fresh().unwrap();
                engine.halt().unwrap();
                let path = format!("urn:test:{class_path}:block.{name}");
                let result = engine.set_param(&path, candidate.clone());
                swept += 1;
                if changes {
                    changing.insert((class_path.to_owned(), name.to_string()));
                    assert!(
                        matches!(result, Err(OcError::ParamStructural { .. })),
                        "{class_path}.{name} = {candidate:?} changes the schedule shape but \
                         set_param returned {result:?}"
                    );
                } else {
                    assert!(
                        !matches!(result, Err(OcError::ParamStructural { .. })),
                        "{class_path}.{name} = {candidate:?} keeps the schedule shape but was \
                         refused as structural"
                    );
                }
            }
        }
    }
    // The only value-dependent shape in the registry: PIDWithReset's controllerType (decision 10).
    assert_eq!(
        changing,
        BTreeSet::from([(
            "CDL.Reals.PIDWithReset".to_owned(),
            "controllerType".to_owned()
        )]),
    );
    assert!(swept > 500, "the sweep staged {swept} candidates");
    // Skipped because their one-block model needs more than scalar defaults: the three
    // TimeTables need their `table_<row>_<col>` array (and have no input, so no feedthrough to
    // change); `Assert` needs a String `message` (and has no output).
    assert_eq!(
        skipped,
        [
            "CDL.Reals.Sources.TimeTable",
            "CDL.Logical.Sources.TimeTable",
            "CDL.Integers.Sources.TimeTable",
            "CDL.Utilities.Assert",
        ],
        "classes whose defaults do not build"
    );
}
