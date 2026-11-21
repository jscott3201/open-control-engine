//! What a load costs, in allocations, never in wall-clock time: comment removal is one buffer of
//! the text's length whatever the text holds, a refusal stops where it is found, and the memory
//! a load holds is linear in the document, under a pinned factor.
//!
//! Measured with `allocation-counter` (counters local to the measuring thread). The factors
//! below are ceilings with headroom over what was measured (in brackets); the byte bound
//! (`SpecLimits::max_bytes`, 4 MiB, lowerable) turns them into an absolute bound.

use allocation_counter::AllocationInfo;
use oce_verify::{CommissioningSpec, PointMapping, SpecLimits};

use crate::{one_reference, top_level};

/// The allocations of one load of `text` as a spec, and its code or `loaded`.
fn measured(text: &str) -> (AllocationInfo, &'static str) {
    let mut code = "loaded";
    let info = allocation_counter::measure(|| {
        if let Err(error) = CommissioningSpec::from_json_str(text, &SpecLimits::default()) {
            code = error.code();
        }
    });
    (info, code)
}

fn length(document: &str) -> u64 {
    u64::try_from(document.len()).expect("fits")
}

/// A document near the byte bound: a small spec, then `filler` repeated to about `bytes` bytes.
fn filled(filler: &str, bytes: usize) -> String {
    let mut document = top_level("\"sampling\": 5");
    document.push_str(&filler.repeat(bytes / filler.len()));
    document
}

const NEAR_BOUND: usize = 4 * 1024 * 1024 - 64;

#[test]
fn removing_comments_costs_one_buffer_of_the_texts_length_whatever_the_comments() {
    // The same spec with no comment, and with one: the difference is one allocation of the
    // document's length, and nothing else.
    let plain = top_level("\"sampling\": 5");
    let commented = format!("{plain}/**/");
    let (without, code) = measured(&plain);
    assert_eq!(code, "loaded");
    let (with, code) = measured(&commented);
    assert_eq!(code, "loaded");
    assert_eq!(with.count_total, without.count_total + 1);
    assert_eq!(
        with.bytes_total,
        without.bytes_total + length(&commented),
        "the one buffer is exactly the text's length"
    );
    // Every hostile shape of comment costs that one buffer more than the comment-free load of
    // the same spec: four megabytes of two-byte comments, of empty block comments, of block
    // comments that almost close, one comment of four megabytes, and comment markers inside
    // comments.
    for (shape, filler) in [
        ("line comments", "//\n"),
        ("empty block comments", "/**/"),
        ("block comments of stars", "/***/"),
        ("near closings", "/*/*/"),
        ("markers in a line comment", "///*\n"),
    ] {
        let document = filled(filler, NEAR_BOUND);
        assert!(document.len() > 4_000_000, "{shape}: near the byte bound");
        let (info, code) = measured(&document);
        assert_eq!(code, "loaded", "{shape}");
        assert_eq!(info.count_total, without.count_total + 1, "{shape}");
        assert_eq!(
            info.bytes_total,
            without.bytes_total + length(&document),
            "{shape}"
        );
    }
    let one_long = format!("{plain}/*{}*/", "x".repeat(NEAR_BOUND - 64));
    let (info, code) = measured(&one_long);
    assert_eq!(code, "loaded");
    assert_eq!(info.count_total, without.count_total + 1);
    // Whitespace is no comment: four megabytes of it allocate nothing more than the small spec.
    let (spaces, code) = measured(&filled(" \n", NEAR_BOUND));
    assert_eq!(code, "loaded");
    assert_eq!(spaces.count_total, without.count_total);
    assert_eq!(spaces.bytes_max, without.bytes_max);
}

#[test]
fn an_unterminated_comment_is_refused_for_the_cost_of_its_refusal() {
    // A comment opened at the first byte and never closed, two bytes long and four megabytes
    // long: the buffer is made once and the refusal is the same.
    let (short, code) = measured("/*");
    assert_eq!(code, "unterminated-comment");
    // Stars and slashes that never spell `*/` after the opening: the worst a comment can hold.
    for filler in ["*", "/", "* /", "x"] {
        let long = format!("/*{}", filler.repeat(NEAR_BOUND / filler.len()));
        let (far, code) = measured(&long);
        assert_eq!(code, "unterminated-comment", "{filler}");
        assert_eq!(short.count_total, far.count_total, "{filler}");
        assert!(far.count_total <= 8, "{far:?}");
        // The buffer is the text's length and is gone before the refusal is written; nothing
        // else grows with the text.
        assert_eq!(far.bytes_max, length(&long), "{filler}");
    }
    assert!(short.bytes_max <= 512, "{short:?}");
    // Strings that are never closed swallow every marker: no buffer at all, and one refusal.
    let (quoted, code) = measured(&format!("\"{}", "/*".repeat(1_000)));
    assert_eq!(code, "json-syntax");
    assert!(quoted.bytes_max <= 512, "{quoted:?}");
}

#[test]
fn a_refusal_at_the_first_bytes_costs_the_same_whatever_follows() {
    // Nine open brackets, and four million: the reader stops at the ninth either way.
    let (short, code) = measured(&"[".repeat(9));
    assert_eq!(code, "nesting-too-deep");
    let (long, code) = measured(&"[".repeat(4_000_000));
    assert_eq!(code, "nesting-too-deep");
    assert_eq!(
        (short.count_total, short.bytes_max),
        (long.count_total, long.bytes_max)
    );
    assert!(long.count_total <= 8 && long.bytes_max <= 512, "{long:?}");
    // A document over the byte bound is refused from its length alone.
    let (over, code) = measured(&"[".repeat(5 * 1024 * 1024));
    assert_eq!(code, "input-too-large");
    assert!(over.count_total <= 8 && over.bytes_max <= 512, "{over:?}");
    // A list of 65,537 integers where a spec belongs is read to its last item and none is kept.
    let list = format!("[{}]", vec!["0"; 65_537].join(","));
    let (listed, code) = measured(&list);
    assert_eq!(code, "list-too-long");
    assert!(
        listed.count_total <= 8 && listed.bytes_max <= 512,
        "{listed:?}"
    );
}

/// A reference whose `outputs` map holds `count` keys with empty tolerance objects.
fn output_keys(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"s.{index:x}\":{{}}"))
        .collect();
    one_reference(&format!("\"outputs\":{{{}}}", entries.join(",")))
}

/// `references` references of `sources` one-column sources each.
fn source_lists(references: usize, sources: usize) -> String {
    let source = r#"{"path":"t","format":"csv","timeColumn":1,"columns":{"a":"b"}}"#;
    let list = vec![source; sources].join(",");
    let all: Vec<String> = (0..references)
        .map(|index| format!("{{\"model\":\"M\",\"sequence\":\"s{index}\",\"inputs\":[{list}]}}"))
        .collect();
    format!("{{\"references\":[{}]}}", all.join(","))
}

/// A spec whose ignored `controller` field holds `lists` lists of 65,536 one-digit numbers: the
/// most values a document can put in the fewest bytes, since nothing else in the format takes a
/// list of numbers.
fn ignored_numbers(lists: usize) -> String {
    let list = format!("[{}]", vec!["0"; 65_536].join(","));
    top_level(&format!(
        "\"controller\":[{}]",
        vec![list.as_str(); lists].join(",")
    ))
}

#[test]
fn the_densest_document_holds_at_most_thirty_two_bytes_per_document_byte() {
    // The densest shape the format has is not one it reads: a field the verifier ignores takes
    // any strict JSON value and keeps it for the canonical form, so a document can be two bytes
    // per value (`0,`), where every value the reader interprets sits behind a key. Each kept
    // value is one node of the canonical writer's tree [measured: 25.6 bytes held per document
    // byte at the byte bound, and 1.0 allocation per value]. With the 4 MiB byte bound that is
    // the most a load can hold: about 134 MB at the ceiling pinned here.
    let mut peaks = Vec::new();
    for lists in [4, 31] {
        let document = ignored_numbers(lists);
        let (info, code) = measured(&document);
        assert_eq!(code, "loaded");
        assert!(
            info.bytes_max <= 32 * length(&document),
            "{} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        let values = u64::try_from(lists * 65_536).expect("fits");
        assert!(
            info.count_total <= 2 * values + 1_024,
            "{}",
            info.count_total
        );
        peaks.push(info.bytes_max);
    }
    assert!(ignored_numbers(31).len() > 4_000_000, "near the byte bound");
    assert!(
        ignored_numbers(32).len() > 4 * 1024 * 1024,
        "one more list is past it"
    );
    // About eight times the values hold about eight times the memory: no super-linear term.
    let ratio = peaks[1] / peaks[0];
    assert!((6..=10).contains(&ratio), "{peaks:?}");
    // The same values where the format reads them are refused at the first one: nothing but
    // an ignored field can hold them.
    let (refused, code) = measured(&one_reference(&format!(
        "\"indicators\":{{\"s.y\":[{}]}}",
        vec!["0"; 65_536].join(",")
    )));
    assert_eq!(code, "wrong-type");
    assert!(refused.bytes_max <= 1_024, "{refused:?}");
}

#[test]
fn a_dense_spec_holds_a_bounded_number_of_bytes_per_document_byte() {
    // The densest maps the format reads, at their bound: 4,096 keys of empty objects [measured:
    // 17.5 bytes per byte and 2.2 allocations per key], and many small sources up to the byte
    // bound [measured: 5.4 bytes per byte and 22.0 allocations per source]. A field the
    // verifier ignores can be denser still; the test above pins that one.
    let keys = output_keys(4_096);
    let (info, code) = measured(&keys);
    assert_eq!(code, "loaded");
    assert!(
        info.bytes_max <= 24 * length(&keys),
        "{} bytes held for {} bytes read",
        info.bytes_max,
        keys.len()
    );
    assert!(info.count_total <= 4 * 4_096 + 128, "{}", info.count_total);
    let mut peaks = Vec::new();
    for (references, sources) in [(8, 1_000), (64, 1_000)] {
        let document = source_lists(references, sources);
        let (info, code) = measured(&document);
        assert_eq!(code, "loaded");
        assert!(
            info.bytes_max <= 8 * length(&document),
            "{} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        let items = u64::try_from(references * sources).expect("fits");
        assert!(
            info.count_total <= 28 * items + 1_024,
            "{}",
            info.count_total
        );
        peaks.push(info.bytes_max);
    }
    // Eight times the sources hold about eight times the memory: no super-linear term.
    let ratio = peaks[1] / peaks[0];
    assert!((6..=10).contains(&ratio), "{peaks:?}");
}

#[test]
fn a_document_of_unknown_keys_is_refused_within_eight_bytes_per_document_byte() {
    // The strict-JSON pass remembers an object's keys to refuse a repeat, so a refused document
    // still costs its keys [measured: 5.7]; the shape pass then stops at the first key.
    let entries: Vec<String> = (0..400_000)
        .map(|index| format!("\"{index:x}\":0"))
        .collect();
    let document = format!("{{{}}}", entries.join(","));
    let (info, code) = measured(&document);
    assert_eq!(code, "unknown-key");
    assert!(
        info.bytes_max <= 8 * length(&document),
        "{} bytes held for {} bytes read",
        info.bytes_max,
        document.len()
    );
}

#[test]
fn a_mapping_of_many_points_is_checked_for_repeats_without_a_quadratic_pass() {
    // Forty thousand pairs with distinct names load, and the same pairs with the last name
    // repeated are refused; the two cost the same number of allocations to within the refusal.
    let pair = |cdl: usize, device: usize| {
        format!(
            "{{\"cdl\":{{\"name\":\"c{cdl}\",\"type\":\"int\"}},\"device\":{{\"name\":\"d{device}\",\"type\":\"int\"}}}}"
        )
    };
    let distinct: Vec<String> = (0..40_000).map(|index| pair(index, index)).collect();
    let mut repeated = distinct.clone();
    repeated[39_999] = pair(0, 39_999);
    let measure = |pairs: &[String]| {
        let text = format!("[{}]", pairs.join(","));
        let mut code = "loaded";
        let info = allocation_counter::measure(|| {
            if let Err(error) = PointMapping::from_json_str(&text, &SpecLimits::default()) {
                code = error.code();
            }
        });
        (info, code, text.len())
    };
    let (loaded, code, bytes) = measure(&distinct);
    assert_eq!(code, "loaded");
    let (refused, code, _) = measure(&repeated);
    assert_eq!(code, "duplicate-point");
    assert!(
        refused.count_total.abs_diff(loaded.count_total) <= 16,
        "{} against {}",
        refused.count_total,
        loaded.count_total
    );
    // A bounded number of allocations per pair [measured: 23.0], and of bytes per document
    // byte [measured: 3.8].
    assert!(
        loaded.count_total <= 28 * 40_000 + 128,
        "{}",
        loaded.count_total
    );
    assert!(
        loaded.bytes_max <= 6 * u64::try_from(bytes).expect("fits"),
        "{loaded:?}"
    );
}
