//! A bounded deterministic mutation sweep over every document the importers read (importer note,
//! "Test plan", hostile corpus): the 31 imported corpus fixtures as committed, every emitted
//! golden, and the three vendored v0 graphs. Every mutant returns a typed result; none panics
//! (`catch_unwind` names the mutant). The sweep is split by document family so the runner spreads
//! it over its threads; each family stays well inside the per-test time bound in a debug build.

use crate::support;

use oce_author::{ImportError, ImportOptions, Imported, import_cxf, import_graph_v0};

use support::cxf::{IMPORTED, corpus_fixture};
use support::fixture;

/// An importer under sweep.
type Importer = fn(&[u8], &ImportOptions) -> Result<Imported, ImportError>;

/// The most offsets a sweep visits in one document.
const MAX_OFFSETS: usize = 300;

/// The offsets a sweep visits: every offset of a document of at most `4 * MAX_OFFSETS` bytes;
/// otherwise at most [`MAX_OFFSETS`], half spread evenly over the structural bytes (`"`, `\`,
/// braces, brackets, `:`, `,`) and the digit boundaries, half evenly over every offset, so a
/// large document is probed at its syntax and not only mid-string.
fn offsets(bytes: &[u8]) -> Vec<usize> {
    if bytes.len() <= 4 * MAX_OFFSETS {
        return (0..bytes.len()).collect();
    }
    let structural: Vec<usize> = (0..bytes.len())
        .filter(|&i| {
            matches!(
                bytes[i],
                b'"' | b'\\' | b'{' | b'}' | b'[' | b']' | b':' | b','
            ) || (bytes[i].is_ascii_digit() != bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        })
        .collect();
    let spread = |all: &[usize], count: usize| -> Vec<usize> {
        let stride = all.len().div_ceil(count.max(1)).max(1);
        all.iter().copied().step_by(stride).collect()
    };
    let every: Vec<usize> = (0..bytes.len()).collect();
    let mut chosen = spread(&structural, MAX_OFFSETS / 2);
    chosen.extend(spread(&every, MAX_OFFSETS / 2));
    chosen.sort_unstable();
    chosen.dedup();
    chosen
}

/// Import `bytes`, turning a panic into a failure that names the mutant.
fn import_named(
    import: Importer,
    bytes: &[u8],
    document: &str,
    at: usize,
    mutation: &str,
) -> Result<(), ()> {
    match std::panic::catch_unwind(|| import(bytes, &ImportOptions::new())) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err(()),
        Err(_) => panic!("the importer panicked on {document} at byte {at} ({mutation})"),
    }
}

/// Sweep `documents`, each of which must read unmutated. Per offset: bit flips, a multi-byte
/// character, an escape, a quote, a backslash, a lone surrogate escape or a bracket replacing the
/// byte or inserted before it, and the truncation there. A document above 16 KiB gets a reduced
/// mutation set (one flip, a quote replacing or inserted, the truncation; above 128 KiB only
/// inserted), so every document runs a few hundred offsets at bounded cost. Returns the case
/// count.
fn sweep(import: Importer, documents: &[(String, Vec<u8>)]) -> usize {
    let full: [&[u8]; 8] = [
        "\u{e9}".as_bytes(),
        "\u{1f321}".as_bytes(),
        b"\\",
        b"\"",
        b"\\u0000",
        b"\\ud800",
        b"[",
        b"}",
    ];
    let reduced: [&[u8]; 1] = [full[3]];
    let mut cases = 0usize;
    for (name, bytes) in documents {
        let read = import(bytes, &ImportOptions::new());
        assert!(read.is_ok(), "{name} reads unmutated: {read:?}");
        let large = bytes.len() > 16 * 1024;
        let huge = bytes.len() > 128 * 1024;
        let (flips, insertions): (&[u8], &[&[u8]]) = if large {
            (&[0x80], &reduced)
        } else {
            (&[0x01, 0x20, 0x80], &full)
        };
        let visited = offsets(bytes);
        assert!(
            visited.len() >= 250.min(bytes.len()),
            "{name}: {} offsets",
            visited.len()
        );
        for at in visited {
            for &flip in flips {
                let mut mutated = bytes.clone();
                mutated[at] ^= flip;
                let _ = import_named(import, &mutated, name, at, &format!("xor {flip:#04x}"));
                cases += 1;
            }
            for insert in insertions {
                let label = String::from_utf8_lossy(insert).into_owned();
                if !huge {
                    let mut replaced = bytes[..at].to_vec();
                    replaced.extend_from_slice(insert);
                    replaced.extend_from_slice(&bytes[at + 1..]);
                    let mutation = format!("replace with {label:?}");
                    let _ = import_named(import, &replaced, name, at, &mutation);
                    cases += 1;
                }
                let mut inserted = bytes[..at].to_vec();
                inserted.extend_from_slice(insert);
                inserted.extend_from_slice(&bytes[at..]);
                let _ = import_named(import, &inserted, name, at, &format!("insert {label:?}"));
                cases += 1;
            }
            if at + 1 < bytes.len() {
                assert!(
                    import_named(import, &bytes[..at], name, at, "truncate").is_err(),
                    "{name} truncated at {at}"
                );
                cases += 1;
            }
        }
    }
    cases
}

/// The committed corpus fixtures among [`IMPORTED`] that `keep` selects.
fn corpus(keep: impl Fn(&str) -> bool) -> Vec<(String, Vec<u8>)> {
    IMPORTED
        .iter()
        .filter(|name| keep(name))
        .map(|name| {
            (
                (*name).to_owned(),
                std::fs::read(corpus_fixture(name)).expect("fixture"),
            )
        })
        .collect()
}

/// The corpus families the sweep runs one test each over; together they are [`IMPORTED`].
const FAMILIES: &[(&str, &[&str])] = &[
    (
        "economizer high limits",
        &["generic_air_economizer_high_limits_"],
    ),
    (
        "terminal units and generic",
        &["cooling_only_", "generic_time_", "reheat_"],
    ),
    ("economizer subsequences", &["multizone_vav_economizer_"]),
    (
        "multizone setpoints",
        &[
            "multizone_vav_freeze_",
            "multizone_vav_outdoor_",
            "multizone_vav_relief_",
            "multizone_vav_return_",
        ],
    ),
    ("zones", &["thermal_zones_", "ventilation_zones_"]),
];

/// The committed corpus fixtures of the family named `family`.
fn family(family: &str) -> Vec<(String, Vec<u8>)> {
    let prefixes = FAMILIES
        .iter()
        .find(|(name, _)| *name == family)
        .map(|(_, prefixes)| *prefixes)
        .expect("a family");
    corpus(|name| prefixes.iter().any(|p| name.starts_with(p)))
}

fn sweep_family(name: &str) {
    let documents = family(name);
    assert!(!documents.is_empty(), "{name}");
    assert!(sweep(import_cxf, &documents) > 1_000, "{name}");
}

#[test]
fn the_families_partition_the_imported_corpus() {
    let mut covered: Vec<String> = FAMILIES
        .iter()
        .flat_map(|(name, _)| family(name).into_iter().map(|(fixture, _)| fixture))
        .collect();
    covered.sort();
    assert_eq!(covered, IMPORTED);
}

#[test]
fn a_sweep_over_the_economizer_high_limit_fixtures_never_panics() {
    sweep_family("economizer high limits");
}

#[test]
fn a_sweep_over_the_terminal_unit_and_generic_fixtures_never_panics() {
    sweep_family("terminal units and generic");
}

#[test]
fn a_sweep_over_the_economizer_subsequence_fixtures_never_panics() {
    sweep_family("economizer subsequences");
}

#[test]
fn a_sweep_over_the_multizone_setpoint_fixtures_never_panics() {
    sweep_family("multizone setpoints");
}

#[test]
fn a_sweep_over_the_zone_fixtures_never_panics() {
    sweep_family("zones");
}

#[test]
fn a_sweep_over_every_emitted_golden_never_panics() {
    let mut paths: Vec<_> = std::fs::read_dir(fixture("emit"))
        .expect("goldens")
        .map(|e| e.expect("entry").path())
        .collect();
    paths.sort();
    let documents: Vec<(String, Vec<u8>)> = paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("name")
                .to_owned();
            (name, std::fs::read(path).expect("golden"))
        })
        .collect();
    assert!(documents.len() >= 2);
    assert!(sweep(import_cxf, &documents) > 10_000);
}

#[test]
fn a_sweep_over_the_vendored_v0_graphs_never_panics() {
    let documents: Vec<(String, Vec<u8>)> = ["ahu_0005", "ahu_0027", "width_max_min"]
        .iter()
        .map(|name| {
            let path = fixture(&format!("v0/{name}.graph.json"));
            ((*name).to_owned(), std::fs::read(path).expect("graph"))
        })
        .collect();
    assert!(sweep(import_graph_v0, &documents) > 10_000);
}
