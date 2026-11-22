//! The enumeration and Integer constant-package table, beside the revision-1 catalog (issue #468;
//! authoring-layer design note, decision 18).
//!
//! [`enumerations()`] lists every named type the engine knows: the four CDL enumerations
//! (`CDL.Types.SimpleController`, `Smoothness`, `Extrapolation`, `ZeroTime`), the ten source-pinned
//! ASHRAE G36 enumerations and the four G36 Integer constant packages (`DemandLimitLevels`,
//! `FreezeProtectionStages`, `OperationModes`, `ZoneStates`), each with its members in source order.
//! It is the same `&'static` table every enum lookup in the engine reads, so a host and the engine
//! cannot disagree on a member's spelling or order.
//!
//! Every [`EnumerationClass::class_path`] is relative to `Buildings.Controls.OBC.`
//! (`CDL.Types.SimpleController`, `ASHRAE.G36.Types.OperationModes`), the same form the catalog
//! uses for block classes. A row is either an [`EnumerationKind::Enumeration`] — a valid parameter
//! and connector type whose values are one-based ordinals, carried in `Value::Enum` with the row's
//! [`EnumClassId`] — or an [`EnumerationKind::ConstantPackage`]: its members are named Integer
//! constants (`OperationModes.occupied` is `1`) and **a package is never a parameter type**; a
//! parameter or connector typed by one is an Integer.
//!
//! None of this enters the revision-1 catalog: [`crate::CATALOG_JSON`],
//! [`crate::catalog_content_id`] and every class signature hash are unchanged.

pub use oce_model::{EnumClassId, EnumerationClass, EnumerationKind, MemberValue};

/// What `member` of the row at `class_path` stands for: an enumeration's one-based ordinal or a
/// constant package's Integer. Exact names only; equal to
/// `enumeration(class_path)?.member_value(member)` ([`EnumerationClass::member_value`]).
#[must_use]
pub fn enumeration_member_value(class_path: &str, member: &str) -> Option<MemberValue> {
    oce_model::enumeration_member_value(class_path, member)
}

/// The enumeration rows only — the types a parameter or connector may have — in table order.
/// Equivalent to `enumerations().iter().filter(|r| r.kind == EnumerationKind::Enumeration)`; a
/// constant package is never a parameter type.
pub fn enumeration_types() -> impl Iterator<Item = &'static EnumerationClass> {
    enumerations()
        .iter()
        .filter(|row| row.kind == EnumerationKind::Enumeration)
}

/// Every enumeration and Integer constant package the engine knows, in a fixed order: the four CDL
/// enumerations, the ten G36 enumerations, then the four G36 constant packages. Pure; no
/// allocation.
#[must_use]
pub fn enumerations() -> &'static [EnumerationClass] {
    oce_model::enumeration_classes()
}

/// The row whose class path is exactly `class_path`, relative to `Buildings.Controls.OBC.`
/// (`CDL.Types.ZeroTime`). The full Modelica name, another case or a trailing dot is `None`.
#[must_use]
pub fn enumeration(class_path: &str) -> Option<&'static EnumerationClass> {
    oce_model::enumeration_class(class_path)
}

/// The row a document names by its **canonical class path**, exactly
/// `Buildings.Controls.OBC.<class path>` (`Buildings.Controls.OBC.CDL.Types.ZeroTime`). Any other
/// spelling is `None`: a bare or relative name, another package (`Modelica.Blocks.Types.…`, a CDL
/// enumeration under `…ASHRAE.G36.Types.`), another case or surrounding whitespace (issue #762).
/// It is the engine's one identity for enumeration text, so a host or the authoring layer that
/// resolves through it accepts exactly what the engine accepts.
#[must_use]
pub fn enumeration_by_path(path: &str) -> Option<&'static EnumerationClass> {
    oce_model::enumeration_class_by_path(path)
}

/// The row a typing IRI names: [`enumeration_by_path`] on its fragment after the last `#` or `/`
/// (`http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTime`,
/// `http://data.ashrae.org/S231#Buildings.Controls.OBC.CDL.Types.ZeroTime`). The namespace is not
/// judged; `…#ZeroTime` or `…#Foo.ZeroTime` is `None` (issues #637, #762).
#[must_use]
pub fn enumeration_by_iri(iri: &str) -> Option<&'static EnumerationClass> {
    oce_model::enumeration_class_by_iri(iri)
}

/// The enumeration whose [`EnumClassId`] is `id` — the class a `ValueType::Enum(id)` or
/// `Value::Enum { class: id, .. }` names. `None` for an id the engine does not know.
#[must_use]
pub fn enumeration_by_id(id: EnumClassId) -> Option<&'static EnumerationClass> {
    oce_model::enumeration_class_by_id(id)
}

/// The authoring class path (relative to `Buildings.Controls.OBC.`) of the enumeration `id` names:
/// `CDL.Types.SimpleController`, `ASHRAE.G36.Types.CoolingCoil`. `None` for an id the engine does
/// not know.
///
/// This is the form an authoring document or a CXF class IRI after `Buildings.Controls.OBC.`
/// spells. It is **not** the durable spelling the engine's own records use (state snapshots,
/// compiled-schedule blobs, the replay wire, class signature hashes), which is fully qualified for
/// a G36 enumeration; a trace that must agree with those records writes that durable form.
#[must_use]
pub fn enumeration_class_path(id: EnumClassId) -> Option<&'static str> {
    oce_model::enumeration_class_path(id)
}
