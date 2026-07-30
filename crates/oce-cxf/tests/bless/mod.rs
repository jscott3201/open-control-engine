//! Shared truthiness policy for opt-in golden regeneration.
//!
//! An unset value, `""`, `"0"`, or `"false"` (case-insensitive) disables blessing. Any other
//! Unicode value enables it. A non-Unicode value is treated as disabled.
//!
//! Blessing is intentionally silent: `clippy::print_stderr` is warn-by-default workspace-wide
//! and becomes an error when the gate promotes warnings to errors. The file write is the effect.

const BLESS_DISABLED_VALUES: [&str; 3] = ["", "0", "false"];

/// Reports whether `OCE_BLESS` arms golden regeneration.
pub fn enabled() -> bool {
    std::env::var("OCE_BLESS").is_ok_and(|value| {
        !BLESS_DISABLED_VALUES
            .iter()
            .any(|disabled| value.eq_ignore_ascii_case(disabled))
    })
}
