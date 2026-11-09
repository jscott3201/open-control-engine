//! Compiled-schedule blob goldens (GitHub #400): every G36 catalog fixture's blob is checked in
//! under `tests/fixtures/blob/` and re-encoded byte for byte, decoded and re-encoded byte for byte,
//! and the per-class signature-hash table is pinned. Bless with `OCE_BLESS_BLOB=1`.
//!
//! With `OCE_BLOB_OUT` set, the concatenated blobs are written there for the determinism matrix's
//! cross-architecture and cross-codegen byte comparison in `pr-gate.yml`.

#[path = "support/blob_corpus.rs"]
mod blob_corpus;

use std::fmt::Write as _;
use std::path::PathBuf;

use blob_corpus::{FIXTURES, compile, fixture_names_on_disk};
use oce_graph::blob::{decode, encode};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blob")
}

fn bless() -> bool {
    oce_bless::enabled("OCE_BLESS_BLOB")
}

#[test]
fn the_corpus_is_every_g36_catalog_fixture_and_not_the_resolver_contract_document() {
    let on_disk = fixture_names_on_disk();
    assert_eq!(
        on_disk.len(),
        47,
        "46 catalog fixtures plus member_list_interface"
    );
    let mut expected: Vec<&str> = FIXTURES.to_vec();
    expected.push("member_list_interface");
    expected.sort_unstable();
    assert_eq!(on_disk, expected);
    assert_eq!(FIXTURES.len(), 46);
}

#[test]
fn every_fixture_blob_matches_its_checked_in_golden_byte_for_byte() {
    let mut matrix = Vec::new();
    let mut mismatches = Vec::new();
    for name in FIXTURES {
        let compiled = compile(name);
        let first = compile(name);
        assert_eq!(compiled.bytes, first.bytes, "{name}: two encodings differ");
        matrix.extend_from_slice(&compiled.bytes);
        let path = golden_dir().join(format!("{name}.oceb"));
        if bless() {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, &compiled.bytes).unwrap();
            continue;
        }
        let golden = std::fs::read(&path)
            .unwrap_or_else(|err| panic!("{name}: read {}: {err}", path.display()));
        if golden != compiled.bytes {
            let first = golden
                .iter()
                .zip(&compiled.bytes)
                .position(|(a, b)| a != b)
                .unwrap_or(golden.len().min(compiled.bytes.len()));
            mismatches.push(format!(
                "{name}: first difference at byte {first:#x}; golden {} bytes, encoded {} bytes",
                golden.len(),
                compiled.bytes.len()
            ));
        }
    }
    if let Some(out) = std::env::var_os("OCE_BLOB_OUT").filter(|out| !out.is_empty()) {
        // A relative path is relative to the workspace root, as the CI cells pass it; nextest
        // runs this binary from the package directory.
        let out = PathBuf::from(out);
        let out = if out.is_absolute() {
            out
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(out)
        };
        std::fs::write(&out, &matrix)
            .unwrap_or_else(|err| panic!("write {}: {err}", out.display()));
    }
    assert!(
        mismatches.is_empty(),
        "blob bytes differ from the goldens:\n{}\nbless deliberately with OCE_BLESS_BLOB=1",
        mismatches.join("\n")
    );
}

#[test]
fn decoding_and_re_encoding_reproduces_every_golden() {
    for name in FIXTURES {
        let bytes = std::fs::read(golden_dir().join(format!("{name}.oceb"))).unwrap();
        let decoded = decode(&bytes).unwrap_or_else(|err| panic!("{name}: {err}"));
        let again = encode(&decoded.source()).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(
            again == bytes,
            "{name}: decode then encode is not byte-identical"
        );
        assert!(
            !decoded.target_bound,
            "{name}: no catalog fixture is target-bound"
        );
    }
}

#[test]
fn the_content_id_field_matches_the_facade_content_id_or_is_zero_for_a_partial_export() {
    let mut partial = Vec::new();
    for name in FIXTURES {
        let compiled = compile(name);
        let decoded = decode(&compiled.bytes).unwrap();
        match (&compiled.content_id, decoded.content_id) {
            (Some(id), Some(field)) => {
                let hash = u128::from_le_bytes(field[..16].try_into().unwrap());
                assert_eq!(*id, format!("cxf:fnv1a128:{hash:032x}"), "{name}");
                assert_eq!(field[16..], [0u8; 16], "{name}: padding");
            }
            (None, None) => partial.push(*name),
            other => panic!("{name}: content id and field disagree: {other:?}"),
        }
        assert_eq!(
            compiled.bytes[20..52] == [0u8; 32],
            compiled.content_id.is_none()
        );
    }
    assert_eq!(partial.len(), 13, "the 13 partial exports: {partial:?}");
}

#[test]
fn the_worked_example_has_the_design_note_layout() {
    let compiled = compile("generic_air_economizer_high_limits_ashrae_differential");
    let bytes = &compiled.bytes;
    assert_eq!(bytes.len(), 418);
    assert_eq!(&bytes[..4], b"OCEB");
    assert_eq!(&bytes[4..8], [1, 0, 1, 0]);
    assert_eq!(&bytes[8..14], [0, 0, 4, 0, 0, 0]);
    assert_eq!(&bytes[14..20], [0, 0, 0xa2, 0x01, 0, 0]);
    assert_eq!(
        compiled.content_id.as_deref(),
        Some("cxf:fnv1a128:9aeed987b862eb2504fbde693778e5fd")
    );
    // Section frames at the offsets the note's dump shows.
    for (offset, id, length) in [
        (0x34, 1u32, 0x26u32),
        (0x62, 2, 0x37),
        (0xa1, 3, 8),
        (0xb1, 4, 4),
        (0xbd, 5, 0xd9),
    ] {
        let frame: [u8; 8] = bytes[offset..offset + 8].try_into().unwrap();
        assert_eq!(frame[..4], id.to_le_bytes(), "section {id} id");
        assert_eq!(frame[4..], length.to_le_bytes(), "section {id} length");
    }
    let hash = oce_blocks::class_signature_hash("CDL.Reals.AddParameter").unwrap();
    assert_eq!(bytes[0x5a..0x62], hash.to_le_bytes());
}

/// The per-class signature hashes, one `<16 hex digits>\t<class path>` line per registered class
/// in catalog order: any change to a class's record shows up here in review.
#[test]
fn the_per_class_signature_hash_table_matches_its_golden() {
    let mut table = String::new();
    for entry in oce_blocks::catalog() {
        writeln!(table, "{:016x}\t{}", entry.signature_hash, entry.class_path).unwrap();
    }
    let path = golden_dir().join("class_signatures.tsv");
    if bless() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, &table).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path).unwrap();
    assert!(
        golden == table,
        "class signature hashes changed; review the diff and bless with OCE_BLESS_BLOB=1"
    );
}
