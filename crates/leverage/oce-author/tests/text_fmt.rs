//! `fmt` (issue #482; decisions 6 and 12): parse then print, idempotent over canonical,
//! non-canonical and comment-laden input, with comment and blank-line preservation goldens and one
//! test per printer convention the crate README documents.

mod support;

use oce_author::{
    Literal, MALFORMED_NUMBER, ParamValue, fmt_text, parse_text, print_text, to_wire_string,
};

use support::text::{bundle_model, bundles, enumerated_models, note_text_example};
use support::{compare_or_bless, fixture};

/// The hand-written `fmt` inputs under `tests/fixtures/text/fmt/`, each with a blessed output.
const FMT_CASES: &[&str] = &["arrays", "comments", "sections"];

fn fmt_input(case: &str) -> String {
    std::fs::read_to_string(fixture(&format!("text/fmt/{case}.in.oce"))).expect("input")
}

fn header() -> &'static str {
    "sequence s\nextends none\n"
}

/// `fmt`, asserting idempotence on the way.
fn fmt_once(text: &str) -> String {
    let once = fmt_text(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let twice = fmt_text(&once).unwrap_or_else(|e| panic!("{e}\n{once}"));
    assert_eq!(twice, once, "fmt is idempotent\n--- input\n{text}");
    once
}

#[test]
fn comment_and_layout_goldens() {
    for case in FMT_CASES {
        let out = fmt_once(&fmt_input(case));
        compare_or_bless(&fixture(&format!("text/fmt/{case}.out.oce")), &out);
    }
}

#[test]
fn fmt_is_idempotent_over_every_text_the_tests_hold() {
    let mut inputs: Vec<String> = FMT_CASES.iter().map(|c| fmt_input(c)).collect();
    inputs.push(note_text_example());
    inputs.extend(bundles().iter().map(|(_, w)| print_text(&bundle_model(w))));
    inputs.extend(enumerated_models().iter().map(print_text));
    for text in inputs {
        fmt_once(&text);
    }
}

#[test]
fn canonical_text_is_its_own_fmt() {
    for (name, _) in bundles() {
        let text = std::fs::read_to_string(fixture(&format!("text/{name}.oce"))).expect("golden");
        assert_eq!(fmt_text(&text).expect("parses"), text, "{name}");
    }
    assert_eq!(
        fmt_text(&note_text_example()).expect("parses"),
        note_text_example()
    );
}

#[test]
fn fmt_keeps_every_comment_and_the_blank_lines_before_statements() {
    let input = fmt_input("comments");
    let out = fmt_once(&input);
    for line in input.lines() {
        if let Some((_, comment)) = line.split_once('#') {
            assert!(
                out.contains(&format!("#{comment}")),
                "lost `#{comment}`\n{out}"
            );
        }
    }
    let model = parse_text(&input).expect("parses");
    assert_eq!(
        model.annotations.comments,
        [
            " File header, line one.",
            "   indented header line",
            " end-of-line on sequence",
            " between sequence and extends",
            " end-of-line on extends",
        ]
    );
    assert_eq!(
        model.trailing_comments,
        [
            " Trailing comment one.",
            "Trailing comment two, after a blank."
        ]
    );
    // The blank run after `extends` collapses to one blank line before the parameter's comment.
    assert!(out.contains(
        "extends none\n\n# Comment above the parameter, after a blank run.\n# end-of-line on the parameter\nparam"
    ));
    // The blank line inside the block's list stays before `uHigh`.
    assert!(
        out.contains("  uLow = 0.0 K,\n\n  uHigh = dTHys\n)"),
        "{out}"
    );
}

#[test]
fn an_end_of_line_comment_moves_above_its_statement() {
    let out = fmt_once(&format!("{}param p : Real = 1 # why one\n", header()));
    assert_eq!(out, format!("{}# why one\nparam p : Real = 1\n", header()));
    // Inside a block's list it moves above its assignment; after the open parenthesis or the
    // close, above the block.
    let out = fmt_once(&format!(
        "{}block h = CDL.Reals.Hysteresis( # on the block\n  uLow = 0.0, # on uLow\n  uHigh = 1.0\n) # also on the block\n",
        header()
    ));
    assert_eq!(
        out,
        format!(
            "{}# on the block\n# also on the block\nblock h = CDL.Reals.Hysteresis(\n  # on uLow\n  uLow = 0.0,\n  uHigh = 1.0\n)\n",
            header()
        )
    );
}

#[test]
fn trailing_spaces_in_a_comment_are_dropped() {
    let out = fmt_once(&format!("{}# spaced   \nparam p : Real = 1\n", header()));
    assert!(out.contains("# spaced\nparam"), "{out}");
}

#[test]
fn statements_regroup_into_section_order_and_keep_authored_order_inside() {
    let out = fmt_once(&format!(
        "{}b.y -> o\nblock b = CDL.Reals.Abs\nout o : Real\nparam z : Real = 1\nblock a = CDL.Reals.Abs\nparam y : Real = 2\nin u : Real\nu -> a.u\n",
        header()
    ));
    assert_eq!(
        out,
        format!(
            "{}param z : Real = 1\nparam y : Real = 2\nin u : Real\nout o : Real\nblock b = CDL.Reals.Abs\nblock a = CDL.Reals.Abs\nb.y -> o\nu -> a.u\n",
            header()
        )
    );
}

#[test]
fn reals_take_their_shortest_spelling_and_integers_keep_theirs() {
    let out = fmt_once(&format!(
        "{}param a : Real = 1.50\nparam b : Real = 1E5\nparam c : Real = 0.000001000\nparam d : Real = -0.0\nparam e : Real = 1\nparam f : Integer = -0\nparam g : Real = 100e-2\n",
        header()
    ));
    assert_eq!(
        out,
        format!(
            "{}param a : Real = 1.5\nparam b : Real = 100000.0\nparam c : Real = 1e-6\nparam d : Real = -0.0\nparam e : Real = 1\nparam f : Integer = 0\nparam g : Real = 1.0\n",
            header()
        )
    );
    let model = parse_text(&out).expect("parses");
    assert_eq!(
        model.params[4].value,
        Literal::Integer(1),
        "an Integer never becomes a Real"
    );
}

#[test]
fn a_leading_plus_or_zero_is_refused_not_normalized() {
    for value in ["+1", "01", "00.5", "+0.5", "-01"] {
        let error =
            fmt_text(&format!("{}param p : Real = {value}\n", header())).expect_err("refused");
        assert_eq!(error.code, MALFORMED_NUMBER, "{value}: {error}");
    }
}

#[test]
fn suffixes_survive_fmt_and_never_reach_the_wire() {
    let text = format!(
        "{}param t : Real = 86400 s\nblock d = CDL.Logical.TrueDelay(delayTime = 1.0 s)\n",
        header()
    );
    let out = fmt_once(&text);
    assert_eq!(out, text);
    let plain = text.replace("86400 s", "86400").replace("1.0 s", "1.0");
    assert_ne!(plain, text);
    assert_eq!(
        to_wire_string(&parse_text(&text).expect("parses")),
        to_wire_string(&parse_text(&plain).expect("parses"))
    );
}

#[test]
fn blocks_print_on_one_line_only_when_short_and_unannotated() {
    let one = fmt_once(&format!(
        "{}block h = CDL.Reals.Hysteresis(\n  uLow = 0.0,\n  uHigh = 1.0\n)\n",
        header()
    ));
    assert!(one.contains("block h = CDL.Reals.Hysteresis(uLow = 0.0, uHigh = 1.0)\n"));
    let three = fmt_once(&format!(
        "{}block c = CDL.Reals.PID(k = 1.0, Ti = 2.0, Td = 3.0)\n",
        header()
    ));
    assert!(three.contains("block c = CDL.Reals.PID(\n  k = 1.0,\n  Ti = 2.0,\n  Td = 3.0\n)\n"));
    let empty = fmt_once(&format!("{}block s = CDL.Reals.Subtract()\n", header()));
    assert!(empty.contains("block s = CDL.Reals.Subtract\n"));
    // Exactly 100 columns stays on one line; 101 breaks.
    for (len, one_line) in [(100, true), (101, false)] {
        let prefix = "block a = CDL.Utilities.Assert(message = \"";
        let pad = "x".repeat(len - prefix.len() - 2);
        let out = fmt_once(&format!("{}{prefix}{pad}\")\n", header()));
        assert_eq!(
            out.contains(&format!("{prefix}{pad}\")\n")),
            one_line,
            "{len}"
        );
    }
}

#[test]
fn arrays_regroup_only_complete_consecutive_uncommented_runs() {
    let out = fmt_once(&fmt_input("arrays"));
    for expected in [
        "block sum3 = CDL.Reals.MultiSum(nin = 3, k = [1, 2, 3])\n",
        "block sum4 = CDL.Reals.MultiSum(k = [1], k_3 = 3)\n",
        "block sum5 = CDL.Reals.MultiSum(k = [1 K, 2 K, 3 K])\n",
        "block gain = CDL.Reals.MatrixGain(K = [[0.5, 0.5], [1.5, -1.5]])\n",
        "block gain2 = CDL.Reals.MatrixGain(K = [[1.0, 0.0], [0.0, 1.0]])\n",
        "block gain3 = CDL.Reals.MatrixGain(K_2_1 = 1.0, K = [[2.0]])\n",
        "block odd = CDL.Reals.Nope(x_1 = 1, x_2 = 2)\n",
    ] {
        assert!(out.contains(expected), "missing {expected:?}\n{out}");
    }
    let commented = fmt_once(&fmt_input("comments"));
    assert!(
        // The first element alone is a complete run of one; the commented second stays flat.
        commented.contains("  # inside the list, before its first item\n  k = [1.0],\n  # above the second element: the list prints flat\n  k_2 = 2.0\n"),
        "{commented}"
    );
}

#[test]
fn a_list_lowers_to_the_catalog_element_names() {
    let model = parse_text(&fmt_input("arrays")).expect("parses");
    let names = |block: usize| -> Vec<String> {
        model.blocks[block]
            .assignments
            .iter()
            .map(|a| a.name.as_str().to_owned())
            .collect()
    };
    assert_eq!(names(2), ["k_1", "k_2", "k_3"]);
    assert_eq!(names(4), ["K_1_1", "K_1_2", "K_2_1", "K_2_2"]);
    assert_eq!(
        names(6),
        [
            "table_1_1",
            "table_1_2",
            "table_2_1",
            "table_2_2",
            "smoothness"
        ]
    );
    // An unknown class lowers by the same shape, so the validator names the elements.
    assert_eq!(names(7), ["x_1", "x_2"]);
    assert_eq!(
        model.blocks[2].assignments[1].suffix.as_deref(),
        Some("K"),
        "each element keeps its own suffix"
    );
    assert_eq!(
        model.blocks[2].assignments[1].value,
        ParamValue::Literal(Literal::Integer(2))
    );
}

#[test]
fn a_long_array_wraps_one_part_per_line_and_reads_back() {
    let rows: Vec<String> = (0..12)
        .map(|i| format!("[{}.0, {i}.5]", i * 3600))
        .collect();
    let text = format!(
        "{}block tab = CDL.Reals.Sources.TimeTable(table = [{}])\n",
        header(),
        rows.join(", ")
    );
    let out = fmt_once(&text);
    assert!(
        out.contains("block tab = CDL.Reals.Sources.TimeTable(\n  table = [\n    [0.0, 0.5],\n    [3600.0, 1.5],\n"),
        "{out}"
    );
    assert!(out.contains("    [39600.0, 11.5]\n  ]\n)\n"), "{out}");
    assert_eq!(
        parse_text(&out).expect("parses"),
        parse_text(&text).expect("parses")
    );
    let vector: Vec<String> = (1..=40).map(|i| i.to_string()).collect();
    let out = fmt_once(&format!(
        "{}block s = CDL.Reals.MultiSum(k = [{}])\n",
        header(),
        vector.join(", ")
    ));
    assert!(out.contains("  k = [\n    1,\n    2,\n"), "{out}");
    assert!(out.contains("    40\n  ]\n)\n"), "{out}");
}
