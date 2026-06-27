//! Source-verified CDL enumeration helpers.

use crate::EnumClassId;

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

/// Resolve a known CDL enum class to its stable in-memory id.
#[must_use]
pub fn enum_class_id(qualified: &str) -> Option<EnumClassId> {
    match normalized_class(qualified) {
        "SimpleController" => Some(EnumClassId::SIMPLE_CONTROLLER),
        "Smoothness" => Some(EnumClassId::SMOOTHNESS),
        "Extrapolation" => Some(EnumClassId::EXTRAPOLATION),
        "ZeroTime" => Some(EnumClassId::ZERO_TIME),
        _ => None,
    }
}

/// Resolve a member literal within a known CDL enum class to its 1-based ordinal.
#[must_use]
pub fn enum_member_ordinal(class: EnumClassId, literal: &str) -> Option<u32> {
    match class {
        EnumClassId::SIMPLE_CONTROLLER => match literal {
            "P" => Some(1),
            "PI" => Some(2),
            "PD" => Some(3),
            "PID" => Some(4),
            _ => None,
        },
        EnumClassId::SMOOTHNESS => match literal {
            "LinearSegments" => Some(1),
            "ConstantSegments" => Some(2),
            _ => None,
        },
        EnumClassId::EXTRAPOLATION => match literal {
            "HoldLastPoint" => Some(1),
            "LastTwoPoints" => Some(2),
            "Periodic" => Some(3),
            _ => None,
        },
        EnumClassId::ZERO_TIME => ZeroTime::from_member(literal).map(ZeroTime::ordinal),
        _ => None,
    }
}

fn split_qualified_member(s: &str) -> Option<(&str, &str)> {
    let (prefix, member) = s.rsplit_once('.')?;
    (!prefix.is_empty() && !member.is_empty()).then_some((prefix, member))
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
}
