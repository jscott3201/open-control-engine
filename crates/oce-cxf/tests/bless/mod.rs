//! Golden-regeneration arming for this crate's test binaries.
//!
//! The truthiness policy itself lives in `oce-bless`; this module only names the variable.

/// Reports whether `OCE_BLESS` arms golden regeneration.
pub fn enabled() -> bool {
    oce_bless::enabled("OCE_BLESS")
}
