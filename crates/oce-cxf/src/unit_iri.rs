//! The closed table that reads a unit IRI as the unit string it stands for (issue #631; design
//! note `docs/design/unit-spellings.md`, decisions 2 and 8, owner rulings of 2026-11-12).
//!
//! modelica-json, the upstream translator, writes a declared Modelica `unit` or `displayUnit` as
//! an IRI node: a QUDT unit identifier where its fixed table (`getQudtUnit`, `lib/cxfExtractor.js`)
//! has one, and otherwise the original string in the S231 namespace. The engine's corpus, its
//! §7.10 unit gate and the host's point list all speak Modelica unit strings (`K`, `Pa`, `1`,
//! `m3/s`), compared byte for byte. So a `unit` or `displayUnit` written as an IRI node is read
//! back into that string here, and only here:
//!
//! - **A QUDT unit IRI** is one of [`QUDT_UNIT_NAMESPACES`] followed by a local name of
//!   [`QUDT_UNITS`], and reads as that entry's unit string. Both namespaces read alike:
//!   modelica-json binds `unit` to the `#` one and QUDT publishes the `/` one.
//! - **The S231 fallback** is the S231 namespace ([`crate::export::S231_CONTEXT_IRI`]) followed by
//!   the unit string itself (`http://data.ashrae.org/S231#s/K` reads `s/K`). The string is not
//!   empty and holds no whitespace and no Unicode format or control character
//!   ([`crate::is_format_or_control`]): an IRI holds none, a blank unit would reach the unit gate
//!   as a declared unit, and no reader can see a zero-width space. So whatever this module
//!   returns is a term the term-shape rule accepts.
//! - **Anything else maps to nothing**, and the resolver refuses the term as outside the subset
//!   (`non-subset-construct`): another namespace, a local name the table lacks, a relative
//!   reference, a different case.
//!
//! The table is `const` and closed. No IRI is ever dereferenced, nothing registers an entry at run
//! time, and the same bytes read as the same string on every host. One lookup is one prefix strip
//! and a binary search of 25 entries.
//!
//! The matching is on the IRI after `@context` expansion, which the caller does: the resolver
//! with its own prefix expansion (`expand`), a second reader above the facade with its own. Both
//! then call [`unit_of_iri`], so they give one IRI one reading; whether they expand an identifier
//! to the same IRI is each expansion's to hold.

use crate::export::S231_CONTEXT_IRI;
use oce_diag::is_format_or_control;

/// The two namespaces a QUDT unit IRI is read in: the one modelica-json binds `unit` to in the
/// documents it writes, and the one QUDT publishes.
pub const QUDT_UNIT_NAMESPACES: [&str; 2] =
    ["http://qudt.org/vocab/unit#", "http://qudt.org/vocab/unit/"];

/// The two namespaces the QUDT schema property `hasUnit` is read in: the one the modelica-json
/// translations bind `qudt` to, and the one QUDT publishes and `CXF-Core.jsonld` binds.
pub const QUDT_SCHEMA_NAMESPACES: [&str; 2] = [
    "http://qudt.org/schema/qudt#",
    "http://qudt.org/schema/qudt/",
];

/// `(QUDT unit local name, Modelica unit string)`, sorted by local name in byte order: the
/// inverse of modelica-json's `getQudtUnit` table at the vendored pin (`85721b82`), less the two
/// entries whose QUDT identifier names another unit than the Modelica string does.
///
/// Those two are `M3-PER-MIN`, which modelica-json writes for `cfm` and which is cubic metres a
/// minute, and `IN_H2O_60DEG_F`, which it writes for `inH2O` and which is the inch of water at
/// 60 °F, not the conventional one. Reading either back would report a unit the IRI does not
/// name, so neither is here and a document carrying one is refused.
///
/// Invariants, pinned by this module's tests: 25 entries, strictly ascending local names, and no
/// unit string twice.
pub const QUDT_UNITS: &[(&str, &str)] = &[
    ("BAR", "bar"),
    ("BTU_IT", "Btu"),
    ("BTU_IT-PER-HR", "BtuPerHour"),
    ("DEG", "deg"),
    ("DEG_C", "degC"),
    ("DEG_F", "degF"),
    ("GAL_US", "gal"),
    ("HP", "hp"),
    ("HR", "h"),
    ("HZ", "Hz"),
    ("J", "J"),
    ("J-PER-KiloGM", "J/kg"),
    ("K", "K"),
    ("KiloJ-PER-KiloGM", "kJ/kg"),
    ("M2", "m2"),
    ("M3", "m3"),
    ("M3-PER-SEC", "m3/s"),
    ("PA", "Pa"),
    ("PSI", "psi"),
    ("QUAD", "quad"),
    ("RAD", "rad"),
    ("SEC", "s"),
    ("TON_FG", "ton"),
    ("UNITLESS", "1"),
    ("W", "W"),
];

/// The unit string the absolute IRI `iri` stands for, or `None` when the closed table does not
/// map it.
///
/// `iri` is an IRI already expanded against its document's `@context`; this function expands
/// nothing and trims nothing. A QUDT unit IRI ([`QUDT_UNIT_NAMESPACES`], then exactly a local name
/// of [`QUDT_UNITS`]) reads as its entry's unit string. An IRI in the S231 namespace reads as the
/// text after the namespace, when that text is not empty and holds no whitespace (Unicode
/// `White_Space`, as [`char::is_whitespace`] reads it) and no format or control character
/// ([`crate::is_format_or_control`]). Every other input is `None`. A string this returns is
/// therefore never blank and never carries a character no reader can see.
///
/// Work is one prefix comparison per namespace and one binary search; it allocates nothing and
/// never panics.
#[must_use]
pub fn unit_of_iri(iri: &str) -> Option<&str> {
    if let Some(local) = QUDT_UNIT_NAMESPACES
        .iter()
        .find_map(|namespace| iri.strip_prefix(namespace))
    {
        return QUDT_UNITS
            .binary_search_by(|(name, _)| (*name).cmp(local))
            .ok()
            .map(|at| QUDT_UNITS[at].1);
    }
    iri.strip_prefix(S231_CONTEXT_IRI).filter(|unit| {
        !unit.is_empty() && !unit.contains(|c: char| c.is_whitespace() || is_format_or_control(c))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// modelica-json's `getQudtUnit` table at the vendored pin, as written: `(Modelica unit
    /// string, QUDT unit local name)` in its order, 27 entries. Stated here by hand, apart from
    /// [`QUDT_UNITS`], so the inversion is checked and not restated.
    const GET_QUDT_UNIT: [(&str, &str); 27] = [
        ("Btu", "BTU_IT"),
        ("BtuPerHour", "BTU_IT-PER-HR"),
        ("W", "W"),
        ("J", "J"),
        ("bar", "BAR"),
        ("Pa", "PA"),
        ("cfm", "M3-PER-MIN"),
        ("m3/s", "M3-PER-SEC"),
        ("degC", "DEG_C"),
        ("degF", "DEG_F"),
        ("K", "K"),
        ("gal", "GAL_US"),
        ("hp", "HP"),
        ("m2", "M2"),
        ("m3", "M3"),
        ("s", "SEC"),
        ("kJ/kg", "KiloJ-PER-KiloGM"),
        ("inH2O", "IN_H2O_60DEG_F"),
        ("psi", "PSI"),
        ("quad", "QUAD"),
        ("ton", "TON_FG"),
        ("deg", "DEG"),
        ("rad", "RAD"),
        ("1", "UNITLESS"),
        ("h", "HR"),
        ("J/kg", "J-PER-KiloGM"),
        ("Hz", "HZ"),
    ];

    /// The two entries left out: the QUDT identifier names another unit than the Modelica string.
    const DROPPED: [&str; 2] = ["IN_H2O_60DEG_F", "M3-PER-MIN"];

    #[test]
    fn the_table_is_the_upstream_table_inverted_less_the_two_dropped_entries() {
        let mut expected: Vec<(&str, &str)> = GET_QUDT_UNIT
            .iter()
            .filter(|(_, qudt)| !DROPPED.contains(qudt))
            .map(|(unit, qudt)| (*qudt, *unit))
            .collect();
        expected.sort_unstable();
        assert_eq!(QUDT_UNITS, expected.as_slice());
        assert_eq!(QUDT_UNITS.len(), 25);
    }

    #[test]
    fn the_table_is_strictly_ascending_by_local_name_and_repeats_no_unit_string() {
        for pair in QUDT_UNITS.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "`{}` is not before `{}`",
                pair[0].0,
                pair[1].0
            );
        }
        let units: BTreeSet<&str> = QUDT_UNITS.iter().map(|(_, unit)| *unit).collect();
        assert_eq!(units.len(), QUDT_UNITS.len());
    }

    #[test]
    fn every_entry_reads_in_both_unit_namespaces() {
        for (local, unit) in QUDT_UNITS {
            for namespace in QUDT_UNIT_NAMESPACES {
                assert_eq!(
                    unit_of_iri(&format!("{namespace}{local}")),
                    Some(*unit),
                    "{namespace}{local}"
                );
            }
        }
    }

    #[test]
    fn the_two_dropped_entries_map_to_nothing_in_either_namespace() {
        for local in DROPPED {
            for namespace in QUDT_UNIT_NAMESPACES {
                assert_eq!(unit_of_iri(&format!("{namespace}{local}")), None, "{local}");
            }
        }
    }

    #[test]
    fn a_local_name_is_matched_whole_and_in_its_exact_case() {
        let ns = QUDT_UNIT_NAMESPACES[0];
        for local in [
            "", "k", "Pa", "pa", "PA ", " PA", "PAX", "P", "K/", "K#", "DEG_", "DEG_C ", "W\0",
            "unit:K", "SEC\n",
        ] {
            assert_eq!(unit_of_iri(&format!("{ns}{local}")), None, "{local:?}");
        }
    }

    #[test]
    fn an_iri_outside_the_three_namespaces_maps_to_nothing() {
        for iri in [
            "",
            "K",
            "degC",
            "unit:K",
            "S231:1/s",
            "http://qudt.org/vocab/unit",
            "http://qudt.org/vocab/unitK",
            "https://qudt.org/vocab/unit#K",
            "http://qudt.org/vocab/unit#",
            "http://qudt.org/vocab/unit/",
            "http://QUDT.org/vocab/unit#K",
            "http://qudt.org/vocab/quantitykind#K",
            "http://qudt.org/schema/qudt#K",
            "http://data.ashrae.org/S231P#K",
            "https://data.ashrae.org/S231#K",
            "http://data.ashrae.org/S231",
            " http://qudt.org/vocab/unit#K",
            "http://example.org#K",
        ] {
            assert_eq!(unit_of_iri(iri), None, "{iri:?}");
        }
    }

    #[test]
    fn the_s231_fallback_reads_the_text_after_the_namespace_verbatim() {
        for unit in [
            "s/K", "1/K", "1/s", "ppm", "5", "K", "W/(m2.K)", "unit", "µm",
        ] {
            assert_eq!(
                unit_of_iri(&format!("{S231_CONTEXT_IRI}{unit}")),
                Some(unit),
                "{unit}"
            );
        }
    }

    #[test]
    fn an_empty_or_whitespace_bearing_fallback_maps_to_nothing() {
        for unit in [
            "",
            " ",
            "\t",
            "s /K",
            " s/K",
            "s/K ",
            "s/K\n",
            "\u{a0}",
            "s\u{2028}K",
            "\u{3000}",
        ] {
            assert_eq!(
                unit_of_iri(&format!("{S231_CONTEXT_IRI}{unit}")),
                None,
                "{unit:?}"
            );
        }
    }

    #[test]
    fn a_fallback_carrying_a_format_or_control_character_maps_to_nothing() {
        // Outside `White_Space`: a zero-width space, a byte-order mark, a soft hyphen, a
        // bidirectional control, NUL, U+001F, DEL and a C1 control, at each end and inside.
        for c in [
            '\u{200B}', '\u{FEFF}', '\u{AD}', '\u{202E}', '\u{0}', '\u{1F}', '\u{7F}', '\u{9F}',
        ] {
            for unit in [
                format!("{c}K"),
                format!("K{c}"),
                format!("s{c}/K"),
                c.to_string(),
            ] {
                assert_eq!(
                    unit_of_iri(&format!("{S231_CONTEXT_IRI}{unit}")),
                    None,
                    "{unit:?}"
                );
            }
        }
        // No table entry carries one, so nothing this module returns does.
        for (_, unit) in QUDT_UNITS {
            assert!(!unit.contains(|c: char| c.is_whitespace() || is_format_or_control(c)));
        }
    }

    #[test]
    fn a_megabyte_long_iri_is_read_without_a_panic() {
        let long = "K".repeat(1 << 20);
        assert_eq!(
            unit_of_iri(&format!("{}{long}", QUDT_UNIT_NAMESPACES[0])),
            None
        );
        assert_eq!(
            unit_of_iri(&format!("{S231_CONTEXT_IRI}{long}")),
            Some(long.as_str())
        );
    }
}
