//! What the array elements a document mints cost a load, in requested bytes and never in elapsed
//! time (issue #756; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A broadcast (`q[n]` with the value `[v]`) and an array expression (`fill(v, n)`) make `n`
//! elements from one value the document writes once. The element budget (1,048,576 a document)
//! admitted `q[1048576]` broadcasting `1.5`, a 5.6 KB document, which the reader expanded at about
//! 339 bytes an element before anything refused it (a 355 MB peak), and which once loaded at
//! 786 MB. The elements a document mints are now held to 65,536 a document
//! (`MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS`), charged before the array's value is read or any
//! element is built; the elements a document writes out keep the element budget alone.
//!
//! The arrays these documents add to the minimal loop's constant (`q`, `B`) are names its class
//! does not declare, which the facade's load gate refuses, so the reader (`oce_cxf::import_cxf`,
//! the facade's first step) is measured where a document must read, and the facade is held to the
//! reader's refusal. A whole load at the budget is measured on a `MatrixGain` whose `K` is
//! minted.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change).

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const GT: &str = "{ \"@id\": \"http://example.org#MinLoop.gt\" }\n      ],";
const LAST: &str = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";

/// The minted-element budget (`oce-cxf`'s `MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS`), restated so this
/// test fails if it moves without review.
const MINTED: usize = 65_536;

/// A preserved array node `id` of `dims` with the JSON `value`.
fn array(id: &str, dims: &[usize], value: &str) -> String {
    let size: Vec<String> = dims.iter().map(usize::to_string).collect();
    format!(
        "{{ \"@id\": \"{id}\", \"S231:isArray\": true, \"S231:numberDimensions\": {}, \
         \"S231:sizeOfDimensions\": \"({})\", \"S231:value\": {value} }}",
        dims.len(),
        size.join(", ")
    )
}

/// [`MODEL`] with `leaves` more `Constant` leaves, leaf `i` listing its own `k` and the nodes
/// `listed(i)`, and the nodes `nodes` declared at the end.
fn leaves(leaves: usize, listed: impl Fn(usize) -> Vec<String>, nodes: &[String]) -> Vec<u8> {
    named_leaves(leaves, |i| format!("urn:c{i}"), listed, nodes)
}

/// [`leaves`] with leaf `i` named `id(i)`.
fn named_leaves(
    leaves: usize,
    id: impl Fn(usize) -> String,
    listed: impl Fn(usize) -> Vec<String>,
    nodes: &[String],
) -> Vec<u8> {
    assert_eq!(MODEL.matches(GT).count(), 1);
    assert_eq!(MODEL.matches(LAST).count(), 1);
    let mut contains = String::new();
    let mut declared = String::new();
    for i in 0..leaves {
        let refs: String = listed(i)
            .iter()
            .map(|id| format!(", {{ \"@id\": \"{id}\" }}"))
            .collect();
        let c = id(i);
        contains.push_str(&format!(", {{ \"@id\": \"{c}\" }}"));
        declared.push_str(&format!(
            ",\n    {{ \"@id\": \"{c}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"{c}.k\" }}{refs} ], \
             \"S231:hasOutput\": {{ \"@id\": \"{c}.y\" }} }},\n    \
             {{ \"@id\": \"{c}.k\", \"S231:value\": 1.0 }},\n    \
             {{ \"@id\": \"{c}.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
        ));
    }
    for node in nodes {
        declared.push_str(&format!(",\n    {node}"));
    }
    MODEL
        .replacen(
            GT,
            &format!("{{ \"@id\": \"http://example.org#MinLoop.gt\" }}{contains}\n      ],"),
            1,
        )
        .replacen(
            LAST,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }}{declared}\n  ]"),
            1,
        )
        .into_bytes()
}

/// One leaf listing the one array node `urn:a.q` of `dims` with `value`.
fn one(dims: &[usize], value: &str) -> Vec<u8> {
    leaves(
        1,
        |_| vec!["urn:a.q".to_owned()],
        &[array("urn:a.q", dims, value)],
    )
}

/// One read of `bytes` by the CXF reader alone, measured, with its diagnostics when refused.
/// The unedited model is read once first.
fn read(bytes: &[u8]) -> (Option<Vec<Diagnostic>>, AllocationInfo) {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let mut result = None;
    let info = measure(|| result = Some(import_cxf(bytes, &ResolveOptions::default())));
    match result.expect("the read ran") {
        Ok(_) => (None, info),
        Err(CxfError::Validation(diagnostics)) => (Some(diagnostics), info),
        Err(other) => panic!("not a validation refusal: {other}"),
    }
}

/// The minted-budget refusal of `minted` elements on the parameter node `subject` when `used`
/// are already minted, stated by hand.
fn minted_refusal(subject: &str, minted: usize, used: usize) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "array parameter mints {minted} element(s) the document does not write out (a \
             broadcast, an array expression, or a further listing of an array node) but the \
             document's array parameters already mint {used} of the 65536 minted elements a \
             document may hold"
        ),
    )
    .with_subject(subject.to_owned())
}

/// Whether `diagnostics` hold the minted-budget refusal and no other array refusal.
fn refused_for_minting(diagnostics: &[Diagnostic], expected: &Diagnostic) -> bool {
    diagnostics.contains(expected)
        && diagnostics
            .iter()
            .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
            .count()
            == 1
}

#[test]
fn the_issues_broadcast_and_its_fill_are_refused_before_a_million_elements_are_built() {
    // `q[1048576]` broadcasting 1.5, the issue's 5.6 KB document, and the same array as
    // `fill(1.5, 1048576)`: refused before the value is read, at the cost of reading the
    // document. [measured: peaks of 126,587 and 126,425 bytes; before, the broadcast read at a
    // peak of about 355 MB (339 bytes an element at 32,768 to 131,072) and loaded at 786 MB]
    for value in ["[ 1.5 ]", "\"fill(1.5, 1048576)\""] {
        let (refused, info) = read(&one(&[1_048_576], value));
        let diagnostics = refused.expect("refused");
        assert!(
            refused_for_minting(&diagnostics, &minted_refusal("urn:a.q", 1_048_576, 0)),
            "{value}: {diagnostics:?}"
        );
        assert!(
            info.bytes_max <= 512 * 1024,
            "{value}: a peak of {}",
            info.bytes_max
        );
    }
    // And the facade refuses the issue's document on the reader's refusal.
    let error = Engine::new()
        .load_cxf(&one(&[1_048_576], "[ 1.5 ]"))
        .expect_err("refused");
    assert!(
        error
            .all_diagnostics()
            .any(|d| *d == minted_refusal("urn:a.q", 1_048_576, 0))
    );
}

#[test]
fn a_refused_broadcast_costs_the_same_however_many_elements_it_asks_for() {
    // 262,144, 524,288 and 1,048,576 elements: one refusal each, at the same peak.
    // [measured: 126,583 / 126,583 / 126,587 bytes]
    let peaks: Vec<u64> = [262_144, 524_288, 1_048_576]
        .into_iter()
        .map(|n| read(&one(&[n], "[ 1.5 ]")).1.bytes_max)
        .collect();
    assert!(
        peaks.iter().all(|&peak| peak.abs_diff(peaks[0]) <= 4_096),
        "{peaks:?}"
    );
}

/// A document of `n` minted elements along one minting path.
type Builder = Box<dyn Fn(usize) -> Vec<u8>>;

#[test]
fn exactly_the_budget_is_minted_and_one_element_more_is_refused_on_every_minting_path() {
    // A broadcast, a two-dimensional broadcast, an array expression, two arrays together, and
    // one array node two leaves list (each listing mints its own elements, as two nodes would).
    let cases: Vec<(&str, Builder)> = vec![
        ("broadcast", Box::new(|n| one(&[n], "[ 1.5 ]"))),
        (
            "two-dimensional broadcast",
            Box::new(|n| {
                let dims = if n % 256 == 0 { [256, n / 256] } else { [1, n] };
                one(&dims, "[ 1.5 ]")
            }),
        ),
        (
            "array expression",
            Box::new(|n| one(&[n], &format!("\"fill(1.5, {n})\""))),
        ),
        (
            "two arrays",
            Box::new(|n| {
                leaves(
                    1,
                    |_| vec!["urn:a.q".to_owned(), "urn:a.r".to_owned()],
                    &[
                        array("urn:a.q", &[2], "[ 1.5 ]"),
                        array("urn:a.r", &[n - 2], "[ 2.5 ]"),
                    ],
                )
            }),
        ),
    ];
    for (path, document) in &cases {
        let (refused, info) = read(&document(MINTED));
        assert!(refused.is_none(), "{path}: {refused:?}");
        let (refused, _) = read(&document(MINTED + 1));
        let diagnostics = refused.unwrap_or_else(|| panic!("{path}: one more is refused"));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == DiagCode::ResourceLimitExceeded
                    && d.message.starts_with("array parameter mints")),
            "{path}: {diagnostics:?}"
        );
        // [measured: peaks of 22,275,406 (broadcast), 22,472,623 (two dimensions), 23,847,110
        // (expression) and 22,278,104 (two arrays) bytes at the budget]
        assert!(
            info.bytes_max <= 40 * 1024 * 1024,
            "{path}: a peak of {}",
            info.bytes_max
        );
    }
}

#[test]
fn a_shared_array_node_mints_for_each_listing_as_separate_nodes_do() {
    // `q[32768]` listed by two leaves mints the budget exactly; `q[32769]` is refused at the
    // second listing. Shared or one node a leaf, the reads agree, refusal and all.
    for size in [32_768, 32_769] {
        let shared = leaves(
            2,
            |_| vec!["urn:s.q".to_owned()],
            &[array("urn:s.q", &[size], "[ 1.5 ]")],
        );
        let separate = leaves(
            2,
            |i| vec![format!("urn:s{i}.q")],
            &[
                array("urn:s0.q", &[size], "[ 1.5 ]"),
                array("urn:s1.q", &[size], "[ 1.5 ]"),
            ],
        );
        let (shared, _) = read(&shared);
        let (separate, _) = read(&separate);
        let renamed = |d: Option<Vec<Diagnostic>>| {
            format!("{d:?}")
                .replace("urn:s0.q", "urn:s.q")
                .replace("urn:s1.q", "urn:s.q")
        };
        assert_eq!(renamed(shared.clone()), renamed(separate), "{size}");
        if size == 32_768 {
            assert!(shared.is_none());
        } else {
            let shared = shared.expect("refused");
            assert!(refused_for_minting(
                &shared,
                &minted_refusal("urn:s.q", 32_769, 32_769)
            ));
        }
    }
}

#[test]
fn the_refusal_names_the_same_node_whatever_order_its_owners_are_listed_in() {
    // Three leaves listing one node `q[30000]`: the third listing crosses the budget, and which
    // leaf comes third does not change the refusal; reading it again gives it again.
    let node = [array("urn:s.q", &[30_000], "[ 1.5 ]")];
    let forward = leaves(3, |_| vec!["urn:s.q".to_owned()], &node);
    let text = String::from_utf8(forward.clone()).expect("utf-8");
    let reversed = text.replace(
        "{ \"@id\": \"urn:c0\" }, { \"@id\": \"urn:c1\" }, { \"@id\": \"urn:c2\" }",
        "{ \"@id\": \"urn:c2\" }, { \"@id\": \"urn:c1\" }, { \"@id\": \"urn:c0\" }",
    );
    assert_ne!(text, reversed);
    let expected = minted_refusal("urn:s.q", 30_000, 60_000);
    for bytes in [forward.clone(), reversed.into_bytes(), forward] {
        let (refused, _) = read(&bytes);
        let refused = refused.expect("refused");
        assert!(refused_for_minting(&refused, &expected), "{refused:?}");
    }
}

#[test]
fn a_written_out_list_past_the_minted_budget_still_reads() {
    // 65,537 elements written one by one mint nothing: the element budget alone holds them.
    // (A written-out list of the whole element budget, 1,048,576, is accounted the same way;
    // `oce-cxf`'s `arrays_tests.rs` pins that the minted budget draws nothing for it, without a
    // read that would hold hundreds of megabytes.)
    let elements = vec!["1.5"; MINTED + 1].join(", ");
    let (refused, info) = read(&one(&[MINTED + 1], &format!("[ {elements} ]")));
    assert!(refused.is_none(), "{refused:?}");
    // [measured: a peak of 40,099,814 bytes; the written list's JSON is most of the difference]
    assert!(
        info.bytes_max <= 48 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
}

/// A root holding one `MatrixGain` of `rows` by `cols` whose `K` broadcasts `0.5`, every input
/// driven by the root's one boundary input: at 256 by 256, the minted budget exactly.
fn minted_gain(rows: usize, cols: usize) -> Vec<u8> {
    rooted_gain("http://example.org#M", rows, cols)
}

/// [`minted_gain`] under the root `root`.
fn rooted_gain(root: &str, rows: usize, cols: usize) -> Vec<u8> {
    let id = format!("{root}.g");
    let refs = |ids: Vec<String>| {
        ids.iter()
            .map(|i| format!("{{\"@id\":\"{i}\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inputs: Vec<String> = (1..=cols).map(|i| format!("{id}.u{i}")).collect();
    let outputs: Vec<String> = (1..=rows).map(|i| format!("{id}.y{i}")).collect();
    let port = |port: &str, kind: &str| {
        format!(
            "{{\"@id\":\"{port}\",\"@type\":\"S231:Real{kind}\",\"S231:isOfDataType\":{{\"@id\":\"S231:Real\"}}}}"
        )
    };
    let mut nodes = vec![
        format!(
            "{{\"@id\":\"{root}\",\"@type\":\"S231:Block\",\"S231:containsBlock\":[{{\"@id\":\"{id}\"}}],\"S231:hasInput\":[{{\"@id\":\"{root}.u\"}}]}}"
        ),
        format!(
            "{{\"@id\":\"{root}.u\",\"@type\":\"S231:RealInput\",\"S231:isOfDataType\":{{\"@id\":\"S231:Real\"}},\"S231:isConnectedTo\":[{}]}}",
            refs(inputs.clone())
        ),
        format!(
            "{{\"@id\":\"{id}\",\"@type\":\"http://example.org#Buildings.Controls.OBC.CDL.Reals.MatrixGain\",\"S231:hasParameter\":[{{\"@id\":\"{id}.K\"}}],\"S231:hasInput\":[{}],\"S231:hasOutput\":[{}]}}",
            refs(inputs.clone()),
            refs(outputs.clone())
        ),
        format!(
            "{{\"@id\":\"{id}.K\",\"@type\":\"S231:Parameter\",\"S231:isArray\":true,\"S231:numberDimensions\":2,\"S231:sizeOfDimensions\":\"({rows}, {cols})\",\"S231:value\":[0.5]}}"
        ),
    ];
    nodes.extend(inputs.iter().map(|i| port(i, "Input")));
    nodes.extend(outputs.iter().map(|o| port(o, "Output")));
    format!(
        "{{\"@context\":{{\"S231\":\"http://data.ashrae.org/S231#\"}},\"@graph\":[{}]}}",
        nodes.join(",\n")
    )
    .into_bytes()
}

#[test]
fn a_whole_load_at_the_minted_budget_is_bounded() {
    // A `MatrixGain` whose `K` mints the whole budget, 65,536 elements, loads through the facade:
    // what a host pays for the most a document may mint. One element more is refused.
    // [measured: a 82,856-byte document loads at a peak of 30,577,248 bytes, 95,000,839
    // requested]
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let document = minted_gain(256, 256);
    let mut engine = Engine::new();
    let mut loaded = None;
    let info: AllocationInfo = measure(|| loaded = Some(engine.load_cxf(&document).map(|_| ())));
    loaded.expect("ran").expect("the gain loads");
    assert!(
        info.bytes_max <= 48 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    // One row more, a gain of 257 by 256 with 257 outputs: a shape the class accepts, so the
    // one refusal is the minted budget's.
    let error = Engine::new()
        .load_cxf(&minted_gain(257, 256))
        .expect_err("65,792 minted elements are refused");
    let refusals: Vec<&Diagnostic> = error
        .all_diagnostics()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .collect();
    assert_eq!(
        refusals,
        [&minted_refusal("http://example.org#M.g.K", 65_792, 0)]
    );
}

/// A written `q[1024]`, the node `id`.
fn written_q(id: &str) -> String {
    array(
        id,
        &[1_024],
        &format!("[ {} ]", vec!["1"; 1_024].join(", ")),
    )
}

#[test]
fn a_written_array_listed_again_mints_each_further_listing() {
    // A written `q[1024]` listed 65 times, by one leaf or by 65 leaves: the first listing writes
    // its elements and the 64 further listings mint 65,536, which reads; a 66th listing is
    // refused, before it builds an element. [measured: one leaf, a peak of 19,453,563 bytes; 65
    // leaves, 13,918,219; before, every listing read, 1,024 listings about 304 MB (review)]
    let node = [written_q("urn:a.q")];
    for listings in [65, 66] {
        let one_leaf = leaves(1, |_| vec!["urn:a.q".to_owned(); listings], &node);
        let many_leaves = leaves(listings, |_| vec!["urn:a.q".to_owned()], &node);
        for (shape, document) in [("one leaf", one_leaf), ("many leaves", many_leaves)] {
            let (refused, info) = read(&document);
            if listings == 65 {
                assert!(refused.is_none(), "{shape}: {refused:?}");
            } else {
                let refused = refused.unwrap_or_else(|| panic!("{shape}: refused"));
                assert!(
                    refused_for_minting(&refused, &minted_refusal("urn:a.q", 1_024, 65_536)),
                    "{shape}: {refused:?}"
                );
            }
            assert!(
                info.bytes_max <= 24 * 1024 * 1024,
                "{shape}: a peak of {}",
                info.bytes_max
            );
        }
    }
}

#[test]
fn separate_written_arrays_listed_once_each_mint_nothing() {
    // 66 written `q[1024]` nodes, one for each leaf: 67,584 elements written out, none minted.
    let nodes: Vec<String> = (0..66).map(|i| written_q(&format!("urn:a{i}.q"))).collect();
    let document = leaves(66, |i| vec![format!("urn:a{i}.q")], &nodes);
    let (refused, _) = read(&document);
    assert!(refused.is_none(), "{refused:?}");
}

#[test]
fn the_facade_refuses_a_leaf_listing_one_array_again() {
    // Through the facade a leaf listing one node twice is `duplicate-parameter`, which the
    // validator raises once the reader has read the leaf; the reader's work on the further
    // listings is the minted budget's, so past it the reader refuses first. At the cap the
    // duplicate refusal comes after a reader peak of about 19.5 MB and a facade peak of about
    // 25.7 MB (owner ruling of 2026-11-22: accepted, as any minted array at the cap); the minted
    // path budget does not change this for short names (these paths take 0.8 MB).
    // [measured: 65 listings, a facade peak of 25,673,877 bytes, refused as duplicates; 66, a
    // peak of 19,453,920, refused by the minted budget]
    let node = [written_q("urn:a.q")];
    for listings in [65, 66] {
        let document = leaves(1, |_| vec!["urn:a.q".to_owned(); listings], &node);
        Engine::new()
            .load_cxf(MODEL.as_bytes())
            .expect("the model loads");
        let mut result = None;
        let info = measure(|| result = Some(Engine::new().load_cxf(&document).map(|_| ())));
        let error = result.expect("ran").expect_err("refused");
        let codes: Vec<DiagCode> = error.all_diagnostics().map(|d| d.code).collect();
        if listings == 65 {
            assert!(codes.contains(&DiagCode::DuplicateParameter), "{codes:?}");
            assert!(
                error
                    .all_diagnostics()
                    .all(|d| !d.message.starts_with("array parameter")),
                "no array budget refuses 65 listings"
            );
        } else {
            assert!(
                error
                    .all_diagnostics()
                    .any(|d| *d == minted_refusal("urn:a.q", 1_024, 65_536)),
                "{codes:?}"
            );
        }
        assert!(
            info.bytes_max <= 28 * 1024 * 1024,
            "{listings}: a peak of {}",
            info.bytes_max
        );
    }
}

/// The minted-path budget (`oce-cxf`'s `MAX_DOCUMENT_MINTED_ARRAY_PATH_BYTES`), restated.
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

/// A long-named leaf listing a broadcast `urn:a.q[n]` and a one-element `fill` whose name is
/// `name` bytes, whose paths are the rest of the minted path budget less `short` bytes.
fn at_the_path_edge(owner: usize, over: usize) -> (Vec<u8>, String, usize, usize) {
    let mut n = (MINTED_PATHS - owner - 4) / (owner + 3);
    while paths(owner, 1, n) > MINTED_PATHS - owner - 4 {
        n -= 1;
    }
    let used = paths(owner, 1, n);
    // A one-element array named `name` on the leaf takes `owner + 1 + name + 2` bytes.
    let name = MINTED_PATHS - used - owner - 3 + over;
    let b = format!("urn:b.{}", "b".repeat(name));
    let document = named_leaves(
        1,
        |_| iri(owner, 'x'),
        |_| vec!["urn:a.q".to_owned(), b.clone()],
        &[
            array("urn:a.q", &[n], "[ 1.5 ]"),
            array(&b, &[1], "\"fill(1.5, 1)\""),
        ],
    );
    (document, b, used, owner + 1 + name + 2)
}

#[test]
fn minted_paths_fill_their_budget_exactly_and_one_byte_more_is_refused() {
    // A broadcast under a 1,005-byte instance IRI and a one-element `fill` whose name takes the
    // last bytes: exactly 8 MiB of minted paths read, one byte more is refused on the `fill`.
    // [measured: a 12,549-byte document read at a peak of 18,081,022 bytes at the edge]
    let (document, _, used, last) = at_the_path_edge(1_005, 0);
    assert_eq!(used + last, MINTED_PATHS);
    let (refused, info) = read(&document);
    assert!(
        info.bytes_max <= 24 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    assert!(refused.is_none(), "{refused:?}");
    let (document, b, used, last) = at_the_path_edge(1_005, 1);
    let (refused, _) = read(&document);
    let refused = refused.expect("one byte past is refused");
    assert!(
        refused_for_minting(&refused, &minted_path_refusal(&b, 1, last, used)),
        "{refused:?}"
    );
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
fn the_reviews_long_iri_broadcasts_are_refused_before_a_name_is_built() {
    // Broadcasts of 16,384, 32,768 and 65,536 under a 1,005-byte IRI (paths of 16.5 to 66 MB, a
    // 12 KB document at most): refused at one flat peak. [measured: 151,779 bytes each; before, about
    // 2,170 bytes an element, 35.6 MB at 16,384 (review)]
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
fn the_minted_path_refusal_names_the_same_node_whatever_order_its_owners_come_in() {
    // Three 1,005-byte-named leaves listing one `q[3000]` broadcast: the third crosses the minted
    // path budget, whichever leaf comes third.
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

/// A brace literal of `n` ones.
fn brace(n: usize) -> String {
    format!("\"{{{}}}\"", vec!["1"; n].join(", "))
}

#[test]
fn brace_literals_are_minted_and_the_same_values_written_as_lists_are_not() {
    // Sixteen brace literals of 4,095 and one of 16 hold exactly 65,536 elements and read; one of
    // 17 is refused; the same 65,537 values written as JSON lists read.
    let build = |last: usize, spelling: &dyn Fn(usize) -> String| {
        let mut sizes = vec![4_095; 16];
        sizes.push(last);
        let ids: Vec<String> = (0..sizes.len()).map(|i| format!("urn:a.q{i:02}")).collect();
        let nodes: Vec<String> = ids
            .iter()
            .zip(&sizes)
            .map(|(id, &n)| array(id, &[n], &spelling(n)))
            .collect();
        leaves(1, |_| ids.clone(), &nodes)
    };
    let (refused, info) = read(&build(16, &brace));
    assert!(
        info.bytes_max <= 28 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    assert!(refused.is_none(), "{refused:?}");
    let (refused, _) = read(&build(17, &brace));
    let refused = refused.expect("one element more is refused");
    assert!(
        refused_for_minting(&refused, &minted_refusal("urn:a.q16", 17, 65_520)),
        "{refused:?}"
    );
    let list = |n: usize| format!("[ {} ]", vec!["1"; n].join(", "));
    let (refused, _) = read(&build(17, &list));
    assert!(refused.is_none(), "{refused:?}");
}
