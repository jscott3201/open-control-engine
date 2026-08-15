//! Adversarial tests for the Toggle raw-output boundary.

use super::*;

const VALID: &[u8] = b"\"time\",\"clr\",\"u\",\"y\"\n0,0,1,1\n1,0,1,1\n1,0,0,1\n";

#[test]
fn valid_event_group_keeps_the_post_event_row() {
    let result = canonicalize_bytes(VALID, "toggle").unwrap();
    assert_eq!(result.raw_rows.len(), 3);
    assert_eq!(result.group_sizes, [1, 2]);
    assert_eq!(result.rows.len(), 2);
    assert!(!result.rows[1].u);
    assert_eq!(
        result.bytes,
        b"#1\n# columns: time u clr y\ndouble toggle(2,4)\n0 1.0 0.0 1.0\n1 0.0 0.0 1.0\n"
    );
}

#[test]
fn raw_header_is_exact_and_closed() {
    for (input, code) in [
        (
            b"time,clr,u,y\n0,0,1,1\n".as_slice(),
            ErrorCode::HeaderIdentity,
        ),
        (
            b"\"time\",\"u\",\"clr\",\"y\"\n0,1,0,1\n".as_slice(),
            ErrorCode::HeaderIdentity,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\",\"z\"\n0,0,1,1,0\n".as_slice(),
            ErrorCode::HeaderIdentity,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n0,0,1\n".as_slice(),
            ErrorCode::Shape,
        ),
    ] {
        assert_eq!(canonicalize_bytes(input, "toggle").unwrap_err().code, code);
    }
}

#[test]
fn malformed_cells_times_and_names_fail() {
    for (input, code) in [
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n0,0,2,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\nNaN,0,1,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\ninf,0,1,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n-inf,0,1,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n1,0,1,1\n0,0,1,1\n".as_slice(),
            ErrorCode::TimeOrder,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n0,0,1,1\n1,0,1,1\n0,0,1,1\n".as_slice(),
            ErrorCode::TimeOrder,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\r\n0,0,1,1\r\n".as_slice(),
            ErrorCode::CsvSyntax,
        ),
    ] {
        assert_eq!(canonicalize_bytes(input, "toggle").unwrap_err().code, code);
    }
    assert_eq!(
        canonicalize_bytes(VALID, "not-a-name").unwrap_err().code,
        ErrorCode::Scenario
    );
}

#[test]
fn hostile_quoted_fields_return_errors_without_unwinding() {
    for (input, code) in [
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n\"\"\"\",0,1,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n\"0\"\"x\",0,1,1\n".as_slice(),
            ErrorCode::CellType,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n\"unterminated,0,1,1\n".as_slice(),
            ErrorCode::CsvSyntax,
        ),
        (
            b"\"time\",\"clr\",\"u\",\"y\"\n\"0\"x,0,1,1\n".as_slice(),
            ErrorCode::CsvSyntax,
        ),
    ] {
        let result = std::panic::catch_unwind(|| canonicalize_bytes(input, "toggle"));
        assert!(result.is_ok(), "hostile CSV unwound the parser");
        assert_eq!(
            result.unwrap().unwrap_err().code,
            code,
            "hostile CSV returned the wrong error"
        );
    }
}

#[cfg(unix)]
#[test]
fn path_reader_rejects_symlinks_and_non_regular_files_before_open() {
    use std::os::unix::fs::symlink;

    let directory = unique_temp_dir("oce-toggle-canonicalizer-path");
    let regular = directory.join("regular.csv");
    let link = directory.join("link.csv");
    let fifo = directory.join("input.fifo");
    std::fs::write(&regular, VALID).unwrap();
    assert_eq!(canonicalize_path(&regular, "toggle").unwrap().rows.len(), 2);
    symlink(&regular, &link).unwrap();
    assert_eq!(read_bounded_path(&link).unwrap_err().code, ErrorCode::Io);
    assert_eq!(
        read_bounded_path(&directory).unwrap_err().code,
        ErrorCode::Io
    );
    assert_eq!(
        read_bounded_path(Path::new("/dev/null")).unwrap_err().code,
        ErrorCode::Io
    );
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());
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

#[test]
fn byte_and_row_bounds_fail_closed() {
    let oversized = vec![b' '; MAX_FILE_BYTES + 1];
    assert_eq!(
        canonicalize_bytes(&oversized, "toggle").unwrap_err().code,
        ErrorCode::FileSize
    );
    let mut too_many = RAW_HEADER.as_bytes().to_vec();
    too_many.push(b'\n');
    for _ in 0..=MAX_ROWS {
        too_many.extend_from_slice(b"0,0,1,1\n");
    }
    assert_eq!(
        canonicalize_bytes(&too_many, "toggle").unwrap_err().code,
        ErrorCode::Shape
    );
}
