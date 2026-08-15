//! Adversarial tests for the Line raw-output boundary.

use super::*;

const HEADER: &str =
    "\"time\",\"f1\",\"f2\",\"u\",\"x1\",\"x2\",\"yAbove\",\"yBelow\",\"yBoth\",\"yUnlimited\"\n";
const ROW: &str = "0,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n";

fn valid() -> Vec<u8> {
    format!("{HEADER}{ROW}1,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n1,1.25,3.25,-2,-2,2,1.25,1.25,1.25,1.25\n").into_bytes()
}

#[test]
fn valid_event_group_keeps_the_post_event_row_and_reorders_columns() {
    let result = canonicalize_bytes(&valid(), "line").unwrap();
    assert_eq!(result.raw_rows.len(), 3);
    assert_eq!(result.group_sizes, [1, 2]);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[1].u.to_bits(), (-2.0_f64).to_bits());
    assert_eq!(
        result.bytes,
        b"#1\n# columns: time x1 f1 x2 f2 u yBoth yBelow yAbove yUnlimited\ndouble line(2,10)\n0 -2 1.25 2 3.25 -4 1.25 1.25 0.25 0.25\n1 -2 1.25 2 3.25 -2 1.25 1.25 1.25 1.25\n"
    );
}

#[test]
fn raw_header_identity_width_and_quoting_are_closed() {
    for (input, code) in [
        (
            format!("{}{}", HEADER.replace("\"time\"", "time"), ROW),
            ErrorCode::HeaderIdentity,
        ),
        (
            format!(
                "{}{}",
                HEADER.replace("\"f1\",\"f2\"", "\"f2\",\"f1\""),
                ROW
            ),
            ErrorCode::HeaderIdentity,
        ),
        (
            format!(
                "{}{}",
                HEADER.replace("\"yUnlimited\"", "\"yUnlimited\",\"z\""),
                ROW
            ),
            ErrorCode::HeaderIdentity,
        ),
        (
            format!("{HEADER}0,1.25,3.25,-4,-2,2,0.25,1.25,1.25\n"),
            ErrorCode::Shape,
        ),
    ] {
        assert_eq!(
            canonicalize_bytes(input.as_bytes(), "line")
                .unwrap_err()
                .code,
            code
        );
    }
}

#[test]
fn every_non_finite_column_is_rejected() {
    for column in 0..MAX_COLUMNS {
        for token in ["NaN", "inf", "-inf"] {
            let mut cells = ROW.trim_end().split(',').collect::<Vec<_>>();
            cells[column] = token;
            let input = format!("{HEADER}{}\n", cells.join(","));
            assert_eq!(
                canonicalize_bytes(input.as_bytes(), "line")
                    .unwrap_err()
                    .code,
                ErrorCode::CellType,
                "column {column} token {token}"
            );
        }
    }
}

#[test]
fn malformed_cells_times_and_names_fail_with_typed_codes() {
    for (input, code) in [
        (
            format!("{HEADER}0,1.25,3.25,\"-4\",-2,2,0.25,1.25,1.25,0.25\n"),
            ErrorCode::CellType,
        ),
        (
            format!("{HEADER}1,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n{ROW}"),
            ErrorCode::TimeOrder,
        ),
        (
            format!(
                "{HEADER}{ROW}1,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n0,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n"
            ),
            ErrorCode::TimeOrder,
        ),
        (
            format!("{}{}", HEADER.replace('\n', "\r\n"), ROW),
            ErrorCode::CsvSyntax,
        ),
        (format!("{HEADER}{}", ROW.trim_end()), ErrorCode::CsvSyntax),
    ] {
        assert_eq!(
            canonicalize_bytes(input.as_bytes(), "line")
                .unwrap_err()
                .code,
            code
        );
    }
    assert_eq!(
        canonicalize_bytes(&valid(), "not-a-name").unwrap_err().code,
        ErrorCode::Scenario
    );
}

#[test]
fn hostile_quoted_fields_return_errors_without_unwinding() {
    for input in [
        format!("{HEADER}\"unterminated,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n"),
        format!("{HEADER}\"0\"x,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n"),
        format!("{HEADER}0\"x,1.25,3.25,-4,-2,2,0.25,1.25,1.25,0.25\n"),
    ] {
        let result = std::panic::catch_unwind(|| canonicalize_bytes(input.as_bytes(), "line"));
        assert!(result.is_ok(), "hostile CSV unwound the parser");
        assert!(matches!(
            result.unwrap().unwrap_err().code,
            ErrorCode::CsvSyntax | ErrorCode::CellType
        ));
    }
}

#[test]
fn configured_bounds_reject_their_first_out_of_range_value() {
    let oversized = vec![b' '; MAX_FILE_BYTES + 1];
    assert_eq!(
        canonicalize_bytes(&oversized, "line").unwrap_err().code,
        ErrorCode::FileSize
    );

    let mut too_many = HEADER.as_bytes().to_vec();
    for _ in 0..=MAX_ROWS {
        too_many.extend_from_slice(ROW.as_bytes());
    }
    assert_eq!(
        canonicalize_bytes(&too_many, "line").unwrap_err().code,
        ErrorCode::Shape
    );

    let long_cell = "1".repeat(MAX_CELL_BYTES + 1);
    let input = format!("{HEADER}0,{long_cell},3.25,-4,-2,2,0.25,1.25,1.25,0.25\n");
    assert_eq!(
        canonicalize_bytes(input.as_bytes(), "line")
            .unwrap_err()
            .code,
        ErrorCode::CellType
    );

    let long_line = format!("{HEADER}{}\n", "0".repeat(MAX_LINE_BYTES + 1));
    assert_eq!(
        canonicalize_bytes(long_line.as_bytes(), "line")
            .unwrap_err()
            .code,
        ErrorCode::CsvSyntax
    );
    assert_eq!(
        canonicalize_bytes(&valid(), &"x".repeat(MAX_TABLE_NAME_BYTES + 1))
            .unwrap_err()
            .code,
        ErrorCode::Scenario
    );
    assert_eq!(MAX_CELLS, MAX_ROWS * MAX_COLUMNS);
}

#[cfg(unix)]
#[test]
fn path_reader_rejects_symlinks_directories_devices_and_fifos() {
    use std::os::unix::fs::symlink;

    let directory = unique_temp_dir("oce-line-canonicalizer-path");
    let regular = directory.join("regular.csv");
    let link = directory.join("link.csv");
    let fifo = directory.join("input.fifo");
    std::fs::write(&regular, valid()).unwrap();
    assert_eq!(canonicalize_path(&regular, "line").unwrap().rows.len(), 2);
    symlink(&regular, &link).unwrap();
    for path in [&link, &directory, Path::new("/dev/null")] {
        assert_eq!(read_bounded_path(path).unwrap_err().code, ErrorCode::Io);
    }
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(read_bounded_path(&fifo).unwrap_err().code, ErrorCode::Io);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt as _;
    let root = std::env::temp_dir().canonicalize().unwrap();
    for nonce in 0_u32..1024 {
        let candidate = root.join(format!("{prefix}-{}-{nonce}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => return candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot claim temporary directory: {error}"),
        }
    }
    panic!("cannot claim a unique temporary directory")
}
