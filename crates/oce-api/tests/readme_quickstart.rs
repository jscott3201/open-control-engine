//! The README's Quickstart code block and `examples/quickstart.rs` must stay byte-identical.
//!
//! The example is what actually compiles: `clippy --workspace --all-targets` builds it on every
//! PR, so a Quickstart that no longer matches the facade fails the gate rather than misleading the
//! first reader who copies it. That protection is worth nothing if the two copies drift, which is
//! what this test exists to prevent.

use std::fs;
use std::path::PathBuf;

/// Repository root, derived from this crate's manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// The body of the README's first ```` ```rust ```` fenced block.
fn readme_rust_block(readme: &str) -> String {
    let open = "```rust\n";
    let start = readme
        .find(open)
        .unwrap_or_else(|| panic!("README.md has no ```rust block — the Quickstart is gone"))
        + open.len();
    let rest = &readme[start..];
    let end = rest
        .find("```")
        .unwrap_or_else(|| panic!("README.md ```rust block is unterminated"));
    rest[..end].to_string()
}

/// The example file with its `//!` header stripped, leaving the code the README shows.
fn example_body(example: &str) -> String {
    example
        .lines()
        .skip_while(|line| {
            // `//!` docs and crate-level attributes are file scaffolding, not part of the
            // snippet a reader copies. Everything after them must match the README exactly.
            line.starts_with("//!") || line.starts_with("#![") || line.trim().is_empty()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn readme_quickstart_matches_the_compiled_example() {
    let root = repo_root();
    let readme = fs::read_to_string(root.join("README.md")).expect("README.md is readable");
    let example = fs::read_to_string(root.join("crates/oce-api/examples/quickstart.rs"))
        .expect("examples/quickstart.rs is readable");

    let from_readme = readme_rust_block(&readme);
    let from_example = example_body(&example);

    assert_eq!(
        from_readme, from_example,
        "README.md's Quickstart has drifted from crates/oce-api/examples/quickstart.rs.\n\
         The example is the copy that compiles; make the README match it, or update both."
    );
}

#[test]
fn the_quickstart_loads_a_fixture_that_exists() {
    let root = repo_root();
    let example = fs::read_to_string(root.join("crates/oce-api/examples/quickstart.rs"))
        .expect("examples/quickstart.rs is readable");

    // A Quickstart that names a path nobody can open is worse than one with no path at all.
    let quoted: Vec<&str> = example
        .match_indices(".jsonld\"")
        .map(|(end, _)| {
            let head = &example[..end];
            let start = head.rfind('"').expect("path literal opens with a quote") + 1;
            &example[start..end + ".jsonld".len()]
        })
        .collect();

    assert!(
        !quoted.is_empty(),
        "the Quickstart no longer names a .jsonld fixture; if that is deliberate, delete this test"
    );
    for path in quoted {
        assert!(
            root.join(path).is_file(),
            "the Quickstart reads {path}, which does not exist in the repository"
        );
    }
}
