//! Lowering a topology into the diff's own terms: block and boundary names with the root prefix
//! stripped, ports named by index, ground values as the data's [`Literal`], and the level-neutral
//! [`Side`] the `wiring` module matches.
//!
//! **Names.** A block or boundary path names its element when it is the root id
//! (`Topology::model_id`), a `.` and a non-empty name with no `#`, `/` or `:`; the name is what
//! follows. That is the modelica-json convention the emitter and the corpus follow, and the only
//! one this level reads: the root is never guessed from the paths, which in a one-block document
//! share that block's whole path. The delimiters are refused because the canonical content id
//! names an IRI by what follows its last `#`, `/` or `:`: with none in the name, the id names the
//! path `{sequence}.{name}`, the sequence being the root id after its own last delimiter, so
//! this level's name and the id's name say the same thing. A block path of the facade's unnamed
//! spelling (`b` and decimal digits) is an unnamed block, matched by the rename pass only. Any
//! other path is refused ([`DiffError::OutsideRoot`]).
//!
//! **Ports.** A block's input and output connector paths are positional: a boundary-fed input
//! carries the boundary's path, not a port name. So port `i` of a block is the `i`-th name the
//! facade's `class_ports` gives for its class and ground parameters, never a suffix of a path;
//! a block whose ports cannot be named that way is refused ([`DiffError::UnresolvedPorts`]).
//!
//! **Links.** A block input that carries a boundary input's path is driven by that boundary; a
//! connection links the output port and the input port whose paths it names; a declared output
//! is driven by the port its driver path names; a pass-through links its two boundaries. A path
//! that names no port, or several (no loaded document has one), is kept as spelled: an end that
//! is neither a declared boundary nor a block, which the matching never pairs.

use std::collections::BTreeMap;

use oce_api::{Topology, TopologyBlock, Value, ValueType, class_ports, enumeration_by_id};
use oce_author::MemberRef;

use crate::data::{Assignment, Literal, Side as Which};
use crate::error::DiffError;
use crate::wiring::{End, Node, Side};
use crate::work::{Counted, compared, visited};

/// One block, named.
#[derive(Clone, Debug)]
pub(super) struct NamedBlock<'t> {
    /// The path with the root prefix stripped, or the unnamed spelling as it is.
    pub(super) name: &'t str,
    /// Whether `name` is an identity (false for an unnamed block).
    pub(super) named: bool,
    /// The input port names, by index.
    pub(super) inputs: Vec<String>,
    /// The output port names, by index.
    pub(super) outputs: Vec<String>,
    /// The ground parameters as data with the authored literal each stands for, sorted by name,
    /// then value.
    pub(super) params: Vec<(Assignment, oce_author::Literal)>,
}

/// A topology with every name it needs.
#[derive(Clone, Debug)]
pub(super) struct Lowered<'t> {
    /// The topology.
    pub(super) topology: &'t Topology,
    /// Its blocks, in topology order.
    pub(super) blocks: Vec<NamedBlock<'t>>,
    /// The name of each boundary input, in topology order.
    pub(super) inputs: Vec<&'t str>,
    /// The name of each declared boundary output, in topology order.
    pub(super) outputs: Vec<&'t str>,
}

impl Lowered<'_> {
    /// Whether a block is unnamed.
    pub(super) fn has_unnamed(&self) -> bool {
        self.blocks.iter().any(|block| !block.named)
    }
}

/// The name a path has under `root`: what follows the root and a `.`, never empty. `Err(None)`
/// when the path has no such name, `Err(Some(delimiter))` when the name holds a `#`, `/` or `:`
/// (the first one), which the content id reads as the end of a namespace.
fn under<'t>(root: &str, path: &'t str) -> Result<&'t str, Option<char>> {
    let name = path
        .strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('.'))
        .filter(|name| !name.is_empty())
        .ok_or(None)?;
    match name.chars().find(|c| matches!(c, '#' | '/' | ':')) {
        Some(delimiter) => Err(Some(delimiter)),
        None => Ok(name),
    }
}

/// Whether a block path is the facade's spelling of a block with no instance IRI: `b` and one
/// or more decimal digits.
fn is_unnamed(path: &str) -> bool {
    path.strip_prefix('b')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Name every block and boundary of `topology`, the side being `which`.
pub(super) fn lower<'t>(topology: &'t Topology, which: Which) -> Result<Lowered<'t>, DiffError> {
    let root = topology.model_id.as_str();
    let outside = |path: &str, delimiter: Option<char>| DiffError::OutsideRoot {
        side: which,
        root: root.to_owned(),
        path: path.to_owned(),
        delimiter,
    };
    let boundary = |path: &'t str| under(root, path).map_err(|delimiter| outside(path, delimiter));
    let mut blocks = Vec::with_capacity(topology.blocks.len());
    for block in &topology.blocks {
        let path = block.instance_path.as_str();
        let (name, named) = match under(root, path) {
            Ok(name) => (name, true),
            Err(None) if is_unnamed(path) => (path, false),
            Err(delimiter) => return Err(outside(path, delimiter)),
        };
        let (inputs, outputs) = ports(block, which)?;
        visited(block.params.len());
        let mut params: Vec<(Assignment, oce_author::Literal)> = block
            .params
            .iter()
            .map(|(name, value)| {
                let authored = authored(value);
                let assignment = Assignment {
                    name: name.clone(),
                    value: crate::authoring::lower::literal(&authored),
                };
                (assignment, authored)
            })
            .collect();
        params.sort_by(|a, b| {
            compared();
            a.0.cmp(&b.0)
        });
        blocks.push(NamedBlock {
            name,
            named,
            inputs,
            outputs,
            params,
        });
    }
    let inputs = topology
        .boundary_inputs
        .iter()
        .map(|input| boundary(&input.path))
        .collect::<Result<_, _>>()?;
    let outputs = topology
        .boundary_outputs
        .iter()
        .map(|output| boundary(&output.path))
        .collect::<Result<_, _>>()?;
    Ok(Lowered {
        topology,
        blocks,
        inputs,
        outputs,
    })
}

/// The input and output port names of `block`, by index.
fn ports(block: &TopologyBlock, which: Which) -> Result<(Vec<String>, Vec<String>), DiffError> {
    visited(block.inputs.len() + block.outputs.len());
    let params: Vec<(&str, Value)> = block
        .params
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let names = |ports: Vec<oce_api::ClassPort>| -> Vec<String> {
        ports.into_iter().map(|port| port.name).collect()
    };
    match class_ports(&block.class_iri, &params) {
        Ok(ports)
            if ports.inputs.len() == block.inputs.len()
                && ports.outputs.len() == block.outputs.len() =>
        {
            Ok((names(ports.inputs), names(ports.outputs)))
        }
        _ => Err(DiffError::UnresolvedPorts {
            side: which,
            block: block.instance_path.clone(),
            class: block.class_iri.clone(),
        }),
    }
}

/// A ground value as the authoring model would write it: a Real, an Integer, a Boolean, a string,
/// or an enumeration value as its `Type.Member` member.
pub(super) fn authored(value: &Value) -> oce_author::Literal {
    match value {
        Value::Real(real) => oce_author::Literal::Real(*real),
        Value::Integer(integer) => oce_author::Literal::Integer(*integer),
        Value::Boolean(boolean) => oce_author::Literal::Boolean(*boolean),
        Value::String(text) => oce_author::Literal::Text(text.to_string()),
        Value::Enum { class, ordinal } => {
            let member = enumeration_by_id(*class).and_then(|row| {
                let index = usize::try_from(*ordinal).ok()?.checked_sub(1)?;
                Some(format!("{}.{}", row.type_name(), row.members.get(index)?))
            });
            match member {
                Some(spelling) => oce_author::Literal::Member(MemberRef::parse(&spelling)),
                // An enumeration value the table does not hold, kept distinct as its debug text.
                None => oce_author::Literal::Text(format!("{value:?}")),
            }
        }
    }
}

/// A ground value as the data's literal: a Real by its bits.
pub(super) fn literal(value: &Value) -> Literal {
    crate::authoring::lower::literal(&authored(value))
}

/// A boundary's type as the authoring form spells it: `Real`, `Integer`, `Boolean`, `String`, or
/// an enumeration by its type name (`SimpleController`).
pub(super) fn type_name(value_type: ValueType) -> String {
    match value_type {
        ValueType::Real => "Real".to_owned(),
        ValueType::Integer => "Integer".to_owned(),
        ValueType::Boolean => "Boolean".to_owned(),
        ValueType::String => "String".to_owned(),
        ValueType::Enum(class) => enumeration_by_id(class).map_or_else(
            || format!("{value_type:?}"),
            |row| row.type_name().to_owned(),
        ),
    }
}

/// Where each connector path is a port: `(block, index)` for every port that carries it.
type Ports<'t> = BTreeMap<Counted<&'t str>, Vec<(usize, usize)>>;

impl<'t> Lowered<'t> {
    /// The level-neutral side the matching runs on.
    pub(super) fn side(&self) -> Side<'_> {
        let topology = self.topology;
        let mut inputs_by_path: BTreeMap<Counted<&str>, &str> = BTreeMap::new();
        for (input, &name) in topology.boundary_inputs.iter().zip(&self.inputs) {
            inputs_by_path.insert(Counted(input.path.as_str()), name);
        }
        let mut outputs_by_path: BTreeMap<Counted<&str>, &str> = BTreeMap::new();
        for (output, &name) in topology.boundary_outputs.iter().zip(&self.outputs) {
            outputs_by_path.insert(Counted(output.path.as_str()), name);
        }
        let mut fed: Ports<'_> = BTreeMap::new();
        let mut driving: Ports<'_> = BTreeMap::new();
        let mut links = Vec::new();
        for (position, block) in topology.blocks.iter().enumerate() {
            for (index, path) in block.inputs.iter().enumerate() {
                match inputs_by_path.get(&Counted(path.as_str())) {
                    Some(&name) => {
                        if let Some(target) = self.port(position, false, index) {
                            links.push((End::Boundary(name), target));
                        }
                    }
                    None => fed
                        .entry(Counted(path.as_str()))
                        .or_default()
                        .push((position, index)),
                }
            }
            for (index, path) in block.outputs.iter().enumerate() {
                driving
                    .entry(Counted(path.as_str()))
                    .or_default()
                    .push((position, index));
            }
        }
        visited(topology.connections.len() + topology.boundary_outputs.len());
        for connection in &topology.connections {
            let source = self.end(&driving, true, &connection.from);
            let target = self.end(&fed, false, &connection.to);
            links.push((source, target));
        }
        for (output, &name) in topology.boundary_outputs.iter().zip(&self.outputs) {
            // A pass-through output is driven by its input, through the pairs below.
            if output.driver_path != output.path {
                let source = self.end(&driving, true, &output.driver_path);
                links.push((source, End::Boundary(name)));
            }
        }
        visited(topology.pass_through.len());
        for pair in &topology.pass_through {
            let boundary = |names: &BTreeMap<Counted<&str>, &'t str>, path: &'t str| {
                End::Boundary(names.get(&Counted(path)).copied().unwrap_or(path))
            };
            links.push((
                boundary(&inputs_by_path, &pair.input),
                boundary(&outputs_by_path, &pair.output),
            ));
        }
        Side {
            nodes: self
                .blocks
                .iter()
                .zip(&topology.blocks)
                .map(|(block, source)| Node {
                    name: block.name,
                    named: block.named,
                    class: &source.class_iri,
                    assignments: block
                        .params
                        .iter()
                        .map(|(assignment, _)| assignment.clone())
                        .collect(),
                })
                .collect(),
            boundaries: self.inputs.iter().chain(&self.outputs).copied().collect(),
            links,
        }
    }

    /// The end at port `index` of the block at `position`: an output port or an input port.
    fn port(&self, position: usize, output: bool, index: usize) -> Option<End<'_>> {
        let block = self.blocks.get(position)?;
        let ports = if output {
            &block.outputs
        } else {
            &block.inputs
        };
        Some(End::Port {
            block: block.name,
            port: ports.get(index)?,
        })
    }

    /// The end a connector path names: the one port that carries it, or, when no port or several
    /// do, the path as spelled.
    fn end<'s>(&'s self, ports: &Ports<'_>, output: bool, path: &'s str) -> End<'s> {
        match ports.get(&Counted(path)).map(Vec::as_slice) {
            Some(&[(position, index)]) => self
                .port(position, output, index)
                .unwrap_or(End::Boundary(path)),
            _ => End::Boundary(path),
        }
    }
}
