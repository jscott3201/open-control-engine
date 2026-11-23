//! The path index finds the row a map from each spelled path to its first row would find.

use std::collections::BTreeMap;
use std::sync::Arc;

use oce_model::BlockId;

use super::{PathBase, PathHash, PathIndex, StablePathHash};

/// Rows whose spellings collide, nest and differ by one byte, with and without an IRI.
fn rows() -> Vec<(PathBase, &'static str)> {
    let iri = |text: &str| PathBase::Iri(Arc::from(text));
    vec![
        (iri("urn:a"), "b.c"),
        (iri("urn:a.b"), "c"),
        (iri("urn:a"), "k"),
        (iri("urn:a"), "k"),
        (iri("urn:a."), "k"),
        (iri("urn:a"), ".k"),
        (iri("urn:a"), ""),
        (iri(""), "k"),
        (iri("urn:é"), "k[1,2]"),
        (PathBase::Unnamed(BlockId(7)), "k"),
        (PathBase::Unnamed(BlockId(70)), "k"),
        (iri("b7"), "k"),
        (iri("b7.k"), "x"),
    ]
}

fn find(index: &PathIndex, rows: &[(PathBase, &'static str)], path: &str) -> Option<usize> {
    index.find(path, |row| (&rows[row].0, rows[row].1))
}

#[test]
fn every_path_finds_the_row_a_map_of_spelled_paths_finds() {
    let rows = rows();
    let index = PathIndex::of(rows.iter().map(|(base, name)| (base, *name)));
    let mut first: BTreeMap<String, usize> = BTreeMap::new();
    for (row, (base, name)) in rows.iter().enumerate() {
        first.entry(base.path(name)).or_insert(row);
    }
    assert_eq!(
        first.len(),
        9,
        "four rows repeat an earlier row's path: {first:?}"
    );
    for (path, row) in &first {
        assert_eq!(find(&index, &rows, path), Some(*row), "{path}");
    }
    for absent in [
        "", ".", "urn:a", "urn:a.b", "urn:a.k.", "b7", "b70.", "b07.k", "urn:é.k",
    ] {
        assert_eq!(find(&index, &rows, absent), None, "{absent}");
    }
}

#[test]
fn the_first_of_rows_spelling_one_path_is_found_however_the_rows_are_ordered() {
    // `urn:a` + `b.c` and `urn:a.b` + `c` spell one path, and `b7` (no IRI, id 7) and the IRI
    // `b7` another: each is found as the first of its rows in the table's order.
    let mut rows = rows();
    for _ in 0..2 {
        let index = PathIndex::of(rows.iter().map(|(base, name)| (base, *name)));
        for path in ["urn:a.b.c", "b7.k", "urn:a.k"] {
            let expected = rows.iter().position(|(base, name)| base.path(name) == path);
            assert_eq!(find(&index, &rows, path), expected, "{path}");
        }
        rows.reverse();
    }
}

#[test]
fn the_hash_is_a_fold_over_the_bytes_however_they_are_cut() {
    // Every cut of a path into pieces, including empty ones, hashes as the path whole.
    let path = "urn:é.b.c[1,2]";
    let whole = {
        let mut hash = StablePathHash::default();
        hash.write(path.as_bytes());
        hash.finish()
    };
    let bytes = path.as_bytes();
    for first in 0..=bytes.len() {
        for second in first..=bytes.len() {
            let mut hash = StablePathHash::default();
            hash.write(&bytes[..first]);
            hash.write(&[]);
            hash.write(&bytes[first..second]);
            hash.write(&bytes[second..]);
            assert_eq!(hash.finish(), whole, "cut at {first} and {second}");
        }
    }
    // A pinned value: the fold is the crate's FNV-1a over 128 bits, the same on every toolchain.
    let mut empty = StablePathHash::default();
    empty.write(&[]);
    assert_eq!(empty.finish(), 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d);
}

#[test]
fn a_path_hashes_alike_whole_and_in_parts() {
    // The index hashes a row's parts, a lookup the whole path: one keyed hasher gives one hash.
    let rows = rows();
    let index = PathIndex::of(rows.iter().map(|(base, name)| (base, *name)));
    for (row, (base, name)) in rows.iter().enumerate() {
        let path = base.path(name);
        let found = find(&index, &rows, &path).expect("each row's path is found");
        assert_eq!(rows[found].0.path(rows[found].1), path, "row {row}");
        assert!(found <= row);
    }
}

/// A hasher every path collides under, so a lookup can tell rows apart only by their parts.
#[derive(Default)]
struct Colliding;

impl PathHash for Colliding {
    fn write(&mut self, _bytes: &[u8]) {}

    fn finish(self) -> u128 {
        7
    }
}

#[test]
fn rows_whose_hashes_collide_are_told_apart_by_their_parts() {
    // Under a hasher that gives every path one hash, each path still finds its own first row,
    // and a path no row spells finds none: the hash only narrows the rows compared.
    let rows = rows();
    let index = PathIndex::hashed_by::<Colliding>(rows.iter().map(|(base, name)| (base, *name)));
    let find =
        |path: &str| index.find_hashed_by::<Colliding>(path, |row| (&rows[row].0, rows[row].1));
    for (row, (base, name)) in rows.iter().enumerate() {
        let path = base.path(name);
        let first = rows.iter().position(|(b, n)| b.path(n) == path);
        assert_eq!(find(&path), first, "row {row}: {path}");
    }
    for absent in ["urn:a.b", "urn:a.b.", "b7", "b8.k", "urn:a.kk"] {
        assert_eq!(find(absent), None, "{absent}");
    }
}

#[test]
fn the_table_shows_its_rows_and_never_its_index() {
    // `ParamTable`'s `Debug` prints the rows in order; the hashes, which a reader of the output
    // has no use for, stay out of it.
    let mut model = oce_model::ModelGraph::new();
    model.blocks.push(oce_model::BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![],
        params: oce_model::ParamTable {
            values: vec![(Arc::from("k"), oce_model::Value::Real(1.5))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:a")),
    });
    let text = format!("{:?}", crate::params::ParamTable::build_at_load(&model));
    assert!(text.starts_with("ParamTable { entries: ["), "{text}");
    assert!(
        text.contains("urn:a") && text.contains("Real(1.5)"),
        "{text}"
    );
    assert!(
        !text.contains("by_hash") && !text.contains("index"),
        "{text}"
    );
}
