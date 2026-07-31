#![forbid(unsafe_code)]
//! Shared truthiness policy for opt-in golden regeneration.
//!
//! Status: verification-only helper; no shipped crate reaches it through a non-dev edge.

/// Values that explicitly disable opt-in golden regeneration.
pub const BLESS_DISABLED_VALUES: [&str; 3] = ["", "0", "false"];

/// Returns whether an environment-variable value enables golden regeneration.
///
/// Leading and trailing whitespace is ignored, and the disabled vocabulary is compared without
/// ASCII case. Every value outside [`BLESS_DISABLED_VALUES`] enables regeneration.
#[must_use]
pub fn enabled_for(value: &str) -> bool {
    let value = value.trim();
    !BLESS_DISABLED_VALUES
        .iter()
        .any(|disabled| value.eq_ignore_ascii_case(disabled))
}

/// Returns whether an environment variable enables golden regeneration.
///
/// An unset variable is disabled. If the variable is present, its value is interpreted by
/// [`enabled_for`]. This function does not mutate the process environment and does not panic.
#[must_use]
pub fn enabled(var: &str) -> bool {
    std::env::var(var).is_ok_and(|value| enabled_for(&value))
}

#[cfg(test)]
mod tests {
    use super::enabled_for;

    #[test]
    fn disabled_and_enabling_values_follow_the_canonical_truth_table() {
        for (value, expected) in [
            ("", false),
            ("0", false),
            ("false", false),
            ("FALSE", false),
            ("False", false),
            ("  false  ", false),
            ("1", true),
            ("true", true),
            ("yes", true),
            ("0.0", true),
        ] {
            assert_eq!(enabled_for(value), expected, "truthiness for {value:?}");
        }
    }
}
