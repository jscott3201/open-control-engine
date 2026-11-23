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
    assert_eq!(MODEL.matches(GT).count(), 1);
    assert_eq!(MODEL.matches(LAST).count(), 1);
    let mut contains = String::new();
    let mut declared = String::new();
    for i in 0..leaves {
        let refs: String = listed(i)
            .iter()
            .map(|id| format!(", {{ \"@id\": \"{id}\" }}"))
            .collect();
        contains.push_str(&format!(", {{ \"@id\": \"urn:c{i}\" }}"));
        declared.push_str(&format!(
            ",\n    {{ \"@id\": \"urn:c{i}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"urn:c{i}.k\" }}{refs} ], \
             \"S231:hasOutput\": {{ \"@id\": \"urn:c{i}.y\" }} }},\n    \
             {{ \"@id\": \"urn:c{i}.k\", \"S231:value\": 1.0 }},\n    \
             {{ \"@id\": \"urn:c{i}.y\", \"@type\": \"S231:RealOutput\", \
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
            "array parameter mints {minted} element(s) from a value it writes once (a broadcast \
             or an array expression) but the document's array parameters already mint {used} of \
             the 65536 minted elements a document may hold"
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

/// A root holding one `MatrixGain` of 256 by 256 whose `K` broadcasts `0.5`, the minted budget
/// exactly, every input driven by the root's one boundary input.
fn minted_gain() -> Vec<u8> {
    let root = "http://example.org#M";
    let id = format!("{root}.g");
    let refs = |ids: Vec<String>| {
        ids.iter()
            .map(|i| format!("{{\"@id\":\"{i}\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inputs: Vec<String> = (1..=256).map(|i| format!("{id}.u{i}")).collect();
    let outputs: Vec<String> = (1..=256).map(|i| format!("{id}.y{i}")).collect();
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
            "{{\"@id\":\"{id}.K\",\"@type\":\"S231:Parameter\",\"S231:isArray\":true,\"S231:numberDimensions\":2,\"S231:sizeOfDimensions\":\"(256, 256)\",\"S231:value\":[0.5]}}"
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
    let document = minted_gain();
    let mut engine = Engine::new();
    let mut loaded = None;
    let info: AllocationInfo = measure(|| loaded = Some(engine.load_cxf(&document).map(|_| ())));
    loaded.expect("ran").expect("the gain loads");
    assert!(
        info.bytes_max <= 256 * 1024 * 1024,
        "a peak of {}",
        info.bytes_max
    );
    let over = String::from_utf8(document)
        .expect("utf-8")
        .replace("\"(256, 256)\"", "\"(257, 256)\"");
    assert!(Engine::new().load_cxf(over.as_bytes()).is_err());
}
