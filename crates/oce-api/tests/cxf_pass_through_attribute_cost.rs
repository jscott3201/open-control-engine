//! What a boundary output's attributes cost a load when many boundary inputs are wired to it, in
//! requested bytes and never in elapsed time (issue #750; `TESTING.md`: cost is asserted with
//! deterministic measures).
//!
//! A root boundary output wired from many boundary inputs gets one pass-through block a pair, and
//! each block's output connector carried the output's §7.4.1 attributes. They were read again for
//! each pair, so a long `quantity`, `unit` or `displayUnit` was copied once a pair while the
//! document wrote it once and each further pair cost it one reference: 1,024 inputs onto a 64 KiB
//! `quantity`, a 232 KB document, peaked at 74 MB before the output was refused as multiply
//! driven. The output's attributes are now read once and every pair shares the reading; its
//! findings are raised again for each pair, so the list is the one a read per pair made.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change); a copy of the term a pair is far over them.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{Engine, OcError};
use oce_diag::{DiagCode, Diagnostic};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The issue's term: a `quantity` of 64 KiB.
const TERM_BYTES: usize = 64 * 1024;

/// A root with one `Constant` leaf and `pairs` Real boundary inputs, each wired to the one
/// boundary output `urn:o`, which declares `attributes` (JSON members, each led by a comma).
fn fan_in(pairs: usize, attributes: &str) -> Vec<u8> {
    let inputs: Vec<String> = (0..pairs)
        .map(|i| format!("{{ \"@id\": \"urn:i{i}\" }}"))
        .collect();
    let nodes: String = (0..pairs)
        .map(|i| {
            format!(
                ",\n  {{ \"@id\": \"urn:i{i}\", \"@type\": \"S231:RealInput\", \"S231:isOfDataType\": \
                 {{ \"@id\": \"S231:Real\" }}, \"S231:isConnectedTo\": {{ \"@id\": \"urn:o\" }} }}"
            )
        })
        .collect();
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [
  {{ \"@id\": \"urn:r\", \"@type\": \"S231:Block\", \"S231:hasInput\": [ {} ],
    \"S231:hasOutput\": {{ \"@id\": \"urn:o\" }},
    \"S231:containsBlock\": {{ \"@id\": \"urn:r.keep\" }} }},
  {{ \"@id\": \"urn:r.keep\",
    \"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\",
    \"S231:hasParameter\": {{ \"@id\": \"urn:r.keep.k\" }},
    \"S231:hasOutput\": {{ \"@id\": \"urn:r.keep.y\" }} }},
  {{ \"@id\": \"urn:r.keep.k\", \"S231:value\": 1.0 }},
  {{ \"@id\": \"urn:r.keep.y\", \"@type\": \"S231:RealOutput\",
    \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }},
  {{ \"@id\": \"urn:o\", \"@type\": \"S231:RealOutput\", \
         \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}{attributes} }}{nodes} ] }}",
        inputs.join(", ")
    )
    .into_bytes()
}

/// A long `quantity` of [`TERM_BYTES`] bytes, as an attribute member.
fn long_quantity() -> String {
    format!(", \"S231:quantity\": \"{}\"", "q".repeat(TERM_BYTES))
}

/// One load of `bytes`, measured. The unedited model is loaded once first.
fn load(bytes: &[u8]) -> (OcError, AllocationInfo) {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes).map(|_| ())));
    let error = result
        .expect("the load ran")
        .expect_err("a multiply driven output is refused");
    (error, info)
}

/// Whether `error` is the multiply driven refusal of `urn:o` with `pairs` drivers.
fn multiply_driven(error: &OcError, pairs: usize) -> bool {
    let wanted = format!("boundary output is multiply driven (distinct drivers {pairs})");
    error
        .all_diagnostics()
        .any(|d| d.code == DiagCode::SingleAssignment && d.message.contains(&wanted))
}

#[test]
fn many_pairs_onto_one_long_quantity_hold_one_copy_of_it() {
    // The issue's shape at 256, 512 and 1,024 pairs: the peak and the total are the document's
    // own cost a pair, the term held once. [measured: peaks of 1,909,151 / 3,540,627 / 6,803,940
    // bytes and totals of 3,860,374 / 7,437,622 / 14,592,679; before, a peak of 18,624,899 at
    // 256 pairs and 73.9 MB at 1,024 (issue), about 72 KB a pair]
    let mut peaks = Vec::new();
    for pairs in [256, 512, 1_024] {
        let document = fan_in(pairs, &long_quantity());
        let (error, info) = load(&document);
        assert!(multiply_driven(&error, pairs), "{error}");
        assert!(
            info.bytes_max <= (2 * TERM_BYTES + 32 * document.len()) as u64,
            "{pairs} pairs: a peak of {} for a {}-byte document",
            info.bytes_max,
            document.len()
        );
        peaks.push(info.bytes_max);
    }
    // Linear in the pairs at the document's own cost a pair: a copy of the term a pair would
    // add 64 KiB a pair.
    for (pair, added) in peaks.windows(2).zip([256_u64, 512]) {
        let per_pair = (pair[1] - pair[0]) / added;
        assert!(
            per_pair <= 8 * 1024,
            "{per_pair} bytes a further pair: {peaks:?}"
        );
    }
}

#[test]
fn a_long_term_costs_a_pair_what_a_one_letter_term_costs_it() {
    // The same documents with a one-letter `quantity`: what each further pair costs is the same
    // within a few bytes, so no pair copies the term. [measured: 6,373 and 6,373 bytes a pair]
    let cost = |quantity: &str| {
        let attribute = format!(", \"S231:quantity\": \"{quantity}\"");
        let small = load(&fan_in(256, &attribute)).1.bytes_max;
        let large = load(&fan_in(1_024, &attribute)).1.bytes_max;
        (large - small) / 768
    };
    let long = cost(&"q".repeat(TERM_BYTES));
    let short = cost("q");
    assert!(
        long.abs_diff(short) <= 64,
        "a pair costs {long} bytes with the long term and {short} with the short one"
    );
}

#[test]
fn every_pair_still_raises_the_findings_of_the_output_it_reads() {
    // An output with a blank `quantity` and a non-number `min`, wired from 40 inputs: the read
    // is shared, and each pair raises the output's two findings in the order a read of its own
    // raised them, so the list holds 40 of each as before.
    let attributes = ", \"S231:quantity\": \" \", \"S231:min\": \"no\"";
    let (error, _) = load(&fan_in(40, attributes));
    let about_output: Vec<&Diagnostic> = error
        .all_diagnostics()
        .filter(|d| d.subject.as_deref() == Some("urn:o") && d.code != DiagCode::SingleAssignment)
        .collect();
    let quantity = about_output
        .iter()
        .filter(|d| d.code == DiagCode::MalformedDocument && d.message.starts_with("S231:quantity"))
        .count();
    let min = about_output
        .iter()
        .filter(|d| d.code == DiagCode::GroundingFailed && d.message.starts_with("S231:min"))
        .count();
    assert_eq!((quantity, min), (40, 40), "{about_output:?}");
}
