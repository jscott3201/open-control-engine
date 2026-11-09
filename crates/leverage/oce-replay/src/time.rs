//! Time columns, the wall-clock origin and their conversion to model seconds (decision 17).
//!
//! Engine time is model seconds from the run's origin. Converting source timestamps is the
//! adapter's job, with one fixed rule: a signed integer nanosecond offset `d` from the origin
//! becomes `whole as f64 + (rem as f64) / 1e9`, where `whole = d.div_euclid(1e9)` and
//! `rem = d.rem_euclid(1e9)` (so `rem` is in `[0, 1e9)` and a time before the origin has a
//! negative `whole`). Each of the two conversions and the sum round once, to nearest. The origin
//! itself is header metadata only; the engine never sees it.

use std::fmt;

use crate::SourceError;

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// The model seconds of a signed nanosecond offset, by the fixed split documented above.
#[must_use]
#[allow(clippy::cast_precision_loss)] // The documented conversion rounds to nearest.
pub fn seconds_from_nanos(offset: i128) -> f64 {
    let whole = offset.div_euclid(NANOS_PER_SECOND);
    let rem = offset.rem_euclid(NANOS_PER_SECOND);
    whole as f64 + (rem as f64) / 1e9
}

/// A wall-clock origin: an RFC 3339 instant with an explicit `Z` or `±hh:mm` offset, kept as
/// written for the trace header and as nanoseconds since the Unix epoch for conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeOrigin {
    text: String,
    nanos: i128,
}

impl TimeOrigin {
    /// Parse an origin in the RFC 3339 subset `YYYY-MM-DDTHH:MM:SS[.fraction]` followed by `Z` or
    /// `±hh:mm`: an upper-case `T` and `Z`, one to nine fraction digits, years 0000 to 9999, no
    /// leap second, no zone names.
    ///
    /// # Errors
    /// [`SourceError::BadInstant`] with row 0.
    pub fn parse(text: &str) -> Result<Self, SourceError> {
        let nanos = parse_instant(text.as_bytes()).ok_or(SourceError::BadInstant { row: 0 })?;
        Ok(Self {
            text: text.to_owned(),
            nanos,
        })
    }

    /// The origin as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Nanoseconds since 1970-01-01T00:00:00Z.
    #[must_use]
    pub fn unix_nanos(&self) -> i128 {
        self.nanos
    }

    /// The model seconds of `instant` (same subset) relative to this origin, or `None` when it
    /// does not parse.
    #[must_use]
    pub fn seconds_of(&self, instant: &str) -> Option<f64> {
        parse_instant(instant.as_bytes()).map(|n| seconds_from_nanos(n - self.nanos))
    }
}

impl fmt::Display for TimeOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// A columnar source's time column (decision 17).
#[derive(Clone, Debug)]
pub enum TimeColumn {
    /// Model seconds, used as given; NaN is a missing time.
    Seconds(Vec<f64>),
    /// Integer nanoseconds (numpy `datetime64[ns]` as `i64`) relative to an integer origin,
    /// subtracted in integers and converted by the fixed split. `i64::MIN` (NaT) is a missing
    /// time.
    EpochNanos {
        /// The timestamps.
        values: Vec<i64>,
        /// The origin, same unit.
        origin: i64,
    },
}

impl TimeColumn {
    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Seconds(values) => values.len(),
            Self::EpochNanos { values, .. } => values.len(),
        }
    }

    /// Whether the column has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Row `row`'s model seconds; NaN for a missing time (NaN or NaT). Panics past the end.
    pub(crate) fn seconds(&self, row: usize) -> f64 {
        match self {
            Self::Seconds(values) => values[row],
            Self::EpochNanos { values, origin } => {
                if values[row] == i64::MIN {
                    f64::NAN
                } else {
                    seconds_from_nanos(i128::from(values[row]) - i128::from(*origin))
                }
            }
        }
    }
}

/// Parse the RFC 3339 subset into nanoseconds since the Unix epoch.
pub(crate) fn parse_instant(s: &[u8]) -> Option<i128> {
    // YYYY-MM-DDTHH:MM:SS = 19 bytes, then an optional fraction, then the offset.
    if s.len() < 20 {
        return None;
    }
    let year = digits(&s[0..4])?;
    let month = digits(&s[5..7])?;
    let day = digits(&s[8..10])?;
    let hour = digits(&s[11..13])?;
    let minute = digits(&s[14..16])?;
    let second = digits(&s[17..19])?;
    if s[4] != b'-' || s[7] != b'-' || s[10] != b'T' || s[13] != b':' || s[16] != b':' {
        return None;
    }
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut rest = &s[19..];
    let mut fraction: i128 = 0;
    if rest.first() == Some(&b'.') {
        let count = rest[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        if count == 0 || count > 9 {
            return None;
        }
        fraction = i128::from(digits(&rest[1..=count])?);
        fraction *= 10_i128.pow(9 - count as u32);
        rest = &rest[1 + count..];
    }
    let offset_minutes: i128 = match rest {
        [b'Z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hours = digits(&[*h1, *h2])?;
            let minutes = digits(&[*m1, *m2])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let total = i128::from(hours * 60 + minutes);
            if *sign == b'-' { -total } else { total }
        }
        _ => return None,
    };
    let days = days_from_civil(i64::from(year), month, day);
    let seconds = i128::from(days) * 86_400
        + i128::from(hour) * 3_600
        + i128::from(minute) * 60
        + i128::from(second)
        - offset_minutes * 60;
    Some(seconds * NANOS_PER_SECOND + fraction)
}

fn digits(bytes: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(value)
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
