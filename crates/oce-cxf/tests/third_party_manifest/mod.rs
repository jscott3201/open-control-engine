//! Byte-integrity manifest for the vendored Modelica Buildings CDL corpus.
//!
//! The whitelist is a fail-closed stand-in for Git's tracked set: every walked file must belong
//! to one of the upstream-Buildings, generated-CXF, or repo-authored provenance buckets. Git blob
//! and tree objects are recomputed byte-for-byte, including Git's trailing-slash ordering rule
//! for subtrees. The checked-in manifest can be regenerated, but the tree-SHA Rust constant is a
//! deliberate hand edit so tampering plus re-blessing cannot silently move the independent pin.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha1::{Digest as _, Sha1};
use sha2::Sha256;

const BUILDINGS_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
const MODELICA_JSON_COMMIT: &str = "85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb";
// Derived after the README commit with:
// git rev-parse HEAD:third_party/modelica-buildings-cdl
const SUBTREE_TREE_SHA: &str = "c5e80bbceea88a6a1488f8c54a41c553637d1a85";
const SCHEMA: &str = "open-control/modelica-buildings-cdl-hash-manifest/v1";
const MANIFEST_PATH: &str =
    "../../tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json";
const MANIFEST_BYTES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json"
));
const REBLESS: &str = "OCE_BLESS=1 cargo test -p oce-cxf --test \
                       fixture_structural_oracle \
                       checked_in_manifest_bytes_equal_fresh_render";

#[derive(Clone, Debug)]
struct FileData {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Corpus {
    listing: Vec<FileData>,
    manifest: Manifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Manifest {
    schema: String,
    buildings_commit: String,
    modelica_json_commit: String,
    subtree_tree_sha: String,
    bucket_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Entry {
    path: String,
    origin: String,
    size_bytes: u64,
    git_blob_oid: String,
    sha256: String,
}

#[derive(Default)]
struct Tree {
    files: BTreeMap<String, [u8; 20]>,
    dirs: BTreeMap<String, Tree>,
}

static CORPUS: OnceLock<Result<Corpus, String>> = OnceLock::new();

fn vendor_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/modelica-buildings-cdl")
}

fn corpus() -> Result<&'static Corpus, String> {
    CORPUS
        .get_or_init(|| {
            let listing = walk(&vendor_root())?;
            let manifest = build_manifest(&listing)?;
            Ok(Corpus { listing, manifest })
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn walk(root: &Path) -> Result<Vec<FileData>, String> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<FileData>) -> Result<(), String> {
        let mut children = fs::read_dir(dir)
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot enumerate {}: {error}", dir.display()))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let file_type = child
                .file_type()
                .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
            if file_type.is_dir() {
                visit(root, &path, out)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("walked path stays under root")
                    .to_str()
                    .ok_or_else(|| format!("non-UTF-8 vendored path: {}", path.display()))?
                    .replace('\\', "/");
                validate_path(&relative)?;
                reject_executable(&path, &relative)?;
                let bytes = fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                out.push(FileData {
                    path: relative,
                    bytes,
                });
            } else {
                return Err(format!(
                    "unsupported vendored filesystem entry: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

#[cfg(unix)]
fn reject_executable(path: &Path, relative: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = fs::metadata(path)
        .map_err(|error| format!("cannot read mode for {}: {error}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o111 != 0 {
        return Err(format!(
            "executable vendored file `{relative}`: vendored files must use Git mode 100644 — \
             remove the executable bit deliberately"
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn reject_executable(_path: &Path, _relative: &str) -> Result<(), String> {
    Ok(())
}

fn validate_path(path: &str) -> Result<&'static str, String> {
    let basename = path.rsplit('/').next().unwrap_or(path);
    let whitelisted = path.ends_with(".mo")
        || path.ends_with(".jsonld")
        || basename == "legal.html"
        || basename == "README.md";
    if !whitelisted {
        return Err(format!(
            "non-whitelisted vendored path `{path}`: untracked junk (e.g. `.DS_Store`) — \
             delete it; if this is a new vendored file type, extend the whitelist and the \
             manifest deliberately"
        ));
    }
    if path.starts_with("Buildings/") {
        Ok("upstream-buildings")
    } else if path.starts_with("cxf/") {
        Ok("generated-cxf")
    } else if path == "README.md" {
        Ok("repo-authored")
    } else {
        Err(format!(
            "whitelisted vendored path has no origin bucket: `{path}`"
        ))
    }
}

fn build_manifest(listing: &[FileData]) -> Result<Manifest, String> {
    let mut entries = Vec::with_capacity(listing.len());
    let mut counts = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for file in listing {
        if !seen.insert(file.path.as_str()) {
            return Err(format!("duplicate vendored path: `{}`", file.path));
        }
        let origin = validate_path(&file.path)?;
        *counts.entry(origin.to_owned()).or_insert(0) += 1;
        entries.push(Entry {
            path: file.path.clone(),
            origin: origin.to_owned(),
            size_bytes: file.bytes.len() as u64,
            git_blob_oid: hex(&git_object_oid("blob", &file.bytes)),
            sha256: hex(&Sha256::digest(&file.bytes)),
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Manifest {
        schema: SCHEMA.to_owned(),
        buildings_commit: BUILDINGS_COMMIT.to_owned(),
        modelica_json_commit: MODELICA_JSON_COMMIT.to_owned(),
        subtree_tree_sha: tree_oid(listing)?,
        bucket_counts: counts,
        entries,
    })
}

fn render(manifest: &Manifest) -> String {
    let mut rendered = serde_json::to_string_pretty(manifest).expect("manifest is serializable");
    rendered.push('\n');
    rendered
}

fn git_object_oid(kind: &str, body: &[u8]) -> [u8; 20] {
    let mut hash = Sha1::new();
    hash.update(format!("{kind} {}\0", body.len()).as_bytes());
    hash.update(body);
    hash.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn tree_oid(listing: &[FileData]) -> Result<String, String> {
    let mut root = Tree::default();
    for file in listing {
        let mut parts = file.path.split('/').peekable();
        let mut node = &mut root;
        while let Some(part) = parts.next() {
            if parts.peek().is_some() {
                node = node.dirs.entry(part.to_owned()).or_default();
            } else if node
                .files
                .insert(part.to_owned(), git_object_oid("blob", &file.bytes))
                .is_some()
            {
                return Err(format!("duplicate tree entry: `{}`", file.path));
            }
        }
    }
    Ok(hex(&hash_tree(&root)))
}

fn hash_tree(tree: &Tree) -> [u8; 20] {
    struct Item<'a> {
        sort_name: String,
        mode: &'static str,
        name: &'a str,
        oid: [u8; 20],
    }

    let mut items = Vec::with_capacity(tree.files.len() + tree.dirs.len());
    for (name, oid) in &tree.files {
        items.push(Item {
            sort_name: name.clone(),
            mode: "100644",
            name,
            oid: *oid,
        });
    }
    for (name, child) in &tree.dirs {
        items.push(Item {
            sort_name: format!("{name}/"),
            mode: "40000",
            name,
            oid: hash_tree(child),
        });
    }
    items.sort_by(|left, right| left.sort_name.as_bytes().cmp(right.sort_name.as_bytes()));

    let mut body = Vec::new();
    for item in items {
        body.extend_from_slice(item.mode.as_bytes());
        body.push(b' ');
        body.extend_from_slice(item.name.as_bytes());
        body.push(0);
        body.extend_from_slice(&item.oid);
    }
    git_object_oid("tree", &body)
}

fn parse_manifest(bytes: &str) -> Result<Manifest, String> {
    serde_json::from_str(bytes)
        .map_err(|error| format!("cannot parse checked-in manifest: {error}"))
}

fn structured_diff(checked: &Manifest, disk: &Manifest) -> Result<(), String> {
    let mut findings = Vec::new();
    if checked.schema != disk.schema {
        findings.push("header differs: schema".to_owned());
    }
    if checked.buildings_commit != disk.buildings_commit {
        findings.push("header differs: buildings_commit".to_owned());
    }
    if checked.modelica_json_commit != disk.modelica_json_commit {
        findings.push("header differs: modelica_json_commit".to_owned());
    }
    if checked.subtree_tree_sha != disk.subtree_tree_sha {
        findings.push("header differs: subtree_tree_sha".to_owned());
    }
    if checked.bucket_counts != disk.bucket_counts {
        findings.push("header differs: bucket_counts".to_owned());
    }
    let checked_by_path: BTreeMap<_, _> = checked
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let disk_by_path: BTreeMap<_, _> = disk
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    for (path, expected) in &checked_by_path {
        let Some(actual) = disk_by_path.get(path) else {
            findings.push(format!("manifest-only path `{path}`"));
            continue;
        };
        if expected.sha256 != actual.sha256 {
            findings.push(format!("`{path}` differs: sha256"));
        }
        if expected.git_blob_oid != actual.git_blob_oid {
            findings.push(format!("`{path}` differs: git_blob_oid"));
        }
        if expected.size_bytes != actual.size_bytes {
            findings.push(format!("`{path}` differs: size_bytes"));
        }
        if expected.origin != actual.origin {
            findings.push(format!("`{path}` differs: origin"));
        }
    }
    for path in disk_by_path.keys() {
        if !checked_by_path.contains_key(path) {
            findings.push(format!("disk-only path `{path}`"));
        }
    }
    if findings.is_empty() {
        return Ok(());
    }
    let total = findings.len();
    let shown = findings.into_iter().take(10).collect::<Vec<_>>().join("\n");
    let omitted = total.saturating_sub(10);
    Err(format!(
        "vendored manifest differs in {total} finding(s):\n{shown}{}\nre-bless with: {REBLESS}",
        if omitted == 0 {
            String::new()
        } else {
            format!("\n... and {omitted} more")
        }
    ))
}

#[test]
fn checked_in_manifest_bytes_equal_fresh_render() {
    let fresh = &corpus().expect("vendored corpus must be readable").manifest;
    let first = render(fresh);
    let independent_listing = walk(&vendor_root()).expect("independent vendor walk must succeed");
    let independent =
        build_manifest(&independent_listing).expect("independent manifest build must succeed");
    assert_eq!(
        fresh, &independent,
        "independent vendor walks produced different manifests"
    );
    if std::env::var_os("OCE_BLESS").is_some() {
        fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST_PATH),
            first,
        )
        .expect("write blessed manifest");
        return;
    }
    let checked =
        parse_manifest(MANIFEST_BYTES).unwrap_or_else(|error| panic!("{error}\n{REBLESS}"));
    structured_diff(&checked, fresh).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        MANIFEST_BYTES, first,
        "manifest serialization is stale; re-bless with: {REBLESS}"
    );
}

#[test]
fn recomputed_subtree_sha_matches_independent_git_pin() {
    let actual = &corpus()
        .expect("vendored corpus must be readable")
        .manifest
        .subtree_tree_sha;
    assert_eq!(
        actual, SUBTREE_TREE_SHA,
        "re-derive with `git rev-parse HEAD:third_party/modelica-buildings-cdl` after \
         committing, then deliberately hand-edit SUBTREE_TREE_SHA"
    );
}

#[test]
fn manifest_headers_match_source_pins() {
    let manifest = parse_manifest(MANIFEST_BYTES).expect("checked-in manifest must parse");
    assert_eq!(manifest.schema, SCHEMA);
    assert_eq!(manifest.buildings_commit, BUILDINGS_COMMIT);
    assert_eq!(manifest.modelica_json_commit, MODELICA_JSON_COMMIT);
    assert_eq!(
        manifest.subtree_tree_sha, SUBTREE_TREE_SHA,
        "re-derive with `git rev-parse HEAD:third_party/modelica-buildings-cdl` after \
         committing, then deliberately hand-edit SUBTREE_TREE_SHA"
    );
    let provenance: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/reference-catalog/Buildings.Controls.OBC.CDL.prov.json"
    )))
    .expect("CDL provenance is JSON");
    assert_eq!(provenance["commit"].as_str(), Some(BUILDINGS_COMMIT));

    let readme = corpus()
        .expect("vendored corpus must be readable")
        .listing
        .iter()
        .find(|file| file.path == "README.md")
        .expect("vendored README must be listed");
    let tokens = modelica_json_tokens(&readme.bytes);
    assert_eq!(
        tokens.len(),
        2,
        "vendored README must carry exactly two modelica-json commit tokens"
    );
    assert!(
        tokens.iter().all(|token| token == MODELICA_JSON_COMMIT),
        "vendored README modelica-json tokens must equal MODELICA_JSON_COMMIT: {tokens:?}"
    );
}

fn modelica_json_tokens(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| line.contains("modelica-json"))
        .flat_map(|line| {
            line.split(|character: char| !character.is_ascii_hexdigit())
                .filter(|token| token.len() == 40)
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn cryptographic_known_answers_match_independent_tools() {
    let bytes = b"open-control manifest KAT\n";
    // printf 'open-control manifest KAT\n' | git hash-object --stdin
    assert_eq!(
        hex(&git_object_oid("blob", bytes)),
        "204b1491be2bb8302b23cdf21a3710eabf24faf4"
    );
    // printf 'open-control manifest KAT\n' | shasum -a 256
    assert_eq!(
        hex(&Sha256::digest(bytes)),
        "01ecb8d575244713bc120e4e490563e0aeede59790bf7df39db4942cebdca1f2"
    );
}

#[test]
fn git_tree_order_uses_trailing_slash_for_subtrees() {
    let listing = vec![
        FileData {
            path: "Foo/README.md".to_owned(),
            bytes: Vec::new(),
        },
        FileData {
            path: "Foo.mo".to_owned(),
            bytes: b"synthetic blob\n".to_vec(),
        },
    ];
    // kat_repo=$(mktemp -d /tmp/oce-tree-kat.XXXXXX)
    // git -C "$kat_repo" init -q
    // empty_blob=$(printf '' | git -C "$kat_repo" hash-object -w --stdin)
    // foo_tree=$(printf '100644 blob %s\tREADME.md\n' "$empty_blob" |
    //   git -C "$kat_repo" mktree)
    // root_blob=$(printf 'synthetic blob\n' | git -C "$kat_repo" hash-object -w --stdin)
    // printf '040000 tree %s\tFoo\n100644 blob %s\tFoo.mo\n' "$foo_tree" "$root_blob" |
    //   git -C "$kat_repo" mktree
    assert_eq!(
        tree_oid(&listing).expect("synthetic tree is valid"),
        "1ce8076992b366b4f40ea620aede0aaf6641b3ca"
    );
}

#[test]
fn additions_deletions_and_junk_are_distinct_failures() {
    let base = corpus().expect("vendored corpus must be readable");
    let mut ghost = base.listing.clone();
    ghost.push(FileData {
        path: "cxf/Ghost.jsonld".to_owned(),
        bytes: b"{}".to_vec(),
    });
    let error = structured_diff(
        &base.manifest,
        &build_manifest(&ghost).expect("valid ghost"),
    )
    .expect_err("ghost must fail");
    assert!(
        error.contains("disk-only path `cxf/Ghost.jsonld`"),
        "{error}"
    );

    let removed_path = base.listing[0].path.clone();
    let removed: Vec<_> = base
        .listing
        .iter()
        .filter(|file| file.path != removed_path)
        .cloned()
        .collect();
    let error = structured_diff(
        &base.manifest,
        &build_manifest(&removed).expect("valid deletion"),
    )
    .expect_err("deletion must fail");
    assert!(
        error.contains(&format!("manifest-only path `{removed_path}`")),
        "{error}"
    );

    let mut junk = base.listing.clone();
    junk.push(FileData {
        path: "cxf/.DS_Store".to_owned(),
        bytes: Vec::new(),
    });
    let error = build_manifest(&junk).expect_err("junk must fail");
    assert!(error.contains("cxf/.DS_Store"), "{error}");
    assert!(error.contains("untracked junk"), "{error}");
    assert!(error.contains("new vendored file type"), "{error}");
}

#[test]
fn listing_mutations_trip_hash_and_tree_controls() {
    let base = corpus().expect("vendored corpus must be readable");
    structured_diff(&base.manifest, &base.manifest).expect("unmutated control is green");
    assert_eq!(base.manifest.subtree_tree_sha, SUBTREE_TREE_SHA);

    let mut changed = base.listing.clone();
    changed[0].bytes[0] ^= 1;
    let changed_manifest = build_manifest(&changed).expect("changed listing remains valid");
    let path = &changed[0].path;
    let error =
        structured_diff(&base.manifest, &changed_manifest).expect_err("content flip must fail");
    assert!(
        error.contains(&format!("`{path}` differs: sha256")),
        "{error}"
    );
    assert!(
        error.contains(&format!("`{path}` differs: git_blob_oid")),
        "{error}"
    );
    assert_ne!(changed_manifest.subtree_tree_sha, SUBTREE_TREE_SHA);
}

#[test]
fn manifest_field_mutations_name_the_path_and_field() {
    let base = corpus().expect("vendored corpus must be readable");
    let checked = parse_manifest(MANIFEST_BYTES).expect("checked-in manifest must parse");
    structured_diff(&checked, &base.manifest).expect("unmutated control is green");
    let path = checked.entries[0].path.clone();

    let mut wrong_origin = checked.clone();
    wrong_origin.entries[0].origin = "wrong-origin".to_owned();
    let error = structured_diff(&wrong_origin, &base.manifest).expect_err("wrong origin must fail");
    assert!(
        error.contains(&format!("`{path}` differs: origin")),
        "{error}"
    );

    let mut wrong_size = checked;
    wrong_size.entries[0].size_bytes += 1;
    let error = structured_diff(&wrong_size, &base.manifest).expect_err("wrong size must fail");
    assert!(
        error.contains(&format!("`{path}` differs: size_bytes")),
        "{error}"
    );
}
