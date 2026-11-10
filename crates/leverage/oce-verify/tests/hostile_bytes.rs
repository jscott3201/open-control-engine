//! Hostile input never panics and never half-loads: every prefix of a valid document, every
//! single-byte change to one, and a seeded sweep of 10,000 mutated documents each give either a
//! set that survives its own canonical form or one of the pinned codes.

mod support;

use oce_verify::{VECTOR_ERROR_CODES, VectorSet, VerifyLimits};

use support::{LIBRARY_RULES, Sequence, fixture_bytes, library_vectors};

/// Load `bytes` and hold the outcome to the contract: a pinned code with a subject and a
/// sentence, or a set whose canonical form is a fixed point. Returns the code, or `loaded`.
fn judge(bytes: &[u8]) -> &'static str {
    match VectorSet::from_json_bytes(bytes, &VerifyLimits::default()) {
        Err(error) => {
            assert!(
                VECTOR_ERROR_CODES.contains(&error.code()),
                "{}",
                error.code()
            );
            assert!(!error.subject().is_empty() && !error.detail().is_empty());
            error.code()
        }
        Ok(set) => {
            let written = set.to_canonical_json();
            let again = VectorSet::from_json_bytes(written.as_bytes(), &VerifyLimits::default())
                .unwrap_or_else(|error| panic!("the canonical form does not load: {error}"));
            assert_eq!(again, set);
            assert_eq!(again.to_canonical_json(), written);
            "loaded"
        }
    }
}

const SMALL: &str = r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 0.5, "horizon_s": 2},
 "params": {"a.k": -0.0},
 "scenarios": [{"name": "sé", "description": "dé", "params": {"b.k": 1e3},
   "inputs": {"u": [{"t": 0, "value": 1}, {"t": 1.25, "value": true}], "v": -7},
   "expect": [{"output": "y", "from_s": 0, "to_s": 2, "equals": false},
              {"diagnostic": {"block": "B", "message": "m\n"}, "at_s": 1.5}]}]}"#;

#[test]
fn the_small_document_loads() {
    assert_eq!(judge(SMALL.as_bytes()), "loaded");
}

#[test]
fn every_proper_prefix_of_a_document_is_refused() {
    let bytes = SMALL.as_bytes();
    let mut codes = std::collections::BTreeSet::new();
    for end in 0..bytes.len() {
        let code = judge(&bytes[..end]);
        assert_ne!(code, "loaded", "the first {end} bytes are no document");
        codes.insert(code);
    }
    // A cut lands in the middle of the JSON, or in the middle of the one two-byte character.
    assert_eq!(
        codes.into_iter().collect::<Vec<_>>(),
        ["json-syntax", "not-utf8"]
    );
}

#[test]
fn every_single_byte_change_is_judged_without_a_panic() {
    let bytes = SMALL.as_bytes();
    let (mut loaded, mut refused) = (0_u32, 0_u32);
    for at in 0..bytes.len() {
        for replacement in [
            b' ', b'0', b'9', b'"', b'{', b'}', b'[', b']', b',', b':', b'-', b'e', b'.', b'\\',
            0x00, 0x7f, 0xff,
        ] {
            if bytes[at] == replacement {
                continue;
            }
            let mut changed = bytes.to_vec();
            changed[at] = replacement;
            if judge(&changed) == "loaded" {
                loaded += 1;
            } else {
                refused += 1;
            }
        }
        // And the byte removed.
        let mut shorter = bytes.to_vec();
        shorter.remove(at);
        if judge(&shorter) == "loaded" {
            loaded += 1;
        } else {
            refused += 1;
        }
    }
    // Most changes break the document; some (a digit in a number, a letter in a name, a space)
    // leave another valid set.
    assert!(refused > 5_000, "refused {refused}");
    assert!(loaded > 100, "loaded {loaded}");
}

/// The bytes a mutation draws from: structure, digits, signs, an exponent, escapes, whitespace,
/// letters that begin a word, and bytes that are not UTF-8 or are control characters.
const ALPHABET: &[u8] = b"{}[]:,\"\\ \n\t0123456789-+.eEtrufalsn/_\x00\x1f\x7f\x80\xc2\xff";

fn mutate(seed: &[u8], random: &mut Sequence) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    for _ in 0..=random.below(4) {
        let at = random.below(bytes.len() + 1);
        let byte = ALPHABET[random.below(ALPHABET.len())];
        match random.below(5) {
            0 if at < bytes.len() => bytes[at] = byte,
            1 if at < bytes.len() => {
                bytes.remove(at);
            }
            2 => bytes.insert(at, byte),
            // Repeat a slice: duplicated members, doubled lists, runaway nesting.
            3 if at < bytes.len() => {
                let end = (at + 1 + random.below(40)).min(bytes.len());
                let slice = bytes[at..end].to_vec();
                bytes.splice(at..at, slice);
            }
            // Cut a slice out.
            _ if at < bytes.len() => {
                let end = (at + 1 + random.below(40)).min(bytes.len());
                bytes.drain(at..end);
            }
            _ => bytes.push(byte),
        }
    }
    bytes
}

#[test]
fn ten_thousand_mutated_documents_are_each_a_set_or_a_pinned_refusal() {
    let mut seeds: Vec<Vec<u8>> = vec![
        SMALL.as_bytes().to_vec(),
        fixture_bytes("sets/every_field.json"),
        fixture_bytes("canonical/every_field.json"),
    ];
    seeds.extend(LIBRARY_RULES.iter().map(|rule| library_vectors(rule)));
    let mut random = Sequence(0x0519);
    let mut seen = std::collections::BTreeMap::new();
    for round in 0..10_000 {
        let mutated = mutate(&seeds[round % seeds.len()], &mut random);
        *seen.entry(judge(&mutated)).or_insert(0_u32) += 1;
    }
    // The sweep reached well past the syntax layer: sets that still load, shape refusals and
    // rule refusals all occurred.
    let count = |code: &str| seen.get(code).copied().unwrap_or(0);
    assert!(count("loaded") > 300, "{seen:?}");
    assert!(count("json-syntax") > 2_000, "{seen:?}");
    for reached in [
        "not-utf8",
        "control-character",
        "duplicate-key",
        "unknown-key",
        "missing-key",
        "unknown-schema",
        "step-times-not-increasing",
    ] {
        assert!(count(reached) >= 10, "{reached} hardly occurred: {seen:?}");
    }
    assert!(seen.len() >= 12, "{seen:?}");
    assert_eq!(seen.values().sum::<u32>(), 10_000);
}

#[test]
fn the_same_seed_judges_the_same_way_twice() {
    let run = || {
        let mut random = Sequence(7);
        (0..500)
            .map(|_| judge(&mutate(SMALL.as_bytes(), &mut random)))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
