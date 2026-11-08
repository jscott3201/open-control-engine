//! The core four crates (`oce-model`, `oce-expr`, `oce-blocks`, `oce-graph`) use no hash
//! collections at all.
//!
//! They build `no_std + alloc`, where `std::collections::HashMap` does not exist, and every list
//! they produce feeds the schedule, the state image or an exported name, so issue #398 settles the
//! question by ratchet: ordered collections (`BTreeMap`, `BTreeSet`, `BinaryHeap`) or sorted
//! `Vec`s only. The `no-std-build` gate already refuses `std::collections` outside the `std`
//! feature; this test also refuses the spellings that would compile there (`hashbrown`, `ahash`,
//! `indexmap`, a hand-rolled `BuildHasher`) and any use gated behind the `std` feature.
//!
//! A clippy `disallowed-types` entry cannot express this: `clippy.toml` is read once for the whole
//! workspace (the nearest file wins, files do not merge), so it would also refuse the lookup-only
//! hash maps of the `std` crates `oce-api`, `oce-cxf`, `oce-validate` and `oce-store`.
//!
//! The scan reads code, not comments: text after `//` is ignored, so documentation may name the
//! types it rules out. A string literal containing `//` hides the rest of its line; none of the
//! four crates relies on that, and the cost of a miss is bounded by the order-leak test in
//! `hash_order_independence.rs`.

use std::fs;
use std::path::{Path, PathBuf};

/// The `no_std + alloc` crates, by directory under `crates/`.
const CORE_CRATES: [&str; 4] = ["oce-model", "oce-expr", "oce-blocks", "oce-graph"];

/// Identifiers that name a hash collection, its hasher plumbing, or a crate providing one.
const HASH_TOKENS: [&str; 13] = [
    "HashMap",
    "HashSet",
    "hash_map",
    "hash_set",
    "hashbrown",
    "RandomState",
    "DefaultHasher",
    "BuildHasher",
    "IndexMap",
    "IndexSet",
    "ahash",
    "foldhash",
    "FxHash",
];

/// Dependency names that would bring a hash collection into a core crate.
const HASH_CRATES: [&str; 6] = [
    "hashbrown",
    "ahash",
    "indexmap",
    "foldhash",
    "rustc-hash",
    "fxhash",
];

/// Every `HASH_TOKENS` hit in the code part of `text`, as `(1-based line, token)`.
fn hash_tokens_in(text: &str) -> Vec<(usize, &'static str)> {
    let mut hits = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(code, _)| code);
        for token in HASH_TOKENS {
            if code.contains(token) {
                hits.push((index + 1, token));
            }
        }
    }
    hits
}

/// Every key or table header in `manifest` with a dotted segment naming a hash-collection crate
/// (`hashbrown = ...`, `hashbrown.workspace = true`, `[dependencies.hashbrown]`).
fn hash_crate_dependencies_in(manifest: &str) -> Vec<&'static str> {
    let mut hits = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        let key = match line.strip_prefix('[') {
            Some(header) => header.trim_end_matches(']'),
            None => line.split('=').next().unwrap_or_default(),
        };
        for segment in key.split('.') {
            let segment = segment.trim().trim_matches(['"', '\'']);
            if let Some(name) = HASH_CRATES.iter().find(|name| **name == segment) {
                hits.push(*name);
            }
        }
    }
    hits
}

fn crate_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(name)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn core_crates_name_no_hash_collection() {
    let mut violations = Vec::new();
    for name in CORE_CRATES {
        let dir = crate_dir(name);
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        rust_files(&dir.join("tests"), &mut files);
        rust_files(&dir.join("benches"), &mut files);
        assert!(
            files.iter().any(|file| file.ends_with("src/lib.rs")),
            "{name}: the scan found no src/lib.rs, so it would pass vacuously"
        );
        for file in files {
            let text = fs::read_to_string(&file).expect("read core crate source");
            for (line, token) in hash_tokens_in(&text) {
                violations.push(format!("{}:{line}: {token}", file.display()));
            }
        }
        let manifest =
            fs::read_to_string(dir.join("Cargo.toml")).expect("read core crate manifest");
        for dependency in hash_crate_dependencies_in(&manifest) {
            violations.push(format!("{name}/Cargo.toml: depends on {dependency}"));
        }
    }
    assert!(
        violations.is_empty(),
        "the no_std core crates use ordered collections only (issue #398); found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn scan_flags_hash_collections_in_code_and_ignores_comments() {
    let source = "\
use alloc::collections::BTreeMap;
use std::collections::HashMap; // a real use
/// Documentation may say HashSet.
// So may a plain comment about hashbrown.
let seen: hashbrown::HashSet<u32> = Default::default();
#[cfg(feature = \"std\")] type Seeds = std::hash::RandomState;
";
    assert_eq!(
        hash_tokens_in(source),
        vec![
            (2, "HashMap"),
            (5, "HashSet"),
            (5, "hashbrown"),
            (6, "RandomState"),
        ]
    );
    assert!(hash_tokens_in("let order = BinaryHeap::new(); // no HashMap here").is_empty());
}

#[test]
fn manifest_scan_flags_every_dependency_spelling() {
    let manifest = "\
[dependencies]
libm = { workspace = true }
hashbrown = { version = \"0.15\", default-features = false }
[target.'cfg(unix)'.dependencies]
ahash = \"0.8\"
[dev-dependencies.indexmap]
version = \"2\"
[features]
std = [\"serde/std\"]
";
    assert_eq!(
        hash_crate_dependencies_in(manifest),
        vec!["hashbrown", "ahash", "indexmap"]
    );
    assert!(hash_crate_dependencies_in("[dependencies]\nserde = \"1\"\n").is_empty());
}
