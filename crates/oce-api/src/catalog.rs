//! Independent host catalog metadata and its canonical content identity.

use std::sync::OnceLock;

use serde_json::{Value as Json, json};

use crate::CatalogRule;
use crate::catalog_adapter::RuleAdapter;
use crate::catalog_json::rule_json;
use crate::stable_hash::StableHash;

/// Signal kind in a default-parameter port or a TimeTable output column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogPortKind {
    /// IEEE-754 binary64 Real.
    Real,
    /// Signed Integer, carried in an i64.
    Integer,
    /// Boolean.
    Boolean,
}

impl CatalogPortKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Boolean => "Boolean",
        }
    }
}

/// Semantic type required by a class parameter, with durable enum identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogValueKind {
    /// Binary64 Real; CDL parameter validation may widen Integer literals.
    Real,
    /// Signed Integer.
    Integer,
    /// Boolean.
    Boolean,
    /// Metadata string.
    String,
    /// Enumeration; members are listed in one-based ordinal order.
    Enum {
        /// Canonical class path, independent of in-memory enum class indices.
        class_path: &'static str,
        /// Members in declaration order.
        members: &'static [&'static str],
    },
}

/// How the resolved ports of a class are addressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogPortNaming {
    /// Stable scalar CDL declaration names.
    Named,
    /// Positional ports whose arity may depend on a table shape.
    Positional,
    /// Parameters drive port width or structure.
    WidthDriven,
}

/// One default-parameter resolved port, in declaration/index order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPort {
    /// Declaration name for named ports; absent for positional/width-driven ports.
    pub name: Option<&'static str>,
    /// Signal kind.
    pub kind: CatalogPortKind,
}

/// Authored parameter-default payload. Real literals retain all binary64 bits.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogDefault {
    /// Real literal (including signed zero); canonical JSON records its bits.
    Real(f64),
    /// Signed Integer literal.
    Integer(i64),
    /// Boolean literal.
    Boolean(bool),
    /// Metadata string literal.
    String(&'static str),
    /// Qualified CDL enumeration member token.
    EnumMember(&'static str),
    /// Documentation formula evaluated by the owning block's parameter resolution.
    Derived(&'static str),
    /// No authored default; the parameter must be supplied.
    Required,
}

/// One parameter-default declaration, in registry declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogParamDefault {
    /// CDL name or template using `<i>`, `<row>`, or `<col>` for shape expansion.
    pub name: &'static str,
    /// Complete authored default payload.
    pub default: CatalogDefault,
}

/// Facade-owned class metadata, independent of a loaded engine or registry type.
///
/// Port counts describe default parameters; statefulness is a conservative class hint.
/// Resolved instance behavior remains authoritative. Units and quantities are not available
/// in this catalog. Strings are static source metadata; vectors may be cloned and owned freely.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEntry {
    /// Canonical registered class identity.
    pub class_path: &'static str,
    /// Default-parameter inputs, in declaration order.
    pub inputs: Vec<CatalogPort>,
    /// Default-parameter outputs, in declaration order.
    pub outputs: Vec<CatalogPort>,
    /// Port identification regime.
    pub naming: CatalogPortNaming,
    /// Complete parameter rules, in declaration order.
    pub param_rules: Vec<CatalogRule>,
    /// Complete parameter defaults, in declaration order.
    pub param_defaults: Vec<CatalogParamDefault>,
    /// Whether a rule drives resolved port structure.
    pub width_driven: bool,
    /// Conservative class-level statefulness hint, not resolved instance truth.
    pub stateful: bool,
    /// Engine-reserved lowering identity; not authorable CDL.
    pub reserved: bool,
}

/// Schema revision of the catalog DTO/JSON contract. Separate from state/ABI revisions.
pub const CATALOG_SCHEMA_REVISION: u32 = 1;

/// Packaged canonical catalog JSON. The live registry projection is checked against these bytes.
pub const CATALOG_JSON: &str = include_str!("../contracts/catalog.json");

/// Read the complete facade catalog in deterministic registry order.
///
/// The first call allocates metadata once. It performs no engine/store mutation and is never
/// called by tick. A registry constructor defect may panic during default metadata construction.
#[must_use]
pub fn catalog() -> &'static [CatalogEntry] {
    static CATALOG: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| oce_blocks::catalog().iter().map(project_entry).collect())
}

fn project_entry(entry: &oce_blocks::CatalogEntry) -> CatalogEntry {
    use oce_blocks::{DefaultLiteral as Literal, DefaultSource as Source, PortNaming};
    CatalogEntry {
        class_path: entry.class_path,
        inputs: entry.inputs.iter().map(project_port).collect(),
        outputs: entry.outputs.iter().map(project_port).collect(),
        naming: match entry.naming {
            PortNaming::Named => CatalogPortNaming::Named,
            PortNaming::Positional => CatalogPortNaming::Positional,
            PortNaming::WidthDriven => CatalogPortNaming::WidthDriven,
        },
        param_rules: entry
            .param_rules
            .iter()
            .map(|rule| rule.project::<RuleAdapter>())
            .collect(),
        param_defaults: entry
            .param_defaults
            .iter()
            .map(|param| CatalogParamDefault {
                name: param.name,
                default: match param.default {
                    Source::Literal(literal) => match literal {
                        Literal::Real(value) => CatalogDefault::Real(value),
                        Literal::Integer(value) => CatalogDefault::Integer(value),
                        Literal::Boolean(value) => CatalogDefault::Boolean(value),
                        Literal::Str(value) => CatalogDefault::String(value),
                        Literal::EnumMember(value) => CatalogDefault::EnumMember(value),
                    },
                    Source::Derived { formula } => CatalogDefault::Derived(formula),
                    Source::Required => CatalogDefault::Required,
                },
            })
            .collect(),
        width_driven: entry.width_driven,
        stateful: entry.stateful,
        reserved: entry.reserved,
    }
}

fn project_port(port: &oce_blocks::PortInfo) -> CatalogPort {
    use oce_blocks::PortKind;
    CatalogPort {
        name: port.name,
        kind: match port.kind {
            PortKind::Real => CatalogPortKind::Real,
            PortKind::Integer => CatalogPortKind::Integer,
            PortKind::Boolean => CatalogPortKind::Boolean,
        },
    }
}

/// Serialize any catalog projection canonically for schema revision 1.
///
/// UTF-8 compact JSON plus one LF; object keys are lexical, arrays preserve input order.
/// Every field is emitted, including absent names as null. Real payloads/bounds are 16 lowercase
/// hex digits of binary64 bits, preserving signed zero and nonfinite payloads. This is catalog
/// metadata serialization, not a general engine-value codec. Allocates; does not validate entries.
#[must_use]
pub fn catalog_to_json(entries: &[CatalogEntry]) -> String {
    let entries: Vec<_> = entries.iter().map(|entry| {
        let ports = |ports: &[CatalogPort]| -> Vec<Json> { ports.iter()
            .map(|port| json!({"name": port.name, "kind": port.kind.label()})).collect() };
        let defaults: Vec<_> = entry.param_defaults.iter().map(|param| {
            json!({"name": param.name, "default": default_json(&param.default)})
        }).collect();
        json!({
            "class_path": entry.class_path, "inputs": ports(&entry.inputs),
            "outputs": ports(&entry.outputs), "naming": match entry.naming {
                CatalogPortNaming::Named => "named", CatalogPortNaming::Positional => "positional",
                CatalogPortNaming::WidthDriven => "width_driven",
            },
            "param_rules": entry.param_rules.iter().map(rule_json).collect::<Vec<_>>(),
            "param_defaults": defaults, "width_driven": entry.width_driven,
            "stateful": entry.stateful, "reserved": entry.reserved,
        })
    }).collect();
    // Explicit recursion keeps lexical keys even if a downstream crate unifies serde_json's
    // preserve_order feature. Insertion order of our objects is not a contract dependency.
    let mut value = json!({"schema_revision": CATALOG_SCHEMA_REVISION, "entries": entries});
    sort_keys(&mut value);
    format!("{value}\n")
}

fn sort_keys(value: &mut Json) {
    match value {
        Json::Object(map) => {
            map.sort_keys();
            for value in map.values_mut() {
                sort_keys(value);
            }
        }
        Json::Array(values) => {
            for value in values {
                sort_keys(value);
            }
        }
        _ => {}
    }
}

fn default_json(value: &CatalogDefault) -> Json {
    match value {
        CatalogDefault::Real(value) => {
            json!({"kind": "real", "bits": format!("{:016x}", value.to_bits())})
        }
        CatalogDefault::Integer(value) => json!({"kind": "integer", "value": value}),
        CatalogDefault::Boolean(value) => json!({"kind": "boolean", "value": value}),
        CatalogDefault::String(value) => json!({"kind": "string", "value": value}),
        CatalogDefault::EnumMember(value) => json!({"kind": "enum", "value": value}),
        CatalogDefault::Derived(value) => json!({"kind": "derived", "formula": value}),
        CatalogDefault::Required => json!({"kind": "required"}),
    }
}

/// Compute a non-security content tag over every canonical catalog byte.
///
/// FNV-1a-128 hashes ASCII `oce:catalog:1\0` followed by [`catalog_to_json`] bytes; the result is
/// `catalog:1:fnv1a128:` followed by 32 lowercase hex digits. This identifies metadata only,
/// not a signature, executable/build compatibility, registry fingerprint, or state identity.
#[must_use]
pub fn catalog_content_id(entries: &[CatalogEntry]) -> String {
    let mut hash = StableHash::new();
    hash.write_bytes(b"oce:catalog:1\0");
    hash.write_bytes(catalog_to_json(entries).as_bytes());
    format!("catalog:1:fnv1a128:{:032x}", hash.finish())
}
