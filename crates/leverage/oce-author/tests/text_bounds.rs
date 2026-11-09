//! Every text-form parse bound pinned at its boundary (authoring note, "Bounds"; issue #482):
//! accepted at the limit, refused one past it with its typed code at the exact location.

use oce_author::{
    INPUT_TOO_LARGE, LINE_TOO_LONG, MAX_LINE_BYTES, MAX_STATEMENTS_PER_KIND, MAX_STRING_BYTES,
    MAX_TEXT_BYTES, MAX_TEXT_LINES, MAX_TOKEN_BYTES, NESTING_TOO_DEEP, STRING_TOO_LONG,
    TOKEN_TOO_LONG, TOO_MANY_LINES, TOO_MANY_STATEMENTS, parse_text,
};

const HEAD: &str = "sequence s\nextends none\n";

/// Makes a token of a given length, or a statement from an index.
type Maker<'a> = &'a dyn Fn(usize) -> String;
/// Places a token in a document.
type Placer<'a> = &'a dyn Fn(&str) -> String;

fn refused_at(text: &str, code: &str, line: u32, column: u32) {
    let error = parse_text(text).expect_err(code);
    assert_eq!(
        (error.code, error.location.line, error.location.column),
        (code, line, column),
        "{error}"
    );
}

/// A document of exactly `bytes` bytes: the header, then comment lines of 4,096 bytes.
fn document_of(bytes: usize) -> String {
    let mut text = HEAD.to_owned();
    let line = format!("#{}\n", "x".repeat(4094));
    while bytes - text.len() > line.len() {
        text.push_str(&line);
    }
    let rest = bytes - text.len();
    text.push('#');
    text.push_str(&"x".repeat(rest - 1));
    assert_eq!(text.len(), bytes);
    text
}

#[test]
fn the_document_size_is_pinned() {
    parse_text(&document_of(MAX_TEXT_BYTES)).expect("4 MiB is accepted");
    refused_at(&document_of(MAX_TEXT_BYTES + 1), INPUT_TOO_LARGE, 1, 1);
}

#[test]
fn the_line_count_is_pinned() {
    let mut text = HEAD.to_owned();
    text.push_str(&"#\n".repeat(MAX_TEXT_LINES - 2));
    parse_text(&text).expect("100,000 lines, with the final line feed, are accepted");
    let line = u32::try_from(MAX_TEXT_LINES + 1).expect("fits");
    refused_at(&format!("{text}#"), TOO_MANY_LINES, line, 1);
    refused_at(&format!("{text}\n"), TOO_MANY_LINES, line, 1);
}

#[test]
fn the_line_length_is_pinned() {
    let at_limit = format!("{HEAD}#{}\n", "x".repeat(MAX_LINE_BYTES - 1));
    parse_text(&at_limit).expect("a line at the limit is accepted");
    let column = u32::try_from(MAX_LINE_BYTES + 1).expect("fits");
    refused_at(
        &format!("{HEAD}#{}\n", "x".repeat(MAX_LINE_BYTES)),
        LINE_TOO_LONG,
        3,
        column,
    );
}

#[test]
fn every_token_kind_is_pinned_at_the_token_bound() {
    let name = |len: usize| format!("p{}", "a".repeat(len - 1));
    let atom = |len: usize| "K".repeat(len);
    let id = |len: usize| format!("cxf:{}", "0".repeat(len - 4));
    let suffix = |len: usize| {
        // `kg.m.m…`, ending `m2` for an odd length.
        let mut s = format!("kg{}", ".m".repeat((len - 2) / 2));
        if len % 2 == 1 {
            s.push('2');
        }
        assert_eq!(s.len(), len);
        s
    };
    let number = |len: usize| format!("1.{}", "0".repeat(len - 2));
    let cases: [(Maker<'_>, Placer<'_>, u32, u32); 5] = [
        (&name, &|t| format!("{HEAD}param {t} : Real = 1\n"), 3, 7),
        (&atom, &|t| format!("{HEAD}in u : Real @unit {t}\n"), 3, 19),
        (&id, &|t| format!("sequence s\nextends {t}\n"), 2, 9),
        (
            &suffix,
            &|t| format!("{HEAD}param p : Real = 1 {t}\n"),
            3,
            20,
        ),
        (&number, &|t| format!("{HEAD}param p : Real = {t}\n"), 3, 18),
    ];
    for (token, place, line, column) in cases {
        let at_limit = place(&token(MAX_TOKEN_BYTES));
        parse_text(&at_limit).unwrap_or_else(|e| panic!("{e}: {at_limit}"));
        refused_at(
            &place(&token(MAX_TOKEN_BYTES + 1)),
            TOKEN_TOO_LONG,
            line,
            column,
        );
    }
}

#[test]
fn the_string_length_is_pinned_as_written() {
    let place = |s: &str| format!("{HEAD}block a = CDL.Utilities.Assert(message = \"{s}\")\n");
    parse_text(&place(&"x".repeat(MAX_STRING_BYTES))).expect("at the limit");
    refused_at(
        &place(&"x".repeat(MAX_STRING_BYTES + 1)),
        STRING_TOO_LONG,
        3,
        42,
    );
    // Escapes count as written: 2,048 `\n` escapes are 4,096 bytes.
    parse_text(&place(&"\\n".repeat(MAX_STRING_BYTES / 2))).expect("escapes at the limit");
    refused_at(
        &place(&format!("{}x", "\\n".repeat(MAX_STRING_BYTES / 2))),
        STRING_TOO_LONG,
        3,
        42,
    );
    // Multi-byte characters count by their bytes.
    let e = "\u{e9}".repeat(MAX_STRING_BYTES / 2);
    parse_text(&place(&e)).expect("2,048 two-byte characters");
    refused_at(&place(&format!("{e}x")), STRING_TOO_LONG, 3, 42);
}

#[test]
fn list_nesting_is_pinned_and_deep_nesting_is_refused_in_bounded_time() {
    parse_text(&format!(
        "{HEAD}block g = CDL.Reals.MatrixGain(K = [[1.0]])\n"
    ))
    .expect("depth 2");
    refused_at(
        &format!("{HEAD}block g = CDL.Reals.MatrixGain(K = [[[1.0]]])\n"),
        NESTING_TOO_DEEP,
        3,
        38,
    );
    // Thousands of brackets on one line: refused at the third, never recursing past it.
    let deep = "[".repeat(MAX_LINE_BYTES - 40);
    refused_at(
        &format!("{HEAD}block g = C(K = {deep}\n"),
        NESTING_TOO_DEEP,
        3,
        19,
    );
}

/// `count` statements of one kind after the header, each made by `statement(i)`.
fn many(count: usize, statement: impl Fn(usize) -> String) -> String {
    let mut text = HEAD.to_owned();
    for i in 0..count {
        text.push_str(&statement(i));
        text.push('\n');
    }
    text
}

#[test]
fn the_statement_count_of_each_kind_is_pinned() {
    let kinds: [(&str, Maker<'_>, u32); 5] = [
        ("parameters", &|i| format!("param p{i} : Real = 1"), 1),
        ("inputs", &|i| format!("in u{i} : Real"), 1),
        ("outputs", &|i| format!("out y{i} : Real"), 1),
        ("blocks", &|i| format!("block b{i} = CDL.Reals.Abs"), 1),
        ("connections", &|i| format!("u{i} -> y{i}"), 1),
    ];
    let line = u32::try_from(MAX_STATEMENTS_PER_KIND + 3).expect("fits");
    for (kind, statement, column) in kinds {
        let at_limit = many(MAX_STATEMENTS_PER_KIND, statement);
        let model = parse_text(&at_limit).unwrap_or_else(|e| panic!("{kind}: {e}"));
        let len = match kind {
            "parameters" => model.params.len(),
            "inputs" => model.inputs.len(),
            "outputs" => model.outputs.len(),
            "blocks" => model.blocks.len(),
            _ => model.connections.len(),
        };
        assert_eq!(len, MAX_STATEMENTS_PER_KIND, "{kind}");
        refused_at(
            &many(MAX_STATEMENTS_PER_KIND + 1, statement),
            TOO_MANY_STATEMENTS,
            line,
            column,
        );
    }
}

#[test]
fn the_assignment_count_of_one_block_is_pinned_elements_included() {
    let block = |count: usize| {
        let mut text = format!("{HEAD}block b = CDL.Reals.MultiSum(k = [\n");
        for i in 0..count {
            text.push_str(if i + 1 == count {
                "  1.0\n"
            } else {
                "  1.0,\n"
            });
        }
        text.push_str("])\n");
        text
    };
    let model = parse_text(&block(MAX_STATEMENTS_PER_KIND)).expect("at the limit");
    assert_eq!(model.blocks[0].assignments.len(), MAX_STATEMENTS_PER_KIND);
    let line = u32::try_from(MAX_STATEMENTS_PER_KIND + 4).expect("fits");
    refused_at(
        &block(MAX_STATEMENTS_PER_KIND + 1),
        TOO_MANY_STATEMENTS,
        line,
        3,
    );
}
