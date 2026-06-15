#![forbid(unsafe_code)]
//! `oce-model` — the pure value, connector, instance, and connection types that are the
//! shared, executable truth of the Open Control Engine (FRAME decision D1).
//!
//! This crate is **Group A**: it has zero dependency on `selene-db` or any store
//! (D-OWNER-1), and it carries only the *computational* payload of a CDL model. The
//! non-computational metadata seam (CDL §7.17) lives in `oce-semantics`/`oce-store`,
//! never here on the hot path.
//!
//! Status: **M0 scaffold.** The public type *shapes* below match the spec
//! (`02-type-system-and-values.md` §2.3, `01-execution-model.md` §3); method bodies are
//! stubs (`unimplemented!()`) to be filled in M0/M1.

use std::sync::Arc;

/// Identifies an enumeration *class* (e.g. `CDL.Types.SimpleController`) in the flattened
/// model's enum registry. Resolved at flatten time; stable for the run.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EnumClassId(pub u32);

/// A scalar CDL value (CDL §7.4.1). The payload **only** — attributes live in [`Attrs`] and
/// never travel on the hot path (CDL §7.17).
///
/// Exactly five variants per `02-type-system-and-values.md` §2.3:
/// `Real` = IEEE-754 `f64`; `Integer` = `i64` (honors the required ±2_147_483_647 i32 range
/// as the logical domain); `Boolean` = `bool`; `String` = `Arc<str>` (metadata/identifiers
/// only — never a tick signal, §7.8); `Enum` = a 1-based ordinal plus its class.
#[derive(Clone, Debug)]
pub enum Value {
    /// Real — IEEE-754 `f64` (§7.4.1.1).
    Real(f64),
    /// Integer — stored `i64`, ≥ 32-bit range required (§7.4.1.2).
    Integer(i64),
    /// Boolean (§7.4.1.3).
    Boolean(bool),
    /// String — metadata/identifiers only; never a signal (§7.4.1.4, §7.8).
    String(Arc<str>),
    /// Enumeration — 1-based ordinal + the class it belongs to (§7.4.1.5).
    Enum {
        /// The enumeration class this ordinal belongs to.
        class: EnumClassId,
        /// 1-based ordinal (literal 1 → ordinal 1).
        ordinal: u32,
    },
}

/// The structural discriminant of a [`Value`] (CDL §7.4.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ValueType {
    /// Real (`f64`).
    Real,
    /// Integer (`i64`).
    Integer,
    /// Boolean (`bool`).
    Boolean,
    /// String (metadata only).
    String,
    /// Enumeration of a specific class.
    Enum(EnumClassId),
}

/// Typed-accessor failure for a [`Value`] (never panics on the hot path).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TypeError {
    /// The value type that was expected.
    pub expected: ValueType,
    /// The value type that was found.
    pub found: ValueType,
}

impl Value {
    /// Structural discriminant of this value (attributes are ignored).
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Real(_) => ValueType::Real,
            Value::Integer(_) => ValueType::Integer,
            Value::Boolean(_) => ValueType::Boolean,
            Value::String(_) => ValueType::String,
            Value::Enum { class, .. } => ValueType::Enum(*class),
        }
    }

    /// Total `Real` accessor; returns a typed error (never panics) on mismatch (R3).
    pub fn as_real(&self) -> Result<f64, TypeError> {
        match self {
            Value::Real(v) => Ok(*v),
            other => Err(TypeError {
                expected: ValueType::Real,
                found: other.value_type(),
            }),
        }
    }

    /// Total `Integer` accessor; returns a typed error (never panics) on mismatch (R3).
    pub fn as_integer(&self) -> Result<i64, TypeError> {
        match self {
            Value::Integer(v) => Ok(*v),
            other => Err(TypeError {
                expected: ValueType::Integer,
                found: other.value_type(),
            }),
        }
    }

    /// Total `Boolean` accessor; returns a typed error (never panics) on mismatch (R3).
    pub fn as_boolean(&self) -> Result<bool, TypeError> {
        match self {
            Value::Boolean(v) => Ok(*v),
            other => Err(TypeError {
                expected: ValueType::Boolean,
                found: other.value_type(),
            }),
        }
    }

    /// Bit-exact equality for the deterministic trace path (R2). `Real` compares by bits
    /// (so `NaN == NaN` here, unlike `PartialEq`).
    #[must_use]
    pub fn bit_eq(&self, _other: &Value) -> bool {
        // M0 scaffold: full per-variant bit comparison lands with the conformance harness.
        unimplemented!("Value::bit_eq — M0 scaffold")
    }
}

/// Direction of a connector (String has no connector type per §7.8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    /// Input connector (in-degree exactly 1 under single assignment, §7.10).
    In,
    /// Output connector.
    Out,
}

/// The attribute set carried alongside a declared variable (CDL §7.4.1). Metadata only —
/// `step()` must never branch on it (R1). Full per-type structs land in M1; the M0 scaffold
/// keeps the tag enum so signatures elsewhere can name it.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Attrs {
    /// Physical quantity (Real only), e.g. `"ThermodynamicTemperature"`; `None` = default `""`.
    pub quantity: Option<Arc<str>>,
    /// Computation unit (Real only); `None` = default `""`. UI-only `displayUnit` is separate.
    pub unit: Option<Arc<str>>,
}

/// Stable, dense, 0-based index of a block instance within the flattened model's arena.
/// **Not** a selene `NodeId` — purely an in-memory `oce-model` position.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BlockId(pub u32);

/// Stable, dense, 0-based index of a connector instance (one per block, connector,
/// array element). Purely in-memory (not a selene id).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ConnectorId(pub u32);

/// A resolved connector instance. Arrays are pre-flattened to one [`ConnectorId`] per element
/// (row-major), so the engine only ever sees scalars on the hot path (`01` §3).
#[derive(Clone, Debug)]
pub struct Connector {
    /// This connector's id.
    pub id: ConnectorId,
    /// The owning block instance.
    pub block: BlockId,
    /// Input or output.
    pub dir: Dir,
    /// Signal value type (Real | Integer | Boolean | Enum; String is metadata-only).
    pub value_type: ValueType,
    /// Position in the source declaration — the tie-break key for the deterministic sort (D6).
    pub decl_order: u32,
}

/// A resolved, ground parameter/constant table for one block instance (D5: parameters are
/// typed properties on the block). M0 scaffold holds name → ground [`Value`] pairs.
#[derive(Clone, Debug, Default)]
pub struct ParamTable {
    /// Ground (fully evaluated) parameter values, keyed by parameter name.
    pub values: Vec<(Arc<str>, Value)>,
}

/// A resolved, monomorphic block instance (elementary only on the tick — composites are
/// fully flattened away before BUILD, §7.2/§7.15).
#[derive(Clone, Debug)]
pub struct BlockInstance {
    /// This instance's id.
    pub id: BlockId,
    /// Class IRI — the join key to the native block impl in `oce-blocks`.
    pub class_iri: Arc<str>,
    /// Input connectors in declared order.
    pub inputs: Vec<ConnectorId>,
    /// Output connectors in declared order.
    pub outputs: Vec<ConnectorId>,
    /// Ground (or symbolic-resolved) parameter values.
    pub params: ParamTable,
    /// Declaration order of the instance — the tie-break key for the deterministic sort (D6).
    pub decl_order: u32,
}

/// A single-assignment connection: exactly one output drives one input element (§7.10).
/// Equivalent to `ModelGraph::Edge` (an output→input dataflow edge).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Connection {
    /// The driving `Out` connector.
    pub from: ConnectorId,
    /// The driven `In` connector (in-degree exactly 1, validated upstream).
    pub to: ConnectorId,
}

/// The flattened, monomorphic model the engine schedules and ticks (D1's executable truth).
///
/// This is the canonical in-memory artifact named `ModelGraph` by `oce-cxf` and the store
/// projection; the short alias [`Model`] names the same type for the scheduler-facing subset.
#[derive(Clone, Debug, Default)]
pub struct ModelGraph {
    /// Block instances, indexed by `BlockId.0`.
    pub blocks: Vec<BlockInstance>,
    /// Connector instances, indexed by `ConnectorId.0`.
    pub connectors: Vec<Connector>,
    /// Output→input dataflow edges.
    pub connections: Vec<Connection>,
}

/// Short alias for the scheduler-facing view of [`ModelGraph`] (`01` §3). Same in-memory type.
pub type Model = ModelGraph;

impl ModelGraph {
    /// Construct an empty model graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
