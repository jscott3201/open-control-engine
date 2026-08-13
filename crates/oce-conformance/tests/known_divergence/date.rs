//! Canonical Gregorian date validation for register lifecycle fields.

use super::reader::{ValidationCode, ValidationError};

pub(crate) fn valid_date(value: &str, entry: Option<usize>) -> Result<(), ValidationError> {
    let canonical = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !canonical {
        return Err(ValidationError::new(
            ValidationCode::InvalidDate,
            entry,
            "date is not canonical YYYY-MM-DD",
        ));
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..].parse().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > max_day {
        return Err(ValidationError::new(
            ValidationCode::InvalidDate,
            entry,
            "date is not a valid Gregorian date",
        ));
    }
    Ok(())
}
