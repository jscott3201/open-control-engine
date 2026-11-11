//! What the CXF importer's refusal of a giant list costs, in allocations, never in elapsed time
//! (issue #578; `TESTING.md`: cost is asserted with deterministic measures, never wall-clock time).
//!
//! The two refusals are 100,000 references to nodes that do not exist (`unresolved-reference`)
//! and a 1,000,000-element array value whose declared size is `(2)` (`array-shape`). Each is
//! measured at that size and at a sixteenth of it. A refusal must cost what reading the document
//! costs, plus nothing per list item:
//! - The reference list is read and parsed whole, then refused at its first reference. Parsing
//!   one `{"@id": …}` object costs three allocations, and the refusal adds none per reference.
//! - The array is refused on its length before any element becomes an assignment. Its
//!   allocations do not grow with the elements, beyond the doublings of the parsed list itself.
//!
//! Peak memory stays within a fixed multiple of the document's bytes for both.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. Naming
//! that crate links its counting global allocator, which is why this file is a test binary of its
//! own (`Cargo.toml`). The counts are a function of the input, the importer, and the `serde_json`
//! and `std` that `Cargo.lock` and the pinned toolchain fix: a lockfile change that moves them
//! is re-measured. That includes `serde_json`'s features, which Cargo unifies across the
//! workspace. Today the only one on beyond the defaults is `float_roundtrip`. `preserve_order`
//! would make a parsed object an `IndexMap`, at least four allocations per reference, and
//! `arbitrary_precision` would give each number a `String`, about 1,000,000 more in the array
//! case. Either breaks these bounds in a `--workspace` run while `-p oce-author` still passes.
//! Each test imports the unedited document once before measuring, so one-time setup is not
//! counted. The ceilings below have headroom over
//! what was measured (in brackets). The headroom is a fixed amount, never a part of an allocation
//! per item, so one more allocation per reference or per element fails.
//!
//! What this cannot see: work that allocates nothing, or allocates once whatever the size. For
//! the reference list that is a scan per item, such as one over the node index for each lookup in
//! place of the ordered map the importer keeps. For the array it is converting every element
//! before the shape check into one buffer sized up front: one allocation, and about two bytes of
//! peak per document byte. The comparison counter of `src/counted.rs` covers scans for stage 1,
//! not for the importer.

use allocation_counter::AllocationInfo;
use oce_api::MAX_CXF_BYTES;
use oce_author::{ImportError, ImportErrorKind, ImportOptions, SourceForm, import_cxf};

/// The root `@id` of the document every case edits.
const ROOT: &str = "urn:oce:author:econ_enable";

/// The scalar value of the parameter node the array case edits.
const LOW_VALUE: &str = "\"S231:value\": {\n        \"@value\": \"0.0\",\n        \"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      }";

/// The emitter's golden of the economizer-enable sequence, which imports clean.
fn econ() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/emit/econ_enable.cxf.json");
    std::fs::read_to_string(path).expect("golden")
}

/// The allocations of one import, and its refusal (`None` when it imported).
fn measured(bytes: &[u8]) -> (AllocationInfo, Option<ImportError>) {
    let mut refusal = None;
    let info = allocation_counter::measure(|| {
        refusal = import_cxf(bytes, &ImportOptions::new()).err();
    });
    (info, refusal)
}

/// Import the unedited document once, so one-time setup is not counted in what follows.
fn warm_up() {
    assert!(
        measured(econ().as_bytes()).1.is_none(),
        "the golden imports"
    );
}

fn length(document: &[u8]) -> u64 {
    u64::try_from(document.len()).expect("fits")
}

/// The root's `hasParameter` list led by `count` references to nodes that do not exist.
fn missing_references(count: usize) -> Vec<u8> {
    let refs: Vec<String> = (0..count)
        .map(|i| format!("{{\"@id\": \"{ROOT}.p{i}\"}}"))
        .collect();
    let text = econ();
    let list = "\"S231:hasParameter\": [";
    assert!(text.contains(list));
    text.replacen(list, &format!("{list}{},", refs.join(",")), 1)
        .into_bytes()
}

/// The node `hys.uLow` declared an array of size `(2)` and given `count` elements.
fn oversized_array(count: usize) -> Vec<u8> {
    let items = vec!["1.0"; count].join(",");
    let node = format!("\"@id\": \"{ROOT}.hys.uLow\",\n      \"@type\": \"S231:Parameter\",");
    let text = econ();
    assert!(text.contains(&node) && text.contains(LOW_VALUE));
    text.replacen(
        &node,
        &format!("{node}\n      \"S231:isArray\": true, \"S231:numberDimensions\": 1, \"S231:sizeOfDimensions\": \"(2)\","),
        1,
    )
    .replacen(LOW_VALUE, &format!("\"S231:value\": [{items}]"), 1)
    .into_bytes()
}

#[test]
fn a_giant_reference_list_is_refused_within_the_three_allocations_per_reference_of_its_parse() {
    warm_up();
    let mut peaks = Vec::new();
    for count in [6_250_usize, 100_000] {
        let document = missing_references(count);
        assert!(document.len() <= MAX_CXF_BYTES, "{} bytes", document.len());
        let (info, refusal) = measured(&document);
        // Refused at the first reference, whatever follows it.
        assert_eq!(
            refusal,
            Some(ImportError::new(
                SourceForm::Cxf,
                ROOT,
                ImportErrorKind::Unresolved {
                    reference: format!("{ROOT}.p0"),
                },
            )),
            "{count} references"
        );
        // Three per reference is the parse of its object: the one-entry map, its key and its
        // value. [measured: three per reference and 291 or 303 more]
        let references = u64::try_from(count).expect("fits");
        assert!(
            info.count_total <= 3 * references + 1_024,
            "{count} references: {} allocations",
            info.count_total
        );
        // [measured: 16.7 and 16.5 bytes held per document byte]
        assert!(
            info.bytes_max <= 24 * length(&document),
            "{count} references: {} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        peaks.push(info.bytes_max);
    }
    // Sixteen times the references holds about sixteen times the memory: no super-linear term.
    // [measured: 15.9]
    let ratio = peaks[1] / peaks[0];
    assert!((12..=20).contains(&ratio), "{peaks:?}");
}

#[test]
fn a_giant_array_of_the_wrong_size_is_refused_before_its_elements_are_read() {
    warm_up();
    let mut counts = Vec::new();
    for count in [62_500_usize, 1_000_000] {
        let document = oversized_array(count);
        assert!(document.len() <= MAX_CXF_BYTES, "{} bytes", document.len());
        let (info, refusal) = measured(&document);
        assert_eq!(
            refusal,
            Some(ImportError::new(
                SourceForm::Cxf,
                format!("{ROOT}.hys.uLow"),
                ImportErrorKind::ArrayShape,
            )),
            "{count} elements"
        );
        // Turning each element into an assignment costs at least one allocation per element; the
        // refusal costs a fixed number [measured: 313 and 317].
        assert!(
            info.count_total <= 512,
            "{count} elements: {} allocations",
            info.count_total
        );
        // [measured: 12.4 and 12.6 bytes held per document byte]
        assert!(
            info.bytes_max <= 24 * length(&document),
            "{count} elements: {} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        counts.push(info.count_total);
    }
    // Sixteen times the elements adds only the parsed list's own doublings [measured: 4]. The
    // difference is taken either way round, so a larger document that allocates less still
    // prints both counts rather than overflowing.
    assert!(counts[1].abs_diff(counts[0]) <= 8, "{counts:?}");
}
