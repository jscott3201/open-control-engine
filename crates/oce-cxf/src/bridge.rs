//! The IRI → canonical class-path bridge (doc 04 §7.3, `_spec/11-m1-cxf-plan.md` fixture-plan rule).
//!
//! A CXF instance node's `@type` is a full class IRI, e.g.
//! `http://example.org#Buildings.Controls.OBC.CDL.Reals.Add`. The native block registry
//! ([`oce_blocks::lookup`]) is keyed on the **canonical class path** `CDL.Reals.Add`. This module
//! is the single, pure, total mapping between the two:
//!
//! 1. take the fragment after the last `#` (or the whole string if there is no `#`);
//! 2. if it begins with the OBC namespace prefix `Buildings.Controls.OBC.`, strip that prefix;
//! 3. the remainder is the candidate class path.
//!
//! It does **not** itself decide validity — the caller checks `oce_blocks::lookup(candidate)` and
//! raises [`oce_diag::DiagCode::ClassNotFound`] on `None` (an unregistered or non-subset class,
//! e.g. `CDL.Reals.PID`, which strips cleanly but is not implemented). This function **never
//! panics**: a `@type` with no `#` simply yields the whole string as the candidate (which then
//! fails the registry lookup), so a malformed `@type` is a typed diagnostic, never a crash.

/// The OBC namespace prefix stripped from a class IRI fragment to obtain the canonical class path.
/// The exporter re-prepends the same constant (its inverse direction), so the two directions
/// cannot drift.
pub(crate) const OBC_PREFIX: &str = "Buildings.Controls.OBC.";

/// Map a CXF `@type` class IRI to its candidate canonical class path (the registry key).
///
/// Pure and total — returns a borrowed slice of `type_iri`; never panics, even when `type_iri`
/// contains no `#`. The returned string is a *candidate*: the caller must still confirm it via
/// [`oce_blocks::lookup`].
#[must_use]
pub(crate) fn class_path_of(type_iri: &str) -> &str {
    // Fragment after the last '#'. `rsplit` always yields at least one element, so `next()` is
    // infallible — no `#` means the whole string is the fragment. Never indexes or unwraps a split.
    let fragment = type_iri.rsplit('#').next().unwrap_or(type_iri);
    fragment.strip_prefix(OBC_PREFIX).unwrap_or(fragment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_obc_prefix_from_full_iri() {
        assert_eq!(
            class_path_of("http://example.org#Buildings.Controls.OBC.CDL.Reals.Add"),
            "CDL.Reals.Add"
        );
        assert_eq!(
            class_path_of("http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant"),
            "CDL.Reals.Sources.Constant"
        );
        assert_eq!(
            class_path_of("http://example.org#Buildings.Controls.OBC.CDL.Discrete.UnitDelay"),
            "CDL.Discrete.UnitDelay"
        );
    }

    #[test]
    fn passes_through_already_canonical_fragment() {
        // A fragment that is already a bare class path (no OBC prefix) is returned unchanged.
        assert_eq!(class_path_of("http://x#CDL.Reals.Add"), "CDL.Reals.Add");
        // A non-subset class (valid CDL, not implemented): strips/passes clean, registry rejects it.
        assert_eq!(
            class_path_of("http://x#Buildings.Controls.OBC.CDL.Reals.PID"),
            "CDL.Reals.PID"
        );
    }

    #[test]
    fn no_hash_yields_whole_string_no_panic() {
        // The panic-surface: a `@type` with no '#'. Must not panic; the whole string is the
        // candidate (which the registry lookup then rejects → ClassNotFound at the call site).
        assert_eq!(class_path_of("NotAnIri"), "NotAnIri");
        assert_eq!(class_path_of(""), "");
        // Bare OBC-prefixed string with no '#' still strips the prefix.
        assert_eq!(
            class_path_of("Buildings.Controls.OBC.CDL.Logical.And"),
            "CDL.Logical.And"
        );
    }

    #[test]
    fn only_strips_prefix_at_the_start() {
        // The prefix is stripped only as a leading prefix, never mid-string.
        assert_eq!(
            class_path_of("http://x#CDL.Buildings.Controls.OBC.Foo"),
            "CDL.Buildings.Controls.OBC.Foo"
        );
    }
}
