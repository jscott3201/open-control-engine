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

/// The enumeration whose [`EnumClassId`] is `id` — the class a `ValueType::Enum(id)` or
/// `Value::Enum { class: id, .. }` names. `None` for an id the engine does not know.
#[must_use]
pub fn enumeration_by_id(id: EnumClassId) -> Option<&'static EnumerationClass> {
    oce_model::enumeration_class_by_id(id)
}

/// The class path (relative to `Buildings.Controls.OBC.`) of the enumeration `id` names, for
/// labelling an Enum port or trace column. `None` for an id the engine does not know.
#[must_use]
pub fn enum_class_path(id: EnumClassId) -> Option<&'static str> {
    oce_model::enum_class_path(id)
}
