//! The catalog walk: every authorable class, its connectors and its parameters, by the names
//! `oce-api` gives them (design note decision 4).
//!
//! A named class's connectors are its catalog ports. A width-driven or positional class has
//! ports only at a parameter table, so it is resolved at its probe ([`crate::widths::probe`]) and
//! the port names are folded back into connectors by the engine's naming contract: `u1`, `u2` is
//! the vector connector `u`, `y1_1` … `y2_3` the matrix connector `y`.

use oce_api::{CatalogPortNaming, ClassPort, catalog, class_metadata, class_ports};
use oce_vocab::{Side, parameter_base_name};

use super::GenerateError;
use crate::widths;

/// One connector as the catalog's names give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorWalk {
    /// The CDL connector name.
    pub name: String,
    /// Input or output.
    pub side: Side,
    /// 0 scalar, 1 vector, 2 matrix.
    pub dims: u8,
}

/// One authorable class as the catalog's names give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassWalk {
    /// The catalog class path.
    pub class_path: String,
    /// Connectors, inputs then outputs, in declaration order.
    pub connectors: Vec<ConnectorWalk>,
    /// Parameter base names in the catalog's order.
    pub parameters: Vec<String>,
}

/// Walk `oce_api::catalog()`: every class that is not an engine-reserved identity, in catalog
/// order.
pub fn catalog_walk() -> Result<Vec<ClassWalk>, GenerateError> {
    let metadata = class_metadata();
    let mut walk = Vec::new();
    for entry in catalog().iter().filter(|entry| !entry.reserved) {
        let class_path = entry.class_path;
        let Some(meta) = metadata.iter().find(|meta| meta.class_path == class_path) else {
            return Err(unreadable(class_path, "the class has no metadata"));
        };
        let connectors = if entry.naming == CatalogPortNaming::Named {
            let mut connectors = Vec::new();
            for (side, ports) in [(Side::Input, &entry.inputs), (Side::Output, &entry.outputs)] {
                for port in ports {
                    let Some(name) = port.name else {
                        return Err(unreadable(class_path, "a named class has an unnamed port"));
                    };
                    connectors.push(ConnectorWalk {
                        name: name.to_owned(),
                        side,
                        dims: 0,
                    });
                }
            }
            connectors
        } else {
            probed_connectors(class_path)?
        };
        walk.push(ClassWalk {
            class_path: class_path.to_owned(),
            connectors,
            parameters: meta
                .parameters
                .iter()
                .map(|parameter| parameter_base_name(parameter.name).to_owned())
                .collect(),
        });
    }
    Ok(walk)
}

fn unreadable(class_path: &str, detail: &str) -> GenerateError {
    GenerateError::UnreadablePorts {
        class: class_path.to_owned(),
        detail: detail.to_owned(),
    }
}

/// The connectors of a width-driven or positional class, from its ports at the probe table.
fn probed_connectors(class_path: &str) -> Result<Vec<ConnectorWalk>, GenerateError> {
    let Some(table) = widths::probe(class_path) else {
        return Err(GenerateError::MissingProbe {
            class: class_path.to_owned(),
        });
    };
    let ports = class_ports(class_path, &widths::borrowed(&table))
        .map_err(|error| unreadable(class_path, &error.to_string()))?;
    let mut connectors = fold_ports(class_path, Side::Input, &ports.inputs)?;
    connectors.extend(fold_ports(class_path, Side::Output, &ports.outputs)?);
    Ok(connectors)
}

/// A port name split into its connector name and one-based indices: `u` is `("u", [])`, `u3`
/// is `("u", [3])`, `y2_1` is `("y", [2, 1])`.
fn split_port(name: &str) -> (&str, Vec<usize>) {
    let digits =
        |text: &str| text.len() - text.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let last = digits(name);
    if last == 0 || last == name.len() {
        return (name, Vec::new());
    }
    let (head, column) = name.split_at(name.len() - last);
    let Ok(column) = column.parse() else {
        return (name, Vec::new());
    };
    if let Some(before) = head.strip_suffix('_') {
        let row_digits = digits(before);
        if row_digits > 0
            && row_digits < before.len()
            && let Ok(row) = before[before.len() - row_digits..].parse()
        {
            return (&before[..before.len() - row_digits], vec![row, column]);
        }
    }
    (head, vec![column])
}

/// Fold one side's resolved ports into connectors, refusing any list the naming contract does
/// not produce: a vector must be `1..=n` with `n >= 2` (the probe is two wide, so a lone `u1`
/// is a scalar the contract cannot tell from an element), a matrix a complete row-major grid.
pub fn fold_ports(
    class_path: &str,
    side: Side,
    ports: &[ClassPort],
) -> Result<Vec<ConnectorWalk>, GenerateError> {
    let mut groups: Vec<(&str, Vec<Vec<usize>>)> = Vec::new();
    for port in ports {
        let (base, indices) = split_port(&port.name);
        match groups.last_mut() {
            Some((last, elements)) if *last == base => elements.push(indices),
            _ => {
                if groups.iter().any(|(earlier, _)| *earlier == base) {
                    return Err(unreadable(
                        class_path,
                        &format!("the ports of `{base}` are not contiguous"),
                    ));
                }
                groups.push((base, vec![indices]));
            }
        }
    }
    let mut connectors = Vec::with_capacity(groups.len());
    for (base, elements) in groups {
        let rank = elements[0].len();
        let well_formed = match rank {
            0 => elements.len() == 1,
            1 => {
                elements.len() >= 2
                    && elements
                        .iter()
                        .enumerate()
                        .all(|(at, element)| *element == [at + 1])
            }
            _ => {
                let rows = elements
                    .iter()
                    .map(|e| e.first().copied().unwrap_or(0))
                    .max();
                let cols = elements
                    .iter()
                    .map(|e| e.get(1).copied().unwrap_or(0))
                    .max();
                let (rows, cols) = (rows.unwrap_or(0), cols.unwrap_or(0));
                elements.len() >= 2
                    && elements.len() == rows * cols
                    && elements
                        .iter()
                        .enumerate()
                        .all(|(at, element)| *element == [at / cols + 1, at % cols + 1])
            }
        };
        if base.is_empty() || !well_formed {
            return Err(unreadable(
                class_path,
                &format!("the ports of `{base}` do not follow the naming contract"),
            ));
        }
        connectors.push(ConnectorWalk {
            name: base.to_owned(),
            side,
            dims: rank as u8,
        });
    }
    Ok(connectors)
}
