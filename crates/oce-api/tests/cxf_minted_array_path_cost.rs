//! What the paths of the array elements a document mints cost a load, in requested bytes and
//! never in elapsed time (issue #756; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A minted element's path is the instance IRI, `.` and the element's name, and the document
//! writes the IRI once whatever the number of elements: a broadcast of 65,536 under a 1,005-byte
//! IRI is a 12 KB document whose paths take 66 MB. The paths of the elements a document mints are
//! held to 8 MiB (`MAX_DOCUMENT_MINTED_ARRAY_PATH_BYTES`), charged with the minted element budget
//! (`cxf_minted_array_cost.rs`, whose documents and reader these tests share) before any name is
//! built.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change).

use allocation_counter::measure;
use oce_api::Engine;
use oce_diag::{DiagCode, Diagnostic};

use crate::cxf_minted_array_cost::{
    MODEL, array, named_leaves, read, refused_for_minting, rooted_gain,
};

/// The minted-path budget (`oce-cxf`'s `MAX_DOCUMENT_MINTED_ARRAY_PATH_BYTES`), restated so this
/// test fails if it moves without review.
const MINTED_PATHS: usize = 8 * 1024 * 1024;

/// An IRI of `bytes` bytes.
fn iri(bytes: usize, fill: char) -> String {
    format!("urn:{}", fill.to_string().repeat(bytes - 4))
}

/// The decimal digits of 1 to `n` together.
fn digits_to(n: usize) -> usize {
    (1..=n).map(|i| i.to_string().len()).sum()
}

/// The bytes of the paths of a one-dimensional array of `n` elements named `base` on an instance
/// whose IRI is `owner` bytes: `<owner>.<base>_<i>` for each `i`.
fn paths(owner: usize, base: usize, n: usize) -> usize {
    n * (owner + 1 + base + 1) + digits_to(n)
}

/// The bytes of the paths of a two-dimensional array of `rows` by `cols` named `base` on an
/// instance whose IRI is `owner` bytes: `<owner>.<base>_<i>_<j>` for each `i` and `j`.
fn grid_paths(owner: usize, base: usize, rows: usize, cols: usize) -> usize {
    rows * cols * (owner + 1 + base + 2) + cols * digits_to(rows) + rows * digits_to(cols)
}

/// The minted-path refusal, stated by hand.
fn minted_path_refusal(subject: &str, minted: usize, bytes: usize, used: usize) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "array parameter's {minted} minted element path(s) (instance IRI, `.`, element name) \
             need {bytes} byte(s) but the paths of the elements the document's array parameters \
             mint already use {used} of the 8388608 bytes a document may mint"
        ),
    )
    .with_subject(subject.to_owned())
}

/// A document whose minted paths take the budget exactly, or `over` bytes more.
struct PathEdge {
    /// The document.
    document: Vec<u8>,
    /// The one-element `fill` node that takes the last bytes.
    fill: String,
    /// The bytes of the broadcast's paths.
    used: usize,
    /// The bytes of the `fill`'s one path.
    last: usize,
    /// The broadcast's element count.
    count: usize,
}

/// A leaf whose IRI is `owner` bytes listing a broadcast `urn:a.q`, of one dimension when `rows`
/// is 1 and of `rows` by `n` otherwise, and a one-element `fill` whose name takes the rest of the
/// minted path budget, and `over` bytes more.
fn at_the_path_edge(owner: usize, rows: usize, over: usize) -> PathEdge {
    // Leave the `fill` a path of at least `owner + 1 + 1 + 2` bytes.
    let edge = MINTED_PATHS - owner - 4;
    let used = |n: usize| {
        if rows == 1 {
            paths(owner, 1, n)
        } else {
            grid_paths(owner, 1, rows, n)
        }
    };
    let mut n = edge / (owner + 3) / rows;
    while used(n) > edge {
        n -= 1;
    }
    let dims = if rows == 1 { vec![n] } else { vec![rows, n] };
    let used = used(n);
    // A one-element array named `name` on the leaf takes `owner + 1 + name + 2` bytes.
    let name = MINTED_PATHS - used - owner - 3 + over;
    let fill = format!("urn:b.{}", "b".repeat(name));
    let document = named_leaves(
        1,
        |_| iri(owner, 'x'),
        |_| vec!["urn:a.q".to_owned(), fill.clone()],
        &[
            array("urn:a.q", &dims, "[ 1.5 ]"),
            array(&fill, &[1], "\"fill(1.5, 1)\""),
        ],
    );
    PathEdge {
        document,
        fill,
        used,
        last: owner + 1 + name + 2,
        count: rows * n,
    }
}

#[test]
fn minted_paths_fill_their_budget_exactly_and_one_byte_more_is_refused() {
    // A broadcast under a 1,005-byte instance IRI and a one-element `fill` whose name takes the
    // last bytes: exactly 8 MiB of minted paths read, one byte more is refused on the `fill`. The
    // broadcast is one row, then 64 rows of 129, whose paths carry two indices each: charged as
    // one row of as many elements, its paths would count 9,546 bytes fewer and one byte past
    // would read.
    // [measured: peaks of 18,081,022 bytes at the one-row edge, from a 12,549-byte document, and
    // 18,228,799 at the 64-row edge]
    for rows in [1, 64] {
        let edge = at_the_path_edge(1_005, rows, 0);
        assert_eq!(edge.used + edge.last, MINTED_PATHS, "{rows}");
        if rows > 1 {
            assert_eq!(
                edge.used - paths(1_005, 1, edge.count),
                9_546,
                "the two-index paths are what is charged"
            );
        }
        let (refused, info) = read(&edge.document);
        assert!(
            info.bytes_max <= 24 * 1024 * 1024,
            "{rows}: a peak of {}",
            info.bytes_max
        );
        assert!(refused.is_none(), "{rows}: {refused:?}");
        let edge = at_the_path_edge(1_005, rows, 1);
        let (refused, _) = read(&edge.document);
        let refused = refused.expect("one byte past is refused");
        assert!(
            refused_for_minting(
                &refused,
                &minted_path_refusal(&edge.fill, 1, edge.last, edge.used)
            ),
            "{rows}: {refused:?}"
        );
    }
}

#[test]
fn a_written_array_listed_again_under_a_long_iri_is_charged_its_paths() {
    // A written `q[n]` listed by a short-named leaf (written) and a 1,005-byte-named one (minted):
    // the second listing's paths are minted. At the budget it reads; one element more is refused.
    let owner = 1_005;
    let mut n = MINTED_PATHS / (owner + 3);
    while paths(owner, 1, n) > MINTED_PATHS {
        n -= 1;
    }
    let build = |n: usize| {
        let values = vec!["1"; n].join(", ");
        named_leaves(
            2,
            |i| {
                if i == 0 {
                    "urn:c0".to_owned()
                } else {
                    iri(owner, 'x')
                }
            },
            |_| vec!["urn:a.q".to_owned()],
            &[array("urn:a.q", &[n], &format!("[ {values} ]"))],
        )
    };
    let (refused, info) = read(&build(n));
    assert!(
        info.bytes_max <= 28 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    assert!(refused.is_none(), "{refused:?}");
    let (refused, _) = read(&build(n + 1));
    let refused = refused.expect("refused");
    assert!(
        refused_for_minting(
            &refused,
            &minted_path_refusal("urn:a.q", n + 1, paths(owner, 1, n + 1), 0)
        ),
        "{refused:?}"
    );
}

#[test]
fn long_iri_broadcasts_are_refused_before_a_name_is_built() {
    // Broadcasts of 16,384, 32,768 and 65,536 under a 1,005-byte IRI (paths of 16.5 to 66 MB, a
    // 12 KB document at most): refused at one flat peak. [measured: 151,779 bytes each; before, about
    // 2,170 bytes an element, 35.6 MB at 16,384]
    let mut peaks = Vec::new();
    for n in [16_384, 32_768, 65_536] {
        let document = named_leaves(
            1,
            |_| iri(1_005, 'x'),
            |_| vec!["urn:a.q".to_owned()],
            &[array("urn:a.q", &[n], "[ 1.5 ]")],
        );
        let (refused, info) = read(&document);
        let refused = refused.expect("refused");
        assert!(
            refused_for_minting(
                &refused,
                &minted_path_refusal("urn:a.q", n, paths(1_005, 1, n), 0)
            ),
            "{refused:?}"
        );
        peaks.push(info.bytes_max);
    }
    assert!(peaks.iter().all(|&peak| peak <= 512 * 1024), "{peaks:?}");
    assert!(
        peaks.iter().all(|&peak| peak.abs_diff(peaks[0]) <= 4_096),
        "{peaks:?}"
    );
}

#[test]
fn a_gain_under_a_long_root_is_bounded_by_its_minted_paths() {
    // A `MatrixGain` under a 921-byte root: 90 by 90 (7.5 MB of `K` paths) loads; 128 by 128
    // (16.4 MB) is refused before `K` is built. [measured: 90 by 90 loads at a peak of 26,511,411
    // bytes from a 441,038-byte document; 128 by 128 is refused at 5,153,480]
    let root = iri(921, 'r');
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let document = rooted_gain(&root, 90, 90);
    let mut loaded = None;
    let info = measure(|| loaded = Some(Engine::new().load_cxf(&document).map(|_| ())));
    loaded.expect("ran").expect("90 by 90 loads");
    assert!(
        info.bytes_max <= 48 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    let mut refused = None;
    let info = measure(|| refused = Some(Engine::new().load_cxf(&rooted_gain(&root, 128, 128))));
    assert!(
        info.bytes_max <= 8 * 1024 * 1024,
        "a refusal peak of {}",
        info.bytes_max
    );
    let error = refused.expect("ran").expect_err("128 by 128 is refused");
    assert!(error.all_diagnostics().any(|d| {
        d.code == DiagCode::ResourceLimitExceeded
            && d.message
                .starts_with("array parameter's 16384 minted element path(s)")
    }));
}

#[test]
fn minted_paths_of_a_shared_node_are_charged_as_separate_nodes_are() {
    // Two 1,005-byte-named leaves listing one broadcast node, or a node each: the same reads, at
    // a size whose two listings fit and one whose second does not.
    for n in [4_000, 4_200] {
        let shared = named_leaves(
            2,
            |i| iri(1_005, char::from(b'p' + u8::try_from(i).expect("small"))),
            |_| vec!["urn:s.q".to_owned()],
            &[array("urn:s.q", &[n], "[ 1.5 ]")],
        );
        let separate = named_leaves(
            2,
            |i| iri(1_005, char::from(b'p' + u8::try_from(i).expect("small"))),
            |i| vec![format!("urn:s{i}.q")],
            &[
                array("urn:s0.q", &[n], "[ 1.5 ]"),
                array("urn:s1.q", &[n], "[ 1.5 ]"),
            ],
        );
        let renamed = |d: Option<Vec<Diagnostic>>| {
            format!("{d:?}")
                .replace("urn:s0.q", "urn:s.q")
                .replace("urn:s1.q", "urn:s.q")
        };
        let (shared, _) = read(&shared);
        let (separate, _) = read(&separate);
        assert_eq!(shared.is_none(), n == 4_000, "{n}: {shared:?}");
        assert_eq!(renamed(shared), renamed(separate), "{n}");
    }
}

#[test]
fn a_broadcasts_minted_path_refusal_under_iris_of_one_length_is_the_same_in_any_owner_order() {
    // Three leaves whose IRIs are all 1,005 bytes listing one `q[3000]` broadcast: each listing
    // mints the same paths, so the third crosses the minted path budget, whichever leaf comes
    // third. (Under IRIs of different lengths the order can matter; see the next test.)
    let node = [array("urn:s.q", &[3_000], "[ 1.5 ]")];
    let ids = |i: usize| iri(1_005, char::from(b'p' + u8::try_from(i).expect("small")));
    let forward = named_leaves(3, ids, |_| vec!["urn:s.q".to_owned()], &node);
    let reversed = named_leaves(3, |i| ids(2 - i), |_| vec!["urn:s.q".to_owned()], &node);
    let expected = minted_path_refusal(
        "urn:s.q",
        3_000,
        paths(1_005, 1, 3_000),
        2 * paths(1_005, 1, 3_000),
    );
    for document in [forward.clone(), reversed, forward] {
        let (refused, _) = read(&document);
        let refused = refused.expect("refused");
        assert!(refused_for_minting(&refused, &expected), "{refused:?}");
    }
}

#[test]
fn which_listing_of_a_written_node_writes_follows_contains_block_order() {
    // A written `q[8322]` listed by the short-named leaf `urn:c0` and a 1,005-byte-named one. The
    // first listing in `containsBlock` order writes the elements and the other mints them, its
    // paths spelled under its own IRI: the short leaf first leaves the long one 8,420,757 bytes of
    // minted paths, refused; the long leaf first leaves the short one 107,079, which reads (the
    // long listing's paths are charged to the 64 MiB path budget, as a separate node's would
    // be). So the outcome depends on that order, deterministically: each order reads the same
    // way twice (owner ruling of 2026-11-22). [measured: short first, refused at a peak of
    // 4,721,327 bytes; long first, read at 21,719,161]
    let n = 8_322;
    assert_eq!(paths(1_005, 1, n), 8_420_757);
    assert_eq!(paths("urn:c0".len(), 1, n), 107_079);
    let node = [array(
        "urn:a.q",
        &[n],
        &format!("[ {} ]", vec!["1"; n].join(", ")),
    )];
    let build = |long_first: bool| {
        named_leaves(
            2,
            |i| {
                if (i == 0) == long_first {
                    iri(1_005, 'x')
                } else {
                    "urn:c0".to_owned()
                }
            },
            |_| vec!["urn:a.q".to_owned()],
            &node,
        )
    };
    let expected = minted_path_refusal("urn:a.q", n, 8_420_757, 0);
    for _ in 0..2 {
        let (refused, info) = read(&build(false));
        let refused = refused.expect("the short leaf first is refused");
        assert!(refused_for_minting(&refused, &expected), "{refused:?}");
        assert!(
            info.bytes_max <= 8 * 1024 * 1024,
            "a refusal peak of {}",
            info.bytes_max
        );
        let (refused, info) = read(&build(true));
        assert!(refused.is_none(), "the long leaf first reads: {refused:?}");
        assert!(
            info.bytes_max <= 28 * 1024 * 1024,
            "a peak of {}",
            info.bytes_max
        );
    }
}

#[test]
fn a_document_at_both_minted_budgets_reads_and_loads_within_a_bound() {
    // The worst case at both budgets together: 65,536 minted elements whose paths take nearly the
    // whole 8 MiB (a 120-byte instance IRI), read; and a 256 by 256 `MatrixGain` under a 116-byte
    // root, loaded. [measured: 8,377,502 bytes of minted paths read at a peak of 25,948,872
    // bytes from a 6,604-byte document; the gain loads at 49,674,320, 140,301,119 requested]
    let owner = 120;
    assert!(paths(owner, 1, 65_536) <= MINTED_PATHS);
    let reader = named_leaves(
        1,
        |_| iri(owner, 'x'),
        |_| vec!["urn:a.q".to_owned()],
        &[array("urn:a.q", &[65_536], "[ 1.5 ]")],
    );
    let (refused, info) = read(&reader);
    assert!(refused.is_none(), "{refused:?}");
    assert!(
        info.bytes_max <= 48 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    // The same budgets reached by listing a written `q[1024]` again: 65 leaves whose IRIs are 120
    // bytes each, whose 64 further listings mint 65,536 elements and 8,252,224 bytes of paths.
    // [measured: a peak of 27,514,325 bytes, the most of any shape at both budgets]
    let id = |i: usize| format!("urn:{i:0>116}");
    assert_eq!(id(64).len(), owner);
    assert_eq!(64 * paths(owner, 1, 1_024), 8_252_224);
    let listed = named_leaves(
        65,
        id,
        |_| vec!["urn:a.q".to_owned()],
        &[array(
            "urn:a.q",
            &[1_024],
            &format!("[ {} ]", vec!["1"; 1_024].join(", ")),
        )],
    );
    let (refused, info) = read(&listed);
    assert!(refused.is_none(), "{refused:?}");
    assert!(
        info.bytes_max <= 48 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let gain = rooted_gain(&iri(116, 'r'), 256, 256);
    let mut loaded = None;
    let info = measure(|| loaded = Some(Engine::new().load_cxf(&gain).map(|_| ())));
    loaded.expect("ran").expect("the gain loads");
    assert!(
        info.bytes_max <= 64 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
}
