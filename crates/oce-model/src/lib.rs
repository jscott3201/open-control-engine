#![forbid(unsafe_code)]
//! `oce-model` — the pure value, connector, instance, and connection types that are the
//! shared, executable truth of the Open Control Engine (FRAME decision D1).
//!
//! This crate is **Group A**: it has zero dependency on any store or database
//! (D-OWNER-1), and it carries only the *computational* payload of a CDL model. The
//! non-computational metadata seam (CDL §7.17) lives in `oce-semantics`/`oce-store`,
//! never here on the hot path.
//!
//!
//! The public type shapes and helper bodies below are implemented and tested; they back the
//! non-panicking facade and scheduler contracts.

use std::sync::Arc;

pub mod determinism;
pub mod types;

/// Identifies an enumeration *class* (e.g. `CDL.Types.SimpleController`) in the flattened
/// model's enum registry. Resolved at flatten time; stable for the run.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EnumClassId(pub u32);

impl EnumClassId {
    /// Stable class id for `CDL.Types.SimpleController`.
    pub const SIMPLE_CONTROLLER: Self = Self(1);
    /// Stable class id for `CDL.Types.Smoothness`.
    pub const SMOOTHNESS: Self = Self(2);
    /// Stable class id for `CDL.Types.Extrapolation`.
    pub const EXTRAPOLATION: Self = Self(3);
    /// Stable class id for `CDL.Types.ZeroTime`.
    pub const ZERO_TIME: Self = Self(4);
}

pub use types::{
    SimpleController, ZeroTime, enum_class_id, enum_member_ordinal, g36_enum_class_id,
    g36_enum_literals, g36_integer_constant, g36_integer_constant_in_package,
    is_g36_integer_constant_package, is_g36_type_path,
};

/// A scalar CDL value (CDL §7.4.1). The payload **only** — attributes live in [`Attrs`] and
/// never travel on the hot path (CDL §7.17).
///
/// Exactly five variants per `02-type-system-and-values.md` §2.3:
/// `Real` = IEEE-754 `f64`; `Integer` = `i64` (honors the required ±2_147_483_647 i32 range
/// as the logical domain); `Boolean` = `bool`; `String` = `Arc<str>` (metadata/identifiers
/// only — never a tick signal, §7.8); `Enum` = a 1-based ordinal plus its class.
///
/// This is a closed domain fixed by the CDL native data types. It is deliberately exhaustive so
/// every handler must cover every signal kind at compile time; adding a variant is a major-version
/// change the compiler should force through all match sites. Do not silence that with a wildcard
/// arm.
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
///
/// This is a closed domain fixed by the CDL native data types. It is deliberately exhaustive so
/// every type handler must cover every signal kind at compile time; adding a variant is a
/// major-version change the compiler should force through all match sites. Do not silence that with
/// a wildcard arm.
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

impl ValueType {
    /// The type-appropriate zero used to seed connector values before the first tick
    /// (`01-execution-model.md` §7 req 3): `Real(0.0)`, `Integer(0)`, `Boolean(false)`. `String`
    /// seeds the empty string and `Enum` the first (1-based) ordinal — neither is a hot-path
    /// signal (§7.8), but a total mapping keeps callers panic-free.
    #[must_use]
    pub fn zero_value(self) -> Value {
        match self {
            ValueType::Real => Value::Real(0.0),
            ValueType::Integer => Value::Integer(0),
            ValueType::Boolean => Value::Boolean(false),
            ValueType::String => Value::String(Arc::from("")),
            ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
        }
    }
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

    /// Bit-exact equality for the deterministic trace path (R2). `Real` compares **by bits**
    /// (so `NaN == NaN` and `+0.0 != -0.0` here, unlike `PartialEq`); other variants compare
    /// structurally. Differing variants are never equal.
    #[must_use]
    pub fn bit_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => a.to_bits() == b.to_bits(),
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (
                Value::Enum {
                    class: ca,
                    ordinal: oa,
                },
                Value::Enum {
                    class: cb,
                    ordinal: ob,
                },
            ) => ca == cb && oa == ob,
            _ => false,
        }
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

/// Attributes for a `Real` variable (CDL §7.4.1.1 / §3.3). Each `Option` distinguishes
/// **unset/default** from an explicit value — §7.10 attribute unification needs to tell a default
/// from a declared non-default. Metadata only: `step()` never branches on it (R1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RealAttrs {
    /// Physical quantity (e.g. `"ThermodynamicTemperature"`); `None` ⇒ default `""`.
    pub quantity: Option<Arc<str>>,
    /// The *computation* unit; `None` ⇒ default `""`.
    pub unit: Option<Arc<str>>,
    /// UI-only display unit; `None` ⇒ default `""`. Never on the tick.
    pub display_unit: Option<Arc<str>>,
    /// Lower validation bound; `None` ⇒ `-∞`.
    pub min: Option<f64>,
    /// Upper validation bound; `None` ⇒ `+∞`.
    pub max: Option<f64>,
    /// Tolerance-scaling nominal; `None` ⇒ `1.0`.
    pub nominal: Option<f64>,
    /// Whether the variable is unbounded; `None` ⇒ `false`.
    pub unbounded: Option<bool>,
}

impl RealAttrs {
    /// The computation unit, or `""` if unset.
    #[must_use]
    pub fn unit(&self) -> &str {
        self.unit.as_deref().unwrap_or("")
    }
    /// The physical quantity, or `""` if unset.
    #[must_use]
    pub fn quantity(&self) -> &str {
        self.quantity.as_deref().unwrap_or("")
    }
    /// The display unit, or `""` if unset.
    #[must_use]
    pub fn display_unit(&self) -> &str {
        self.display_unit.as_deref().unwrap_or("")
    }
    /// The lower validation bound, or `-∞` if unset.
    #[must_use]
    pub fn min(&self) -> f64 {
        self.min.unwrap_or(f64::NEG_INFINITY)
    }
    /// The upper validation bound, or `+∞` if unset.
    #[must_use]
    pub fn max(&self) -> f64 {
        self.max.unwrap_or(f64::INFINITY)
    }
    /// The nominal value, or `1.0` if unset.
    #[must_use]
    pub fn nominal(&self) -> f64 {
        self.nominal.unwrap_or(1.0)
    }
    /// Whether the variable is unbounded, or `false` if unset.
    #[must_use]
    pub fn unbounded(&self) -> bool {
        self.unbounded.unwrap_or(false)
    }
}

/// Attributes for an `Integer` variable (CDL §7.4.1.2 / §3.3). No unit/quantity (§3.1 matrix).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntAttrs {
    /// Lower validation bound; `None` ⇒ the CDL Integer minimum (−2_147_483_648).
    pub min: Option<i64>,
    /// Upper validation bound; `None` ⇒ the CDL Integer maximum (+2_147_483_647).
    pub max: Option<i64>,
}

impl IntAttrs {
    /// The lower validation bound, or the CDL Integer minimum if unset.
    #[must_use]
    pub fn min(&self) -> i64 {
        self.min.unwrap_or(-2_147_483_648)
    }
    /// The upper validation bound, or the CDL Integer maximum if unset.
    #[must_use]
    pub fn max(&self) -> i64 {
        self.max.unwrap_or(2_147_483_647)
    }
}

/// `Boolean`, `String`, and `Enumeration` carry **no** attributes (CDL §7.4.1.3–.5). The unit
/// struct makes the type system forbid attaching any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoAttrs;

/// The attribute set carried alongside a connector, tagged to **match** the connector's
/// [`ValueType`] (R5). Metadata only — `step()` never branches on it (CDL §7.17 / R1). Build the
/// empty default for a type with [`Attrs::default_for`].
#[derive(Clone, Debug, PartialEq)]
pub enum Attrs {
    /// Attributes for a `Real` variable.
    Real(RealAttrs),
    /// Attributes for an `Integer` variable.
    Integer(IntAttrs),
    /// `Boolean` — no attributes.
    Boolean(NoAttrs),
    /// `String` — no attributes.
    String(NoAttrs),
    /// `Enumeration` — no attributes.
    Enum(NoAttrs),
}

impl Attrs {
    /// The empty/default attribute set for `value_type` (R5: the returned variant matches the
    /// type, so a connector built with it always satisfies the tag invariant).
    #[must_use]
    pub fn default_for(value_type: ValueType) -> Self {
        match value_type {
            ValueType::Real => Attrs::Real(RealAttrs::default()),
            ValueType::Integer => Attrs::Integer(IntAttrs::default()),
            ValueType::Boolean => Attrs::Boolean(NoAttrs),
            ValueType::String => Attrs::String(NoAttrs),
            ValueType::Enum(_) => Attrs::Enum(NoAttrs),
        }
    }

    /// The `RealAttrs` if this is a `Real` attribute set (for §7.10 unit/quantity unification).
    #[must_use]
    pub fn as_real(&self) -> Option<&RealAttrs> {
        match self {
            Attrs::Real(a) => Some(a),
            _ => None,
        }
    }

    /// The `IntAttrs` if this is an `Integer` attribute set.
    #[must_use]
    pub fn as_integer(&self) -> Option<&IntAttrs> {
        match self {
            Attrs::Integer(a) => Some(a),
            _ => None,
        }
    }

    /// Whether this attribute set's variant matches `value_type` (the R5 tag invariant).
    #[must_use]
    pub fn matches(&self, value_type: ValueType) -> bool {
        matches!(
            (self, value_type),
            (Attrs::Real(_), ValueType::Real)
                | (Attrs::Integer(_), ValueType::Integer)
                | (Attrs::Boolean(_), ValueType::Boolean)
                | (Attrs::String(_), ValueType::String)
                | (Attrs::Enum(_), ValueType::Enum(_))
        )
    }
}

/// Stable, dense, 0-based index of a block instance within the flattened model's arena.
/// **Not** a store/DB node id — purely an in-memory `oce-model` position.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BlockId(pub u32);

/// Stable, dense, 0-based index of a connector instance (one per block, connector,
/// array element). Purely in-memory (not a store/DB id).
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
    /// Per-type attribute set (metadata only; never on the tick). Its variant **matches**
    /// `value_type` (R5); `oce-validate` reads it for §7.10 unit/quantity unification.
    pub attrs: Attrs,
    /// Position in the source declaration — the tie-break key for the deterministic sort (D6).
    pub decl_order: u32,
    /// Source IRI of this connector in the originating CXF document, if any — used for
    /// diagnostics and boundary-input identity. `None` for hand-built models.
    pub iri: Option<Arc<str>>,
    /// Whether an attached IRI belonged to the host-visible point surface before CXF import began
    /// retaining every authored connector IRI. Defaults true for hand-built connectors and is
    /// cleared only by the resolver's general authored-IRI assignment.
    pub iri_was_legacy_host_path: bool,
}

/// The R5 tag-invariant violation returned by [`Connector::with_attrs`] when an attribute set's
/// variant does not match the connector's [`ValueType`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttrTypeMismatch {
    /// The connector's declared value type — the attribute variant that *should* have been given.
    pub value_type: ValueType,
}

impl Connector {
    /// A connector with the **default** (empty) attributes for `value_type` and no source IRI —
    /// the common case for the resolver and hand-built models. Use [`Connector::with_attrs`] /
    /// [`Connector::with_iri`] to populate them.
    #[must_use]
    pub fn new(
        id: ConnectorId,
        block: BlockId,
        dir: Dir,
        value_type: ValueType,
        decl_order: u32,
    ) -> Self {
        Self {
            id,
            block,
            dir,
            value_type,
            attrs: Attrs::default_for(value_type),
            decl_order,
            iri: None,
            iri_was_legacy_host_path: true,
        }
    }

    /// Replace the attribute set (builder style), **checking** the R5 tag invariant: the `attrs`
    /// variant must match this connector's `value_type`. This makes a tag mismatch
    /// unconstructible through the builder path (doc 02 R5: "only checked constructors").
    ///
    /// # Errors
    /// Returns [`AttrTypeMismatch`] when `attrs`'s variant disagrees with `self.value_type`.
    pub fn with_attrs(mut self, attrs: Attrs) -> Result<Self, AttrTypeMismatch> {
        if attrs.matches(self.value_type) {
            self.attrs = attrs;
            Ok(self)
        } else {
            Err(AttrTypeMismatch {
                value_type: self.value_type,
            })
        }
    }

    /// Attach the source IRI (builder style).
    #[must_use]
    pub fn with_iri(mut self, iri: impl Into<Arc<str>>) -> Self {
        self.iri = Some(iri.into());
        self
    }
}

/// A resolved, ground parameter/constant table for one block instance (D5: parameters are typed
/// properties on the block). Stores name → ground [`Value`] pairs.
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
    /// Source IRI — the `@id` — of this *instance* in the originating CXF document, if any.
    /// Named to contrast with `class_iri` (its `@type`/class), so the two IRIs never get
    /// confused at call sites. `None` for hand-built models.
    pub instance_iri: Option<Arc<str>>,
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

/// An authored top-composite boundary output and the child output connector that drives it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BoundaryOutput {
    /// The boundary output's authored full subject IRI, preserved verbatim for CXF export.
    pub iri: Arc<str>,
    /// The surviving child output connector whose value is exposed at the boundary.
    pub source: ConnectorId,
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
    /// Input connectors driven from **outside** the model — top-composite boundary inputs elided by
    /// the CXF boundary-input rule. These legally have in-degree 0; `oce-validate` treats an
    /// in-degree-0 input that is **not** listed here as a single-assignment error. Empty for a
    /// fully-internal hand-built model.
    pub external_inputs: Vec<ConnectorId>,
    /// Authored top-composite boundary outputs driven by surviving child output connectors.
    ///
    /// Unlike a boundary input, whose IRI can be back-filled onto its surviving target connector,
    /// an output boundary node is elided without leaving a connector of its own. Its identity must
    /// therefore be carried alongside the source connector. Entries follow the boundary nodes'
    /// positions in the source document's `@graph`; this is deterministic for a document and
    /// independent of `hasOutput` and `isConnectedTo` array spelling, but intentionally not
    /// invariant under a whole-`@graph` permutation because node order remains load-bearing in the
    /// resolver. Boundary IRIs are unique in a valid document. A source connector may occur more
    /// than once when it intentionally drives multiple distinct boundary outputs.
    pub boundary_outputs: Vec<BoundaryOutput>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_eq_real_is_by_bits() {
        // NaN equals itself by bits; signed zeros are distinct by bits (unlike PartialEq).
        assert!(Value::Real(f64::NAN).bit_eq(&Value::Real(f64::NAN)));
        assert!(!Value::Real(0.0).bit_eq(&Value::Real(-0.0)));
        assert!(Value::Real(1.5).bit_eq(&Value::Real(1.5)));
        assert!(!Value::Real(1.5).bit_eq(&Value::Real(2.5)));
    }

    #[test]
    fn bit_eq_other_variants_and_cross_type() {
        assert!(Value::Integer(7).bit_eq(&Value::Integer(7)));
        assert!(!Value::Integer(7).bit_eq(&Value::Integer(8)));
        assert!(Value::Boolean(true).bit_eq(&Value::Boolean(true)));
        assert!(!Value::Boolean(true).bit_eq(&Value::Boolean(false)));
        assert!(Value::String(Arc::from("x")).bit_eq(&Value::String(Arc::from("x"))));
        assert!(!Value::String(Arc::from("x")).bit_eq(&Value::String(Arc::from("y"))));
        // Differing variants are never equal (no cross-type coercion).
        assert!(!Value::Real(1.0).bit_eq(&Value::Integer(1)));
        let c = EnumClassId(3);
        assert!(
            Value::Enum {
                class: c,
                ordinal: 2
            }
            .bit_eq(&Value::Enum {
                class: c,
                ordinal: 2
            })
        );
        assert!(
            !Value::Enum {
                class: c,
                ordinal: 2
            }
            .bit_eq(&Value::Enum {
                class: c,
                ordinal: 1
            })
        );
    }

    #[test]
    fn zero_value_matches_signal_subset() {
        assert!(ValueType::Real.zero_value().bit_eq(&Value::Real(0.0)));
        assert!(ValueType::Integer.zero_value().bit_eq(&Value::Integer(0)));
        assert!(
            ValueType::Boolean
                .zero_value()
                .bit_eq(&Value::Boolean(false))
        );
    }

    #[test]
    fn simple_controller_from_qualified_parses_known_members() {
        let prefix = "Buildings.Controls.OBC.CDL.Types.SimpleController";
        assert_eq!(
            SimpleController::from_qualified(&format!("{prefix}.P")),
            Some(SimpleController::P)
        );
        assert_eq!(
            SimpleController::from_qualified(&format!("{prefix}.PI")),
            Some(SimpleController::Pi)
        );
        assert_eq!(
            SimpleController::from_qualified(&format!("{prefix}.PD")),
            Some(SimpleController::Pd)
        );
        assert_eq!(
            SimpleController::from_qualified(&format!("{prefix}.PID")),
            Some(SimpleController::Pid)
        );
    }

    #[test]
    fn simple_controller_from_qualified_rejects_adversarial_inputs() {
        for raw in [
            "Buildings.Controls.OBC.CDL.Types.SimpleController.X",
            "",
            "PID",
            ".PID",
            "Buildings.Controls.OBC.CDL.Types.SimpleController.",
        ] {
            assert_eq!(
                SimpleController::from_qualified(raw),
                None,
                "{raw:?} must be rejected without panicking"
            );
        }
    }

    #[test]
    fn real_attrs_accessor_defaults() {
        let a = RealAttrs::default();
        assert_eq!(a.unit(), "");
        assert_eq!(a.quantity(), "");
        assert_eq!(a.display_unit(), "");
        // Exact float comparison via bit pattern (the crate convention; avoids float_cmp).
        assert_eq!(a.min().to_bits(), f64::NEG_INFINITY.to_bits());
        assert_eq!(a.max().to_bits(), f64::INFINITY.to_bits());
        assert_eq!(a.nominal().to_bits(), 1.0_f64.to_bits());

        let b = RealAttrs {
            unit: Some(Arc::from("K")),
            min: Some(0.0),
            ..RealAttrs::default()
        };
        assert_eq!(b.unit(), "K");
        assert_eq!(b.min().to_bits(), 0.0_f64.to_bits());
        assert_eq!(b.max().to_bits(), f64::INFINITY.to_bits()); // still default
    }

    #[test]
    fn int_attrs_accessor_defaults() {
        let a = IntAttrs::default();
        assert_eq!(a.min(), -2_147_483_648);
        assert_eq!(a.max(), 2_147_483_647);
        let b = IntAttrs {
            min: Some(1),
            max: Some(10),
        };
        assert_eq!((b.min(), b.max()), (1, 10));
    }

    #[test]
    fn attrs_default_for_matches_value_type() {
        for vt in [
            ValueType::Real,
            ValueType::Integer,
            ValueType::Boolean,
            ValueType::String,
            ValueType::Enum(EnumClassId(0)),
        ] {
            assert!(
                Attrs::default_for(vt).matches(vt),
                "default mismatch for {vt:?}"
            );
        }
        // Cross-type tag mismatch is rejected.
        assert!(!Attrs::default_for(ValueType::Real).matches(ValueType::Integer));
        assert!(Attrs::default_for(ValueType::Real).as_real().is_some());
        assert!(
            Attrs::default_for(ValueType::Integer)
                .as_integer()
                .is_some()
        );
        assert!(Attrs::default_for(ValueType::Boolean).as_real().is_none());
    }

    #[test]
    fn connector_new_defaults_and_builders() {
        let c = Connector::new(ConnectorId(3), BlockId(1), Dir::In, ValueType::Real, 3);
        assert_eq!(c.id, ConnectorId(3));
        assert_eq!(c.decl_order, 3);
        assert!(c.attrs.matches(ValueType::Real));
        assert_eq!(c.iri, None);

        let c2 = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Integer, 0)
            .with_attrs(Attrs::Integer(IntAttrs {
                min: Some(0),
                max: Some(5),
            }))
            .expect("matching Integer attrs must be accepted")
            .with_iri("http://example.org#Seq.b.y");
        assert_eq!(c2.attrs.as_integer().and_then(|a| a.max), Some(5));
        assert_eq!(c2.iri.as_deref(), Some("http://example.org#Seq.b.y"));
    }

    #[test]
    fn connector_with_attrs_rejects_tag_mismatch() {
        // R5: attaching a mismatched-variant attr set through the builder path is rejected.
        let real = Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0);
        assert!(
            real.clone()
                .with_attrs(Attrs::Real(RealAttrs::default()))
                .is_ok()
        );
        let err = real
            .with_attrs(Attrs::Integer(IntAttrs::default()))
            .unwrap_err();
        assert_eq!(err.value_type, ValueType::Real);
    }

    #[test]
    fn model_graph_external_inputs_defaults_empty() {
        let m = ModelGraph::new();
        assert!(m.external_inputs.is_empty());
        assert!(m.blocks.is_empty() && m.connectors.is_empty() && m.connections.is_empty());
    }
}
