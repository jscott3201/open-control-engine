//! What a parameter value read more than once costs a load, and what the sibling names of many
//! array parameters cost it, in requested bytes and never in elapsed time (issues #736 and #731;
//! `TESTING.md`: cost is asserted with deterministic measures).
//!
//! The one value of a broadcast array (`q[n]` with the value `[v]`) was grounded once per
//! element, each grounding a string of its own when `v` is a string, and no budget counted the
//! bytes: a 60 KB string over 1,024 elements, a 67 KB document, loaded at a peak of 127 MB, and
//! over 4,096 at 506 MB. Now the value is grounded once and shared, and every string value a
//! parameter table holds is counted once per entry against the document's 8 MiB budget of string
//! parameter bytes, which every consumer that writes the table out pays once per entry (the
//! content id's canonical bytes among them).
//!
//! The set of sibling names an array's minted names are checked against was built again for every
//! array parameter of a leaf, so `P` arrays cost `P²`; it is built once per leaf.
//!
//! Both budgets are the reader's, and the parameters these documents add (`q`, `r`, `e0` …) are
//! names the constant's class does not declare, which the facade's load gate refuses (issue
//! #748). So a document the budget admits is read by the reader (`oce_cxf::import_cxf`, the
//! facade's first step) and measured there, and the facade is held to refusing it for the
//! undeclared names, never for a budget.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change).

use allocation_counter::{AllocationInfo, measure};
use oce_api::{Engine, LoadedModel, OcError};
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The string budget (`oce-cxf`'s `MAX_DOCUMENT_STRING_PARAMETER_BYTES`), restated so this test
/// fails if the budget moves without review.
const STRING_BUDGET: usize = 8 * 1024 * 1024;

const CON: &str = "http://example.org#MinLoop.con";
const K_NODE: &str = "{ \"@id\": \"http://example.org#MinLoop.con.k\",\n      \"S231:value\": { \"@value\": \"2.0\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" } }";
const K_REFERENCE: &str =
    "\"S231:hasParameter\": { \"@id\": \"http://example.org#MinLoop.con.k\" }";

/// [`MODEL`] with its constant listing `k` and then `references`, and `nodes` declared after
/// `k`'s node.
fn document(references: &[String], nodes: &[String]) -> Vec<u8> {
    assert_eq!(MODEL.matches(K_NODE).count(), 1);
    assert_eq!(MODEL.matches(K_REFERENCE).count(), 1);
    let listed: String = references
        .iter()
        .map(|id| format!(", {{ \"@id\": \"{id}\" }}"))
        .collect();
    let declared: String = nodes.iter().map(|node| format!(",\n    {node}")).collect();
    MODEL
        .replacen(
            K_REFERENCE,
            &format!("\"S231:hasParameter\": [ {{ \"@id\": \"{CON}.k\" }}{listed} ]"),
            1,
        )
        .replacen(K_NODE, &format!("{K_NODE}{declared}"), 1)
        .into_bytes()
}

/// A preserved one-dimensional array parameter node `id` of `size` elements with `value`.
fn array(id: &str, size: usize, value: &str) -> String {
    format!(
        "{{ \"@id\": \"{id}\", \"S231:isArray\": true, \"S231:numberDimensions\": 1, \
         \"S231:sizeOfDimensions\": \"({size})\", \"S231:value\": {value} }}"
    )
}

/// A string literal of `bytes` bytes, as the JSON value of a parameter.
fn quoted(bytes: usize) -> String {
    format!("\"\\\"{}\\\"\"", "y".repeat(bytes))
}

/// [`MODEL`] with the constant's array `q[size]` broadcasting a string of `bytes` bytes.
fn broadcast(size: usize, bytes: usize) -> Vec<u8> {
    let q = format!("{CON}.q");
    let node = array(&q, size, &format!("[ {} ]", quoted(bytes)));
    document(&[q], &[node])
}

/// One load of `bytes`, measured. The unedited model is loaded once first, so what a process
/// sets up on its first load is not counted.
fn load(bytes: &[u8]) -> (Result<LoadedModel, OcError>, AllocationInfo) {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    (result.expect("the load ran").map(|(_, model)| model), info)
}

/// One read of `bytes` by the CXF reader alone (the facade's first step, with its options),
/// measured. The unedited model is read once first.
fn read(bytes: &[u8]) -> (Result<ModelGraph, CxfError>, AllocationInfo) {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let mut result = None;
    let info = measure(|| {
        result = Some(import_cxf(bytes, &ResolveOptions::default()).map(|(model, _)| model));
    });
    (result.expect("the read ran"), info)
}

/// Hold the facade to refusing `bytes` at its load gate for parameter names the class does not
/// declare, and for nothing a budget of the reader says.
fn assert_refused_for_undeclared_names(bytes: &[u8]) {
    let error = Engine::new()
        .load_cxf(bytes)
        .expect_err("the undeclared names are refused");
    let diagnostics: Vec<&Diagnostic> = error.all_diagnostics().collect();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == DiagCode::UnknownParameter),
        "{:?}",
        diagnostics.first()
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("string parameter value")
                && !d.message.starts_with("array parameter")),
        "a budget of the reader refused it"
    );
}

/// The diagnostics of a refused load of `bytes`, and what it cost.
fn refused(bytes: &[u8]) -> (Vec<Diagnostic>, AllocationInfo) {
    let (result, info) = load(bytes);
    let error = result.expect_err("the document is refused");
    (error.all_diagnostics().cloned().collect(), info)
}

/// The string budget's refusal of a value of `bytes` bytes on the parameter `subject` of the
/// instance `owner`, when the document's tables already hold `used` bytes of strings, stated by
/// hand.
fn over_strings(subject: &str, owner: &str, bytes: usize, used: usize) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "string parameter value of {bytes} byte(s) on `{owner}` does not fit: the parameter \
             tables of the document already hold {used} of the 8388608 bytes of string values a \
             document may hold, each table entry counted"
        ),
    )
    .with_subject(subject.to_owned())
}

/// The string values of the parameters of the block `instance` in a read model, in order.
fn strings_of<'m>(model: &'m ModelGraph, instance: &str) -> Vec<(&'m str, &'m str)> {
    model
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some(instance))
        .expect("the block")
        .params
        .values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::String(text) => Some((name.as_ref(), text.as_ref())),
            _ => None,
        })
        .collect()
}

fn bytes(count: usize) -> u64 {
    u64::try_from(count).expect("fits")
}

#[test]
fn a_broadcast_string_past_the_budget_is_refused_at_the_cost_of_reading_the_document() {
    // The issue's documents: 60 KiB over 1,024 and over 4,096 elements. The 137th element is the
    // first that does not fit, so both are refused there with one diagnostic, for the same cost
    // but for the digits of the size. [measured: peaks of 672,380 bytes and totals of 2,662,334
    // for both; before, both loaded, at peaks of 126,551,003 and 506,136,539 bytes after
    // requesting 704,483,762 and 2,812,020,194]
    let each = 60 * 1024;
    let mut totals = Vec::new();
    for size in [1_024, 4_096] {
        let document = broadcast(size, each);
        let (diagnostics, info) = refused(&document);
        assert_eq!(
            diagnostics,
            [over_strings(&format!("{CON}.q"), CON, each, 136 * each)]
        );
        assert!(
            info.bytes_max <= 48 * bytes(document.len()),
            "{} bytes held for a document of {}",
            info.bytes_max,
            document.len()
        );
        totals.push(info.bytes_total);
    }
    assert!(
        totals[1].abs_diff(totals[0]) <= 64,
        "{totals:?} bytes requested"
    );
}

#[test]
fn a_broadcast_string_that_fills_the_budget_loads_and_one_byte_more_is_refused() {
    // 136 elements of 60 KiB and a scalar of the rest fill the budget to the byte (the reader
    // shares one allocation among the elements, pinned in `oce-cxf`). One byte more on the
    // scalar is refused. [measured: a peak of 8,593,382 bytes loaded through the facade, 1.02
    // times the budget, the loaded model and the content id's canonical bytes copying each
    // element once, and 773,306 refused; before, the same document loaded at 16,889,942 after
    // requesting 96,419,439. Since issue #748 the facade refuses `q` and `r`, so the admitted
    // document is measured at the reader.]
    let each = 60 * 1024;
    let rest = STRING_BUDGET - 136 * each;
    let (q, r) = (format!("{CON}.q"), format!("{CON}.r"));
    let edge = |last: usize| {
        document(
            &[q.clone(), r.clone()],
            &[
                array(&q, 136, &format!("[ {} ]", quoted(each))),
                format!("{{ \"@id\": \"{r}\", \"S231:value\": {} }}", quoted(last)),
            ],
        )
    };
    let (result, loaded) = read(&edge(rest));
    let model = result.expect("strings that fill the budget exactly are read");
    let strings = strings_of(&model, CON);
    assert_eq!(strings.len(), 137);
    let (elements, last) = strings.split_at(136);
    assert!(elements.iter().enumerate().all(|(i, (name, text))| {
        *name == format!("q_{}", i + 1) && text.len() == each && text.bytes().all(|b| b == b'y')
    }));
    assert_eq!(elements[0].1.len(), each);
    assert_eq!((last[0].0, last[0].1.len()), ("r", rest));
    assert!(
        loaded.bytes_max <= 2 * bytes(STRING_BUDGET),
        "{} bytes held for {STRING_BUDGET} bytes of strings",
        loaded.bytes_max
    );
    drop(model);
    assert_refused_for_undeclared_names(&edge(rest));

    let (diagnostics, refusal) = refused(&edge(rest + 1));
    assert_eq!(diagnostics, [over_strings(&r, CON, rest + 1, 136 * each)]);
    assert!(
        refusal.bytes_max <= bytes(STRING_BUDGET) / 8,
        "{} bytes held refusing",
        refusal.bytes_max
    );
}

#[test]
fn a_string_node_listed_by_two_instances_is_counted_once_for_each() {
    // The constant and the gain block both list one 4 MiB string node: the two entries fill the
    // budget, whatever they share. A byte more and the second listing is refused. [measured: a
    // peak of 42,058,440 bytes loaded, about ten times the 4.2 MB document as before, and
    // 46,252,763 refused; before, the second document loaded too]
    let gain = "\"S231:label\": \"gain\",";
    assert_eq!(MODEL.matches(gain).count(), 1);
    let shared = |bytes: usize| -> Vec<u8> {
        let node = format!(
            "{{ \"@id\": \"urn:shared.q\", \"S231:value\": {} }}",
            quoted(bytes)
        );
        String::from_utf8(document(&["urn:shared.q".to_owned()], &[node]))
            .expect("UTF-8")
            .replacen(
                gain,
                &format!("{gain} \"S231:hasConstant\": {{ \"@id\": \"urn:shared.q\" }},"),
                1,
            )
            .into_bytes()
    };
    let half = STRING_BUDGET / 2;
    let document = shared(half);
    let (result, loaded) = read(&document);
    let model = result.expect("two entries of half the budget are read");
    let (a, b) = (
        strings_of(&model, CON),
        strings_of(&model, "http://example.org#MinLoop.gain"),
    );
    assert_eq!((a.len(), b.len()), (1, 1));
    assert_eq!((a[0].1.len(), b[0].1.len()), (half, half));
    // The document is itself half the budget, and reading it holds a multiple of it.
    assert!(
        loaded.bytes_max <= 2 * bytes(STRING_BUDGET) + 12 * bytes(document.len()),
        "{} bytes held for a document of {}",
        loaded.bytes_max,
        document.len()
    );
    drop(model);
    assert_refused_for_undeclared_names(&document);
    let (diagnostics, _) = refused(&shared(half + 1));
    assert_eq!(
        diagnostics,
        [over_strings(
            "urn:shared.q",
            "http://example.org#MinLoop.gain",
            half + 1,
            half + 1
        )]
    );
}

/// [`MODEL`] with `count` array parameters `e0` … on the constant, each `value` over `size`.
fn arrays(count: usize, size: usize, value: &str) -> Vec<u8> {
    let ids: Vec<String> = (0..count).map(|i| format!("{CON}.e{i}")).collect();
    let nodes: Vec<String> = ids.iter().map(|id| array(id, size, value)).collect();
    document(&ids, &nodes)
}

#[test]
fn array_parameters_on_one_leaf_cost_a_fixed_amount_each_however_many_there_are() {
    // The issue's document, 1,000 and 2,000 empty arrays on one leaf, and the same with one
    // element each, read by the reader: every further array costs what reading its node costs. [measured: 7,320
    // bytes a further empty array and 9,532 a further one-element array; before, 216,228 a
    // further empty array, the sibling-name set of 1,000 to 2,000 names built again for each]
    for (size, value, further_bytes) in [(0, "[]", 12_288), (1, "[ 1.0 ]", 16_384)] {
        let (result, small) = read(&arrays(1_000, size, value));
        result.expect("the arrays are read");
        let (result, large) = read(&arrays(2_000, size, value));
        result.expect("the arrays are read");
        let further = large.bytes_total - small.bytes_total;
        assert!(
            further <= further_bytes * 1_000,
            "{size}: {further} more bytes requested for 1,000 more arrays"
        );
    }
    // An empty array mints no parameter, so the facade loads those documents; a one-element
    // array mints `e<i>_1`, which the constant's class does not declare (issue #748).
    Engine::new()
        .load_cxf(&arrays(1_000, 0, "[]"))
        .expect("empty arrays load");
    assert_refused_for_undeclared_names(&arrays(1_000, 1, "[ 1.0 ]"));
}

#[test]
fn a_positional_array_listed_again_grounds_its_values_once() {
    // One array of two 60 KiB literals that do not parse, listed 10,000 times: each value is
    // grounded on the first listing and its error kept, so the later listings copy nothing and
    // their findings are counted once the diagnostic list has ended. [measured: a 598,576-byte
    // document, a peak of 11,738,786 bytes and 41,547,026 requested; 4.94 GB requested when each
    // listing grounded the values again, by the review of GitHub #752]
    let q = format!("{CON}.q");
    let bad = format!(
        "{{ \"@value\": \"{}\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}",
        "y".repeat(60 * 1024)
    );
    let node = array(&q, 2, &format!("[ {bad}, {bad} ]"));
    let document = document(&vec![q; 10_000], &[node]);
    let (diagnostics, info) = refused(&document);
    assert_eq!(
        diagnostics.last().map(|d| d.code),
        Some(DiagCode::ResourceLimitExceeded)
    );
    // The diagnostic list's budget is also 8 MiB; what is requested is a few times it.
    assert!(
        info.bytes_max <= bytes(STRING_BUDGET) + 48 * bytes(document.len()),
        "{} bytes held for a document of {}",
        info.bytes_max,
        document.len()
    );
    assert!(
        info.bytes_total <= 8 * bytes(STRING_BUDGET),
        "{} bytes requested for a document of {}",
        info.bytes_total,
        document.len()
    );
}
