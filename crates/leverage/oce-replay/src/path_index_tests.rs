//! The index answers exactly as `resolve_path` does, and in comparisons that grow with the names
//! plus the paths, never with their product.

use std::cell::Cell;

use super::PathIndex;

thread_local! {
    /// Path comparisons made on this thread since the last [`take`].
    pub(super) static COMPARISONS: Cell<u64> = const { Cell::new(0) };
}

fn take() -> u64 {
    COMPARISONS.with(|count| count.replace(0))
}

/// A small deterministic generator, so the corpus is the same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).expect("fits") % below
    }
}

/// Path fragments with the edge cases the rule cares about: dots, brackets, a `#`, repeats of
/// one last segment, multi-byte characters, and segments that are suffixes of one another.
const SEGMENTS: &[&str] = &[
    "T", "uDam", "Dam", "ahu", "zone", "u", "u[1]", "u[2]", "x.y", "é", "µs", "a", "aa", "k",
    "uLow", "uHigh", "", "#", "1",
];

fn corpus(seed: u64, count: usize) -> Vec<String> {
    let mut rng = Lcg(seed);
    let mut paths: Vec<String> = (0..count)
        .map(|_| {
            let mut path = String::from(["urn:x#top", "urn:y#", "urn:x#top.ahu"][rng.next(3)]);
            for _ in 0..rng.next(4) {
                path.push('.');
                path.push_str(SEGMENTS[rng.next(SEGMENTS.len())]);
            }
            path
        })
        .collect();
    paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    paths.dedup();
    paths
}

/// Every name the corpus can produce: each path, each of its suffixes at every byte that is a
/// character boundary, and fragments that match nothing.
fn names(paths: &[String]) -> Vec<String> {
    let mut names = vec![
        String::new(),
        ".".into(),
        "..".into(),
        "#".into(),
        "top".into(),
        "x#top".into(),
        "missing".into(),
        "u[".into(),
        "[1]".into(),
    ];
    for path in paths {
        for (at, _) in path.char_indices() {
            names.push(path[at..].to_owned());
        }
        names.push(format!("{path}."));
        names.push(format!(".{path}"));
    }
    names.sort();
    names.dedup();
    names
}

fn assert_same(paths: &[String]) {
    let index = PathIndex::new(paths);
    for name in names(paths) {
        assert_eq!(
            index.resolve(&name),
            index.linear(&name),
            "{name:?} in {paths:?}"
        );
    }
}

#[test]
fn every_answer_is_resolve_paths_over_a_generated_corpus() {
    let mut ambiguous = 0;
    let mut suffix = 0;
    for seed in 0..40 {
        let paths = corpus(seed, 3 + usize::try_from(seed).expect("fits") * 2);
        assert_same(&paths);
        let index = PathIndex::new(&paths);
        for name in names(&paths) {
            match index.resolve(&name) {
                Err(candidates) if candidates.len() > 1 => ambiguous += 1,
                Ok(slot) if paths[slot] != name => suffix += 1,
                _ => {}
            }
        }
    }
    // The corpus exercises both refusals and the suffix rule, not only exact names.
    assert!(ambiguous > 100 && suffix > 100, "{ambiguous} {suffix}");
}

#[test]
fn a_list_out_of_order_or_with_a_repeat_is_answered_as_resolve_path_answers_it() {
    let mut shuffled = corpus(7, 30);
    shuffled.reverse();
    assert_same(&shuffled);
    let mut repeated = corpus(9, 20);
    repeated.push(repeated[3].clone());
    repeated.sort();
    assert_same(&repeated);
    assert_same(&[]);
}

#[test]
fn an_ambiguous_name_lists_its_candidates_in_list_order() {
    let paths = [
        "urn:x#top.ahu.T",
        "urn:x#top.ahu.uDam",
        "urn:x#top.b.T",
        "urn:x#top.zone.T",
    ];
    let index = PathIndex::new(&paths);
    assert_eq!(
        index.resolve("T"),
        Err(vec![
            "urn:x#top.ahu.T".to_owned(),
            "urn:x#top.b.T".to_owned(),
            "urn:x#top.zone.T".to_owned()
        ])
    );
    assert_eq!(index.resolve("zone.T"), Ok(3));
    assert_eq!(index.resolve("top.zone.T"), Err(Vec::new()));
}

/// The parameters of `blocks` hysteresis blocks: two per block, every block's named alike.
fn bank(blocks: usize) -> Vec<String> {
    let mut paths: Vec<String> = (0..blocks)
        .flat_map(|i| {
            [
                format!("urn:x#top.h{i}.uLow"),
                format!("urn:x#top.h{i}.uHigh"),
            ]
        })
        .collect();
    paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    paths
}

#[test]
fn comparisons_grow_with_names_plus_paths_not_their_product() {
    // The review of GitHub #570: 51,200 suffix keys against 8,000 parameters. The linear scan
    // makes one comparison per path per key, 409,600,000 here.
    let paths = bank(4_000);
    let keys: Vec<String> = (0..51_200)
        .map(|k| format!("h{}.uLow", k % 4_000))
        .collect();
    take();
    let index = PathIndex::new(&paths);
    let built = take();
    for key in &keys {
        assert!(index.resolve(key).is_ok(), "{key}");
    }
    let looked_up = take();
    let linear = 51_200 * 8_000;
    // Sorting 8,000 paths is about n log2 n; each key is two binary searches (the exact one and
    // the run's start, 13 comparisons each) and the run of one match plus its end.
    assert!(built <= 8_000 * 14, "{built}");
    assert!(looked_up <= 51_200 * (2 * 14 + 2), "{looked_up}");
    assert!((built + looked_up) * 200 < linear, "{built} + {looked_up}");
}

#[test]
fn a_last_segment_every_path_shares_costs_nothing_extra() {
    // Every path ends in `k`: an index by last segment alone would scan all of them per key.
    let mut paths: Vec<String> = (0..10_000).map(|i| format!("urn:x#top.b{i}.k")).collect();
    paths.sort();
    let index = PathIndex::new(&paths);
    take();
    for i in 0..10_000 {
        assert!(index.resolve(&format!("b{i}.k")).is_ok());
    }
    assert!(take() <= 10_000 * (2 * 15 + 2));
    // `k` alone is ambiguous, with every path its candidate, in list order.
    assert_eq!(index.resolve("k").expect_err("ambiguous").len(), 10_000);
}
