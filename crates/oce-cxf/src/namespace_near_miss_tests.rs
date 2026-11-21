//! Which IRIs [`namespace_near_miss`] reads as a near miss of a namespace, class by class, and
//! which it does not: the exact spelling, another authority, another path.

use super::*;

/// The two QUDT schema namespaces and the S231 one: a `#` and a `/` namespace on one stem, and a
/// second host and path.
const QUDT_HASH: &str = "http://qudt.org/schema/qudt#";
const QUDT_SLASH: &str = "http://qudt.org/schema/qudt/";
const S231: &str = "http://data.ashrae.org/S231#";

/// `(namespace as written, local name)` of the near miss `iri` is of `namespace`.
fn split<'a>(iri: &'a str, namespace: &str) -> Option<(&'a str, &'a str)> {
    namespace_near_miss(iri, namespace).map(|miss| (miss.written, miss.local))
}

/// Assert that `written` followed by each local name is a near miss of `namespace`, split
/// exactly between the two.
fn assert_near(written: &str, namespace: &str) {
    for local in ["hasUnit", "unit", "Real", "a/b#c", "µ", ""] {
        let iri = format!("{written}{local}");
        assert_eq!(
            split(&iri, namespace),
            Some((written, local)),
            "{iri} against {namespace}"
        );
    }
}

/// Assert that `prefix` followed by each local name is no near miss of `namespace`.
fn assert_far(prefix: &str, namespace: &str) {
    for local in ["hasUnit", "unit", ""] {
        let iri = format!("{prefix}{local}");
        assert_eq!(split(&iri, namespace), None, "{iri} against {namespace}");
    }
}

#[test]
fn an_iri_in_the_namespace_exactly_is_not_a_near_miss() {
    for namespace in [QUDT_HASH, QUDT_SLASH, S231] {
        for local in ["hasUnit", "unit", "x", "a.b", "s/K", "a#b", "µ", ""] {
            let iri = format!("{namespace}{local}");
            assert_eq!(split(&iri, namespace), None, "{iri}");
        }
    }
}

#[test]
fn the_scheme_in_another_letter_case_is_a_near_miss() {
    for scheme in ["HTTP", "Http", "hTTp", "httP"] {
        assert_near(&format!("{scheme}://qudt.org/schema/qudt#"), QUDT_HASH);
        assert_near(&format!("{scheme}://qudt.org/schema/qudt/"), QUDT_SLASH);
        assert_near(&format!("{scheme}://data.ashrae.org/S231#"), S231);
    }
}

#[test]
fn http_for_https_and_the_reverse_are_near_misses() {
    assert_near("https://qudt.org/schema/qudt#", QUDT_HASH);
    assert_near("https://qudt.org/schema/qudt/", QUDT_SLASH);
    assert_near("HTTPS://qudt.org/schema/qudt#", QUDT_HASH);
    assert_near("https://data.ashrae.org/S231#", S231);
    // The reverse, for a namespace published under `https`.
    assert_near("http://example.org/v#", "https://example.org/v#");
    assert_near("HTTP://example.org/v#", "https://example.org/v#");
    assert_eq!(
        split("https://example.org/v#x", "https://example.org/v#"),
        None
    );
}

#[test]
fn another_scheme_is_matched_in_any_case_and_is_never_a_web_scheme() {
    // Outside `http` and `https` only the letter case is a near miss.
    assert_near("URN://example.org/v#", "urn://example.org/v#");
    for scheme in ["ftp", "httpx", "htt", "ws", "file", "qudt"] {
        assert_far(&format!("{scheme}://qudt.org/schema/qudt#"), QUDT_HASH);
    }
    assert_far("http://example.org/v#", "urn://example.org/v#");
    assert_far("https://example.org/v#", "ftp://example.org/v#");
}

#[test]
fn the_host_in_another_letter_case_is_a_near_miss() {
    for host in ["QUDT.org", "qudt.ORG", "QUDT.ORG", "Qudt.Org"] {
        assert_near(&format!("http://{host}/schema/qudt#"), QUDT_HASH);
        assert_near(&format!("http://{host}/schema/qudt/"), QUDT_SLASH);
    }
    assert_near("http://DATA.ASHRAE.ORG/S231#", S231);
}

#[test]
fn a_default_port_is_a_near_miss_and_any_other_port_is_another_authority() {
    assert_near("http://qudt.org:80/schema/qudt#", QUDT_HASH);
    assert_near("http://qudt.org:/schema/qudt#", QUDT_HASH);
    assert_near("https://qudt.org:443/schema/qudt#", QUDT_HASH);
    assert_near("https://qudt.org:/schema/qudt/", QUDT_SLASH);
    assert_near("HTTP://qudt.org:80/schema/qudt#", QUDT_HASH);
    assert_near("http://data.ashrae.org:80/S231#", S231);
    // The default belongs to the scheme the IRI writes, and is matched as the digits `80` or
    // `443` and nothing else.
    for authority in [
        "http://qudt.org:443",
        "https://qudt.org:80",
        "http://qudt.org:8080",
        "http://qudt.org:8",
        "http://qudt.org:080",
        "http://qudt.org:80 ",
        "http://qudt.org:80:80",
        "http://qudt.org::80",
        "http://qudt.org:+80",
        "http://qudt.org:x",
    ] {
        assert_far(&format!("{authority}/schema/qudt#"), QUDT_HASH);
    }
    // No scheme but `http` and `https` has a default this reads.
    assert_far("urn://example.org:80/v#", "urn://example.org/v#");
    assert_near("urn://example.org:/v#", "urn://example.org/v#");
}

#[test]
fn a_missing_a_wrong_or_a_repeated_separator_is_a_near_miss() {
    // None at all, the other one, and more than one in any mix.
    assert_eq!(
        split("http://qudt.org/schema/qudthasUnit", QUDT_HASH),
        Some(("http://qudt.org/schema/qudt", "hasUnit"))
    );
    assert_eq!(
        split("http://qudt.org/schema/qudt", QUDT_HASH),
        Some(("http://qudt.org/schema/qudt", ""))
    );
    for separators in ["/", "/#", "#/", "##", "//", "#/#", "/////"] {
        let written = format!("http://qudt.org/schema/qudt{separators}");
        assert_eq!(
            split(&format!("{written}hasUnit"), QUDT_HASH),
            Some((written.as_str(), "hasUnit")),
            "{separators}"
        );
    }
    for separators in ["#", "/#", "#/", "##", "//"] {
        let written = format!("http://qudt.org/schema/qudt{separators}");
        assert_eq!(
            split(&format!("{written}hasUnit"), QUDT_SLASH),
            Some((written.as_str(), "hasUnit")),
            "{separators}"
        );
    }
    assert_near("http://data.ashrae.org/S231/", S231);
    // The local name is what follows every separator, whatever it is.
    assert_eq!(
        split("http://qudt.org/schema/qudtX#hasUnit", QUDT_HASH),
        Some(("http://qudt.org/schema/qudt", "X#hasUnit"))
    );
    assert_eq!(
        split("http://qudt.org/schema/qudt?q#hasUnit", QUDT_HASH),
        Some(("http://qudt.org/schema/qudt", "?q#hasUnit"))
    );
    assert_eq!(
        split("http://data.ashrae.org/S231P#unit", S231),
        Some(("http://data.ashrae.org/S231", "P#unit"))
    );
}

#[test]
fn several_near_miss_spellings_together_are_still_one_near_miss() {
    assert_near("HTTPS://QUDT.ORG:443/schema/qudt/#", QUDT_HASH);
    assert_near("HttP://Qudt.org:80/schema/qudt", QUDT_SLASH);
    assert_near("https://DATA.ashrae.org:/S231##", S231);
}

#[test]
fn another_host_user_information_or_path_is_not_a_near_miss() {
    for prefix in [
        // Another host, one that only contains or extends the namespace's among them.
        "http://example.org/schema/qudt#",
        "http://qudt.org.example.org/schema/qudt#",
        "http://xqudt.org/schema/qudt#",
        "http://qudt.or/schema/qudt#",
        "http://qudt.org./schema/qudt#",
        "http://www.qudt.org/schema/qudt#",
        "http:///schema/qudt#",
        // User information, whichever side of it the host is on.
        "http://user@qudt.org/schema/qudt#",
        "http://qudt.org@example.org/schema/qudt#",
        "http://qudt.org:80@example.org/schema/qudt#",
        // Another path: its case, a segment more or less, a percent-encoded octet.
        "http://qudt.org/Schema/qudt#",
        "http://qudt.org/schema/QUDT#",
        "http://qudt.org/schema/qud#",
        "http://qudt.org/schema#",
        "http://qudt.org/schema/#qudt#",
        "http://qudt.org//schema/qudt#",
        "http://qudt.org/schema/qud%74#",
        "http://qudt.org/vocab/unit#",
        "http://qudt.org#schema/qudt#",
        "http://qudt.org?/schema/qudt#",
        "http://qudt.org",
    ] {
        assert_far(prefix, QUDT_HASH);
        assert_far(prefix, QUDT_SLASH);
    }
    assert_far("http://data.ashrae.org/s231#", S231);
    assert_far("http://data.ashrae.org/S23#", S231);
}

#[test]
fn what_is_no_iri_with_an_authority_is_not_a_near_miss() {
    for iri in [
        "",
        "hasUnit",
        "qudt:hasUnit",
        "unit:K",
        "http:",
        "http:/qudt.org/schema/qudt#hasUnit",
        "http//qudt.org/schema/qudt#hasUnit",
        "://qudt.org/schema/qudt#hasUnit",
        " http://qudt.org/schema/qudt#hasUnit",
        "1http://qudt.org/schema/qudt#hasUnit",
        "ht tp://qudt.org/schema/qudt#hasUnit",
        "hté://qudt.org/schema/qudt#hasUnit",
        "x:http://qudt.org/schema/qudt/#hasUnit",
        "urn:x:https://qudt.org/schema/qudt#hasUnit",
    ] {
        for namespace in [QUDT_HASH, QUDT_SLASH, S231] {
            assert_eq!(split(iri, namespace), None, "{iri:?}");
        }
    }
}

#[test]
fn a_namespace_of_another_form_has_no_near_miss() {
    let iri = "https://qudt.org/schema/qudt#hasUnit";
    for namespace in [
        "",
        "qudt",
        "qudt:",
        "http://qudt.org/schema/qudt",
        "http://qudt.org/schema/qudt?",
        "http://qudt.org/schema/qudté",
        "http://qudt.org",
        "http://qudt.org:80/schema/qudt#",
        "http://qudt.org:/schema/qudt#",
        "//qudt.org/schema/qudt#",
    ] {
        assert_eq!(split(iri, namespace), None, "{namespace:?}");
    }
    // A namespace with a port has none even for the IRI that differs from it in the scheme alone.
    assert_eq!(
        split(
            "https://qudt.org:80/schema/qudt#hasUnit",
            "http://qudt.org:80/schema/qudt#"
        ),
        None
    );
    // A namespace whose path is the root alone is still one.
    assert_eq!(
        split("HTTP://example.org/x", "http://example.org/"),
        Some(("HTTP://example.org/", "x"))
    );
    assert_eq!(
        split("http://example.org", "http://example.org/"),
        Some(("http://example.org", ""))
    );
    // The authority ends at a `#` or a `?` as it does at a `/`.
    assert_eq!(
        split("http://example.org#x", "http://example.org/"),
        Some(("http://example.org#", "x"))
    );
    assert_eq!(
        split("http://example.org?x", "http://example.org/"),
        Some(("http://example.org", "?x"))
    );
}

#[test]
fn the_two_parts_are_the_iri_cut_in_two_whatever_it_holds() {
    // Characters of every width in the local name, and a megabyte of it: the cut is on a
    // character boundary and nothing panics.
    let long = "hasUnit".repeat(1 << 17);
    for local in [
        "µ",
        "😀x",
        "\u{200B}hasUnit",
        " hasUnit",
        "é/é#é",
        long.as_str(),
    ] {
        for written in [
            "https://qudt.org/schema/qudt#",
            "http://QUDT.org/schema/qudt",
            "http://qudt.org:80/schema/qudt/#",
        ] {
            let iri = format!("{written}{local}");
            let miss = namespace_near_miss(&iri, QUDT_HASH).expect("a near miss");
            assert_eq!((miss.written, miss.local), (written, local));
            assert_eq!(format!("{}{}", miss.written, miss.local), iri);
        }
    }
    // A megabyte of separators is a near miss too, read in one pass.
    let separators = "/#".repeat(1 << 19);
    let iri = format!("http://qudt.org/schema/qudt{separators}hasUnit");
    let miss = namespace_near_miss(&iri, QUDT_HASH).expect("a near miss");
    assert_eq!(miss.local, "hasUnit");
    assert_eq!(miss.written.len(), iri.len() - "hasUnit".len());
    // Multi-byte text where the scheme, the host and the path are read is no match and no panic.
    for iri in [
        "é://qudt.org/schema/qudt#hasUnit",
        "http://qüdt.org/schema/qudt#hasUnit",
        "http://qudt.org/schéma/qudt#hasUnit",
        "http://qudt.org/schema/qud\u{1F600}#hasUnit",
        "http://qudt.org:８０/schema/qudt#hasUnit",
    ] {
        assert_eq!(split(iri, QUDT_HASH), None, "{iri}");
    }
}
