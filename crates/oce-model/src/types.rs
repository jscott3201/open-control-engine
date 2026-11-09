//! Source-verified CDL enumeration helpers.
//!
//! Every function here reads the one table in [`crate::enumerations`] (issue #468): the member
//! lists, the G36 enumeration ids and the G36 Integer constant packages are written there once.

use crate::EnumClassId;
use crate::enumerations::{EnumerationClass, EnumerationKind, MemberValue, enumeration_classes};

const G36_TYPES_PREFIX: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.";
const CDL_TYPES_PREFIX: &str = "Buildings.Controls.OBC.CDL.Types.";

/// Canonical durable identity and source-order members for one known enum class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumDescriptor {
    /// Stable in-memory class id used by [`crate::ValueType::Enum`].
    pub id: EnumClassId,
    /// Canonical class path used by durable formats instead of the numeric id.
    pub class_path: &'static str,
    /// Members in their one-based source ordinal order.
    pub members: &'static [&'static str],
}

/// CDL `Types.SimpleController` enumeration (`03` section 4.9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SimpleController {
    /// Proportional.
    P,
    /// Proportional-integral.
    Pi,
    /// Proportional-derivative.
    Pd,
    /// Proportional-integral-derivative.
    Pid,
}

impl SimpleController {
    /// Parse a CXF-qualified `CDL.Types.SimpleController.*` value by its trailing member.
    #[must_use]
    pub fn from_qualified(s: &str) -> Option<Self> {
        let (_, member) = split_qualified_member(s)?;
        match member {
            "P" => Some(Self::P),
            "PI" => Some(Self::Pi),
            "PD" => Some(Self::Pd),
            "PID" => Some(Self::Pid),
            _ => None,
        }
    }
}

/// CDL `Types.ZeroTime` enumeration used by `Reals.Sources.CalendarTime`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZeroTime {
    /// Thu, 01 Jan 1970 00:00:00 local time.
    UnixTimeStamp,
    /// Thu, 01 Jan 1970 00:00:00 GMT.
    UnixTimeStampGmt,
    /// User-specified local new-year reference.
    Custom,
    /// New year midnight for the contained year.
    NewYear(i32),
}

impl ZeroTime {
    /// First source-supported new-year enum member.
    pub const FIRST_NEW_YEAR: i32 = 2010;
    /// Last source-supported new-year enum member.
    pub const LAST_NEW_YEAR: i32 = 2050;
    /// Number of `ZeroTime` members in the pinned Buildings source.
    pub const MEMBER_COUNT: u32 = 44;

    /// Parse a 1-based source ordinal.
    #[must_use]
    pub fn from_ordinal(ordinal: u32) -> Option<Self> {
        match ordinal {
            1 => Some(Self::UnixTimeStamp),
            2 => Some(Self::UnixTimeStampGmt),
            3 => Some(Self::Custom),
            4..=44 => Some(Self::NewYear(Self::FIRST_NEW_YEAR + (ordinal as i32 - 4))),
            _ => None,
        }
    }

    /// Return the 1-based source ordinal.
    #[must_use]
    pub fn ordinal(self) -> u32 {
        match self {
            Self::UnixTimeStamp => 1,
            Self::UnixTimeStampGmt => 2,
            Self::Custom => 3,
            Self::NewYear(year) => 4 + (year - Self::FIRST_NEW_YEAR) as u32,
        }
    }

    /// Parse a source member name such as `NY2017` or `UnixTimeStampGMT`.
    #[must_use]
    pub fn from_member(member: &str) -> Option<Self> {
        match member {
            "UnixTimeStamp" => Some(Self::UnixTimeStamp),
            "UnixTimeStampGMT" => Some(Self::UnixTimeStampGmt),
            "Custom" => Some(Self::Custom),
            _ => {
                let year = member.strip_prefix("NY")?.parse::<i32>().ok()?;
                (Self::FIRST_NEW_YEAR..=Self::LAST_NEW_YEAR)
                    .contains(&year)
                    .then_some(Self::NewYear(year))
            }
        }
    }

    /// Parse a CXF-qualified `CDL.Types.ZeroTime.*` value by its trailing member.
    #[must_use]
    pub fn from_qualified(s: &str) -> Option<Self> {
        let (_, member) = split_qualified_member(s)?;
        Self::from_member(member)
    }
}

impl EnumClassId {
    /// Stable class id for `G36.Types.ASHRAEClimateZone`.
    pub const G36_ASHRAE_CLIMATE_ZONE: Self = Self(101);
    /// Stable class id for `G36.Types.ControlEconomizer`.
    pub const G36_CONTROL_ECONOMIZER: Self = Self(102);
    /// Stable class id for `G36.Types.CoolingCoil`.
    pub const G36_COOLING_COIL: Self = Self(103);
    /// Stable class id for `G36.Types.EnergyStandard`.
    pub const G36_ENERGY_STANDARD: Self = Self(104);
    /// Stable class id for `G36.Types.FreezeStat`.
    pub const G36_FREEZE_STAT: Self = Self(105);
    /// Stable class id for `G36.Types.HeatingCoil`.
    pub const G36_HEATING_COIL: Self = Self(106);
    /// Stable class id for `G36.Types.OutdoorAirSection`.
    pub const G36_OUTDOOR_AIR_SECTION: Self = Self(107);
    /// Stable class id for `G36.Types.PressureControl`.
    pub const G36_PRESSURE_CONTROL: Self = Self(108);
    /// Stable class id for `G36.Types.Title24ClimateZone`.
    pub const G36_TITLE24_CLIMATE_ZONE: Self = Self(109);
    /// Stable class id for `G36.Types.VentilationStandard`.
    pub const G36_VENTILATION_STANDARD: Self = Self(110);
}

/// Resolve a known CDL enum class to its stable in-memory id.
///
/// A G36 enumeration resolves only by its full canonical `Buildings.Controls.OBC.ASHRAE.G36.Types.*`
/// path (after the last `#` or `/` of an IRI); a CDL enumeration by the last `.`-segment of
/// `qualified` alone (`SimpleController`, `…CDL.Types.SimpleController`).
#[must_use]
pub fn enum_class_id(qualified: &str) -> Option<EnumClassId> {
    g36_enum_class_id(qualified).or_else(|| {
        let name = normalized_class(qualified);
        enumeration_classes()
            .iter()
            .filter(|row| is_cdl_types_row(row))
            .find(|row| row.type_name() == name)
            .and_then(|row| row.id)
    })
}

/// Return the canonical descriptor for a known enum class id.
///
/// The numeric id is process-local representation; durable consumers use `class_path` and
/// `members` as the class identity. The descriptor's `class_path` is the durable form, frozen in
/// every recorded snapshot, blob and class signature hash: the path relative to
/// `Buildings.Controls.OBC.` for a CDL enumeration (`CDL.Types.SimpleController`) and the full
/// Modelica path for a G36 one (`Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil`). The one
/// relative form for every row is [`crate::EnumerationClass::class_path`].
#[must_use]
pub fn enum_descriptor(class: EnumClassId) -> Option<EnumDescriptor> {
    let row = crate::enumerations::enumeration_class_by_id(class)?;
    Some(EnumDescriptor {
        id: class,
        class_path: durable_class_path(row),
        members: row.members,
    })
}

/// Resolve a canonical durable enum class path.
#[must_use]
pub fn enum_descriptor_by_path(class_path: &str) -> Option<EnumDescriptor> {
    let class = enum_class_id(class_path)?;
    let descriptor = enum_descriptor(class)?;
    (descriptor.class_path == class_path).then_some(descriptor)
}

/// Resolve a member literal within a known CDL enum class to its 1-based ordinal.
#[must_use]
pub fn enum_member_ordinal(class: EnumClassId, literal: &str) -> Option<u32> {
    match crate::enumerations::enumeration_class_by_id(class)?.member_value(literal)? {
        MemberValue::Ordinal(ordinal) => Some(ordinal),
        MemberValue::Integer(_) => None,
    }
}

/// Whether `qualified` names the source-pinned ASHRAE G36 `Types` package.
#[must_use]
pub fn is_g36_type_path(qualified: &str) -> bool {
    canonical_g36_type(qualified).is_some()
}

/// Resolve a source-pinned ASHRAE G36 enum class to its stable in-memory id.
///
/// Only full canonical `Buildings.Controls.OBC.ASHRAE.G36.Types.*` paths resolve here. This keeps
/// G36 structural typing closed-world and avoids treating unrelated short names as G36 types.
#[must_use]
pub fn g36_enum_class_id(qualified: &str) -> Option<EnumClassId> {
    g36_row(qualified, EnumerationKind::Enumeration).and_then(|row| row.id)
}

/// Source-order literals for a source-pinned ASHRAE G36 enum class.
#[must_use]
pub fn g36_enum_literals(class: EnumClassId) -> Option<&'static [&'static str]> {
    crate::enumerations::enumeration_class_by_id(class)
        .filter(|row| is_g36_types_row(row))
        .map(|row| row.members)
}

/// Whether `qualified` names one of the G36 `Types` integer-constant packages.
#[must_use]
pub fn is_g36_integer_constant_package(qualified: &str) -> bool {
    g36_row(qualified, EnumerationKind::ConstantPackage).is_some()
}

/// Resolve a fully-qualified ASHRAE G36 integer constant reference.
#[must_use]
pub fn g36_integer_constant(qualified: &str) -> Option<i64> {
    let (package, literal) = split_qualified_member(qualified)?;
    g36_integer_constant_in_package(package, literal)
}

/// Resolve an ASHRAE G36 integer constant by package and member name.
#[must_use]
pub fn g36_integer_constant_in_package(package: &str, literal: &str) -> Option<i64> {
    match g36_row(package, EnumerationKind::ConstantPackage)?.member_value(literal)? {
        MemberValue::Integer(value) => Some(value),
        MemberValue::Ordinal(_) => None,
    }
}

/// The durable class path [`EnumDescriptor::class_path`] records for `row` (see
/// [`enum_descriptor`]).
fn durable_class_path(row: &EnumerationClass) -> &'static str {
    if is_g36_types_row(row) {
        row.qualified_path
    } else {
        row.class_path
    }
}

fn is_g36_types_row(row: &EnumerationClass) -> bool {
    row.qualified_path.starts_with(G36_TYPES_PREFIX)
}

fn is_cdl_types_row(row: &EnumerationClass) -> bool {
    row.qualified_path.starts_with(CDL_TYPES_PREFIX)
}

/// The G36 `Types` row of `kind` that `qualified` names by its full canonical path.
fn g36_row(qualified: &str, kind: EnumerationKind) -> Option<&'static EnumerationClass> {
    let name = canonical_g36_type(qualified)?;
    enumeration_classes().iter().find(|row| {
        row.kind == kind
            && is_g36_types_row(row)
            && row.qualified_path.strip_prefix(G36_TYPES_PREFIX) == Some(name)
    })
}

fn split_qualified_member(s: &str) -> Option<(&str, &str)> {
    let (prefix, member) = s.rsplit_once('.')?;
    (!prefix.is_empty() && !member.is_empty()).then_some((prefix, member))
}

fn canonical_g36_type(qualified: &str) -> Option<&str> {
    qualified
        .rsplit(['#', '/'])
        .next()
        .unwrap_or(qualified)
        .strip_prefix(G36_TYPES_PREFIX)
}

fn normalized_class(qualified: &str) -> &str {
    qualified.rsplit('.').next().unwrap_or(qualified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_controller_parses_qualified_members() {
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
        assert_eq!(
            SimpleController::from_qualified("Buildings.Controls.OBC.CDL.Types.SimpleController.X"),
            None
        );
    }

    #[test]
    fn zero_time_ordinals_and_qualified_members_match_source() {
        assert_eq!(ZeroTime::from_ordinal(1), Some(ZeroTime::UnixTimeStamp));
        assert_eq!(ZeroTime::from_ordinal(2), Some(ZeroTime::UnixTimeStampGmt));
        assert_eq!(ZeroTime::from_ordinal(3), Some(ZeroTime::Custom));
        assert_eq!(ZeroTime::from_ordinal(4), Some(ZeroTime::NewYear(2010)));
        assert_eq!(ZeroTime::from_ordinal(44), Some(ZeroTime::NewYear(2050)));
        assert_eq!(ZeroTime::from_ordinal(45), None);
        assert_eq!(
            ZeroTime::from_member("NY2024"),
            Some(ZeroTime::NewYear(2024))
        );
        assert_eq!(
            ZeroTime::from_qualified("Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2050"),
            Some(ZeroTime::NewYear(2050))
        );
    }

    #[test]
    fn generic_enum_helpers_cover_structural_cdl_types() {
        assert_eq!(
            enum_class_id("Buildings.Controls.OBC.CDL.Types.ZeroTime"),
            Some(EnumClassId::ZERO_TIME)
        );
        assert_eq!(
            enum_member_ordinal(EnumClassId::ZERO_TIME, "NY2017"),
            Some(11)
        );
        assert_eq!(
            enum_member_ordinal(EnumClassId::EXTRAPOLATION, "Periodic"),
            Some(3)
        );
        assert_eq!(
            enum_member_ordinal(EnumClassId::SMOOTHNESS, "ConstantSegments"),
            Some(2)
        );
    }

    #[test]
    fn g36_enum_helpers_cover_source_ordered_literals() {
        let cases: &[(&str, EnumClassId, &[&str])] = &[
            (
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
            (
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
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
                EnumClassId::G36_COOLING_COIL,
                &["None", "WaterBased", "DXCoil"],
            ),
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard",
                EnumClassId::G36_ENERGY_STANDARD,
                &["ASHRAE90_1", "California_Title_24"],
            ),
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat",
                EnumClassId::G36_FREEZE_STAT,
                &[
                    "No_freeze_stat",
                    "Hardwired_to_equipment",
                    "Hardwired_to_BAS",
                ],
            ),
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.HeatingCoil",
                EnumClassId::G36_HEATING_COIL,
                &["None", "WaterBased", "Electric"],
            ),
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.OutdoorAirSection",
                EnumClassId::G36_OUTDOOR_AIR_SECTION,
                &[
                    "DedicatedDampersAirflow",
                    "DedicatedDampersPressure",
                    "SingleDamper",
                ],
            ),
            (
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
            (
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
            (
                "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
                EnumClassId::G36_VENTILATION_STANDARD,
                &["ASHRAE62_1", "California_Title_24"],
            ),
        ];

        for &(class_path, class, literals) in cases {
            assert_eq!(enum_class_id(class_path), Some(class));
            assert_eq!(g36_enum_literals(class), Some(literals));
            for (index, literal) in literals.iter().enumerate() {
                assert_eq!(
                    enum_member_ordinal(class, literal),
                    Some(index as u32 + 1),
                    "{class_path}.{literal}"
                );
            }
            assert_eq!(enum_member_ordinal(class, "Bogus"), None);
        }

        assert_eq!(enum_class_id("VentilationStandard"), None);
    }

    #[test]
    fn g36_integer_constants_are_explicit_values_not_ordinals() {
        let prefix = "Buildings.Controls.OBC.ASHRAE.G36.Types";
        let cases = [
            ("DemandLimitLevels.cooling0", 0),
            ("DemandLimitLevels.cooling1", 1),
            ("DemandLimitLevels.cooling2", 2),
            ("DemandLimitLevels.cooling3", 3),
            ("DemandLimitLevels.heating0", 0),
            ("DemandLimitLevels.heating1", 1),
            ("DemandLimitLevels.heating2", 2),
            ("DemandLimitLevels.heating3", 3),
            ("FreezeProtectionStages.stage0", 0),
            ("FreezeProtectionStages.stage1", 1),
            ("FreezeProtectionStages.stage2", 2),
            ("FreezeProtectionStages.stage3", 3),
            ("OperationModes.occupied", 1),
            ("OperationModes.coolDown", 2),
            ("OperationModes.setUp", 3),
            ("OperationModes.warmUp", 4),
            ("OperationModes.setBack", 5),
            ("OperationModes.freezeProtection", 6),
            ("OperationModes.unoccupied", 7),
            ("ZoneStates.heating", 1),
            ("ZoneStates.deadband", 2),
            ("ZoneStates.cooling", 3),
        ];
        for (suffix, value) in cases {
            assert_eq!(
                g36_integer_constant(&format!("{prefix}.{suffix}")),
                Some(value)
            );
        }
        assert!(is_g36_integer_constant_package(&format!(
            "{prefix}.OperationModes"
        )));
        assert_eq!(
            g36_integer_constant_in_package(&format!("{prefix}.OperationModes"), "missing"),
            None
        );
        assert_eq!(enum_class_id(&format!("{prefix}.OperationModes")), None);
    }
}
