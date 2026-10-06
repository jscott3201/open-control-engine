//! Adversarial tests for the Reliefs raw-output boundary.

use super::*;

const HEADER: &str = concat!(
    "\"time\",\"uOutDam_max\",\"uOutDam_min\",\"uRetDam_max\",",
    "\"uRetDam_min\",\"uTSup\",\"yOutDam\",\"yRetDam\"\n"
);
const ROW: &str = "0,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n";

fn valid() -> Vec<u8> {
    format!(
        "{HEADER}{ROW}60,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n60,0.875,0.25,0.75,0.125,-0.25,0.25,0.75\n"
    )
    .into_bytes()
}

#[test]
fn complete_tuple_change_keeps_the_post_event_source_row() {
    let result = canonicalize_bytes(&valid(), "reliefs").unwrap();
    assert_eq!(result.raw_rows.len(), 3);
    assert_eq!(result.group_sizes, [1, 2]);
    assert_eq!(result.grouped_rows.len(), 2);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[1].source_index, 2);
    assert_eq!(result.rows[1].u_t_sup.to_bits(), (-0.25_f64).to_bits());
    assert_eq!(
        result.bytes,
        b"#1\n# columns: time uTSup uOutDam_min uOutDam_max uRetDam_min uRetDam_max yOutDam yRetDam\ndouble reliefs(2,8)\n0 -0.5 0.25 0.875 0.125 0.75 0.25 0.75\n60 -0.25 0.25 0.875 0.125 0.75 0.25 0.75\n"
    );
}

#[test]
fn keep_first_preserves_tuples_but_changes_source_and_timestamp_provenance() {
    let input = format!(
        "{HEADER}{ROW}60,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n60,0.875,0.25,0.75,0.125,-0.25,0.25,0.75\n120,0.875,0.25,0.75,0.125,-0.25,0.25,0.75\n"
    );
    let last = canonicalize_bytes(input.as_bytes(), "last").unwrap();
    let first =
        canonicalize_bytes_with_selection(input.as_bytes(), "first", ProjectionSelection::First)
            .unwrap();
    assert_eq!(first.raw_rows, last.raw_rows);
    assert_eq!(first.group_sizes, last.group_sizes);
    assert_eq!(first.rows.len(), last.rows.len());
    assert_eq!(first.rows[1].input_bits(), last.rows[1].input_bits());
    assert_ne!(first.rows[1].source_index, last.rows[1].source_index);
    assert_ne!(first.rows[1].time.to_bits(), last.rows[1].time.to_bits());
}

#[test]
fn header_width_and_quoting_are_closed() {
    for (input, code) in [
        (
            format!("{}{}", HEADER.replace("\"time\"", "time"), ROW),
            ErrorCode::HeaderIdentity,
        ),
        (
            format!(
                "{}{}",
                HEADER.replace(
                    "\"uOutDam_max\",\"uOutDam_min\"",
                    "\"uOutDam_min\",\"uOutDam_max\""
                ),
                ROW
            ),
            ErrorCode::HeaderIdentity,
        ),
        (
            format!("{HEADER}0,0.875,0.25,0.75,0.125,-0.5,0.25\n"),
            ErrorCode::Shape,
        ),
    ] {
        assert_eq!(
            canonicalize_bytes(input.as_bytes(), "reliefs")
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
                canonicalize_bytes(input.as_bytes(), "reliefs")
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
            format!("{HEADER}0,0.875,0.25,0.75,0.125,\"-0.5\",0.25,0.75\n"),
            ErrorCode::CellType,
        ),
        (
            format!("{HEADER}1,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n{ROW}"),
            ErrorCode::TimeOrder,
        ),
        (
            format!("{}{}", HEADER.replace('\n', "\r\n"), ROW),
            ErrorCode::CsvSyntax,
        ),
        (format!("{HEADER}{}", ROW.trim_end()), ErrorCode::CsvSyntax),
    ] {
        assert_eq!(
            canonicalize_bytes(input.as_bytes(), "reliefs")
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
fn hostile_quotes_return_errors_without_unwinding() {
    for input in [
        format!("{HEADER}\"unterminated,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n"),
        format!("{HEADER}\"0\"x,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n"),
        format!("{HEADER}0\"x,0.875,0.25,0.75,0.125,-0.5,0.25,0.75\n"),
    ] {
        let result = std::panic::catch_unwind(|| canonicalize_bytes(input.as_bytes(), "reliefs"));
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap().unwrap_err().code,
            ErrorCode::CsvSyntax | ErrorCode::CellType
        ));
    }
}

#[test]
fn configured_bounds_reject_the_first_out_of_range_value() {
    assert_eq!(
        canonicalize_bytes(&vec![b' '; MAX_FILE_BYTES + 1], "reliefs")
            .unwrap_err()
            .code,
        ErrorCode::FileSize
    );
    let mut too_many = HEADER.as_bytes().to_vec();
    for _ in 0..=MAX_ROWS {
        too_many.extend_from_slice(ROW.as_bytes());
    }
    assert_eq!(
        canonicalize_bytes(&too_many, "reliefs").unwrap_err().code,
        ErrorCode::Shape
    );
    let long_cell = "1".repeat(MAX_CELL_BYTES + 1);
    let input = format!("{HEADER}0,{long_cell},0.25,0.75,0.125,-0.5,0.25,0.75\n");
    assert_eq!(
        canonicalize_bytes(input.as_bytes(), "reliefs")
            .unwrap_err()
            .code,
        ErrorCode::CellType
    );
    let long_line = format!("{HEADER}{}\n", "0".repeat(MAX_LINE_BYTES + 1));
    assert_eq!(
        canonicalize_bytes(long_line.as_bytes(), "reliefs")
            .unwrap_err()
            .code,
        ErrorCode::CsvSyntax
    );
    assert_eq!(MAX_CELLS, MAX_ROWS * MAX_COLUMNS);
}

#[cfg(unix)]
#[test]
fn path_reader_rejects_symlinks_directories_devices_and_fifos() {
    use std::os::unix::fs::symlink;

    let directory = unique_temp_dir("oce-reliefs-canonicalizer-path");
    let real_parent = directory.join("real");
    let linked_parent = directory.join("linked");
    std::fs::create_dir(&real_parent).unwrap();
    let regular = real_parent.join("regular.csv");
    let link = directory.join("link.csv");
    let fifo = directory.join("input.fifo");
    std::fs::write(&regular, valid()).unwrap();
    assert_eq!(
        canonicalize_path(&regular, "reliefs").unwrap().rows.len(),
        2
    );
    symlink(&real_parent, &linked_parent).unwrap();
    symlink(&regular, &link).unwrap();
    for path in [
        &linked_parent.join("regular.csv"),
        &link,
        &directory,
        Path::new("/dev/null"),
    ] {
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
