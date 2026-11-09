//! The one table of every enumeration and Integer constant package the engine knows (issue #468).
//!
//! [`enumeration_classes()`] returns plain `&'static` data: the four CDL enumerations, the ten
//! source-pinned ASHRAE G36 enumerations and the four G36 Integer constant packages, each with its
//! members in source order. Every enum helper in [`crate::types`] (`enum_class_id`,
//! `enum_descriptor`, `g36_enum_literals`, `is_g36_integer_constant_package`,
//! `g36_integer_constant_in_package`, …)
//! reads this table; there is no second copy of the facts.
//!
//! # One class-path form
//!
//! [`EnumerationClass::class_path`] is relative to the `Buildings.Controls.OBC.` package
//! ([`OBC_PACKAGE`]): `CDL.Types.SimpleController`, `ASHRAE.G36.Types.OperationModes`. It is the
//! form an authoring tool writes after a fixed class base, for CDL and G36 alike.
//! [`EnumerationClass::qualified_path`] is the full Modelica name. The durable identity that the
//! state snapshot, the compiled-schedule blob, the replay wire and the class signature hash record
//! is [`crate::EnumDescriptor::class_path`], frozen before this table existed and documented on
//! [`crate::enum_descriptor`]; it is not this field.
//!
//! # Enumerations and constant packages
//!
//! An [`EnumerationKind::Enumeration`] row is a valid parameter and connector type: it carries an
//! [`EnumClassId`] and its members are addressed by one-based ordinal. An
//! [`EnumerationKind::ConstantPackage`] row is **never a parameter type**: its members are named
//! Integer constants (`OperationModes.occupied` is `1`), grounded to `Value::Integer` at load, and
//! a connector typed by the package is an Integer connector.
//!
//! Every lookup is a linear scan over eighteen rows: no allocation, no hashing, `no_std`.

use crate::EnumClassId;

/// The package every [`EnumerationClass::class_path`] is relative to.
pub const OBC_PACKAGE: &str = "Buildings.Controls.OBC.";

/// Whether a table row is an enumeration type or a package of Integer constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnumerationKind {
    /// A Modelica `type … = enumeration(…)`: a valid parameter and connector type whose values are
    /// one-based ordinals of [`EnumerationClass::members`].
    Enumeration,
    /// A Modelica package of `constant Integer` declarations. Never a parameter type: each member
    /// is the Integer in [`EnumerationClass::values`] at the same index.
    ConstantPackage,
}

/// The value one member of a table row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemberValue {
    /// A member of an enumeration: its one-based source ordinal.
    Ordinal(u32),
    /// A member of a constant package: its declared Integer value.
    Integer(i64),
}

/// One enumeration or Integer constant package the engine knows.
///
/// Invariants (pinned by `crates/oce-model/tests/enumeration_table.rs`): `qualified_path` is
/// [`OBC_PACKAGE`] followed by `class_path`; `id` is `Some` exactly for an enumeration, and no two
/// rows share an id, a class path or a last segment ([`Self::type_name`]); `values` is empty for an
/// enumeration and as long as `members` for a constant package; member names are unique within a
/// row.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumerationClass {
    /// Class path relative to `Buildings.Controls.OBC.` (`CDL.Types.SimpleController`).
    pub class_path: &'static str,
    /// Full Modelica class path (`Buildings.Controls.OBC.CDL.Types.SimpleController`).
    pub qualified_path: &'static str,
    /// Enumeration type or Integer constant package.
    pub kind: EnumerationKind,
    /// The in-memory id `ValueType::Enum` carries; `None` for a constant package.
    pub id: Option<EnumClassId>,
    /// Member names in source order; an enumeration member's ordinal is its index plus one.
    pub members: &'static [&'static str],
    /// A constant package's member values, index-aligned with `members`; empty for an
    /// enumeration.
    pub values: &'static [i64],
}

impl EnumerationClass {
    /// The class's last segment (`SimpleController`, `OperationModes`): the type name an authored
    /// `Type.Member` reference spells. Unique across the table.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self.class_path.rsplit_once('.') {
            Some((_, last)) => last,
            None => self.class_path,
        }
    }

    /// What `member` stands for in this row, or `None` when the row has no member of exactly that
    /// name.
    #[must_use]
    pub fn member_value(&self, member: &str) -> Option<MemberValue> {
        let index = self.members.iter().position(|m| *m == member)?;
        match self.kind {
            EnumerationKind::Enumeration => u32::try_from(index + 1).ok().map(MemberValue::Ordinal),
            EnumerationKind::ConstantPackage => {
                self.values.get(index).copied().map(MemberValue::Integer)
            }
        }
    }
}

/// Every enumeration and Integer constant package the engine knows, in a fixed order: the four CDL
/// enumerations by id, the ten G36 enumerations by id, then the four G36 constant packages by name.
#[must_use]
pub fn enumeration_classes() -> &'static [EnumerationClass] {
    ENUMERATION_CLASSES
}

/// The row whose [`EnumerationClass::class_path`] is exactly `class_path` (relative to
/// `Buildings.Controls.OBC.`; the full name, a different case or a trailing dot do not match).
#[must_use]
pub fn enumeration_class(class_path: &str) -> Option<&'static EnumerationClass> {
    ENUMERATION_CLASSES
        .iter()
        .find(|row| row.class_path == class_path)
}

/// The enumeration row whose [`EnumerationClass::id`] is `id`.
#[must_use]
pub fn enumeration_class_by_id(id: EnumClassId) -> Option<&'static EnumerationClass> {
    ENUMERATION_CLASSES.iter().find(|row| row.id == Some(id))
}

/// The authoring class path (relative to `Buildings.Controls.OBC.`, [`EnumerationClass::class_path`])
/// of the enumeration `id` names: `CDL.Types.SimpleController`, `ASHRAE.G36.Types.CoolingCoil`.
///
/// This is **not** the durable spelling. A durable record (state snapshot, compiled-schedule blob,
/// replay wire or trace, class signature hash) writes [`crate::EnumDescriptor::class_path`] from
/// [`crate::enum_descriptor`], which is fully qualified for a G36 enumeration
/// (`Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil`). The two agree only for CDL.
#[must_use]
pub fn enumeration_class_path(id: EnumClassId) -> Option<&'static str> {
    enumeration_class_by_id(id).map(|row| row.class_path)
}

/// What `member` of the row at `class_path` stands for: an enumeration ordinal or a constant's
/// Integer value. Exact names only.
#[must_use]
pub fn enumeration_member_value(class_path: &str, member: &str) -> Option<MemberValue> {
    enumeration_class(class_path)?.member_value(member)
}

/// `Buildings.Controls.OBC.` + `relative`, checked at compile time; returns `relative`.
const fn relative_to_obc(qualified: &'static str) -> &'static str {
    let bytes = qualified.as_bytes();
    let prefix = OBC_PACKAGE.as_bytes();
    assert!(
        bytes.len() > prefix.len(),
        "class path shorter than the OBC package"
    );
    let mut i = 0;
    while i < prefix.len() {
        assert!(
            bytes[i] == prefix[i],
            "class path outside Buildings.Controls.OBC."
        );
        i += 1;
    }
    match core::str::from_utf8(bytes.split_at(prefix.len()).1) {
        Ok(relative) => relative,
        Err(_) => panic!("class path is not UTF-8 after the OBC package"),
    }
}

const fn enumeration(
    qualified_path: &'static str,
    id: EnumClassId,
    members: &'static [&'static str],
) -> EnumerationClass {
    EnumerationClass {
        class_path: relative_to_obc(qualified_path),
        qualified_path,
        kind: EnumerationKind::Enumeration,
        id: Some(id),
        members,
        values: &[],
    }
}

const fn constant_package(
    qualified_path: &'static str,
    members: &'static [&'static str],
    values: &'static [i64],
) -> EnumerationClass {
    assert!(members.len() == values.len(), "one value per constant");
    EnumerationClass {
        class_path: relative_to_obc(qualified_path),
        qualified_path,
        kind: EnumerationKind::ConstantPackage,
        id: None,
        members,
        values,
    }
}

/// The table: 18 rows. Provenance differs by group, and the tests that hold each group differ:
///
/// - The ten G36 enumerations are checked member by member, in order, against the vendored
///   `third_party/modelica-buildings-cdl` `.mo` classes.
/// - The four CDL enumerations are not vendored (`CDL/Types` is outside the vendored subset);
///   they are held to the registry's independent `EnumMembers` rule lists, the typed
///   `SimpleController` / `ZeroTime` parsers, and the match arms they replaced.
/// - The four constant packages are **not vendored**. Their members and values are exactly the
///   ones the resolver always grounded (the hand-written match arms this table replaced, pinned
///   by the differential test in `tests/enumeration_table.rs`); no upstream source checks them,
///   and the corpus-reference test only proves that the corpus names nothing outside them. The
///   member order of `DemandLimitLevels` (`cooling0..3`, then `heating0..3`) is a choice, not an
///   upstream order; for a constant package only the name-to-value map carries meaning.
static ENUMERATION_CLASSES: &[EnumerationClass] = &[
    enumeration(
        "Buildings.Controls.OBC.CDL.Types.SimpleController",
        EnumClassId::SIMPLE_CONTROLLER,
        &["P", "PI", "PD", "PID"],
    ),
    enumeration(
        "Buildings.Controls.OBC.CDL.Types.Smoothness",
        EnumClassId::SMOOTHNESS,
        &["LinearSegments", "ConstantSegments"],
    ),
    enumeration(
        "Buildings.Controls.OBC.CDL.Types.Extrapolation",
        EnumClassId::EXTRAPOLATION,
        &["HoldLastPoint", "LastTwoPoints", "Periodic"],
    ),
    enumeration(
        "Buildings.Controls.OBC.CDL.Types.ZeroTime",
        EnumClassId::ZERO_TIME,
        &[
            "UnixTimeStamp",
            "UnixTimeStampGMT",
            "Custom",
            "NY2010",
            "NY2011",
            "NY2012",
            "NY2013",
            "NY2014",
            "NY2015",
            "NY2016",
            "NY2017",
            "NY2018",
            "NY2019",
            "NY2020",
            "NY2021",
            "NY2022",
            "NY2023",
            "NY2024",
            "NY2025",
            "NY2026",
            "NY2027",
            "NY2028",
            "NY2029",
            "NY2030",
            "NY2031",
            "NY2032",
            "NY2033",
            "NY2034",
            "NY2035",
            "NY2036",
            "NY2037",
            "NY2038",
            "NY2039",
            "NY2040",
            "NY2041",
            "NY2042",
            "NY2043",
            "NY2044",
            "NY2045",
            "NY2046",
            "NY2047",
            "NY2048",
            "NY2049",
            "NY2050",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.ASHRAEClimateZone",
        EnumClassId::G36_ASHRAE_CLIMATE_ZONE,
        &[
            "Not_Specified",
            "Zone_1A",
            "Zone_1B",
            "Zone_2A",
            "Zone_2B",
            "Zone_3A",
            "Zone_3B",
            "Zone_3C",
            "Zone_4A",
            "Zone_4B",
            "Zone_4C",
            "Zone_5A",
            "Zone_5B",
            "Zone_5C",
            "Zone_6A",
            "Zone_6B",
            "Zone_7",
            "Zone_8",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.ControlEconomizer",
        EnumClassId::G36_CONTROL_ECONOMIZER,
        &[
            "FixedDryBulb",
            "DifferentialDryBulb",
            "FixedDryBulbWithDifferentialDryBulb",
            "FixedEnthalpyWithFixedDryBulb",
            "DifferentialEnthalpyWithFixedDryBulb",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
        EnumClassId::G36_COOLING_COIL,
        &["None", "WaterBased", "DXCoil"],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard",
        EnumClassId::G36_ENERGY_STANDARD,
        &["ASHRAE90_1", "California_Title_24"],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat",
        EnumClassId::G36_FREEZE_STAT,
        &[
            "No_freeze_stat",
            "Hardwired_to_equipment",
            "Hardwired_to_BAS",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.HeatingCoil",
        EnumClassId::G36_HEATING_COIL,
        &["None", "WaterBased", "Electric"],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.OutdoorAirSection",
        EnumClassId::G36_OUTDOOR_AIR_SECTION,
        &[
            "DedicatedDampersAirflow",
            "DedicatedDampersPressure",
            "SingleDamper",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.PressureControl",
        EnumClassId::G36_PRESSURE_CONTROL,
        &[
            "BarometricRelief",
            "ReliefDamper",
            "ReliefFan",
            "ReturnFanMeasuredAir",
            "ReturnFanDp",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.Title24ClimateZone",
        EnumClassId::G36_TITLE24_CLIMATE_ZONE,
        &[
            "Not_Specified",
            "Zone_1",
            "Zone_2",
            "Zone_3",
            "Zone_4",
            "Zone_5",
            "Zone_6",
            "Zone_7",
            "Zone_8",
            "Zone_9",
            "Zone_10",
            "Zone_11",
            "Zone_12",
            "Zone_13",
            "Zone_14",
            "Zone_15",
            "Zone_16",
        ],
    ),
    enumeration(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
        EnumClassId::G36_VENTILATION_STANDARD,
        &["ASHRAE62_1", "California_Title_24"],
    ),
    constant_package(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.DemandLimitLevels",
        &[
            "cooling0", "cooling1", "cooling2", "cooling3", "heating0", "heating1", "heating2",
            "heating3",
        ],
        &[0, 1, 2, 3, 0, 1, 2, 3],
    ),
    constant_package(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeProtectionStages",
        &["stage0", "stage1", "stage2", "stage3"],
        &[0, 1, 2, 3],
    ),
    constant_package(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
        &[
            "occupied",
            "coolDown",
            "setUp",
            "warmUp",
            "setBack",
            "freezeProtection",
            "unoccupied",
        ],
        &[1, 2, 3, 4, 5, 6, 7],
    ),
    constant_package(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.ZoneStates",
        &["heating", "deadband", "cooling"],
        &[1, 2, 3],
    ),
];
