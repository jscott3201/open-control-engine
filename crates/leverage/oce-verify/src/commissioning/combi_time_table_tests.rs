//! The `combiTimeTable` reader against OBC §12.5.1's grammar, at every bound it holds.

use super::*;

fn limits() -> ReplayLimits {
    ReplayLimits::default()
}

/// Read `text`'s one matrix, keeping `keep`.
fn read(text: &str, keep: &[u32]) -> Result<Matrix, SourceError> {
    read_matrix(text.as_bytes(), None, keep, &limits())
}

/// The code `text` is refused with, and where.
fn refused(text: &str, keep: &[u32]) -> (&'static str, Option<u64>, Option<u64>, Option<String>) {
    let error = read(text, keep).expect_err("refused");
    (
        error.code(),
        error.line(),
        error.row(),
        error.column().map(str::to_owned),
    )
}

/// OBC §12.5.1's example, as the page prints it.
const LISTED: &str = "#1\n# comment line\ndouble tab1(6,2)\n# time in seconds, column 1\n0 0\n1 0\n1 1\n2 4\n3 9\n4 16\n";

#[test]
fn the_specifications_own_example_reads_with_its_equal_times() {
    let matrix = read(LISTED, &[1, 2]).expect("reads");
    assert_eq!(matrix.rows, 6);
    assert_eq!(matrix.columns[0], [0.0, 1.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(matrix.columns[1], [0.0, 0.0, 1.0, 4.0, 9.0, 16.0]);
}

#[test]
fn only_the_columns_asked_for_are_kept_in_the_order_asked() {
    let text = "#1\ndouble t(2,3)\n1 2 3\n4 5 6\n";
    let matrix = read(text, &[3, 1]).expect("reads");
    assert_eq!(matrix.columns, [vec![3.0, 6.0], vec![1.0, 4.0]]);
    assert!(read(text, &[]).expect("reads").columns.is_empty());
}

#[test]
fn spaces_tabs_commas_semicolons_line_ends_and_comments_all_separate_numbers() {
    let texts = [
        "#1\ndouble t(2,2)\n1 2\n3 4\n",
        "#1\ndouble t(2,2)\n1,2\n3,4\n",
        "#1\ndouble t(2,2)\n1;2\n3;4\n",
        "#1\ndouble t(2,2)\n1\t2\n3\t4\n",
        "#1\r\ndouble t(2,2)\r\n1, 2\r\n3, 4\r\n",
        "#1\rdouble t(2,2)\r1, 2\r3, 4\r",
        "#1\ndouble t(2,2)\n1 2 3 4",
        "#1\ndouble t(2,2)\n1\n2\n3\n4\n",
        "#1\ndouble t(2,2)\n1 ,2,\n 3 ; 4 # trailing comment\n",
        "#1 the tag line's comment\n# a\ndouble t(2,2) # after\n1#c\n2 # c\n# a line\n3,#c\n4\n# end",
        "#1\n\n\ndouble t( 2 , 2 )\n1 2 3 4\n\n",
        "\u{feff}#1\ndouble t(2,2)\n1 2 3 4\n",
    ];
    for text in texts {
        let matrix = read(text, &[1, 2]).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert_eq!(matrix.columns, [vec![1.0, 3.0], vec![2.0, 4.0]], "{text:?}");
    }
}

#[test]
fn the_c_decimal_forms_are_read_and_rounded_correctly() {
    let cases: [(&str, u64); 12] = [
        ("2.3", 2.3_f64.to_bits()),
        ("-2", (-2.0_f64).to_bits()),
        ("+2.e4", 20000.0_f64.to_bits()),
        (".5", 0.5_f64.to_bits()),
        ("5.", 5.0_f64.to_bits()),
        ("1E-3", 0.001_f64.to_bits()),
        ("0.1", 0x3fb9_9999_9999_999a),
        // Halfway between two doubles: ties to even.
        ("9007199254740993", 0x4340_0000_0000_0000),
        ("-0", 0x8000_0000_0000_0000),
        ("5e-324", 1),
        // Below the smallest subnormal: zero of its sign.
        ("1e-400", 0),
        ("-1e-400", 0x8000_0000_0000_0000),
    ];
    for (text, expected) in cases {
        assert_eq!(
            number(text.as_bytes()).map(f64::to_bits),
            Ok(expected),
            "{text}"
        );
    }
    assert_eq!(number(b"1.7976931348623157e308"), Ok(f64::MAX));
}

#[test]
fn anything_outside_the_c_decimal_grammar_is_a_bad_number() {
    for text in [
        "inf", "-inf", "infinity", "nan", "NaN", "0x1p3", "1e", "e5", "+", "-", ".", "1.2.3",
        "--1", "+-1", "1e+", "1e5.0", "1_000", "\u{ff11}", "1d0", "true", "",
    ] {
        assert_eq!(number(text.as_bytes()), Err(Kind::BadNumber), "{text}");
    }
    for text in ["1e309", "-1e309", "1.8e308", "1e99999999999"] {
        assert_eq!(
            number(text.as_bytes()),
            Err(Kind::NonFiniteNumber),
            "{text}"
        );
    }
}

#[test]
fn a_refused_value_names_its_line_row_and_column() {
    let text = "#1\ndouble t(3,2)\n0 1\n5 inf\n10 2\n";
    assert_eq!(
        refused(text, &[1, 2]),
        ("bad-number", Some(4), Some(2), Some("2".to_owned()))
    );
    let text = "#1\ndouble t(2,2)\n0 1\n1e400 2\n";
    assert_eq!(
        refused(text, &[1]),
        ("non-finite-number", Some(4), Some(2), Some("1".to_owned()))
    );
    // A value of a column not kept is still read to the rules.
    let text = "#1\ndouble t(2,2)\n0 1\n1 nan\n";
    assert_eq!(refused(text, &[1]).0, "bad-number");
}

#[test]
fn a_declared_row_count_one_off_either_way_is_a_size_mismatch() {
    assert_eq!(
        refused("#1\ndouble t(3,2)\n0 1\n1 2\n", &[1]).0,
        "size-mismatch"
    );
    assert_eq!(
        refused("#1\ndouble t(1,2)\n0 1\n1 2\n", &[1]).0,
        "size-mismatch"
    );
    // One value short or over, not a row.
    assert_eq!(
        refused("#1\ndouble t(2,2)\n0 1\n1\n", &[1]).0,
        "size-mismatch"
    );
    assert_eq!(
        refused("#1\ndouble t(2,2)\n0 1\n1 2 3\n", &[1]).0,
        "size-mismatch"
    );
    // A matrix cut short by the next header.
    assert_eq!(
        refused("#1\ndouble a(2,1)\n0\ndouble b(1,1)\n0\n", &[1]).0,
        "size-mismatch"
    );
}

#[test]
fn the_tag_must_be_the_first_two_bytes() {
    for text in [
        "",
        "#",
        "#2\ndouble t(1,1)\n0\n",
        " #1\ndouble t(1,1)\n0\n",
        "\u{feff}\u{feff}#1\ndouble t(1,1)\n0\n",
        "\u{feff} #1\ndouble t(1,1)\n0\n",
        "double t(1,1)\n0\n",
        "# 1\ndouble t(1,1)\n0\n",
    ] {
        assert_eq!(
            refused(text, &[1]),
            ("format-tag-missing", Some(1), None, None),
            "{text:?}"
        );
    }
}

#[test]
fn a_header_other_than_double_name_rows_cols_is_refused() {
    for text in [
        "#1\nfloat t(1,1)\n0\n",
        "#1\nint t(1,1)\n0\n",
        "#1\ndouble\tt[1,1]\n0\n",
        "#1\ndouble t(1 1)\n0\n",
        "#1\ndouble t(1,1\n0\n",
        "#1\ndouble (1,1)\n0\n",
        "#1\ndouble 1t(1,1)\n0\n",
        "#1\ndouble t-1(1,1)\n0\n",
        "#1\ndoublet(1,1)\n0\n",
        "#1\ndouble t(0,1)\n",
        "#1\ndouble t(1,0)\n",
        "#1\ndouble t(-1,1)\n0\n",
        "#1\ndouble t(1,1) 0\n",
        "#1\ndouble t(99999999999999999999,1)\n0\n",
        "#1\n# no matrix\n",
        "#1\n",
        "#1\n0 1\n",
        "#1\ndouble t(1,1)\n,0\n",
        "#1\ndouble t(2,1)\n0,,1\n",
        "#1\ndouble t(2,1)\n0;,1\n",
        "#1\ndouble t(2,1)\n0 1,\n",
        "#1\ndouble t(2,1)\n0 1;\ndouble u(1,1)\n2\n",
        // A header starts its line: not indented, not after the last value on its line.
        "#1\n   double t(1,1)\n0\n",
        "#1\n\tdouble t(1,1)\n0\n",
        "#1\ndouble t(1,1)\n0 double u(1,1)\n1\n",
        // Text after the last value that is no number.
        "#1\ndouble t(1,1)\n0 x\n",
        "#1\ndouble t(1,1)\n0\nend\n",
        "#1\ndouble t(1,1)\n0\n1D3\n",
    ] {
        assert_eq!(refused(text, &[1]).0, "table-syntax", "{text:?}");
    }
}

#[test]
fn table_picks_one_of_several_matrices_and_must_be_unique() {
    let two = "#1\ndouble first(1,2)\n1 2\ndouble second(2,1)\n3\n4\n";
    let matrix = read_matrix(two.as_bytes(), Some("second"), &[1], &limits()).expect("reads");
    assert_eq!((matrix.rows, matrix.columns), (2, vec![vec![3.0, 4.0]]));
    let matrix = read_matrix(two.as_bytes(), Some("first"), &[2], &limits()).expect("reads");
    assert_eq!(matrix.columns, [vec![2.0]]);
    let required = read(two, &[1]).unwrap_err();
    assert_eq!(
        (required.code(), required.line()),
        ("table-required", Some(4))
    );
    let missing = read_matrix(two.as_bytes(), Some("third"), &[1], &limits()).unwrap_err();
    assert_eq!(missing.code(), "table-not-found");
    // Names compare byte for byte.
    let case = read_matrix(two.as_bytes(), Some("First"), &[1], &limits()).unwrap_err();
    assert_eq!(case.code(), "table-not-found");
    let twice = "#1\ndouble t(1,1)\n1\ndouble t(1,1)\n2\n";
    let error = read_matrix(twice.as_bytes(), Some("t"), &[1], &limits()).unwrap_err();
    assert_eq!((error.code(), error.line()), ("duplicate-table", Some(4)));
    // A matrix not chosen is still held to the grammar.
    let bad = "#1\ndouble t(1,1)\n1\ndouble u(1,1)\nx\n";
    let error = read_matrix(bad.as_bytes(), Some("t"), &[1], &limits()).unwrap_err();
    assert_eq!(error.code(), "bad-number");
}

#[test]
fn a_position_past_the_matrix_is_a_missing_column() {
    assert_eq!(
        refused("#1\ndouble t(1,2)\n0 1\n", &[1, 3]),
        ("column-missing", Some(2), None, Some("3".to_owned()))
    );
    assert_eq!(
        refused("#1\ndouble t(1,2)\n0 1\n", &[0]).0,
        "column-missing"
    );
    assert!(read("#1\ndouble t(1,2)\n0 1\n", &[2]).is_ok());
}

#[test]
fn each_bound_admits_its_limit_and_refuses_one_past() {
    let rows = |n: usize| format!("#1\ndouble t({n},1)\n{}", "0\n".repeat(n));
    let at = ReplayLimits {
        max_rows: 3,
        ..limits()
    };
    assert!(read_matrix(rows(3).as_bytes(), None, &[1], &at).is_ok());
    let error = read_matrix(rows(4).as_bytes(), None, &[1], &at).unwrap_err();
    assert_eq!((error.code(), error.line()), ("too-many-rows", Some(2)));

    let cols = |n: usize| format!("#1\ndouble t(1,{n})\n{}\n", vec!["0"; n].join(" "));
    let at = ReplayLimits {
        max_columns: 3,
        ..limits()
    };
    assert!(read_matrix(cols(3).as_bytes(), None, &[1], &at).is_ok());
    let error = read_matrix(cols(4).as_bytes(), None, &[1], &at).unwrap_err();
    assert_eq!(error.code(), "too-many-columns");

    let at = ReplayLimits {
        max_cell_bytes: 4,
        ..limits()
    };
    assert!(read_matrix(b"#1\ndouble t(1,1)\n1234\n", None, &[1], &at).is_ok());
    let error = read_matrix(b"#1\ndouble t(1,1)\n12345\n", None, &[1], &at).unwrap_err();
    assert_eq!(error.code(), "cell-too-long");
    assert!(read_matrix(b"#1\ndouble abcd(1,1)\n1\n", None, &[1], &at).is_ok());
    let error = read_matrix(b"#1\ndouble abcde(1,1)\n1\n", None, &[1], &at).unwrap_err();
    assert_eq!(error.code(), "cell-too-long");

    let file = b"#1\ndouble t(1,1)\n1\n";
    let at = ReplayLimits {
        max_source_bytes: file.len() as u64,
        ..limits()
    };
    assert!(read_matrix(file, None, &[1], &at).is_ok());
    let below = ReplayLimits {
        max_source_bytes: file.len() as u64 - 1,
        ..limits()
    };
    let error = read_matrix(file, None, &[1], &below).unwrap_err();
    assert_eq!(error.code(), "too-many-bytes");
}

#[test]
fn the_values_kept_are_held_to_the_byte_cap_before_any_is_stored() {
    // Two rows of two kept columns hold 32 bytes; the file itself is shorter.
    let file = b"#1\ndouble t(2,2)\n0 1\n2 3\n";
    let at = ReplayLimits {
        max_source_bytes: 32,
        ..limits()
    };
    assert!(read_matrix(file, None, &[1, 2], &at).is_ok());
    let past = ReplayLimits {
        max_source_bytes: 31,
        ..limits()
    };
    let error = read_matrix(file, None, &[1, 2], &past).unwrap_err();
    assert_eq!(error.code(), "too-many-bytes");
}

#[test]
fn a_header_declaring_far_more_than_the_file_holds_allocates_only_what_can_follow() {
    let huge = b"#1\ndouble t(16777216,4096)\n0\n";
    let mut result = None;
    let info = allocation_counter::measure(|| {
        result = Some(read_matrix(huge, None, &[1, 4096], &limits()));
    });
    assert_eq!(result.expect("ran").unwrap_err().code(), "size-mismatch");
    // A slot table of 4,096 entries and two near-empty columns, never 16,777,216 values.
    assert!(info.bytes_max < 16 * 1024, "{info:?}");
}

#[test]
fn a_comment_may_hold_any_bytes_and_a_value_may_not() {
    let file = b"#1 \xff\xfe\n# \x00\x01 \xc3\x28\ndouble t(1,1) # \xe2\x82\n7 # \x80\n";
    let matrix = read_matrix(file, None, &[1], &limits()).expect("reads");
    assert_eq!(matrix.columns, [vec![7.0]]);
    let file = b"#1\ndouble t(1,1)\n7\xff\n";
    assert_eq!(
        read_matrix(file, None, &[1], &limits()).unwrap_err().code(),
        "bad-number"
    );
}

#[test]
fn the_words_c_reads_as_numbers_and_fortran_exponents_are_bad_numbers_in_a_cell() {
    for word in [
        "inf", "-inf", "+inf", "infinity", "INF", "nan", "NaN", "-nan", "1D3", "1d-3",
    ] {
        let text = format!("#1\ndouble t(2,1)\n0\n{word}\n");
        assert_eq!(
            refused(&text, &[1]),
            ("bad-number", Some(4), Some(2), Some("1".to_owned())),
            "{word}"
        );
    }
}

#[test]
fn a_bare_carriage_return_ends_a_line_for_the_line_numbers() {
    // Old Mac line ends: each `\r` alone is a line end; `\r\n` is one line end.
    assert_eq!(
        refused("#1\rdouble t(3,1)\r0\r1\rx\r", &[1]),
        ("bad-number", Some(5), Some(3), Some("1".to_owned()))
    );
    assert_eq!(
        refused("#1\r\ndouble t(3,1)\r\n0\r\n1\r\nx\r\n", &[1]),
        ("bad-number", Some(5), Some(3), Some("1".to_owned()))
    );
    assert_eq!(refused("#1\r# c\r\rdouble t(1,1)\r\rx", &[1]).1, Some(6));
}

#[test]
fn a_number_after_the_last_value_is_a_size_mismatch_and_other_text_is_syntax() {
    assert_eq!(
        refused("#1\ndouble t(1,1)\n0 1e999\n", &[1]).0,
        "size-mismatch"
    );
    assert_eq!(
        refused("#1\ndouble t(1,1)\n0 +.5\n", &[1]).0,
        "size-mismatch"
    );
    assert_eq!(refused("#1\ndouble t(1,1)\n0 .\n", &[1]).0, "table-syntax");
}
