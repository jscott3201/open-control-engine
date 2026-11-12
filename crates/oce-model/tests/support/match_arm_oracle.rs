//! The enumeration helpers as hand-written `match` arms, verbatim from before the one table
//! (issue #468), kept as the differential oracle the table-backed functions must agree with on
//! every input. Only the item paths changed (`crate::` became `oce_model::`).

#![allow(dead_code)]

use oce_model::{EnumClassId, EnumDescriptor};

const G36_TYPES_PREFIX: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.";

const SIMPLE_CONTROLLER_MEMBERS: &[&str] = &["P", "PI", "PD", "PID"];
const SMOOTHNESS_MEMBERS: &[&str] = &["LinearSegments", "ConstantSegments"];
const EXTRAPOLATION_MEMBERS: &[&str] = &["HoldLastPoint", "LastTwoPoints", "Periodic"];
const ZERO_TIME_MEMBERS: &[&str] = &[
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
];

/// Resolve a known CDL enum class to its stable in-memory id.
#[must_use]
pub fn enum_class_id(qualified: &str) -> Option<EnumClassId> {
    g36_enum_class_id(qualified).or_else(|| match normalized_class(qualified) {
        "SimpleController" => Some(EnumClassId::SIMPLE_CONTROLLER),
        "Smoothness" => Some(EnumClassId::SMOOTHNESS),
        "Extrapolation" => Some(EnumClassId::EXTRAPOLATION),
        "ZeroTime" => Some(EnumClassId::ZERO_TIME),
        _ => None,
    })
}

/// Return the canonical descriptor for a known enum class id.
///
/// The numeric id is process-local representation; durable consumers use `class_path` and
/// `members` as the class identity.
#[must_use]
pub fn enum_descriptor(class: EnumClassId) -> Option<EnumDescriptor> {
    let (class_path, members) = match class {
        EnumClassId::SIMPLE_CONTROLLER => ("CDL.Types.SimpleController", SIMPLE_CONTROLLER_MEMBERS),
        EnumClassId::SMOOTHNESS => ("CDL.Types.Smoothness", SMOOTHNESS_MEMBERS),
        EnumClassId::EXTRAPOLATION => ("CDL.Types.Extrapolation", EXTRAPOLATION_MEMBERS),
        EnumClassId::ZERO_TIME => ("CDL.Types.ZeroTime", ZERO_TIME_MEMBERS),
        EnumClassId::G36_ASHRAE_CLIMATE_ZONE => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.ASHRAEClimateZone",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_CONTROL_ECONOMIZER => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.ControlEconomizer",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_COOLING_COIL => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_ENERGY_STANDARD => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_FREEZE_STAT => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_HEATING_COIL => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.HeatingCoil",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_OUTDOOR_AIR_SECTION => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.OutdoorAirSection",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_PRESSURE_CONTROL => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.PressureControl",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_TITLE24_CLIMATE_ZONE => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.Title24ClimateZone",
            g36_enum_literals(class)?,
        ),
        EnumClassId::G36_VENTILATION_STANDARD => (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
            g36_enum_literals(class)?,
        ),
        _ => return None,
    };
    Some(EnumDescriptor {
        id: class,
        class_path,
        members,
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
    enum_descriptor(class)?
        .members
        .iter()
        .position(|member| *member == literal)
        .map(|index| index as u32 + 1)
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
    match canonical_g36_type(qualified)? {
        "ASHRAEClimateZone" => Some(EnumClassId::G36_ASHRAE_CLIMATE_ZONE),
        "ControlEconomizer" => Some(EnumClassId::G36_CONTROL_ECONOMIZER),
        "CoolingCoil" => Some(EnumClassId::G36_COOLING_COIL),
        "EnergyStandard" => Some(EnumClassId::G36_ENERGY_STANDARD),
        "FreezeStat" => Some(EnumClassId::G36_FREEZE_STAT),
        "HeatingCoil" => Some(EnumClassId::G36_HEATING_COIL),
        "OutdoorAirSection" => Some(EnumClassId::G36_OUTDOOR_AIR_SECTION),
        "PressureControl" => Some(EnumClassId::G36_PRESSURE_CONTROL),
        "Title24ClimateZone" => Some(EnumClassId::G36_TITLE24_CLIMATE_ZONE),
        "VentilationStandard" => Some(EnumClassId::G36_VENTILATION_STANDARD),
        _ => None,
    }
}

/// Source-order literals for a source-pinned ASHRAE G36 enum class.
#[must_use]
pub fn g36_enum_literals(class: EnumClassId) -> Option<&'static [&'static str]> {
    match class {
        EnumClassId::G36_ASHRAE_CLIMATE_ZONE => Some(&[
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
        ]),
        EnumClassId::G36_CONTROL_ECONOMIZER => Some(&[
            "FixedDryBulb",
            "DifferentialDryBulb",
            "FixedDryBulbWithDifferentialDryBulb",
            "FixedEnthalpyWithFixedDryBulb",
            "DifferentialEnthalpyWithFixedDryBulb",
        ]),
        EnumClassId::G36_COOLING_COIL => Some(&["None", "WaterBased", "DXCoil"]),
        EnumClassId::G36_ENERGY_STANDARD => Some(&["ASHRAE90_1", "California_Title_24"]),
        EnumClassId::G36_FREEZE_STAT => Some(&[
            "No_freeze_stat",
            "Hardwired_to_equipment",
            "Hardwired_to_BAS",
        ]),
        EnumClassId::G36_HEATING_COIL => Some(&["None", "WaterBased", "Electric"]),
        EnumClassId::G36_OUTDOOR_AIR_SECTION => Some(&[
            "DedicatedDampersAirflow",
            "DedicatedDampersPressure",
            "SingleDamper",
        ]),
        EnumClassId::G36_PRESSURE_CONTROL => Some(&[
            "BarometricRelief",
            "ReliefDamper",
            "ReliefFan",
            "ReturnFanMeasuredAir",
            "ReturnFanDp",
        ]),
        EnumClassId::G36_TITLE24_CLIMATE_ZONE => Some(&[
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
        ]),
        EnumClassId::G36_VENTILATION_STANDARD => Some(&["ASHRAE62_1", "California_Title_24"]),
        _ => None,
    }
}

/// Whether `qualified` names one of the G36 `Types` integer-constant packages.
#[must_use]
pub fn is_g36_integer_constant_package(qualified: &str) -> bool {
    matches!(
        canonical_g36_type(qualified),
        Some("DemandLimitLevels")
            | Some("FreezeProtectionStages")
            | Some("OperationModes")
            | Some("ZoneStates")
    )
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
    match (canonical_g36_type(package)?, literal) {
        ("DemandLimitLevels", "cooling0" | "heating0") => Some(0),
        ("DemandLimitLevels", "cooling1" | "heating1") => Some(1),
        ("DemandLimitLevels", "cooling2" | "heating2") => Some(2),
        ("DemandLimitLevels", "cooling3" | "heating3") => Some(3),
        ("FreezeProtectionStages", "stage0") => Some(0),
        ("FreezeProtectionStages", "stage1") => Some(1),
        ("FreezeProtectionStages", "stage2") => Some(2),
        ("FreezeProtectionStages", "stage3") => Some(3),
        ("OperationModes", "occupied") => Some(1),
        ("OperationModes", "coolDown") => Some(2),
        ("OperationModes", "setUp") => Some(3),
        ("OperationModes", "warmUp") => Some(4),
        ("OperationModes", "setBack") => Some(5),
        ("OperationModes", "freezeProtection") => Some(6),
        ("OperationModes", "unoccupied") => Some(7),
        ("ZoneStates", "heating") => Some(1),
        ("ZoneStates", "deadband") => Some(2),
        ("ZoneStates", "cooling") => Some(3),
        _ => None,
    }
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
