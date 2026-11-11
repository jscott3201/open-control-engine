//! Shared test support: the golden switch, fixture paths, the two in-repo rules (their vector
//! files and their CXF subjects, read in place), small document builders and runs.
// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::path::PathBuf;

use oce_verify::{
    Receipt, ReceiptBody, ScenarioReceipt, VectorBody, VectorError, VectorInputs, VectorRun,
    VectorSet, VerifyError, VerifyLimits, VerifyOptions, run_vectors,
};

/// The golden switch for this crate (see `.agents/project-facts.md`). It rewrites the published
/// schema and the golden receipts under `tests/fixtures/receipts/`; the canonical goldens are
/// derived outside the crate and are never blessed.
///
/// The mutant set and its mutants under `tests/fixtures/AHU-0016/` are hand-written, never
/// blessed.
pub const BLESS_SWITCH: &str = "OCE_BLESS_VERIFY";

/// The crate root.
pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A path under `tests/fixtures/`.
pub fn fixture(relative: &str) -> PathBuf {
    crate_dir().join("tests/fixtures").join(relative)
}

/// The bytes of a fixture file.
pub fn fixture_bytes(relative: &str) -> Vec<u8> {
    let path = fixture(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The two in-repo library rules whose vector files this crate reads in place.
pub const LIBRARY_RULES: [&str; 2] = ["AHU-0016", "AHU-0004"];

/// The `vectors.json` of an in-repo library rule, read in place from the facade's fixtures.
pub fn library_vectors(rule: &str) -> Vec<u8> {
    let path = crate_dir()
        .join("../../oce-api/tests/fixtures/library_rules")
        .join(rule)
        .join("vectors.json");
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The CXF subject of an in-repo library rule, read in place from the facade's fixtures.
pub fn library_subject(rule: &str) -> Vec<u8> {
    let path = crate_dir()
        .join("../../oce-api/tests/fixtures/library_rules")
        .join(rule)
        .join("rule.cxf.jsonld");
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// A test subject under `tests/fixtures/subjects/`, by name (`signals`, `hysteresis`).
pub fn subject(name: &str) -> Vec<u8> {
    fixture_bytes(&format!("subjects/{name}.cxf.json"))
}

/// The canonical path prefix of the `signals` subject's root.
pub const SIGNALS: &str = "urn:verify:signals#top.";

/// The `signals` subject's five boundary inputs at plain values, as the body of an `inputs`
/// object: `zone.T / ahu.T` is `ratio`, and each input but `zone.T` and `ahu.T` passes through to
/// one output (`n` to `nOut`, `b` to `bOut`, `lim` to `limOut`), while `zone.T` and `ahu.T` also
/// pass through to `zone.y` and `ahu.y`. `b` drives a `CDL.Utilities.Assert` that warns
/// `b is false` on every frame where it is false. `lim` is bounded to `[0, 10]`.
pub const SIGNALS_INPUTS: &str = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#;

/// The canonical path prefix of the `hysteresis` subject's root.
pub const HYSTERESIS: &str = "urn:verify:hysteresis#top.";

/// The canonical path prefix of AHU-0016's root.
pub const AHU_0016: &str = "urn:cxf-library:ahu-fc-050#ahu_fc_050.";

/// The AHU-0016 vector set with its three mutants (`tests/fixtures/AHU-0016/vectors.json`).
pub fn mutant_set() -> String {
    String::from_utf8(fixture_bytes("AHU-0016/vectors.json")).expect("UTF-8")
}

/// The bytes of one of AHU-0016's three mutants, by name (`threshold_inclusive`,
/// `no_persistence`, `late_alarm`).
pub fn mutant(name: &str) -> Vec<u8> {
    fixture_bytes(&format!("AHU-0016/mutants/{name}.cxf.json"))
}

/// The three mutants' listed paths and bytes, in the set's order.
pub fn ahu_mutants() -> Vec<(String, Vec<u8>)> {
    ["threshold_inclusive", "no_persistence", "late_alarm"]
        .into_iter()
        .map(|name| (format!("mutants/{name}.cxf.json"), mutant(name)))
        .collect()
}

/// `bytes` with every occurrence of each `(old, new)` pair replaced, in order: a mutant written
/// as an edit of a subject. Panics when an `old` does not occur, so an edit never silently does
/// nothing.
pub fn edited(bytes: &[u8], edits: &[(&str, &str)]) -> Vec<u8> {
    let mut text = String::from_utf8(bytes.to_vec()).expect("UTF-8");
    for (old, new) in edits {
        assert!(
            text.contains(old),
            "the edit's text `{old}` is in the document"
        );
        text = text.replace(old, new);
    }
    text.into_bytes()
}

/// AHU-0016's rule with its fault output's driver turned into a Real: a `BooleanToReal` between
/// the persistence timer and `yFault`, which becomes a `RealOutput` at the same path.
pub fn ahu_real_fault() -> Vec<u8> {
    let ns = "urn:cxf-library:ahu-fc-050#ahu_fc_050";
    edited(
        &library_subject("AHU-0016"),
        &[
            (
                &format!(r#"{{ "@id": "{ns}.persist" }}"#),
                &format!(
                    r#"{{ "@id": "{ns}.persist" }},
        {{ "@id": "{ns}.toReal" }}"#
                ),
            ),
            (
                &format!(
                    r#""S231:isConnectedTo": {{ "@id": "{ns}.yFault" }}
    }},"#
                ),
                &format!(
                    r#""S231:isConnectedTo": {{ "@id": "{ns}.toReal.u" }}
    }},
    {{
      "@id": "{ns}.toReal",
      "@type": "urn:cxf-library:ahu-fc-050#Buildings.Controls.OBC.CDL.Conversions.BooleanToReal",
      "S231:label": "toReal",
      "S231:hasInput": {{ "@id": "{ns}.toReal.u" }},
      "S231:hasOutput": {{ "@id": "{ns}.toReal.y" }}
    }},
    {{
      "@id": "{ns}.toReal.u",
      "@type": "S231:BooleanInput",
      "S231:isOfDataType": {{ "@id": "S231:Boolean" }}
    }},
    {{
      "@id": "{ns}.toReal.y",
      "@type": "S231:RealOutput",
      "S231:isOfDataType": {{ "@id": "S231:Real" }},
      "S231:isConnectedTo": {{ "@id": "{ns}.yFault" }}
    }},"#
                ),
            ),
            (
                &format!(
                    r#""@id": "{ns}.yFault",
      "@type": "S231:BooleanOutput",
      "S231:isOfDataType": {{ "@id": "S231:Boolean" }}"#
                ),
                &format!(
                    r#""@id": "{ns}.yFault",
      "@type": "S231:RealOutput",
      "S231:isOfDataType": {{ "@id": "S231:Real" }}"#
                ),
            ),
        ],
    )
}

/// Run the set `text` against `subject` and the mutants `mutants` (path and bytes) with
/// `options`.
pub fn run_mutants_with(
    text: &str,
    subject: &[u8],
    mutants: &[(String, Vec<u8>)],
    options: &VerifyOptions,
) -> Result<VectorRun, VerifyError> {
    let set = load(text).unwrap_or_else(|error| panic!("the set loads: {error}\n{text}"));
    let pairs: Vec<(&str, &[u8])> = mutants
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    run_vectors(
        &set,
        &VectorInputs::new(subject).with_mutants(&pairs),
        options,
    )
}

/// Run the set `text` against `subject` and the mutants `mutants` with the default options.
pub fn run_mutants(
    text: &str,
    subject: &[u8],
    mutants: &[(String, Vec<u8>)],
) -> Result<VectorRun, VerifyError> {
    run_mutants_with(text, subject, mutants, &VerifyOptions::default())
}

/// Run the set `text` against `subject` with `options`.
pub fn run_with(
    text: &str,
    subject: &[u8],
    options: &VerifyOptions,
) -> Result<VectorRun, VerifyError> {
    let set = load(text).unwrap_or_else(|error| panic!("the set loads: {error}\n{text}"));
    run_vectors(&set, &VectorInputs::new(subject), options)
}

/// Run the set `text` against `subject` with the default options.
pub fn run(text: &str, subject: &[u8]) -> Result<VectorRun, VerifyError> {
    run_with(text, subject, &VerifyOptions::default())
}

/// The receipt of a run that must give one.
pub fn receipt(text: &str, subject: &[u8]) -> Receipt {
    run(text, subject)
        .unwrap_or_else(|error| {
            panic!(
                "the run gives a receipt: {} at {}: {error}",
                error.code(),
                error.subject()
            )
        })
        .receipt
}

/// The refusal of a run that must give none.
pub fn refused(text: &str, subject: &[u8]) -> VerifyError {
    match run(text, subject) {
        Ok(run) => panic!(
            "the run was meant to be refused\n{}",
            run.receipt.to_canonical_json()
        ),
        Err(error) => error,
    }
}

/// The vector body of a receipt.
pub fn body(receipt: &Receipt) -> &VectorBody {
    match &receipt.body {
        ReceiptBody::Vectors(body) => body,
        _ => panic!("not a vector receipt"),
    }
}

/// The one scenario of a receipt's body.
pub fn only_scenario(receipt: &Receipt) -> &ScenarioReceipt {
    let scenarios = &body(receipt).scenarios;
    assert_eq!(scenarios.len(), 1, "one scenario");
    &scenarios[0]
}

/// Load `text` under the default limits.
pub fn load(text: &str) -> Result<VectorSet, VectorError> {
    VectorSet::from_json_bytes(text.as_bytes(), &VerifyLimits::default())
}

/// The code `text` is refused with under the default limits, or `loaded`.
pub fn code_of(text: &str) -> &'static str {
    match load(text) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

/// The code and subject `text` is refused with under the default limits.
pub fn refusal(text: &str) -> (&'static str, String) {
    let error = load(text).expect_err("the document is refused");
    (error.code(), error.subject().to_owned())
}

/// A set on the clock `{ step_s: 60, horizon_s: 1800 }` with the given scenarios, written as the
/// comma-separated JSON objects of the `scenarios` list.
pub fn set_of(scenarios: &str) -> String {
    set_on("60", "1800", scenarios)
}

/// A set on the clock `{ step_s, horizon_s }` (each a JSON number token) with the given
/// scenarios.
pub fn set_on(step_s: &str, horizon_s: &str, scenarios: &str) -> String {
    format!(
        "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": {step_s}, \"horizon_s\": {horizon_s}}}, \"scenarios\": [{scenarios}]}}"
    )
}

/// One scenario named `s` with the given `inputs` object body and `expect` list body.
pub fn scenario(inputs: &str, expect: &str) -> String {
    named("s", inputs, expect)
}

/// One scenario named `name` with the given `inputs` object body and `expect` list body.
pub fn named(name: &str, inputs: &str, expect: &str) -> String {
    format!("{{\"name\": \"{name}\", \"inputs\": {{{inputs}}}, \"expect\": [{expect}]}}")
}

/// A set of one scenario with the given `inputs` object body and `expect` list body.
pub fn one(inputs: &str, expect: &str) -> String {
    set_of(&scenario(inputs, expect))
}

/// Compare `actual` with the checked-in file at `path` byte for byte, or rewrite the file when
/// the bless switch is on.
pub fn compare_or_bless(path: &PathBuf, actual: &str) {
    if oce_bless::enabled(BLESS_SWITCH) {
        std::fs::write(path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read golden {}: {error}", path.display()));
    assert!(
        expected == actual,
        "{} differs from the generated bytes; rerun with {BLESS_SWITCH}=1 if the change is intended\n--- expected\n{expected}\n--- actual\n{actual}",
        path.display()
    );
}

/// A deterministic 64-bit generator (SplitMix64), so every run draws the same sequence.
pub struct Sequence(pub u64);

impl Sequence {
    /// The next value.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value in `0..bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).expect("fits")).expect("fits")
    }
}
