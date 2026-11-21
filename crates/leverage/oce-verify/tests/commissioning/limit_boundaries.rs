//! Every bound accepted at the limit and refused one past it (commissioning note, "Test-spec
//! file", as landed): the `SpecLimits`, at their maxima and lowered, and the three bounds the
//! strict profile fixes, through whole documents, for a spec and for a mapping file.

use oce_verify::{
    CommissioningSpec, MAX_DEPTH, MAX_LIST_LEN, MAX_STRING_BYTES, PointMapping, SpecLimits,
};

use crate::{code_of, one_reference, top_level};

const MAX: SpecLimits = SpecLimits::MAX;

fn code_under(text: &str, limits: &SpecLimits) -> &'static str {
    match CommissioningSpec::from_json_str(text, limits) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

fn mapping_code_under(bytes: &[u8], limits: &SpecLimits) -> &'static str {
    match PointMapping::from_json_bytes(bytes, limits) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

/// A document of exactly `length` bytes: a small spec, then spaces.
fn padded_to(length: usize) -> String {
    let mut document = top_level("\"sampling\": 5");
    assert!(document.len() <= length);
    document.push_str(&" ".repeat(length - document.len()));
    document
}

#[test]
fn the_byte_bound_is_checked_from_the_length_before_a_byte_is_read() {
    let limit = usize::try_from(MAX.max_bytes).expect("fits");
    assert_eq!(limit, 4 * 1024 * 1024);
    assert_eq!(code_under(&padded_to(limit), &MAX), "loaded");
    assert_eq!(code_under(&padded_to(limit + 1), &MAX), "input-too-large");
    // Decided from the length: bytes that are no document at all get the same answer, through
    // both entry points.
    let garbage = vec![0xff_u8; limit + 1];
    let error = CommissioningSpec::from_json_bytes(&garbage, &MAX).expect_err("refused");
    assert_eq!(error.code(), "input-too-large");
    assert_eq!(
        error.detail(),
        "the document is 4194305 bytes; at most 4194304 are allowed"
    );
    assert_eq!(mapping_code_under(&garbage, &MAX), "input-too-large");
    assert_eq!(mapping_code_under(&garbage[..limit], &MAX), "not-utf8");
    // Comments count: the bound is on the bytes as written.
    let comment = format!("{}/*{}*/", top_level(""), "c".repeat(limit));
    assert_eq!(code_under(&comment, &MAX), "input-too-large");
    // Lowered, at the limit and one past it; raised, it is still the maximum.
    let document = top_level("\"sampling\": 5");
    let exact = SpecLimits {
        max_bytes: u64::try_from(document.len()).expect("fits"),
        ..MAX
    };
    assert_eq!(code_under(&document, &exact), "loaded");
    let short = SpecLimits {
        max_bytes: exact.max_bytes - 1,
        ..MAX
    };
    assert_eq!(code_under(&document, &short), "input-too-large");
    let none = SpecLimits {
        max_bytes: 0,
        ..MAX
    };
    assert_eq!(
        code_under("", &none),
        "json-syntax",
        "an empty text fits in zero bytes"
    );
    assert_eq!(code_under(" ", &none), "input-too-large");
    let raised = SpecLimits {
        max_bytes: u64::MAX,
        ..MAX
    };
    assert_eq!(
        code_under(&padded_to(limit + 1), &raised),
        "input-too-large"
    );
}

fn parameters(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"p{index}.k\": {index}"))
        .collect();
    one_reference(&format!("\"parameters\": {{{}}}", entries.join(", ")))
}

#[test]
fn a_reference_holds_at_most_256_parameters() {
    assert_eq!(MAX.max_parameters, 256);
    assert_eq!(code_of(&parameters(256)), "loaded");
    let refused = crate::load(&parameters(257)).expect_err("refused");
    assert_eq!(refused.code(), "too-many-parameters");
    assert_eq!(
        refused.detail(),
        "more than 256 parameters in one reference"
    );
    // The bound is per reference: two references of 256 each load.
    let entries: Vec<String> = (0..256).map(|index| format!("\"p{index}\": 0")).collect();
    let reference = format!(
        "{{\"model\": \"M\", \"sequence\": \"s\", \"parameters\": {{{}}}}}",
        entries.join(", ")
    );
    assert_eq!(
        code_of(&format!("{{\"references\": [{reference}, {reference}]}}")),
        "loaded"
    );
    // Lowered, at the limit and one past it; to zero, an empty map still loads.
    let three = SpecLimits {
        max_parameters: 3,
        ..MAX
    };
    assert_eq!(code_under(&parameters(3), &three), "loaded");
    assert_eq!(code_under(&parameters(4), &three), "too-many-parameters");
    let zero = SpecLimits {
        max_parameters: 0,
        ..MAX
    };
    assert_eq!(code_under(&parameters(0), &zero), "loaded");
    assert_eq!(code_under(&parameters(1), &zero), "too-many-parameters");
    let raised = SpecLimits {
        max_parameters: u32::MAX,
        ..MAX
    };
    assert_eq!(code_under(&parameters(257), &raised), "too-many-parameters");
    // The entry past the bound is refused before its value is read.
    let entries: Vec<String> = (0..256).map(|index| format!("\"p{index}\": 0")).collect();
    let bad_value = one_reference(&format!(
        "\"parameters\": {{{}, \"past\": \"not a value\"}}",
        entries.join(", ")
    ));
    assert_eq!(code_of(&bad_value), "too-many-parameters");
}

fn outputs(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"s.y{index}\": {{}}"))
        .collect();
    one_reference(&format!("\"outputs\": {{{}}}", entries.join(",")))
}

fn indicators(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"s.y{index}\": []"))
        .collect();
    one_reference(&format!("\"indicators\": {{{}}}", entries.join(",")))
}

fn columns(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"{index}\": \"p\""))
        .collect();
    one_reference(&format!(
        "\"inputs\": [{{\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": 1, \"columns\": {{{}}}}}]",
        entries.join(",")
    ))
}

#[test]
fn an_author_keyed_map_holds_at_most_4096_keys() {
    assert_eq!(MAX.max_keys, 4_096);
    let five = SpecLimits { max_keys: 5, ..MAX };
    let zero = SpecLimits { max_keys: 0, ..MAX };
    let raised = SpecLimits {
        max_keys: u32::MAX,
        ..MAX
    };
    for (map, document, sentence) in [
        (
            "outputs",
            outputs as fn(usize) -> String,
            "more than 4096 keys in one outputs map",
        ),
        (
            "indicators",
            indicators,
            "more than 4096 keys in one indicators map",
        ),
        ("columns", columns, "more than 4096 keys in one columns map"),
    ] {
        assert_eq!(code_of(&document(4_096)), "loaded", "{map}");
        let refused = crate::load(&document(4_097)).expect_err("refused");
        assert_eq!(refused.code(), "too-many-keys", "{map}");
        assert_eq!(refused.detail(), sentence);
        assert_eq!(code_under(&document(5), &five), "loaded", "{map}");
        assert_eq!(code_under(&document(6), &five), "too-many-keys", "{map}");
        assert_eq!(code_under(&document(0), &zero), "loaded", "{map}");
        assert_eq!(code_under(&document(1), &zero), "too-many-keys", "{map}");
        assert_eq!(
            code_under(&document(4_097), &raised),
            "too-many-keys",
            "{map}"
        );
    }
    // The parameter bound is its own: lowering the key bound leaves it alone, and the reverse.
    assert_eq!(code_under(&parameters(6), &five), "loaded");
    let two_parameters = SpecLimits {
        max_parameters: 2,
        ..MAX
    };
    assert_eq!(code_under(&outputs(3), &two_parameters), "loaded");
}

#[test]
fn nesting_is_bounded_at_eight_levels() {
    assert_eq!(MAX_DEPTH, 8);
    // The deepest well-formed spec nests six levels (spec, references, reference, inputs,
    // source, columns), so the bound is met only inside a field that takes any value.
    let nested = |levels: usize| {
        top_level(&format!(
            "\"controller\": {}{}",
            "[".repeat(levels),
            "]".repeat(levels)
        ))
    };
    assert_eq!(
        code_of(&nested(MAX_DEPTH - 1)),
        "loaded",
        "eight with the root"
    );
    let refused = crate::load(&nested(MAX_DEPTH)).expect_err("refused");
    assert_eq!(refused.code(), "nesting-too-deep");
    assert_eq!(refused.detail(), "nesting deeper than 8 levels");
    let objects = |levels: usize| {
        top_level(&format!(
            "\"modelJsonDirectory\": {}1{}",
            "{\"a\":".repeat(levels),
            "}".repeat(levels)
        ))
    };
    assert_eq!(code_of(&objects(MAX_DEPTH - 1)), "loaded");
    assert_eq!(code_of(&objects(MAX_DEPTH)), "nesting-too-deep");
    // In a mapping file the root list is the first level.
    let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
    assert_eq!(
        mapping_code_under(deep.as_bytes(), &MAX),
        "nesting-too-deep"
    );
    // A misplaced value within the bound is a wrong type, not depth.
    assert_eq!(
        code_of(&one_reference(
            "\"window\": {\"start\": [[[[0]]]], \"stop\": 1}"
        )),
        "wrong-type"
    );
}

#[test]
fn a_list_is_bounded_at_65536_entries() {
    assert_eq!(MAX_LIST_LEN, 65_536);
    let names = |count: usize| {
        one_reference(&format!(
            "\"indicators\": {{\"s.y\": [{}]}}",
            vec!["\"s.u\""; count].join(",")
        ))
    };
    assert_eq!(code_of(&names(MAX_LIST_LEN)), "loaded");
    let refused = crate::load(&names(MAX_LIST_LEN + 1)).expect_err("refused");
    assert_eq!(refused.code(), "list-too-long");
    assert_eq!(refused.detail(), "the list has more than 65536 entries");
    let references = |count: usize| {
        format!(
            "{{\"references\": [{}]}}",
            vec!["{\"model\":\"\",\"sequence\":\"s\"}"; count].join(",")
        )
    };
    assert_eq!(code_of(&references(MAX_LIST_LEN)), "loaded");
    assert_eq!(code_of(&references(MAX_LIST_LEN + 1)), "list-too-long");
    // A mapping file is a list. Sixty-five thousand whole pairs do not fit in the byte bound,
    // so the list bound is met there only by entries that are no pair: the strict pass counts
    // them before the shape is judged.
    let entries = |count: usize| format!("[{}]", vec!["0"; count].join(","));
    assert_eq!(
        mapping_code_under(entries(MAX_LIST_LEN).as_bytes(), &MAX),
        "wrong-type"
    );
    assert_eq!(
        mapping_code_under(entries(MAX_LIST_LEN + 1).as_bytes(), &MAX),
        "list-too-long"
    );
}

#[test]
fn a_string_is_bounded_at_4096_bytes_as_written() {
    assert_eq!(MAX_STRING_BYTES, 4_096);
    let model = |length: usize| {
        format!(
            "{{\"references\": [{{\"model\": \"{}\", \"sequence\": \"s\"}}]}}",
            "m".repeat(length)
        )
    };
    assert_eq!(code_of(&model(MAX_STRING_BYTES)), "loaded");
    let refused = crate::load(&model(MAX_STRING_BYTES + 1)).expect_err("refused");
    assert_eq!(refused.code(), "string-too-long");
    assert_eq!(refused.detail(), "the string is longer than 4096 bytes");
    // A key is a string, and so is a string inside an ignored field.
    let key = |length: usize| {
        one_reference(&format!(
            "\"outputs\": {{\"s.{}\": {{}}}}",
            "y".repeat(length - 2)
        ))
    };
    assert_eq!(code_of(&key(MAX_STRING_BYTES)), "loaded");
    assert_eq!(code_of(&key(MAX_STRING_BYTES + 1)), "string-too-long");
    let ignored = |length: usize| top_level(&format!("\"controller\": \"{}\"", "c".repeat(length)));
    assert_eq!(code_of(&ignored(MAX_STRING_BYTES)), "loaded");
    assert_eq!(code_of(&ignored(MAX_STRING_BYTES + 1)), "string-too-long");
    // Counted as written: an escape is the bytes it is spelled with.
    let escaped =
        |count: usize| top_level(&format!("\"controller\": \"{}\"", "\\u00e9".repeat(count)));
    assert_eq!(code_of(&escaped(682)), "loaded");
    assert_eq!(code_of(&escaped(683)), "string-too-long");
    // A comment is not a string: it may be longer than any string.
    let comment = format!("{}/*{}*/", top_level(""), "c".repeat(MAX_STRING_BYTES * 4));
    assert_eq!(code_of(&comment), "loaded");
}

#[test]
fn the_key_match_work_bound_is_met_exactly() {
    let spec = crate::loaded(&one_reference(
        "\"outputs\": {\"s.yFan\": {}, \"s.T*p\": {}}, \"indicators\": {\"s.*\": []}",
    ));
    let reference = &spec.references()[0];
    let names = ["s.yFan", "s.TSup", "s.TSupHeaEco"];
    assert_eq!(MAX.max_key_match_work, 1 << 28);
    assert!(reference.select_output_keys(&names, &MAX).is_ok());
    // Find the work the selection needs: the least limit that admits it.
    let needed = (0..10_000_u64)
        .find(|work| {
            let limits = SpecLimits {
                max_key_match_work: *work,
                ..MAX
            };
            reference.select_output_keys(&names, &limits).is_ok()
        })
        .expect("a small selection needs little work");
    // One unit per key and name compared (2 * 3), and the matcher's steps for the glob's three
    // names: `T*p` against `yFan` stops at its first byte (1); against `TSup` it takes `T`, the
    // `*`, two characters into the `*` and the `p` (5); against `TSupHeaEco` it does the same,
    // finds text after the `p`, gives the `*` the other seven characters one at a time and is
    // left with a `p` that nothing matches (5 + 7 + 1).
    assert_eq!(needed, 6 + 1 + 5 + 13);
    let at = SpecLimits {
        max_key_match_work: needed,
        ..MAX
    };
    let below = SpecLimits {
        max_key_match_work: needed - 1,
        ..MAX
    };
    assert!(reference.select_output_keys(&names, &at).is_ok());
    let refused = reference
        .select_output_keys(&names, &below)
        .expect_err("refused");
    assert_eq!(refused.code(), "work-limit-exceeded");
    assert_eq!(
        refused.detail(),
        format!(
            "matching the keys against the output names needed more than {} work units",
            needed - 1
        )
    );
    // The same selection costs the same work every time, and a raised limit is the maximum.
    assert!(reference.select_output_keys(&names, &at).is_ok());
    let raised = SpecLimits {
        max_key_match_work: u64::MAX,
        ..MAX
    };
    assert_eq!(
        reference.select_output_keys(&names, &raised),
        reference.select_output_keys(&names, &MAX)
    );
    // No key, no work: a reference without overrides is selected under a limit of zero.
    let none = SpecLimits {
        max_key_match_work: 0,
        ..MAX
    };
    let bare = crate::loaded(&one_reference(""));
    assert!(
        bare.references()[0]
            .select_output_keys(&names, &none)
            .is_ok()
    );
    assert!(
        reference.select_indicator_keys(&[], &none).is_err(),
        "a key and no name"
    );
}
