//! The engine's public block catalog, distinct from the vendored Buildings classification.

use crate::{ParamRule, PortKind, port_names, registry};
use oce_model::ParamTable;
use std::sync::OnceLock;

/// How a class's resolved scalar ports are identified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortNaming {
    /// Every resolved port has a stable declaration name.
    Named,
    /// Ports are fixed-position but cannot be represented by scalar CDL names.
    ///
    /// The three `Sources.TimeTable` classes use this form; their resolved arity follows the table
    /// parameter shape.
    Positional,
    /// Port arity or structure is driven by class parameters.
    WidthDriven,
}

/// One resolved port in declaration/index order.
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    /// Stable declaration name when [`PortNaming::Named`], otherwise `None`.
    pub name: Option<&'static str>,
    /// Runtime signal kind.
    pub kind: PortKind,
}

/// A const-friendly parameter default literal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultLiteral {
    /// A real literal.
    Real(f64),
    /// An integer literal.
    Integer(i64),
    /// A Boolean literal.
    Boolean(bool),
    /// A string literal.
    Str(&'static str),
    /// A CDL enumeration member token.
    EnumMember(&'static str),
}

/// The authored source of a parameter's default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultSource {
    /// A source literal.
    Literal(DefaultLiteral),
    /// A value computed from other resolved parameters.
    Derived {
        /// Exact documentation formula.
        formula: &'static str,
    },
    /// No authored default; callers must supply the parameter.
    Required,
}

/// One parameter-default declaration.
///
/// Width-indexed names use `<i>`, `<row>`, and `<col>` templates expanded according to the
/// corresponding width parameter or, for the TimeTable classes, the resolved table shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDefault {
    /// CDL parameter name or width-indexed name template.
    pub name: &'static str,
    /// Authored default source.
    pub default: DefaultSource,
}

/// Host-facing metadata for one registered block class.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// Canonical registered class path.
    pub class_path: &'static str,
    /// Resolved default-parameter inputs in declaration order.
    pub inputs: Vec<PortInfo>,
    /// Resolved default-parameter outputs in declaration order.
    pub outputs: Vec<PortInfo>,
    /// Port identification policy.
    pub naming: PortNaming,
    /// Static validation and structural rules.
    pub param_rules: &'static [ParamRule],
    /// Authored parameter defaults.
    pub param_defaults: &'static [ParamDefault],
    /// Whether any parameter rule drives resolved port structure.
    pub width_driven: bool,
    /// Conservative class-level state hint.
    ///
    /// [`crate::Block::kind`] on a resolved instance is authoritative. At default parameters,
    /// exactly `CDL.Reals.{Greater,GreaterThreshold,Less,LessThreshold}` diverge: their classes
    /// declare stateful behavior but `h = 0.0` resolves algebraically. `PID` and `PIDWithReset`
    /// default to `PI` and remain stateful; their `P` configuration is parameter-dependent.
    pub stateful: bool,
    /// Whether this is an engine-reserved lowering identity.
    ///
    /// Reserved `urn:oce:lowering#` identities are not authorable CDL and cannot be spelled in
    /// authored CXF. Palette-building hosts should filter on this flag.
    pub reserved: bool,
}

fn ports(kinds: &[PortKind], names: Option<&[&'static str]>) -> Vec<PortInfo> {
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| PortInfo {
            name: names.and_then(|values| values.get(index).copied()),
            kind: *kind,
        })
        .collect()
}

fn build_catalog() -> Vec<CatalogEntry> {
    registry::all_entries()
        .map(|entry| {
            let block = (entry.make)(&ParamTable::default());
            let signature = block.resolved_signature();
            let rules = entry.param_rules();
            let width_driven = rules.iter().any(|rule| {
                matches!(
                    rule,
                    ParamRule::Structural { .. } | ParamRule::StructuralArrayElements { .. }
                )
            });
            let names = port_names::port_names(entry.class_path);
            let naming = if names.is_some() {
                PortNaming::Named
            } else if width_driven {
                PortNaming::WidthDriven
            } else {
                PortNaming::Positional
            };
            CatalogEntry {
                class_path: entry.class_path,
                inputs: ports(&signature.inputs, names.map(|value| value.inputs)),
                outputs: ports(&signature.outputs, names.map(|value| value.outputs)),
                naming,
                param_rules: rules,
                param_defaults: registry::param_defaults(entry.class_path),
                width_driven,
                stateful: block.signature().stateful,
                reserved: entry.class_path.starts_with("urn:oce:lowering#"),
            }
        })
        .collect()
}

/// Enumerate the registered engine block classes in deterministic registry order.
///
/// The first call performs one default-parameter construction per class and allocates the owned
/// port metadata once behind a [`OnceLock`]. This metadata accessor is never called on the tick
/// path.
#[must_use]
pub fn catalog() -> &'static [CatalogEntry] {
    static CATALOG: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(build_catalog)
}
