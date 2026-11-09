//! Resolved port names and per-class authoring metadata, beside the revision-1 catalog
//! (GitHub #440; authoring-layer design note, decision 18).
//!
//! [`catalog()`](crate::catalog()) lists each class's ports at default parameters only, and names
//! them only for the 108 fixed-arity classes. An authoring tool needs the ports of a block as the
//! author configured it — a `MultiSum` with `nin = 3` has inputs `u1`, `u2`, `u3` — and needs to
//! know which parameters it may set. [`class_ports`] and [`class_metadata`] answer both without a
//! dependency on `oce-blocks`, and without changing the canonical catalog: [`crate::CATALOG_JSON`],
//! [`crate::catalog_content_id`] and every class signature hash are untouched.
//!
//! # Port naming is an engine contract
//!
//! Scalar connectors keep their CDL name; a vector connector `u[n]` is `u1` … `un`; a matrix
//! connector `y[r, c]` is `y1_1` … `yr_c`, row-major. The names are synthesized over the block
//! the registry constructor builds from the parameters, so they always match the ports a load of
//! the same block would have, in the same order. See `docs/cdl-coverage.md`.
//!
//! # Array parameters
//!
//! [`ClassParameter::name`] spells an array parameter once, as a template (`k_<i>`,
//! `K_<row>_<col>`). [`param_element_name`] expands a template to the flattened element names the
//! CXF import mints (`k_1`, `K_2_1`), so a host can list one value per element.

use std::sync::{Arc, OnceLock};

use oce_blocks::ParamNameTemplate;
use oce_diag::Diagnostic;
use oce_model::{ParamTable as BlockParams, Value};

use crate::CatalogPortKind;

pub use oce_blocks::{PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER};

/// One resolved port of a configured block: its contract name and signal kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassPort {
    /// Contract name (`u_s`, `u3`, `y2_1`), unique within its side.
    pub name: String,
    /// Signal kind.
    pub kind: CatalogPortKind,
}

/// The resolved interface of one class at one parameter set, in port index order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassPorts {
    /// Canonical registered class path.
    pub class_path: &'static str,
    /// Inputs; `inputs[i]` is input port `i` of a block loaded with the same parameters.
    pub inputs: Vec<ClassPort>,
    /// Outputs; `outputs[i]` is output port `i` of a block loaded with the same parameters.
    pub outputs: Vec<ClassPort>,
}

/// Why [`class_ports`] refused. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ClassPortsError {
    /// The class path is not a registered class.
    #[error("unknown block class `{class_path}`")]
    UnknownClass {
        /// The path as given.
        class_path: String,
    },
    /// The parameters violate the class's rules, exactly as a load would refuse them. Carries the
    /// load gate's error diagnostics (`missing-required-parameter`, `parameter-out-of-range`,
    /// `non-finite-parameter`, `duplicate-parameter`, …), subjects naming the block `block#0`.
    #[error("parameters of `{class_path}` are refused by its rules ({} diagnostic(s))", .diagnostics.len())]
    InvalidParameters {
        /// Canonical class path.
        class_path: &'static str,
        /// The error-severity diagnostics, in the load gate's deterministic order.
        diagnostics: Vec<Diagnostic>,
    },
    /// The configured block's ports could not be named. Unreachable for a table the rules
    /// accept (every registered class is tested); reported rather than guessed.
    #[error("ports of `{class_path}` cannot be named: {detail}")]
    Unresolvable {
        /// Canonical class path.
        class_path: &'static str,
        /// What did not fit.
        detail: String,
    },
}

/// Resolve the named ports of `class_path` configured with `params`.
///
/// `params` are ground parameter values by flattened name, as a CXF load would bind them: a
/// scalar by its CDL name (`nin`), an array element by its flattened name (`k_2`, see
/// [`param_element_name`]). Parameters left out take the class default. The table is first
/// checked against the class's rules by the same checks a load runs, so a table a load would
/// refuse is refused here instead of being named. Pure; allocates the result.
///
/// # Errors
///
/// [`ClassPortsError::UnknownClass`] for an unregistered path,
/// [`ClassPortsError::InvalidParameters`] for a table the class rules refuse, and
/// [`ClassPortsError::Unresolvable`] if the constructed block cannot be named. Never panics.
pub fn class_ports(
    class_path: &str,
    params: &[(&str, Value)],
) -> Result<ClassPorts, ClassPortsError> {
    let Some(entry) = oce_blocks::lookup(class_path) else {
        return Err(ClassPortsError::UnknownClass {
            class_path: class_path.to_owned(),
        });
    };
    let table = BlockParams {
        values: params
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    };
    if let Err(error) = oce_validate::validate_block_parameters(entry.class_path, &table) {
        return Err(ClassPortsError::InvalidParameters {
            class_path: entry.class_path,
            diagnostics: error.diagnostics,
        });
    }
    let ports = oce_blocks::resolved_ports(entry.class_path, &table).map_err(|error| {
        ClassPortsError::Unresolvable {
            class_path: entry.class_path,
            detail: error.to_string(),
        }
    })?;
    let project = |ports: Vec<oce_blocks::ResolvedPort>| -> Vec<ClassPort> {
        ports
            .into_iter()
            .map(|port| ClassPort {
                name: port.name,
                kind: match port.kind {
                    oce_blocks::PortKind::Real => CatalogPortKind::Real,
                    oce_blocks::PortKind::Integer => CatalogPortKind::Integer,
                    oce_blocks::PortKind::Boolean => CatalogPortKind::Boolean,
                },
            })
            .collect()
    };
    Ok(ClassPorts {
        class_path: ports.class_path,
        inputs: project(ports.inputs),
        outputs: project(ports.outputs),
    })
}

/// One parameter of a class, as an author sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassParameter {
    /// CDL name, or the array template (`k_<i>`, `K_<row>_<col>`) for an array parameter.
    pub name: &'static str,
    /// Whether an author may assign it: false for a parameter whose default is derived from
    /// other parameters and for every parameter of an engine-reserved class (GitHub #440).
    pub settable: bool,
}

/// Authoring metadata for one registered class. Not part of the revision-1 catalog JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMetadata {
    /// Canonical registered class path.
    pub class_path: &'static str,
    /// One sentence: the upstream Modelica class description string where a vendored source
    /// exists, otherwise an engine-written sentence. Never empty.
    pub description: &'static str,
    /// The class's authored parameter defaults in declaration order, then any parameter only a
    /// rule names (the `TimeTable` sources' `table_<row>_<col>`).
    pub parameters: Vec<ClassParameter>,
}

/// Authoring metadata for every registered class, in the same deterministic order as
/// [`catalog()`](crate::catalog()). The first call allocates once; never panics.
#[must_use]
pub fn class_metadata() -> &'static [ClassMetadata] {
    static METADATA: OnceLock<Vec<ClassMetadata>> = OnceLock::new();
    METADATA.get_or_init(|| {
        oce_blocks::catalog()
            .iter()
            .map(|entry| ClassMetadata {
                class_path: entry.class_path,
                description: oce_blocks::class_description(entry.class_path).unwrap_or_default(),
                parameters: oce_blocks::class_parameters(entry.class_path)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|parameter| ClassParameter {
                        name: parameter.name,
                        settable: parameter.settable,
                    })
                    .collect(),
            })
            .collect()
    })
}

/// The flattened parameter name of the element at 1-based `indices` of a catalog parameter
/// `template`: `("k_<i>", [2])` is `k_2`, `("K_<row>_<col>", [2, 1])` is `K_2_1`, and a scalar
/// name with no indices is itself. These are the names the CXF import mints for a preserved array
/// and the names [`class_ports`] reads.
///
/// Returns `None` when the number of indices is not the template's rank or an index is zero.
#[must_use]
pub fn param_element_name(template: &str, indices: &[usize]) -> Option<String> {
    ParamNameTemplate::parse(template).element_name(indices)
}
