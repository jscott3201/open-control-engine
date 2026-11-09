//! Resolved port names for every registered class at a bound parameter table (GitHub #440).
//!
//! # Port naming is an engine contract
//!
//! [`resolved_ports`] is the one place that answers "what is port `i` of this block called?" for
//! every registered class, including the 25 width-driven classes and the three `TimeTable`
//! sources, whose interfaces depend on parameters. An authoring tool must not re-derive these
//! names; it reads them here (or through `oce_api::class_ports`).
//!
//! The names follow three rules:
//!
//! - **Scalar connectors keep their CDL declaration name** (`u`, `index`, `y`, `u_s`). For the 108
//!   fixed-arity classes these come straight from [`crate::port_names`].
//! - **A vector connector `u[n]` flattens to `u1`, `u2`, …, `un`**: the base name, then the
//!   1-based element index with no separator. This is the spelling the conformance corpus and the
//!   CXF import already use for flattened vector connectors (`mulSum.u1`, `booRep.y4`), and the
//!   import binds those ports by position in exactly this order.
//! - **A matrix connector `y[r, c]` flattens row-major to `y1_1`, `y1_2`, …, `yr_c`**: the base,
//!   the 1-based row, `_`, the 1-based column, last index fastest — the order every matrix block
//!   in this crate lowers its ports in (`CDL.Reals.MatrixMax` inputs, `CDL.Routing.*Replicator`
//!   outputs).
//!
//! Parameter arrays use a different, older spelling (`k_1`, `K_1_2`); see
//! [`crate::ParamNameTemplate`]. The two are deliberately separate: port names are what the corpus
//! writes, parameter names are what the import mints.
//!
//! # How a name is resolved
//!
//! The class's registry constructor builds the block from the table, exactly as a load does, and
//! [`crate::Block::resolved_signature`] gives the authoritative arity and kinds. The class's static
//! layout (which connectors it declares, in declaration order, and which are vectors) is then laid
//! over that arity. Nothing here reads a width parameter to decide how many ports exist: the
//! constructor decides, and a layout that cannot account for every resolved port exactly is
//! refused with [`ResolvedPortsError::LayoutMismatch`] rather than guessed. So the names can never
//! disagree with the ports a loaded block actually has.
//!
//! The function does not validate the table against the class's parameter rules. Constructors are
//! total, so an out-of-range width (`nin = -1`, a product above
//! [`crate::MAX_RESOLVED_PORT_WIDTH`]) resolves to whatever the constructor builds, typically no
//! vector ports. The load gate refuses such a table; `oce_api::class_ports` runs that gate first.

use alloc::{format, string::String, vec::Vec};

use oce_model::{ParamTable, Value};

use crate::{DefaultLiteral, DefaultSource, PortKind, port_names, registry};

/// One resolved port: its contract name and its signal kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPort {
    /// Contract name: a scalar CDL name (`u_s`), a flattened vector element (`u3`) or a flattened
    /// matrix element (`y2_1`). Unique within one side of one class instance.
    pub name: String,
    /// Signal kind, identical to the resolved signature's kind at this index.
    pub kind: PortKind,
}

/// The resolved interface of one class at one parameter table, in signature index order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPorts {
    /// Canonical registered class path.
    pub class_path: &'static str,
    /// Inputs; `inputs[i]` is input port `i` of the constructed block.
    pub inputs: Vec<ResolvedPort>,
    /// Outputs; `outputs[i]` is output port `i` of the constructed block.
    pub outputs: Vec<ResolvedPort>,
}

/// Which side of a block interface a [`ResolvedPortsError::LayoutMismatch`] concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSide {
    /// Input ports.
    Input,
    /// Output ports.
    Output,
}

/// Why [`resolved_ports`] could not name a class's ports. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedPortsError {
    /// The class path is not registered.
    UnknownClass,
    /// The class is registered but declares no port layout. Unreachable for the registry as
    /// shipped (a test resolves every class); reported rather than guessed if a class is added
    /// without one.
    NoLayout,
    /// The constructed block's arity on `side` cannot be laid out by the class's declared
    /// connectors (for example an odd output count for `CDL.Reals.Sort`, whose `y[nin]` and
    /// `yIdx[nin]` must be equally wide).
    LayoutMismatch {
        /// The side whose resolved arity did not fit.
        side: PortSide,
        /// The resolved port count on that side.
        resolved: usize,
    },
}

impl core::fmt::Display for ResolvedPortsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownClass => f.write_str("the class path is not registered"),
            Self::NoLayout => f.write_str("the class declares no port layout"),
            Self::LayoutMismatch { side, resolved } => write!(
                f,
                "{resolved} resolved {} port(s) do not fit the class's declared connectors",
                match side {
                    PortSide::Input => "input",
                    PortSide::Output => "output",
                }
            ),
        }
    }
}

impl core::error::Error for ResolvedPortsError {}

/// One declared connector of a parameter-shaped class, in declaration order.
#[derive(Clone, Copy)]
enum Connector {
    /// A scalar connector with its CDL name.
    Scalar(&'static str),
    /// A vector connector `base[n]`; `n` follows from the resolved arity.
    Vector(&'static str),
    /// A matrix connector `base[rows, cols]`, flattened row-major.
    Matrix {
        base: &'static str,
        cols: MatrixCols,
    },
}

/// Where a matrix connector's column count comes from.
#[derive(Clone, Copy)]
enum MatrixCols {
    /// The width of the input-side vector (`CDL.Routing.*VectorReplicator`: `y[nout, nin]`).
    InputVectorWidth,
    /// A row/column reduction (`CDL.Reals.MatrixMax`/`MatrixMin`): when the Boolean `flag`
    /// parameter is true the output has one element per row, so `cols = inputs / outputs`;
    /// otherwise one per column, so `cols = outputs`. The flag's default is the catalog's literal.
    Reduction { flag: &'static str },
}

/// The declared connectors of one class without stable scalar names.
pub(crate) struct Layout {
    inputs: &'static [Connector],
    outputs: &'static [Connector],
}

const VECTOR_IN_SCALAR_OUT: Layout = Layout {
    inputs: &[Connector::Vector("u")],
    outputs: &[Connector::Scalar("y")],
};
const VECTOR_IN_VECTOR_OUT: Layout = Layout {
    inputs: &[Connector::Vector("u")],
    outputs: &[Connector::Vector("y")],
};
const EXTRACTOR: Layout = Layout {
    inputs: &[Connector::Scalar("index"), Connector::Vector("u")],
    outputs: &[Connector::Scalar("y")],
};
const SCALAR_REPLICATOR: Layout = Layout {
    inputs: &[Connector::Scalar("u")],
    outputs: &[Connector::Vector("y")],
};
const VECTOR_REPLICATOR: Layout = Layout {
    inputs: &[Connector::Vector("u")],
    outputs: &[Connector::Matrix {
        base: "y",
        cols: MatrixCols::InputVectorWidth,
    }],
};
const MATRIX_MAX: Layout = Layout {
    inputs: &[Connector::Matrix {
        base: "u",
        cols: MatrixCols::Reduction { flag: "rowMax" },
    }],
    outputs: &[Connector::Vector("y")],
};
const MATRIX_MIN: Layout = Layout {
    inputs: &[Connector::Matrix {
        base: "u",
        cols: MatrixCols::Reduction { flag: "rowMin" },
    }],
    outputs: &[Connector::Vector("y")],
};
const SORT: Layout = Layout {
    inputs: &[Connector::Vector("u")],
    outputs: &[Connector::Vector("y"), Connector::Vector("yIdx")],
};
const TIME_TABLE: Layout = Layout {
    inputs: &[],
    outputs: &[Connector::Vector("y")],
};

/// The connector layout of every class [`crate::port_names`] does not name: the 25 width-driven
/// classes and the three `TimeTable` sources, transcribed from their upstream CDL declarations.
pub(crate) fn layout(class_path: &str) -> Option<&'static Layout> {
    Some(match class_path {
        "CDL.Reals.MultiMax"
        | "CDL.Reals.MultiMin"
        | "CDL.Reals.MultiSum"
        | "CDL.Integers.MultiSum"
        | "CDL.Logical.MultiAnd"
        | "CDL.Logical.MultiOr" => &VECTOR_IN_SCALAR_OUT,
        "CDL.Reals.MatrixGain"
        | "CDL.Routing.BooleanExtractSignal"
        | "CDL.Routing.IntegerExtractSignal"
        | "CDL.Routing.RealExtractSignal"
        | "CDL.Routing.BooleanVectorFilter"
        | "CDL.Routing.IntegerVectorFilter"
        | "CDL.Routing.RealVectorFilter" => &VECTOR_IN_VECTOR_OUT,
        "CDL.Routing.BooleanExtractor"
        | "CDL.Routing.IntegerExtractor"
        | "CDL.Routing.RealExtractor" => &EXTRACTOR,
        "CDL.Routing.BooleanScalarReplicator"
        | "CDL.Routing.IntegerScalarReplicator"
        | "CDL.Routing.RealScalarReplicator" => &SCALAR_REPLICATOR,
        "CDL.Routing.BooleanVectorReplicator"
        | "CDL.Routing.IntegerVectorReplicator"
        | "CDL.Routing.RealVectorReplicator" => &VECTOR_REPLICATOR,
        "CDL.Reals.MatrixMax" => &MATRIX_MAX,
        "CDL.Reals.MatrixMin" => &MATRIX_MIN,
        "CDL.Reals.Sort" => &SORT,
        "CDL.Reals.Sources.TimeTable"
        | "CDL.Logical.Sources.TimeTable"
        | "CDL.Integers.Sources.TimeTable" => &TIME_TABLE,
        _ => return None,
    })
}

/// Resolve the named input and output ports of `class_path` constructed from `params`.
///
/// Builds the block with the class's registry constructor (the same call a load makes), reads
/// [`crate::Block::resolved_signature`], and names each resolved port: a scalar connector by its
/// CDL name, a vector `u[n]` as `u1`…`un`, a matrix `y[r, c]` as `y1_1`…`yr_c` row-major (the
/// engine's port-naming contract, `docs/cdl-coverage.md`). Pure: the same class and table always
/// give the same names, and nothing is cached. Allocates one `String` per port, so cost is linear
/// in the resolved arity, which every constructor bounds by [`crate::MAX_RESOLVED_ARITY`].
///
/// # Errors
///
/// [`ResolvedPortsError::UnknownClass`] for an unregistered path; [`ResolvedPortsError::NoLayout`]
/// for a registered class with neither declared names nor a layout;
/// [`ResolvedPortsError::LayoutMismatch`] when the constructed arity cannot be named exactly.
/// Never panics.
pub fn resolved_ports(
    class_path: &str,
    params: &ParamTable,
) -> Result<ResolvedPorts, ResolvedPortsError> {
    let entry = registry::lookup(class_path).ok_or(ResolvedPortsError::UnknownClass)?;
    let block = (entry.make)(params);
    let signature = block.resolved_signature();
    let declared = match port_names::port_names(entry.class_path) {
        Some(names) => Declared::Names(names.inputs, names.outputs),
        None => Declared::Layout(layout(entry.class_path)),
    };
    let (inputs, outputs) = name_signature(
        entry.class_path,
        declared,
        params,
        &signature.inputs,
        &signature.outputs,
    )?;
    Ok(ResolvedPorts {
        class_path: entry.class_path,
        inputs,
        outputs,
    })
}

/// What a class declares about its ports: fixed CDL names, or a connector layout (`None` when a
/// registered class has neither, which [`ResolvedPortsError::NoLayout`] reports).
#[derive(Clone, Copy)]
pub(crate) enum Declared {
    /// Fixed names, inputs then outputs.
    Names(&'static [&'static str], &'static [&'static str]),
    /// A connector layout for a parameter-shaped class.
    Layout(Option<&'static Layout>),
}

/// Name a resolved signature by what its class declares. The seam [`resolved_ports`] calls with
/// the constructed block's signature; tests call it with hand-built signatures to reach every
/// refusal.
pub(crate) fn name_signature(
    class_path: &'static str,
    declared: Declared,
    params: &ParamTable,
    input_kinds: &[PortKind],
    output_kinds: &[PortKind],
) -> Result<(Vec<ResolvedPort>, Vec<ResolvedPort>), ResolvedPortsError> {
    match declared {
        Declared::Names(inputs, outputs) => Ok((
            named_side(inputs, input_kinds, PortSide::Input)?,
            named_side(outputs, output_kinds, PortSide::Output)?,
        )),
        Declared::Layout(layout) => {
            let layout = layout.ok_or(ResolvedPortsError::NoLayout)?;
            let shape = SideShape {
                params,
                class_path,
                input_count: input_kinds.len(),
                output_count: output_kinds.len(),
            };
            let (input_names, input_vector_width) =
                shape.names(layout.inputs, PortSide::Input, None)?;
            let (output_names, _) =
                shape.names(layout.outputs, PortSide::Output, input_vector_width)?;
            Ok((
                zip_kinds(input_names, input_kinds),
                zip_kinds(output_names, output_kinds),
            ))
        }
    }
}

fn named_side(
    names: &[&'static str],
    kinds: &[PortKind],
    side: PortSide,
) -> Result<Vec<ResolvedPort>, ResolvedPortsError> {
    if names.len() != kinds.len() {
        return Err(ResolvedPortsError::LayoutMismatch {
            side,
            resolved: kinds.len(),
        });
    }
    Ok(names
        .iter()
        .zip(kinds)
        .map(|(name, kind)| ResolvedPort {
            name: String::from(*name),
            kind: *kind,
        })
        .collect())
}

fn zip_kinds(names: Vec<String>, kinds: &[PortKind]) -> Vec<ResolvedPort> {
    names
        .into_iter()
        .zip(kinds)
        .map(|(name, kind)| ResolvedPort { name, kind: *kind })
        .collect()
}

/// The facts a layout needs to size its vector and matrix connectors.
struct SideShape<'a> {
    params: &'a ParamTable,
    class_path: &'static str,
    input_count: usize,
    output_count: usize,
}

impl SideShape<'_> {
    /// Name one side. Returns the names and, when the side declares exactly one vector, its width
    /// (the column count of a replicator's output matrix).
    fn names(
        &self,
        connectors: &[Connector],
        side: PortSide,
        input_vector_width: Option<usize>,
    ) -> Result<(Vec<String>, Option<usize>), ResolvedPortsError> {
        let resolved = match side {
            PortSide::Input => self.input_count,
            PortSide::Output => self.output_count,
        };
        let mismatch = ResolvedPortsError::LayoutMismatch { side, resolved };
        let scalars = connectors
            .iter()
            .filter(|c| matches!(c, Connector::Scalar(_)))
            .count();
        let vectors = connectors
            .iter()
            .filter(|c| matches!(c, Connector::Vector(_)))
            .count();
        let matrices = connectors.len() - scalars - vectors;
        let elements = resolved.checked_sub(scalars).ok_or(mismatch.clone())?;
        // Every layout carries at most one matrix, and never beside a vector on the same side.
        let (vector_width, matrix_shape) = match (vectors, matrices) {
            (0, 0) if elements == 0 => (0, (0, 0)),
            (0, 0) => return Err(mismatch),
            (0, 1) => {
                let cols = self.matrix_cols(connectors, input_vector_width);
                (0, matrix_rows_cols(elements, cols).ok_or(mismatch.clone())?)
            }
            (v, 0) if elements.is_multiple_of(v) => (elements / v, (0, 0)),
            _ => return Err(mismatch),
        };
        let mut names = Vec::with_capacity(resolved);
        for connector in connectors {
            match *connector {
                Connector::Scalar(name) => names.push(String::from(name)),
                Connector::Vector(base) => {
                    names.extend((1..=vector_width).map(|i| format!("{base}{i}")));
                }
                Connector::Matrix { base, .. } => {
                    let (rows, cols) = matrix_shape;
                    for row in 1..=rows {
                        names.extend((1..=cols).map(|col| format!("{base}{row}_{col}")));
                    }
                }
            }
        }
        if names.len() != resolved {
            return Err(mismatch);
        }
        Ok((names, (vectors == 1).then_some(vector_width)))
    }

    /// The column count of this side's matrix connector, or `None` when it cannot be known.
    fn matrix_cols(
        &self,
        connectors: &[Connector],
        input_vector_width: Option<usize>,
    ) -> Option<usize> {
        let cols = connectors.iter().find_map(|c| match c {
            Connector::Matrix { cols, .. } => Some(*cols),
            _ => None,
        })?;
        match cols {
            MatrixCols::InputVectorWidth => input_vector_width,
            MatrixCols::Reduction { flag } => {
                if self.flag(flag) {
                    // One output per row: `rows = outputs`, `cols = inputs / rows`.
                    if self.output_count == 0 {
                        return (self.input_count == 0).then_some(0);
                    }
                    self.input_count
                        .is_multiple_of(self.output_count)
                        .then(|| self.input_count / self.output_count)
                } else {
                    Some(self.output_count)
                }
            }
        }
    }

    /// The Boolean `flag` parameter as the constructor reads it: a bound Boolean, else the
    /// catalog's literal default (`true` if the catalog carried none).
    fn flag(&self, flag: &str) -> bool {
        if let Some((_, Value::Boolean(value))) = self
            .params
            .values
            .iter()
            .find(|(name, _)| name.as_ref() == flag)
        {
            return *value;
        }
        registry::param_defaults(self.class_path)
            .iter()
            .find(|param| param.name == flag)
            .and_then(|param| match param.default {
                DefaultSource::Literal(DefaultLiteral::Boolean(value)) => Some(value),
                _ => None,
            })
            .unwrap_or(true)
    }
}

/// Split `elements` flattened matrix cells into `(rows, cols)` given the column count.
fn matrix_rows_cols(elements: usize, cols: Option<usize>) -> Option<(usize, usize)> {
    match cols? {
        0 => (elements == 0).then_some((0, 0)),
        cols => elements
            .is_multiple_of(cols)
            .then(|| (elements / cols, cols)),
    }
}

#[cfg(test)]
#[path = "resolved_ports_tests.rs"]
mod tests;
