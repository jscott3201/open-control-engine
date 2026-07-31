#![forbid(unsafe_code)]
//! Shared truthiness policy for opt-in golden regeneration.
//!
//! Status: verification-only helper; no shipped crate reaches it through a non-dev edge.
//!
//! This guard protects only call sites that explicitly use it. A future `expect_file!` or
//! `expect!` call is unguarded until it is migrated to a first-party compare-or-write path.
//! Dependency hygiene does not enforce that migration: cargo-machete 0.9.2 does not analyze
//! `[dev-dependencies]`, so re-adding `expect-test` would leave the unused-dependency gate green.

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
/// An unset or non-UTF-8 variable is disabled. If the variable is present as UTF-8, its value is
/// interpreted by [`enabled_for`]. This function does not mutate the process environment and does
/// not panic.
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
            ("   ", false),
            ("no", true),
            ("off", true),
            ("1", true),
            ("true", true),
            ("yes", true),
            ("0.0", true),
        ] {
            assert_eq!(enabled_for(value), expected, "truthiness for {value:?}");
        }
    }
}
