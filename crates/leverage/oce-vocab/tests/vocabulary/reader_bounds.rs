//! What bounds a read of either file: the byte bound, checked before anything is parsed, and
//! the closed shape, which stops nesting at the first level it does not have. Hostile input is
//! refused with a typed [`VocabError`], never a panic and never a stack overflow.

use oce_vocab::{
    CURATED_JSON, CuratedDescription, CuratedVocabulary, GENERATED_JSON, GeneratedVocabulary,
    MAX_VOCAB_BYTES, VocabError, Vocabulary,
};

/// The wire reader's list bound (`oce-author`, `src/wire/mod.rs`), restated because this crate
/// sits below `oce-author` and cannot name it.
const WIRE_LIST_LEN: usize = 65_536;

fn generated_error(text: &str) -> VocabError {
    GeneratedVocabulary::from_json(text).expect_err("refused")
}

fn curated_error(text: &str) -> VocabError {
    CuratedVocabulary::from_json(text).expect_err("refused")
}

/// The smallest list item either file can hold: a curated description of one character a field.
fn smallest_item() -> CuratedDescription {
    CuratedDescription {
        evidence: "e".to_owned(),
        subject: "s".to_owned(),
        text: "t".to_owned(),
    }
}

/// A curated document with one class and `items` smallest descriptions.
fn curated_with(items: usize) -> CuratedVocabulary {
    let mut curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    curated.classes.truncate(1);
    curated.classes[0].notes.clear();
    curated.classes[0].descriptions = vec![smallest_item(); items];
    curated
}

#[test]
fn the_document_size_bound_is_checked_before_anything_is_parsed() {
    let oversized = " ".repeat(MAX_VOCAB_BYTES + 1);
    let expected = VocabError::TooLarge {
        bytes: MAX_VOCAB_BYTES + 1,
        limit: MAX_VOCAB_BYTES,
    };
    assert_eq!(generated_error(&oversized), expected);
    assert_eq!(curated_error(&oversized), expected);
    // At the limit the size check passes and the content is what is refused.
    let at_limit = " ".repeat(MAX_VOCAB_BYTES);
    assert!(matches!(
        generated_error(&at_limit),
        VocabError::Json { .. }
    ));
    assert!(matches!(curated_error(&at_limit), VocabError::Json { .. }));
}

#[test]
fn an_accepted_document_cannot_hold_as_many_items_as_the_wire_readers_list_bound() {
    // In the canonical spelling the smallest item is 96 bytes, and the byte bound allows 64 per
    // item at the wire reader's list bound: a canonical list that long is over the byte bound.
    let one = curated_with(1).to_json().len();
    let per_item = curated_with(2).to_json().len() - one;
    assert_eq!(per_item, 96);
    assert!(per_item * WIRE_LIST_LEN > MAX_VOCAB_BYTES);
    let text = curated_with(WIRE_LIST_LEN + 1).to_json();
    assert_eq!(
        curated_error(&text),
        VocabError::TooLarge {
            bytes: text.len(),
            limit: MAX_VOCAB_BYTES,
        }
    );
    // The longest canonical list that fits the byte bound is read, and it is shorter than the
    // wire reader's bound; one more item is over the byte bound.
    let fits = (MAX_VOCAB_BYTES - (one - per_item)) / per_item;
    assert!(fits < WIRE_LIST_LEN, "{fits}");
    let text = curated_with(fits).to_json();
    assert!(text.len() <= MAX_VOCAB_BYTES);
    let read = CuratedVocabulary::from_json(&text).expect("canonical and inside the byte bound");
    assert_eq!(read.classes[0].descriptions.len(), fits);
    assert!(curated_with(fits + 1).to_json().len() > MAX_VOCAB_BYTES);
}

#[test]
fn a_compact_document_under_the_byte_bound_with_a_longer_list_is_read_then_refused() {
    // The list bound is not a bound of the reader. A compact spelling packs an item into 42
    // bytes, so 95,000 of them fit under the byte bound: that document is parsed and checked in
    // full, and what refuses it is that it is not the canonical spelling.
    const ITEMS: usize = 95_000;
    const { assert!(ITEMS > WIRE_LIST_LEN) };
    let item = "{\"evidence\":\"e\",\"subject\":\"s\",\"text\":\"t\"}";
    let items = vec![item; ITEMS].join(",");
    let class = curated_with(0).classes[0].class_path.clone();
    let text = format!(
        "{{\"classes\":[{{\"class\":\"{class}\",\"descriptions\":[{items}],\"notes\":[]}}],\"format\":1}}"
    );
    assert!(text.len() <= MAX_VOCAB_BYTES, "{}", text.len());
    assert_eq!(curated_error(&text), VocabError::NotCanonical { line: 1 });
    // The same data in the canonical spelling is over the byte bound.
    assert!(curated_with(ITEMS).to_json().len() > MAX_VOCAB_BYTES);
    // A field check still comes first: the document was read, not only measured.
    let unreadable = text.replacen("\"text\":\"t\"", "\"text\":\"\"", 1);
    assert_eq!(
        curated_error(&unreadable),
        VocabError::EmptyField {
            path: "classes[0].descriptions[0].text".to_owned()
        }
    );
}

/// The closed shape nests five deep (document, `classes`, class, list, entry).
const SHAPE_DEPTH: usize = 5;

/// Require `error` to be the shape's refusal ([`VocabError::Json`]) starting with `detail`,
/// raised no more than the shape's depth past `prefix` bytes of a one-line document: the reader
/// stopped where the shape ends, not a million levels down and not at a recursion limit.
#[track_caller]
fn refused_where_the_shape_ends(error: &VocabError, prefix: usize, detail: &str) {
    let VocabError::Json {
        line,
        column,
        detail: found,
    } = error
    else {
        panic!("not a shape refusal: {error:?}");
    };
    assert_eq!(*line, 1, "{error:?}");
    assert!(
        (prefix..=prefix + SHAPE_DEPTH).contains(column),
        "{error:?}"
    );
    assert!(found.starts_with(detail), "{error:?}");
    assert!(!found.contains("recursion limit"), "{error:?}");
}

#[test]
fn nesting_a_million_deep_is_refused_where_the_closed_shape_ends() {
    const DEPTH: usize = 1_000_000;
    let a_string = "invalid type: sequence, expected a string";
    // Arrays all the way down, open and balanced. A struct also reads from a positional array,
    // so the reader descends as far as the shape goes (document, `classes`, a class) and
    // refuses the array it meets where the class path's string belongs.
    let open = "[".repeat(DEPTH);
    let balanced = format!("{open}{}", "]".repeat(DEPTH));
    assert!(balanced.len() <= MAX_VOCAB_BYTES);
    for text in [&open, &balanced] {
        refused_where_the_shape_ends(&generated_error(text), 0, a_string);
        refused_where_the_shape_ends(&curated_error(text), 0, a_string);
    }

    // Objects all the way down, as deep as the byte bound lets a document be: the first level
    // names a key the document object does not have.
    let levels = MAX_VOCAB_BYTES / "{\"a\":".len();
    assert!(levels > 800_000);
    let objects = "{\"a\":".repeat(levels);
    assert!(objects.len() <= MAX_VOCAB_BYTES);
    refused_where_the_shape_ends(&generated_error(&objects), 0, "unknown field `a`");
    refused_where_the_shape_ends(&curated_error(&objects), 0, "unknown field `a`");

    // The nest hung where the shape expects a list of classes, a string and a number.
    let nest = "[".repeat(DEPTH);
    for (prefix, detail) in [
        ("{\"classes\":", a_string),
        ("{\"classes\":[{\"class\":", a_string),
        (
            "{\"classes\":[],\"format\":",
            "invalid type: sequence, expected u32",
        ),
    ] {
        let text = format!("{prefix}{nest}");
        refused_where_the_shape_ends(&generated_error(&text), prefix.len(), detail);
        refused_where_the_shape_ends(&curated_error(&text), prefix.len(), detail);
    }
    // And in the deepest place each shape has: a field of `source`, a field of a note.
    let prefix = "{\"classes\":[],\"format\":1,\"source\":{\"sha256\":";
    refused_where_the_shape_ends(
        &generated_error(&format!("{prefix}{nest}")),
        prefix.len(),
        a_string,
    );
    let prefix = "{\"classes\":[{\"class\":\"C\",\"descriptions\":[],\"notes\":[{\"text\":";
    refused_where_the_shape_ends(
        &curated_error(&format!("{prefix}{nest}")),
        prefix.len(),
        a_string,
    );
}

#[test]
fn the_pair_reader_refuses_deep_nesting_in_either_file_the_same_way() {
    let nest = "[".repeat(1_000_000);
    let a_string = "invalid type: sequence, expected a string";
    let small = "{\n  \"classes\": [],\n  \"format\": 1\n}\n";
    assert!(CuratedVocabulary::from_json(small).is_ok());
    let in_generated = Vocabulary::from_json(&nest, small).expect_err("refused");
    refused_where_the_shape_ends(&in_generated, 0, a_string);
    let in_curated = Vocabulary::from_json(GENERATED_JSON, &nest).expect_err("refused");
    refused_where_the_shape_ends(&in_curated, 0, a_string);
}
