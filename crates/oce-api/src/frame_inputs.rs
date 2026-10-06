//! Exact executable-boundary metadata, separate from the lossy Store point projection.

use std::collections::{HashMap, HashSet};

use oce_model::{Attrs, ConnectorId, ModelGraph, Value, ValueType, enum_descriptor};

use crate::io::connector_path;

/// Owned definition of one required executable boundary input for complete-frame construction.
///
/// Entries are returned in canonical UTF-8 path order by `Engine::input_definitions`. Bounds
/// intersect declaration and all target bounds; Integer defaults use the CDL i32 range, represented
/// exactly as Integer values (never converted to f64). Enum bounds are inclusive legal ordinals
/// with the exact class identity. Boolean/String have no bounds. No implicit input defaults exist.
///
/// This is editable host-owned metadata, not a validation authority or reusable model reference.
/// Retain values if useful, but refresh the definition after successful load/reconfiguration.
/// Cloning allocates its path and shares any immutable `Value::String` storage. No Store handles,
/// generation tokens or connector indices are exposed. Reading/cloning has no engine side effects.
#[derive(Clone, Debug)]
pub struct InputDefinition {
    /// Expanded canonical authored identity. No compact-IRI expansion occurs at submission time.
    pub path: String,
    /// Exact native type, including enum class; no Store-carrier coercion.
    pub value_type: ValueType,
    /// Inclusive lower bound in computation units, or absent. Real absence admits negative infinity.
    pub min: Option<Value>,
    /// Inclusive upper bound in computation units, or absent. Real absence admits positive infinity.
    pub max: Option<Value>,
}

impl InputDefinition {
    /// Called only after exact type checking, including enum class.
    pub(crate) fn accepts_domain(&self, value: &Value) -> bool {
        match value {
            Value::Real(value) => {
                self.min
                    .as_ref()
                    .is_none_or(|min| matches!(min, Value::Real(min) if value >= min))
                    && self
                        .max
                        .as_ref()
                        .is_none_or(|max| matches!(max, Value::Real(max) if value <= max))
            }
            Value::Integer(value) => {
                self.min
                    .as_ref()
                    .is_none_or(|min| matches!(min, Value::Integer(min) if value >= min))
                    && self
                        .max
                        .as_ref()
                        .is_none_or(|max| matches!(max, Value::Integer(max) if value <= max))
            }
            Value::Enum { class, ordinal } => enum_descriptor(*class).is_some_and(|descriptor| {
                *ordinal > 0 && (*ordinal as usize) <= descriptor.members.len()
            }),
            Value::Boolean(_) | Value::String(_) => true,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InputBinding {
    pub(crate) targets: Vec<ConnectorId>,
    pub(crate) value_type: ValueType,
    pub(crate) boundary_index: Option<usize>,
}

impl InputBinding {
    pub(crate) fn accepts_type(&self, model: &ModelGraph, value: &Value) -> bool {
        self.targets
            .iter()
            .all(|id| model.connectors[id.0 as usize].value_type == value.value_type())
    }
}

pub(crate) fn build_inputs(
    model: &ModelGraph,
) -> (HashMap<String, InputBinding>, Vec<InputDefinition>) {
    let external: HashSet<_> = model.external_inputs.iter().copied().collect();
    let mut bindings: HashMap<String, InputBinding> = HashMap::new();
    let mut paths = Vec::new();
    for connector in model
        .connectors
        .iter()
        .filter(|c| c.dir == oce_model::Dir::In)
    {
        let path = connector_path(connector.iri.as_deref(), connector.id);
        let binding = bindings
            .entry(path.clone())
            .or_insert_with(|| InputBinding {
                targets: Vec::new(),
                value_type: connector.value_type,
                boundary_index: None,
            });
        binding.targets.push(connector.id);
        if external.contains(&connector.id) && binding.boundary_index.is_none() {
            binding.boundary_index = Some(0);
            paths.push(path);
        }
    }
    paths.sort_unstable();
    let declarations: HashMap<_, _> = model
        .boundary_inputs
        .iter()
        .map(|d| (d.iri.as_ref(), &d.attrs))
        .collect();
    let definitions = paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let binding = bindings
                .get_mut(&path)
                .expect("input path came from bindings");
            binding.boundary_index = Some(index);
            let mut definition = InputDefinition {
                path,
                value_type: binding.value_type,
                min: None,
                max: None,
            };
            for target in &binding.targets {
                add_bounds(&mut definition, &model.connectors[target.0 as usize].attrs);
            }
            if let Some(attrs) = declarations.get(definition.path.as_str()) {
                add_bounds(&mut definition, attrs);
            }
            if let ValueType::Enum(class) = definition.value_type {
                definition.min = Some(Value::Enum { class, ordinal: 1 });
                definition.max = Some(Value::Enum {
                    class,
                    ordinal: enum_descriptor(class).map_or(0, |d| d.members.len() as u32),
                });
            }
            definition
        })
        .collect();
    (bindings, definitions)
}

fn add_bounds(definition: &mut InputDefinition, attrs: &Attrs) {
    match attrs {
        Attrs::Real(attrs) => {
            if let Some(min) = attrs.min {
                let old = definition.min.as_ref().and_then(|v| v.as_real().ok());
                definition.min = Some(Value::Real(
                    old.map_or(min, |old| intersect_real(old, min, true)),
                ));
            }
            if let Some(max) = attrs.max {
                let old = definition.max.as_ref().and_then(|v| v.as_real().ok());
                definition.max = Some(Value::Real(
                    old.map_or(max, |old| intersect_real(old, max, false)),
                ));
            }
        }
        Attrs::Integer(attrs) => {
            let old_min = definition.min.as_ref().and_then(|v| v.as_integer().ok());
            let old_max = definition.max.as_ref().and_then(|v| v.as_integer().ok());
            definition.min = Some(Value::Integer(
                old_min.map_or(attrs.min(), |old| old.max(attrs.min())),
            ));
            definition.max = Some(Value::Integer(
                old_max.map_or(attrs.max(), |old| old.min(attrs.max())),
            ));
        }
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => {}
    }
}

fn intersect_real(old: f64, new: f64, lower: bool) -> f64 {
    // Unlike f64::min/max, never erase an unsatisfiable comparison with a NaN bound.
    // Total ordering makes equal-zero bound bits independent of target traversal order.
    if old.is_nan() || new.is_nan() {
        f64::NAN
    } else if old.total_cmp(&new).is_gt() == lower {
        old
    } else {
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oce_model::RealAttrs;

    #[test]
    fn intersected_bounds_cannot_discard_nan_or_accept_an_empty_domain() {
        for reverse in [false, true] {
            let mut definition = InputDefinition {
                path: "u".into(),
                value_type: ValueType::Real,
                min: None,
                max: None,
            };
            let mut bounds = [f64::NAN, 1.0];
            if reverse {
                bounds.reverse();
            }
            for min in bounds {
                add_bounds(
                    &mut definition,
                    &Attrs::Real(RealAttrs {
                        min: Some(min),
                        ..RealAttrs::default()
                    }),
                );
            }
            assert!(!definition.accepts_domain(&Value::Real(2.0)));
        }
        let definition = InputDefinition {
            path: "u".into(),
            value_type: ValueType::Real,
            min: Some(Value::Real(2.0)),
            max: Some(Value::Real(1.0)),
        };
        for value in [1.0, 1.5, 2.0, f64::NAN] {
            assert!(!definition.accepts_domain(&Value::Real(value)));
        }
    }
}
