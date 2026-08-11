use super::*;

const HEADER: &str = "\"time\",\"u1\",\"u2\",\"y\"\n";

fn csv(rows: &str) -> Vec<u8> {
    format!("{HEADER}{rows}").into_bytes()
}

fn error(input: &[u8]) -> ErrorCode {
    canonicalize_bytes(input, "table").unwrap_err().code
}

#[test]
fn named_columns_project_adjacent_bit_equal_groups_by_last_row() {
    let input =
        b"\"extra\",\"u2\",\"time\",\"y\",\"u1\"\n9,0,0,1,0\n9,0,60,1,0\n9,1,60,1,0\n9,1,120,0,1\n";
    let result = canonicalize_bytes(input, "table").unwrap();
    assert_eq!(result.group_sizes, [1, 2, 1]);
    assert_eq!(result.raw_rows.len(), 4);
    assert_eq!(result.rows.len(), 3);
    assert_eq!((result.rows[1].u1, result.rows[1].u2), (false, true));
    assert_eq!(
        result.bytes,
        b"#1\n# columns: time u1 u2 y\ndouble table(3,4)\n0 0.0 0.0 1.0\n60 0.0 1.0 1.0\n120 1.0 1.0 0.0\n"
    );
}

#[test]
fn malformed_quoting_and_utf8_are_csv_syntax_errors() {
    for input in [
        b"\"time,\"u1\",\"u2\",\"y\"\n0,0,0,1\n".as_slice(),
        b"\"time\"x,\"u1\",\"u2\",\"y\"\n0,0,0,1\n",
        b"ti\"me,u1,u2,y\n0,0,0,1\n",
        b"time,u1,u2,y\r\n0,0,0,1\r\n",
        b"time,u1,u2,y\n\n0,0,0,1\n",
        b"time,u1,u2,y\n0,0,0,\xff\n",
    ] {
        assert_eq!(error(input), ErrorCode::CsvSyntax, "{input:?}");
    }
}

#[test]
fn syntax_failure_precedes_header_shape_and_cell_failures() {
    let input = b"\"missing\",\"u1\",\"u2\",\"y\"\n0,0\n\"unterminated\n";
    assert_eq!(error(input), ErrorCode::CsvSyntax);
}

#[test]
fn required_headers_must_each_exist_exactly_once() {
    assert_eq!(
        error(b"\"time\",\"u1\",\"u2\",\"other\"\n0,0,0,1\n"),
        ErrorCode::HeaderIdentity
    );
    assert_eq!(
        error(b"\"time\",\"u1\",\"u2\",\"y\",\"y\"\n0,0,0,1,1\n"),
        ErrorCode::HeaderIdentity
    );
}

#[test]
fn row_width_and_missing_rows_are_shape_errors() {
    assert_eq!(error(HEADER.as_bytes()), ErrorCode::Shape);
    assert_eq!(error(&csv("0,0,0\n")), ErrorCode::Shape);
    assert_eq!(error(&csv("0,0,0,1,9\n")), ErrorCode::Shape);
}

#[test]
fn booleans_are_only_unquoted_literal_zero_or_one() {
    for value in ["0.0", "true", "-0", "2", "\"1\""] {
        assert_eq!(
            error(&csv(&format!("0,{value},0,1\n"))),
            ErrorCode::CellType
        );
    }
}

#[test]
fn time_must_be_finite_nondecreasing_and_groups_contiguous() {
    for value in ["NaN", "inf", "-inf"] {
        assert_eq!(
            error(&csv(&format!("{value},0,0,1\n"))),
            ErrorCode::CellType
        );
    }
    assert_eq!(error(&csv("1,0,0,1\n0,0,0,1\n")), ErrorCode::TimeOrder);
    assert_eq!(
        error(&csv("-0,0,0,1\n0,0,0,1\n-0,0,0,1\n")),
        ErrorCode::TimeOrder
    );
}

#[test]
fn empty_and_oversized_inputs_fail_before_other_classes() {
    assert_eq!(error(b""), ErrorCode::CsvSyntax);
    let oversized = vec![b'x'; MAX_FILE_BYTES + 1];
    assert_eq!(error(&oversized), ErrorCode::FileSize);
}

#[test]
fn file_size_accepts_limit_and_rejects_limit_plus_one() {
    let mut at_limit = csv("0,0,0,1");
    at_limit.extend(std::iter::repeat_n(b' ', MAX_FILE_BYTES - at_limit.len()));
    at_limit.push(b'\n');
    assert_eq!(at_limit.len(), MAX_FILE_BYTES + 1);
    let accepted = &at_limit[..MAX_FILE_BYTES];
    assert_ne!(error(accepted), ErrorCode::FileSize);
    assert_eq!(error(&at_limit), ErrorCode::FileSize);
}

#[test]
fn path_input_rejects_oversize_metadata_before_reading() {
    let path =
        std::env::temp_dir().join(format!("oce-openmodelica-oversize-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let file = std::fs::File::create(&path).unwrap();
    file.set_len((MAX_FILE_BYTES + 1) as u64).unwrap();
    drop(file);
    assert_eq!(
        canonicalize_path(&path, "table").unwrap_err().code,
        ErrorCode::FileSize
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn physical_line_accepts_limit_and_rejects_limit_plus_one() {
    let line_at = "x".repeat(MAX_LINE_BYTES - 8);
    let accepted = format!("{line_at},u1,u2,y\n0,0,0,1\n");
    assert_eq!(accepted.lines().next().unwrap().len(), MAX_LINE_BYTES);
    assert_eq!(error(accepted.as_bytes()), ErrorCode::HeaderIdentity);
    let rejected = accepted.replacen(&line_at, &format!("{line_at}x"), 1);
    assert_eq!(error(rejected.as_bytes()), ErrorCode::CsvSyntax);
}

#[test]
fn header_name_accepts_limit_and_rejects_limit_plus_one() {
    let at = "x".repeat(MAX_HEADER_BYTES);
    let accepted = format!("time,u1,u2,y,{at}\n0,0,0,1,0\n");
    canonicalize_bytes(accepted.as_bytes(), "table").unwrap();
    let rejected = accepted.replacen(&at, &format!("{at}x"), 1);
    assert_eq!(error(rejected.as_bytes()), ErrorCode::HeaderIdentity);
}

#[test]
fn numeric_cell_accepts_limit_and_rejects_limit_plus_one() {
    let at = format!("0.{}1", "0".repeat(MAX_CELL_BYTES - 3));
    assert_eq!(at.len(), MAX_CELL_BYTES);
    canonicalize_bytes(&csv(&format!("{at},0,0,1\n")), "table").unwrap();
    let over = format!("{at}0");
    assert_eq!(error(&csv(&format!("{over},0,0,1\n"))), ErrorCode::CellType);
}

#[test]
fn ignored_numeric_cell_accepts_limit_and_rejects_limit_plus_one() {
    let at = "0".repeat(MAX_CELL_BYTES);
    canonicalize_bytes(
        format!("time,u1,u2,y,extra\n0,0,0,1,{at}\n").as_bytes(),
        "table",
    )
    .unwrap();
    assert_eq!(
        error(format!("time,u1,u2,y,extra\n0,0,0,1,{at}0\n").as_bytes()),
        ErrorCode::CellType
    );
}

fn wide_csv(columns: usize) -> Vec<u8> {
    let mut headers = vec!["time", "u1", "u2", "y"];
    headers.extend(std::iter::repeat_n("x", columns - 4));
    let mut cells = vec!["0", "0", "0", "1"];
    cells.extend(std::iter::repeat_n("0", columns - 4));
    format!("{}\n{}\n", headers.join(","), cells.join(",")).into_bytes()
}

#[test]
fn column_count_accepts_limit_and_rejects_limit_plus_one() {
    canonicalize_bytes(&wide_csv(MAX_COLUMNS), "table").unwrap();
    assert_eq!(error(&wide_csv(MAX_COLUMNS + 1)), ErrorCode::Shape);
}

fn repeated_rows(count: usize) -> Vec<u8> {
    let mut input = HEADER.as_bytes().to_vec();
    for _ in 0..count {
        input.extend_from_slice(b"0,0,0,1\n");
    }
    input
}

#[test]
fn row_count_accepts_limit_and_rejects_limit_plus_one() {
    canonicalize_bytes(&repeated_rows(MAX_ROWS), "table").unwrap();
    assert_eq!(error(&repeated_rows(MAX_ROWS + 1)), ErrorCode::Shape);
}

#[test]
fn checked_cell_product_accepts_limit_and_rejects_limit_plus_one() {
    validate_shape(MAX_ROWS, MAX_COLUMNS).unwrap();
    assert_eq!(
        validate_shape(MAX_ROWS, MAX_COLUMNS + 1).unwrap_err().code,
        ErrorCode::Shape
    );
    assert_eq!(MAX_ROWS * MAX_COLUMNS, MAX_CELLS);
    assert!(MAX_CELLS.checked_add(1).unwrap() > MAX_CELLS);
}

#[test]
fn table_name_accepts_limit_and_rejects_limit_plus_one() {
    let input = csv("0,0,0,1\n");
    let at_limit = "x".repeat(MAX_TABLE_NAME_BYTES);
    canonicalize_bytes(&input, &at_limit).unwrap();
    assert_eq!(
        canonicalize_bytes(&input, &format!("{at_limit}x"))
            .unwrap_err()
            .code,
        ErrorCode::Scenario
    );
}

#[test]
fn syntax_failure_after_the_row_limit_precedes_shape_failure() {
    let mut input = repeated_rows(MAX_ROWS + 1);
    input.extend_from_slice(b"\"unterminated\n");
    assert_eq!(error(&input), ErrorCode::CsvSyntax);
}

#[test]
fn failure_classes_follow_the_declared_precedence() {
    assert_eq!(
        canonicalize_bytes(b"time,u1,u2\n1,invalid\n", "bad name")
            .unwrap_err()
            .code,
        ErrorCode::HeaderIdentity
    );
    assert_eq!(
        canonicalize_bytes(b"time,u1,u2,y\n1,invalid\n", "bad name")
            .unwrap_err()
            .code,
        ErrorCode::Shape
    );
    assert_eq!(
        canonicalize_bytes(&csv("1,0,0,1\n0,invalid,0,1\n"), "bad name")
            .unwrap_err()
            .code,
        ErrorCode::CellType
    );
    assert_eq!(
        canonicalize_bytes(&csv("1,0,0,1\n0,0,0,1\n"), "bad name")
            .unwrap_err()
            .code,
        ErrorCode::TimeOrder
    );
}

#[test]
fn diagnostics_and_typed_results_are_deterministic() {
    let valid = csv("0,0,0,1\n0,0,1,1\n");
    assert_eq!(
        canonicalize_bytes(&valid, "table"),
        canonicalize_bytes(&valid, "table")
    );
    let invalid = csv("1,0,0,1\n0,0,0,1\n");
    assert_eq!(
        canonicalize_bytes(&invalid, "table"),
        canonicalize_bytes(&invalid, "table")
    );
}
