//! The canonical form and `spec_id` of a commissioning spec (commissioning note, "Test-spec
//! file"; decision 20): byte goldens derived outside this crate, the load, write, load fixed
//! point, and what does and does not move an id.
//!
//! The goldens under `tests/fixtures/commissioning/canonical/` and the ids below were produced
//! by an independent implementation of comment removal, of the canonical form and of FNV-1a-128
//! (`tests/fixtures/commissioning/derive_canonical.py`: a regular expression, `json`, `repr`
//! and arbitrary-precision integers), not by `to_canonical_json`. They are never blessed.

use oce_verify::{COMMISSIONING_SPEC_SCHEMA, PointMapping, SpecLimits};

use crate::{fixture_text, load_mapping, loaded, one_reference, top_level};

/// A source document, its canonical golden and its `spec_id`.
struct Golden {
    source: &'static str,
    canonical: &'static str,
    spec_id: &'static str,
}

/// The Building 33 id, as `derive_canonical.py` printed it.
const BUILDING_33_ID: &str = "spec:fnv1a128:5d2f299203c502aec8285cdae089d2cd";

/// The id of Listing 12.1, verbatim and commented alike.
const LISTING_12_1_ID: &str = "spec:fnv1a128:524cd51137da28089cced9838c1c6b1b";

const GOLDENS: &[Golden] = &[
    Golden {
        source: "listing_12_1.json",
        canonical: "canonical/listing_12_1.json",
        spec_id: LISTING_12_1_ID,
    },
    Golden {
        source: "listing_12_1_commented.json",
        canonical: "canonical/listing_12_1_commented.json",
        spec_id: LISTING_12_1_ID,
    },
    Golden {
        source: "building_33.json",
        canonical: "canonical/building_33.json",
        spec_id: BUILDING_33_ID,
    },
    Golden {
        source: "every_field.json",
        canonical: "canonical/every_field.json",
        spec_id: "spec:fnv1a128:6c63445203167bda177e703930f3c888",
    },
];

/// FNV-1a-128, written again here so the ids are checked against something other than the
/// crate's own hash.
fn reference_hash(bytes: &[u8]) -> u128 {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

#[test]
fn each_spec_is_written_as_its_independently_derived_golden() {
    for golden in GOLDENS {
        let expected = fixture_text(golden.canonical);
        let written = loaded(&fixture_text(golden.source)).to_canonical_json();
        assert!(
            written == expected,
            "{}: the canonical form differs from its golden\n--- expected\n{expected}\n--- written\n{written}",
            golden.source
        );
    }
}

#[test]
fn each_spec_id_is_the_pinned_id_and_the_hash_of_the_golden_bytes() {
    for golden in GOLDENS {
        let spec = loaded(&fixture_text(golden.source));
        assert_eq!(spec.spec_id(), golden.spec_id, "{}", golden.source);
        // The id is the hash of the golden's own bytes, by a second implementation.
        let bytes = fixture_text(golden.canonical);
        assert_eq!(
            format!("spec:fnv1a128:{:032x}", reference_hash(bytes.as_bytes())),
            golden.spec_id,
            "{}",
            golden.source
        );
        assert_eq!(golden.spec_id.len(), "spec:fnv1a128:".len() + 32);
    }
}

#[test]
fn the_canonical_form_is_strict_ascii_with_the_schema_tag_and_no_comment() {
    for golden in GOLDENS {
        let written = loaded(&fixture_text(golden.source)).to_canonical_json();
        assert!(written.is_ascii(), "{}", golden.source);
        assert!(written.ends_with("}\n") && !written.ends_with("\n\n"));
        assert!(!written.contains('\t') && !written.contains('\r'));
        assert!(
            written.contains(&format!("\n  \"schema\": \"{COMMISSIONING_SPEC_SCHEMA}\"")),
            "{}",
            golden.source
        );
        // Strict JSON by a reader this crate does not own: nothing of a comment is left.
        let parsed: serde_json::Value =
            serde_json::from_str(&written).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(parsed["schema"], COMMISSIONING_SPEC_SCHEMA);
    }
    // The commented copy's markers are gone, and markers inside a string are kept.
    let commented = loaded(&fixture_text("listing_12_1_commented.json")).to_canonical_json();
    assert!(!commented.contains("//") && !commented.contains("/*"));
    let every = loaded(&fixture_text("every_field.json")).to_canonical_json();
    assert!(every.contains("Buildings./*not a comment*/ \\u00c9cole // nor this \\ud83c\\udf21"));
}

#[test]
fn load_write_load_is_a_fixed_point_and_two_loads_give_one_id() {
    for golden in GOLDENS {
        let source = fixture_text(golden.source);
        let first = loaded(&source);
        let written = first.to_canonical_json();
        let second = loaded(&written);
        assert_eq!(second, first, "{}", golden.source);
        assert_eq!(second.to_canonical_json(), written, "{}", golden.source);
        assert_eq!(second.spec_id(), first.spec_id());
        assert_eq!(second.findings(), first.findings());
        // Loading the same bytes again is the same spec and the same id.
        let again = loaded(&source);
        assert_eq!(again, first);
        assert_eq!(again.spec_id(), golden.spec_id);
    }
}

#[test]
fn the_commented_copy_is_the_listing_and_its_canonical_golden_is_the_same_file() {
    let verbatim = loaded(&fixture_text("listing_12_1.json"));
    let commented = loaded(&fixture_text("listing_12_1_commented.json"));
    assert_eq!(commented, verbatim);
    assert_eq!(
        fixture_text("canonical/listing_12_1_commented.json"),
        fixture_text("canonical/listing_12_1.json")
    );
    // The two source files do differ: the copy is the listing plus comments.
    assert_ne!(
        fixture_text("listing_12_1_commented.json"),
        fixture_text("listing_12_1.json")
    );
}

#[test]
fn a_missing_schema_and_a_written_one_are_one_spec() {
    let lbl = top_level("\"sampling\": 120");
    let tagged = top_level("\"sampling\": 120, \"schema\": \"oce/commissioning-spec/v1\"");
    assert_eq!(loaded(&lbl), loaded(&tagged));
    assert_eq!(loaded(&lbl).spec_id(), loaded(&tagged).spec_id());
    assert_eq!(
        loaded(&lbl).to_canonical_json(),
        "{\n  \"references\": [],\n  \"sampling\": 120,\n  \"schema\": \"oce/commissioning-spec/v1\"\n}\n"
    );
}

#[test]
fn spelling_does_not_move_an_id() {
    let plain = one_reference(
        "\"cxf\": \"a/b.json\", \"parameters\": {\"b.k\": 1.5, \"a.k\": 2}, \"window\": {\"start\": 0, \"stop\": 60}",
    );
    let id = loaded(&plain).spec_id();
    for respelled in [
        // Key order of an object, whitespace and comments.
        "{\"references\": [{ \"window\": {\"stop\": 60, \"start\": 0},\n\t\"parameters\": {\"a.k\": 2, \"b.k\": 1.5}, // edits\n \"cxf\": \"a/b.json\", \"sequence\": \"s\", \"model\": \"M\" }] /* end */ }".to_owned(),
        // Escapes, and another spelling of the same real.
        one_reference(
            "\"cxf\": \"a\\/b.j\\u0073on\", \"parameters\": {\"b.k\": 15e-1, \"a.k\": 2}, \"window\": {\"start\": 0, \"stop\": 60}",
        ),
        // `-0` is the integer zero.
        one_reference(
            "\"cxf\": \"a/b.json\", \"parameters\": {\"b.k\": 1.5, \"a.k\": 2}, \"window\": {\"start\": -0, \"stop\": 60}",
        ),
    ] {
        assert_eq!(loaded(&respelled).spec_id(), id, "{respelled}");
    }
}

#[test]
fn what_a_spec_says_moves_its_id() {
    let base = one_reference("\"sampling\": 60");
    let id = loaded(&base).spec_id();
    for changed in [
        // A number's kind is part of the spec: 60 is not 60.0.
        one_reference("\"sampling\": 60.0"),
        one_reference("\"sampling\": 61"),
        // An absent list is not an empty one.
        one_reference("\"sampling\": 60, \"inputs\": []"),
        one_reference("\"sampling\": 60, \"outputs\": {}"),
        one_reference("\"sampling\": 60, \"parameters\": {}"),
        // A field the verifier ignores is still part of the document.
        one_reference("\"sampling\": 60, \"generateJson\": false"),
        // -0.0 is not 0.0 as written.
        one_reference("\"sampling\": 60, \"window\": {\"start\": -0.0, \"stop\": 1}"),
    ] {
        assert_ne!(loaded(&changed).spec_id(), id, "{changed}");
    }
    let signed = one_reference("\"sampling\": 60, \"window\": {\"start\": -0.0, \"stop\": 1}");
    let unsigned = one_reference("\"sampling\": 60, \"window\": {\"start\": 0.0, \"stop\": 1}");
    assert_ne!(loaded(&signed).spec_id(), loaded(&unsigned).spec_id());
    // The value of an ignored field moves the id too, at any depth.
    let one = top_level("\"controller\": {\"deviceId\": 240001}");
    let other = top_level("\"controller\": {\"deviceId\": 240002}");
    assert_ne!(loaded(&one).spec_id(), loaded(&other).spec_id());
}

#[test]
fn the_order_of_a_files_keys_is_no_part_of_a_spec_the_override_maps_included() {
    // Which override applies is decided by what the keys are, never by where the file writes
    // them, so the canonical form sorts the two override maps like every other object, and two
    // files that differ only in key order are one spec with one id.
    let forward = one_reference(
        "\"outputs\": {\"s.y*\": {\"atoly\": 1}, \"s.a\": {\"atoly\": 2}}, \"indicators\": {\"s.z\": [\"s.u\"], \"s.b\": []}",
    );
    let backward = one_reference(
        "\"indicators\": {\"s.b\": [], \"s.z\": [\"s.u\"]}, \"outputs\": {\"s.a\": {\"atoly\": 2}, \"s.y*\": {\"atoly\": 1}}",
    );
    let (first, second) = (loaded(&forward), loaded(&backward));
    assert_eq!(first, second);
    assert_eq!(first.spec_id(), second.spec_id());
    assert_eq!(first.to_canonical_json(), second.to_canonical_json());
    let keys = |spec: &oce_verify::CommissioningSpec| -> Vec<String> {
        spec.references()[0]
            .outputs()
            .iter()
            .map(|each| each.key().to_owned())
            .collect()
    };
    assert_eq!(keys(&first), ["s.a", "s.y*"], "sorted by bytes");
    assert_eq!(keys(&second), ["s.a", "s.y*"]);
    let written = first.to_canonical_json();
    let exact = written.find("\"s.a\"").expect("the exact key");
    let glob = written.find("\"s.y*\"").expect("the glob key");
    assert!(
        exact < glob,
        "`s.a` sorts first, though the file writes it second\n{written}"
    );
    let rules = written.find("\"s.b\"").expect("a rule's key");
    assert!(
        rules < written.find("\"s.z\"").expect("the other"),
        "{written}"
    );
    assert_eq!(keys(&loaded(&written)), ["s.a", "s.y*"]);
    // A list is not a map: the order of an indicator rule's names, of the sources and of the
    // references is the file's, and is part of the id.
    let one = one_reference("\"indicators\": {\"s.y\": [\"a\", \"b\"]}");
    let other = one_reference("\"indicators\": {\"s.y\": [\"b\", \"a\"]}");
    assert_ne!(loaded(&one).spec_id(), loaded(&other).spec_id());
    // Every other map is a set of entries too.
    let one = one_reference("\"parameters\": {\"b\": 1, \"a\": 2}");
    let other = one_reference("\"parameters\": {\"a\": 2, \"b\": 1}");
    assert_eq!(loaded(&one).spec_id(), loaded(&other).spec_id());
}

#[test]
fn the_mapping_listing_is_written_as_its_independently_derived_golden() {
    let mapping = load_mapping(&fixture_text("listing_12_2.json")).expect("Listing 12.2 loads");
    let written = mapping.to_canonical_json();
    let expected = fixture_text("canonical/listing_12_2.json");
    assert!(
        written == expected,
        "--- expected\n{expected}\n--- written\n{written}"
    );
    assert_eq!(
        format!("spec:fnv1a128:{:032x}", reference_hash(written.as_bytes())),
        "spec:fnv1a128:e6cbed1cb8c587524a3ad8f64b131d88"
    );
    // A fixed point, and file order is kept: a mapping is a list.
    let again = PointMapping::from_json_str(&written, &SpecLimits::default()).expect("loads");
    assert_eq!(again, mapping);
    assert_eq!(again.to_canonical_json(), written);
    assert_eq!(mapping.points()[0].cdl().name(), "TZonCooSetOcc");
    assert_eq!(mapping.points()[9].cdl().name(), "TZonHeaSet");
}

#[test]
fn an_inline_mapping_is_written_as_the_mapping_file_is() {
    let listing = fixture_text("listing_12_2.json");
    let spec = loaded(&one_reference(&format!("\"pointNameMapping\": {listing}")));
    let written = spec.to_canonical_json();
    // The mapping's canonical form is in the spec's, six spaces deeper, after its key.
    let mapping = load_mapping(&listing).expect("loads").to_canonical_json();
    let nested: Vec<String> = mapping
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                format!("      \"pointNameMapping\": {line}")
            } else {
                format!("      {line}")
            }
        })
        .collect();
    assert!(written.contains(&nested.join("\n")), "{mapping}\n{written}");
    assert_eq!(loaded(&written), spec);
}
