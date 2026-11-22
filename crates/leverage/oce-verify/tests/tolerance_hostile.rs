//! Hostile vector files that use the tolerance keys never panic, at load or in a run: every
//! prefix of a document with every tolerance key, every single-byte change to it, and a seeded
//! sweep of mutated documents each give a set that survives its own canonical form or a pinned
//! load code, and every set that loads runs against a subject to a receipt or a pinned bind or
//! run code, under a frame budget that keeps the sweep's work bounded.

mod support;

use oce_verify::{
    VECTOR_ERROR_CODES, VERIFY_ERROR_CODES, VectorInputs, VectorSet, VerifyLimits, VerifyOptions,
    run_vectors,
};

use support::{Sequence, fixture_bytes, subject};

/// A set on the `signals` subject with every tolerance key, both window forms masked, an
/// expected step between ticks and the rounding case of decision 41 one digit away.
const TOLERANT: &str = r#"{"schema": "oce/vectors/v1", "envelope": "funnel",
 "clock": {"step_s": 60, "horizon_s": 600},
 "scenarios": [{"name": "s",
   "inputs": {"ahu.T": [{"t": 0, "value": 20.0}, {"t": 300, "value": 22.0}], "zone.T": 2.0,
              "n": 3, "b": [{"t": 0, "value": false}, {"t": 120, "value": true}], "lim": 5.0},
   "expect": [{"output": "ahu.y", "from_s": 0, "to_s": 600,
               "within": {"atolx": 60, "atoly": 0.25, "ltolx": 0.5, "ltoly": 0.01, "rtolx": 0.1, "rtoly": 0.02},
               "of": [{"t": 0, "value": 20}, {"t": 290, "value": 22.0}], "when": ["b"]},
              {"output": "ratio", "from_s": 60, "to_s": 600, "within": {"atolx": 60}, "of": 1e-1},
              {"output": "bOut", "from_s": 0, "to_s": 600, "equals": true, "when": ["bOut", "b"]}]}]}"#;

/// Load `bytes`: a pinned code, or a set whose canonical form is a fixed point.
fn judge(bytes: &[u8]) -> Result<VectorSet, &'static str> {
    match VectorSet::from_json_bytes(bytes, &VerifyLimits::default()) {
        Err(error) => {
            assert!(
                VECTOR_ERROR_CODES.contains(&error.code()),
                "{}",
                error.code()
            );
            assert!(!error.subject().is_empty() && !error.detail().is_empty());
            Err(error.code())
        }
        Ok(set) => {
            let written = set.to_canonical_json();
            let again = VectorSet::from_json_bytes(written.as_bytes(), &VerifyLimits::default())
                .unwrap_or_else(|error| panic!("the canonical form does not load: {error}"));
            assert_eq!(again, set);
            assert_eq!(again.to_canonical_json(), written);
            Ok(set)
        }
    }
}

/// Run a loaded set against `signals` under a 2,000-frame budget: a receipt, or a pinned code.
fn run_judged(set: &VectorSet, signals: &[u8]) -> &'static str {
    let options = VerifyOptions {
        limits: VerifyLimits {
            max_total_frames: 2_000,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    match run_vectors(set, &VectorInputs::new(signals), &options) {
        Ok(run) => {
            assert_eq!(
                run.receipt.header.receipt_id,
                run.receipt.computed_receipt_id()
            );
            "receipt"
        }
        Err(error) => {
            assert!(
                VERIFY_ERROR_CODES.contains(&error.code()),
                "{}",
                error.code()
            );
            error.code()
        }
    }
}

#[test]
fn the_tolerant_document_loads_and_runs() {
    let set = judge(TOLERANT.as_bytes()).expect("loads");
    assert_eq!(run_judged(&set, &subject("signals")), "receipt");
}

#[test]
fn every_proper_prefix_of_the_tolerant_document_is_refused() {
    let bytes = TOLERANT.as_bytes();
    for end in 0..bytes.len() {
        assert_eq!(judge(&bytes[..end]).err(), Some("json-syntax"), "{end}");
    }
}

#[test]
fn every_single_byte_change_is_judged_and_every_loaded_set_runs_without_a_panic() {
    let signals = subject("signals");
    let bytes = TOLERANT.as_bytes();
    let mut outcomes = std::collections::BTreeMap::new();
    for at in 0..bytes.len() {
        for replacement in [
            b' ', b'0', b'9', b'"', b'}', b']', b',', b'-', b'e', b'.', 0xff,
        ] {
            if bytes[at] == replacement {
                continue;
            }
            let mut changed = bytes.to_vec();
            changed[at] = replacement;
            let outcome = match judge(&changed) {
                Ok(set) => run_judged(&set, &signals),
                Err(code) => code,
            };
            *outcomes.entry(outcome).or_insert(0_u32) += 1;
        }
    }
    let count = |code: &str| outcomes.get(code).copied().unwrap_or(0);
    assert!(count("receipt") > 100, "{outcomes:?}");
    for reached in [
        "invalid-tolerance",
        "of-not-from-window-start",
        "unknown-envelope",
        "unknown-key",
        "when-unknown",
    ] {
        assert!(count(reached) > 0, "{reached}: {outcomes:?}");
    }
}

/// The bytes a mutation draws from: structure, digits, signs, an exponent, the letters of the
/// tolerance keys, and bytes that are not UTF-8.
const ALPHABET: &[u8] = b"{}[]:,\"\\ 0123456789-+.eEatolxyrwhn_\x00\xff";

fn mutate(seed: &[u8], random: &mut Sequence) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    for _ in 0..=random.below(3) {
        let at = random.below(bytes.len() + 1);
        let byte = ALPHABET[random.below(ALPHABET.len())];
        match random.below(4) {
            0 if at < bytes.len() => bytes[at] = byte,
            1 if at < bytes.len() => {
                bytes.remove(at);
            }
            2 if at < bytes.len() => {
                let end = (at + 1 + random.below(30)).min(bytes.len());
                let slice = bytes[at..end].to_vec();
                bytes.splice(at..at, slice);
            }
            _ => bytes.insert(at.min(bytes.len()), byte),
        }
    }
    bytes
}

#[test]
fn a_seeded_sweep_of_mutated_tolerance_documents_never_panics() {
    let signals = subject("signals");
    let seeds = [
        TOLERANT.as_bytes().to_vec(),
        fixture_bytes("sets/tolerance_every_key.json"),
        fixture_bytes("receipts/tolerance_fail.vectors.json"),
        fixture_bytes("receipts/tolerance_masked.vectors.json"),
    ];
    let mut random = Sequence(0x0684);
    let mut outcomes = std::collections::BTreeMap::new();
    for round in 0..3_000 {
        let mutated = mutate(&seeds[round % seeds.len()], &mut random);
        let outcome = match judge(&mutated) {
            Ok(set) => run_judged(&set, &signals),
            Err(code) => code,
        };
        *outcomes.entry(outcome).or_insert(0_u32) += 1;
    }
    let count = |code: &str| outcomes.get(code).copied().unwrap_or(0);
    assert!(count("receipt") > 50, "{outcomes:?}");
    assert!(outcomes.len() >= 10, "{outcomes:?}");
    assert_eq!(outcomes.values().sum::<u32>(), 3_000);
}
