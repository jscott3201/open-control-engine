//! Mechanical checks for the tracked public-surface classification authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

type Object = serde_json::Map<String, Value>;

const LEDGER: &str = include_str!("../../../docs/public-surface-ledger.json");
const API_BASELINE: &str = include_str!("public-api.txt");
const STORE_BASELINE: &str = include_str!("../../oce-store/tests/public-api.txt");

const ALLOWED_STATUSES: [&str; 5] = [
    "stable-candidate",
    "conditional",
    "deprecated",
    "unstable/deferred",
    "implementation-leakage-to-remove",
];

#[derive(Clone, Copy)]
struct Baseline<'a> {
    id: &'static str,
    path: &'static str,
    contents: &'a str,
}

fn baselines() -> [Baseline<'static>; 2] {
    [
        Baseline {
            id: "oce-api",
            path: "crates/oce-api/tests/public-api.txt",
            contents: API_BASELINE,
        },
        Baseline {
            id: "oce-store",
            path: "crates/oce-store/tests/public-api.txt",
            contents: STORE_BASELINE,
        },
    ]
}

#[derive(Debug, PartialEq, Eq)]
enum LedgerError {
    Json,
    Schema(&'static str),
    InvalidStatus(String),
    BaselineMismatch(String),
    Missing { baseline: String, line: usize },
    Extra { baseline: String, line: usize },
    Duplicate { baseline: String, line: usize },
}

fn object<'a>(value: &'a Value, what: &'static str) -> Result<&'a Object, LedgerError> {
    value.as_object().ok_or(LedgerError::Schema(what))
}

fn array<'a>(value: &'a Value, what: &'static str) -> Result<&'a Vec<Value>, LedgerError> {
    value.as_array().ok_or(LedgerError::Schema(what))
}

fn string<'a>(value: &'a Value, what: &'static str) -> Result<&'a str, LedgerError> {
    value.as_str().ok_or(LedgerError::Schema(what))
}

fn required<'a>(value: &'a Object, key: &'static str) -> Result<&'a Value, LedgerError> {
    value.get(key).ok_or(LedgerError::Schema(key))
}

fn exact_keys(value: &Object, expected: &[&str], what: &'static str) -> Result<(), LedgerError> {
    let actual: BTreeSet<&str> = value.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    if actual == expected {
        Ok(())
    } else {
        Err(LedgerError::Schema(what))
    }
}

fn parse_range(encoded: &str) -> Result<(usize, usize), LedgerError> {
    let mut parts = encoded.split('-');
    let start = parts
        .next()
        .and_then(|part| part.parse::<usize>().ok())
        .ok_or(LedgerError::Schema("range"))?;
    let end = parts
        .next()
        .map(str::parse::<usize>)
        .transpose()
        .map_err(|_| LedgerError::Schema("range"))?
        .unwrap_or(start);
    if parts.next().is_some() || start == 0 || start > end {
        return Err(LedgerError::Schema("range"));
    }
    Ok((start, end))
}

fn validate_ledger(ledger: &str, actual: &[Baseline<'_>]) -> Result<(), LedgerError> {
    let document: Value = serde_json::from_str(ledger).map_err(|_| LedgerError::Json)?;
    let root = object(&document, "ledger root")?;
    exact_keys(
        root,
        &[
            "schema",
            "authority",
            "range_encoding",
            "statuses",
            "baselines",
            "groups",
            "entries",
        ],
        "ledger root fields",
    )?;
    if string(required(root, "schema")?, "schema")? != "oce-public-surface-ledger/v1" {
        return Err(LedgerError::Schema("schema version"));
    }

    let statuses: Vec<&str> = array(required(root, "statuses")?, "statuses")?
        .iter()
        .map(|status| string(status, "status"))
        .collect::<Result<_, _>>()?;
    if statuses != ALLOWED_STATUSES {
        return Err(LedgerError::Schema("status vocabulary"));
    }

    let groups = object(required(root, "groups")?, "groups")?;
    let mut group_statuses = BTreeMap::new();
    for (name, definition) in groups {
        let definition = object(definition, "group")?;
        exact_keys(
            definition,
            &["status", "surface", "provenance", "rationale"],
            "group fields",
        )?;
        let status = string(required(definition, "status")?, "group status")?;
        if !ALLOWED_STATUSES.contains(&status) {
            return Err(LedgerError::InvalidStatus(status.to_owned()));
        }
        for field in ["surface", "provenance", "rationale"] {
            if string(required(definition, field)?, "group text")?
                .trim()
                .is_empty()
            {
                return Err(LedgerError::Schema("empty group text"));
            }
        }
        group_statuses.insert(name.as_str(), status);
    }

    let by_id: BTreeMap<&str, Baseline<'_>> = actual.iter().map(|item| (item.id, *item)).collect();
    let descriptors = array(required(root, "baselines")?, "baselines")?;
    if descriptors.len() != actual.len() {
        return Err(LedgerError::Schema("baseline descriptor count"));
    }
    let mut described = BTreeSet::new();
    for descriptor in descriptors {
        let descriptor = object(descriptor, "baseline descriptor")?;
        exact_keys(
            descriptor,
            &["id", "path", "sha256", "item_count"],
            "baseline descriptor fields",
        )?;
        let id = string(required(descriptor, "id")?, "baseline id")?;
        let baseline = by_id
            .get(id)
            .ok_or_else(|| LedgerError::BaselineMismatch(id.to_owned()))?;
        if !described.insert(id) {
            return Err(LedgerError::Schema("duplicate baseline descriptor"));
        }
        let item_count = required(descriptor, "item_count")?
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(LedgerError::Schema("item_count"))?;
        let expected_hash = string(required(descriptor, "sha256")?, "sha256")?;
        if string(required(descriptor, "path")?, "baseline path")? != baseline.path
            || item_count != baseline.contents.lines().count()
            || expected_hash != sha256_hex(baseline.contents.as_bytes())
        {
            return Err(LedgerError::BaselineMismatch(id.to_owned()));
        }
    }

    let mut assignments: BTreeMap<&str, Vec<Option<&str>>> = by_id
        .iter()
        .map(|(id, baseline)| (*id, vec![None; baseline.contents.lines().count()]))
        .collect();
    let mut used_groups = BTreeSet::new();
    for entry in array(required(root, "entries")?, "entries")? {
        let entry = object(entry, "entry")?;
        exact_keys(entry, &["baseline", "group", "ranges"], "entry fields")?;
        let id = string(required(entry, "baseline")?, "entry baseline")?;
        let group = string(required(entry, "group")?, "entry group")?;
        if !group_statuses.contains_key(group) {
            return Err(LedgerError::Schema("unknown group"));
        }
        used_groups.insert(group);
        let Some(slots) = assignments.get_mut(id) else {
            return Err(LedgerError::Extra {
                baseline: id.to_owned(),
                line: 0,
            });
        };
        let ranges = array(required(entry, "ranges")?, "ranges")?;
        if ranges.is_empty() {
            return Err(LedgerError::Schema("empty ranges"));
        }
        for encoded in ranges {
            let (start, end) = parse_range(string(encoded, "range")?)?;
            for line in start..=end {
                let Some(slot) = slots.get_mut(line - 1) else {
                    return Err(LedgerError::Extra {
                        baseline: id.to_owned(),
                        line,
                    });
                };
                if slot.replace(group).is_some() {
                    return Err(LedgerError::Duplicate {
                        baseline: id.to_owned(),
                        line,
                    });
                }
            }
        }
    }
    if used_groups.len() != groups.len() {
        return Err(LedgerError::Schema("unused group"));
    }
    for (id, slots) in assignments {
        if let Some(index) = slots.iter().position(Option::is_none) {
            return Err(LedgerError::Missing {
                baseline: id.to_owned(),
                line: index + 1,
            });
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct DocError {
    claim: &'static str,
    path: String,
}

fn normalized_paragraphs(contents: &str, rust_source: bool) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();
    for line in contents.lines() {
        let mut line = line.trim();
        if rust_source {
            let Some(rest) = ["//!", "///", "//"]
                .into_iter()
                .find_map(|marker| line.strip_prefix(marker))
            else {
                if !current.is_empty() {
                    paragraphs.push(current.join(" ").to_lowercase());
                    current.clear();
                }
                continue;
            };
            line = rest.trim();
        } else if let Some(rest) = line.strip_prefix('>') {
            line = rest.trim();
        }
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" ").to_lowercase());
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" ").to_lowercase());
    }
    paragraphs
}

fn false_claim(paragraph: &str) -> Option<&'static str> {
    let has = |needle: &str| paragraph.contains(needle);
    if has("pointhandle") && has("pub(crate)") {
        Some("point-handle-private")
    } else if has("pointhandle") && has("never") && has("trait boundary") {
        Some("point-handle-never-crosses-port")
    } else if has("pointhandle") && has("resolve") && (has("set_input") || has("get_output")) {
        Some("string-io-uses-point-handle")
    } else if has("public surface")
        && has("exactly")
        && has("engine::")
        && !has("public-api.txt")
        && !has("baseline")
    {
        Some("prose-exhausts-public-surface")
    } else if has("python") && has("rust surface") && has("must never") && has("domainkey") {
        Some("python-rule-describes-all-rust")
    } else if has("same simspec")
        && has("same paramtable")
        && has("bit-identical")
        && !has("entry connector")
    {
        Some("simulation-omits-entry-image")
    } else if has("snapshot bytes")
        && has("pointstore")
        && (has(" through ") || has(" via "))
        && !has("not ")
        && !has("no ")
        && !has("do not")
    {
        Some("engine-snapshot-through-point-store")
    } else {
        None
    }
}

fn validate_supported_docs(docs: &[(String, String)]) -> Result<(), DocError> {
    for (path, contents) in docs {
        for paragraph in normalized_paragraphs(contents, path.ends_with(".rs")) {
            if let Some(claim) = false_claim(&paragraph) {
                return Err(DocError {
                    claim,
                    path: path.clone(),
                });
            }
        }
    }
    Ok(())
}

fn collect_files(root: &Path, directory: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(root, &path, extension, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            assert!(path.starts_with(root));
            out.push(path);
        }
    }
}

fn supported_docs() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    let mut root_entries: Vec<PathBuf> = fs::read_dir(&root)
        .expect("read repository root")
        .map(|entry| entry.expect("read repository root entry").path())
        .filter(|path| {
            path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("md")
        })
        .collect();
    paths.append(&mut root_entries);
    collect_files(&root, &root.join("docs"), "md", &mut paths);
    for source in [
        "crates/oce-api/src",
        "crates/oce-store/src",
        "crates/oce-model/src",
        "crates/oce-blocks/src",
    ] {
        collect_files(&root, &root.join(source), "rs", &mut paths);
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("supported path under repository root")
                .to_string_lossy()
                .into_owned();
            let contents = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            (relative, contents)
        })
        .collect()
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, bytes) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().expect("four-byte SHA word"));
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let choose = (e & f) ^ (!e & g);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let upper_a = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let upper_e = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let first = h
                .wrapping_add(upper_e)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let second = upper_a.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

fn mutate_ledger(change: impl FnOnce(&mut Value)) -> String {
    let mut ledger: Value = serde_json::from_str(LEDGER).expect("checked-in ledger parses");
    change(&mut ledger);
    serde_json::to_string(&ledger).expect("mutated ledger serializes")
}

fn status_for(ledger: &str, baseline: Baseline<'_>, item: &str) -> String {
    let matching_lines: Vec<usize> = baseline
        .contents
        .lines()
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == item).then_some(index + 1))
        .collect();
    assert_eq!(
        matching_lines.len(),
        1,
        "classification sentinel must be unique"
    );
    let line = matching_lines[0];
    let document: Value = serde_json::from_str(ledger).expect("ledger parses");
    let entries = document["entries"].as_array().expect("entries");
    let groups = document["groups"].as_object().expect("groups");
    let mut matched = None;
    for entry in entries {
        if entry["baseline"].as_str() != Some(baseline.id) {
            continue;
        }
        for range in entry["ranges"].as_array().expect("ranges") {
            let (start, end) =
                parse_range(range.as_str().expect("range string")).expect("valid range");
            if (start..=end).contains(&line) {
                assert!(
                    matched.is_none(),
                    "classification sentinel is multiply assigned"
                );
                let group = entry["group"].as_str().expect("group");
                matched = Some(
                    groups[group]["status"]
                        .as_str()
                        .expect("group status")
                        .to_owned(),
                );
            }
        }
    }
    matched.expect("classification sentinel is assigned")
}

#[test]
fn tracked_authority_covers_exact_baselines_and_supported_docs() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    validate_ledger(LEDGER, &baselines()).expect("ledger covers each exact baseline row once");
    validate_supported_docs(&supported_docs())
        .expect("supported docs contain no historical false claim");
}

#[test]
fn ratified_surface_items_keep_their_reviewed_classifications() {
    let [api, store] = baselines();
    let expected = [
        (
            api,
            "pub oce_api::LoadReport::model_id: oce_store::DomainKey",
            "stable-candidate",
        ),
        (api, "pub use oce_api::ConnectorId", "stable-candidate"),
        (
            api,
            "pub fn oce_api::Engine<S>::schedule(&self) -> &oce_graph::Schedule",
            "implementation-leakage-to-remove",
        ),
        (
            api,
            "pub fn oce_api::Engine<S>::store(&self) -> &S",
            "conditional",
        ),
        (
            api,
            "pub fn oce_api::Engine<S>::with_store(alloc::sync::Arc<S>) -> Self",
            "conditional",
        ),
        (
            api,
            "pub fn oce_api::Engine<S>::load_modelica(&mut self, &std::path::Path) -> core::result::Result<oce_api::LoadReport, oce_api::OcError>",
            "unstable/deferred",
        ),
        (
            api,
            "pub fn oce_api::ExportReport::content_id(&self) -> alloc::string::String",
            "deprecated",
        ),
        (
            store,
            "pub struct oce_store::DomainKey(pub alloc::boxed::Box<str>)",
            "stable-candidate",
        ),
        (
            store,
            "pub struct oce_store::PointHandle(pub u64)",
            "conditional",
        ),
        (
            store,
            "pub trait oce_store::Store: oce_store::ModelStore + oce_store::PointStore + oce_store::SemanticStore + oce_store::Durable + core::marker::Send + core::marker::Sync",
            "conditional",
        ),
    ];
    for (baseline, item, expected_status) in expected {
        assert_eq!(
            status_for(LEDGER, baseline, item),
            expected_status,
            "{item}"
        );
    }
}

#[test]
fn an_unclassified_baseline_row_is_rejected() {
    let ledger = mutate_ledger(|value| {
        value["entries"][0]["ranges"]
            .as_array_mut()
            .expect("ranges")
            .remove(0);
    });
    assert_eq!(
        validate_ledger(&ledger, &baselines()),
        Err(LedgerError::Missing {
            baseline: "oce-api".to_owned(),
            line: 1,
        })
    );
}

#[test]
fn an_out_of_baseline_row_is_rejected() {
    let ledger = mutate_ledger(|value| {
        value["entries"][0]["ranges"]
            .as_array_mut()
            .expect("ranges")
            .push(Value::String("1409".to_owned()));
    });
    assert_eq!(
        validate_ledger(&ledger, &baselines()),
        Err(LedgerError::Extra {
            baseline: "oce-api".to_owned(),
            line: 1409,
        })
    );
}

#[test]
fn a_multiply_classified_baseline_row_is_rejected() {
    let ledger = mutate_ledger(|value| {
        value["entries"][0]["ranges"]
            .as_array_mut()
            .expect("ranges")
            .push(Value::String("42".to_owned()));
    });
    assert_eq!(
        validate_ledger(&ledger, &baselines()),
        Err(LedgerError::Duplicate {
            baseline: "oce-api".to_owned(),
            line: 42,
        })
    );
}

#[test]
fn a_status_outside_the_closed_vocabulary_is_rejected() {
    let ledger = mutate_ledger(|value| {
        value["groups"]["facade-stable"]["status"] = Value::String("invented".to_owned());
    });
    assert_eq!(
        validate_ledger(&ledger, &baselines()),
        Err(LedgerError::InvalidStatus("invented".to_owned()))
    );
}

#[test]
fn baseline_byte_drift_is_rejected_even_when_row_count_is_unchanged() {
    let changed = API_BASELINE.replacen("ContentIdError", "ContentIdentityError", 1);
    let altered = [
        Baseline {
            id: "oce-api",
            path: "crates/oce-api/tests/public-api.txt",
            contents: &changed,
        },
        baselines()[1],
    ];
    assert_eq!(
        validate_ledger(LEDGER, &altered),
        Err(LedgerError::BaselineMismatch("oce-api".to_owned()))
    );
}

#[test]
fn every_historical_false_claim_trips_its_validator() {
    let mutations = [
        (
            "point-handle-private",
            "pub struct PointHandle(pub(crate) u64).",
        ),
        (
            "point-handle-never-crosses-port",
            "PointHandle values never surface across the trait boundary.",
        ),
        (
            "string-io-uses-point-handle",
            "set_input and get_output resolve strings to pre-resolved PointHandle values.",
        ),
        (
            "prose-exhausts-public-surface",
            "The public surface is exactly Engine::load_cxf and Engine::tick.",
        ),
        (
            "python-rule-describes-all-rust",
            "The Python rule says DomainKey must never appear anywhere in the Rust surface.",
        ),
        (
            "simulation-omits-entry-image",
            "Same SimSpec plus same ParamTable produces a bit-identical OutputTrace.",
        ),
        (
            "engine-snapshot-through-point-store",
            "Durable engine snapshot bytes travel through the typed PointStore port.",
        ),
    ];
    for (expected, statement) in mutations {
        let docs = vec![("injected.md".to_owned(), statement.to_owned())];
        assert_eq!(
            validate_supported_docs(&docs),
            Err(DocError {
                claim: expected,
                path: "injected.md".to_owned(),
            }),
            "validator accepted historical claim {expected}"
        );
    }
}
