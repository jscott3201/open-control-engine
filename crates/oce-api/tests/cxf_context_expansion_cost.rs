//! What `@context` expansion costs a load, in requested bytes and never in elapsed time (GitHub
//! issue #709; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A token read through a prefix is as long as the IRI the prefix binds. A document sets that
//! length and the number of tokens, and pays a few bytes for each, so expanding every token costs
//! `tokens × IRI bytes` unless something ties the two together: 1,024 `@type` tokens `p:T` under
//! a 64 KiB binding, a document of 78 KB, loaded and held 202 MB. Two things tie them:
//!
//! - **Reading a token copies nothing.** A token is classified, compared with a namespace or
//!   with a term's mapping, and found relative, in place. Only an expansion the loader keeps is
//!   joined.
//! - **What expansion adds to one document is held to 8 MiB**: an expansion's length less the
//!   token's, summed over every expansion joined. The first that does not fit is refused as
//!   `resource-limit-exceeded`, naming the token and its node, and nothing after it is read.
//!
//! Each case of [`ROUTES`] is one place the loader reads a token through a prefix. Every case is
//! loaded through `Engine::load_cxf` at three sizes, which scale the binding and the number of
//! tokens one at a time, and held at each to a ceiling that is a fixed multiple of the
//! document's bytes, plus a fixed multiple of the budget where the route keeps what it expands
//! or lists a diagnostic that quotes it. `tokens × IRI bytes` is 64 MiB at two of the three
//! sizes and 16 MiB at the third; no ceiling has room for it.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets). What this cannot see: work that
//! allocates nothing. The readers that only compare are held to work that does not grow with the
//! binding by `oce-cxf`'s own tests, which count it: the two-piece IRI's, the bytes read for
//! whitespace in a typing token, and the near-miss checks a `hasUnit` key is given
//! (`expansion_budget_tests.rs`).

use allocation_counter::{AllocationInfo, measure};
use oce_api::{Engine, OcError};
use oce_diag::{DiagCode, Diagnostic};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The bytes expansion may add to one document (`oce-cxf`'s `MAX_CONTEXT_EXPANSION_BYTES`),
/// restated so this test fails if the budget moves without review. The reader's diagnostic list
/// holds the same number of bytes of text.
const BUDGET: usize = 8 * 1024 * 1024;

/// What the model's own tokens add when they are expanded: its 27 compact `S231:` tokens, each
/// 23 bytes longer as an IRI in `http://data.ashrae.org/S231#`.
const MODEL_ADDS: usize = 27 * 23;

/// An absolute IRI of exactly `bytes` bytes that ends in `#`.
fn long_iri(bytes: usize) -> String {
    let stem = "urn:long:";
    format!("{stem}{}#", "x".repeat(bytes - stem.len() - 1))
}

/// [`MODEL`] with `terms` added to its `@context` and `nodes` appended to its `@graph`.
fn with_nodes(terms: &str, nodes: &str) -> Vec<u8> {
    let context = "\"base\": \"http://example.org#\"";
    let last = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";
    assert_eq!(MODEL.matches(context).count(), 1);
    assert_eq!(MODEL.matches(last).count(), 1);
    MODEL
        .replacen(context, &format!("{context}{terms}"), 1)
        .replacen(
            last,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }},\n{nodes}\n  ]"),
            1,
        )
        .into_bytes()
}

/// [`MODEL`] with `p` bound to `iri` and one more node, `urn:n`, carrying `members` (JSON
/// members, comma separated).
fn with_node(iri: &str, members: &str) -> Vec<u8> {
    with_nodes(
        &format!(", \"p\": \"{iri}\""),
        &format!("{{ \"@id\": \"urn:n\", {members} }}"),
    )
}

/// `count` strings from `item`, joined with commas.
fn list(count: usize, item: impl Fn(usize) -> String) -> String {
    (0..count).map(item).collect::<Vec<_>>().join(", ")
}

/// What a route does with the tokens it reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    /// It keeps each expansion, so the budget refuses the document: one
    /// `resource-limit-exceeded` error and nothing else.
    RefusedByTheBudget,
    /// It reads each token in place and refuses each, with a diagnostic that quotes the token as
    /// written.
    RefusedOnceAToken(DiagCode),
    /// It refuses each term with a diagnostic that names the term's expansion whole, so the
    /// diagnostic list ends.
    RefusedUntilTheListEnds(DiagCode),
    /// It reads each token in place and the document loads.
    Loads,
}

/// One place the loader reads a token through a prefix.
struct Route {
    /// What is read.
    name: &'static str,
    /// The document with `tokens` tokens through a binding of `iri`.
    document: fn(iri: &str, tokens: usize) -> Vec<u8>,
    /// What becomes of it.
    outcome: Outcome,
}

const ROUTES: &[Route] = &[
    Route {
        name: "a node @id through a prefix",
        document: |iri, tokens| {
            with_nodes(
                &format!(", \"p\": \"{iri}\""),
                &list(tokens, |i| format!("{{ \"@id\": \"p:n{i}\" }}")),
            )
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a reference through a prefix",
        document: |iri, tokens| {
            let references = list(tokens, |i| format!("{{ \"@id\": \"p:r{i}\" }}"));
            with_node(iri, &format!("\"S231:hasInput\": [ {references} ]"))
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a reference that is a whole term",
        document: |iri, tokens| {
            let references = list(tokens, |_| "{ \"@id\": \"p\" }".to_owned());
            with_node(iri, &format!("\"S231:isConnectedTo\": [ {references} ]"))
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a @type through a prefix",
        document: |iri, tokens| {
            let types = list(tokens, |_| "\"p:T\"".to_owned());
            with_node(iri, &format!("\"@type\": [ {types} ]"))
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a @type that is a whole term",
        document: |iri, tokens| {
            let types = list(tokens, |_| "\"p\"".to_owned());
            with_node(iri, &format!("\"@type\": [ {types} ]"))
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "an isOfDataType through a prefix",
        document: |iri, tokens| {
            let nodes = list(tokens, |i| {
                format!(
                    "{{ \"@id\": \"urn:n{i}\", \"S231:isOfDataType\": {{ \"@id\": \"p:T\" }} }}"
                )
            });
            with_nodes(&format!(", \"p\": \"{iri}\""), &nodes)
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a unit IRI node through a prefix",
        document: |iri, tokens| {
            let nodes = list(tokens, |i| {
                format!("{{ \"@id\": \"urn:n{i}\", \"S231:unit\": {{ \"@id\": \"p:U\" }} }}")
            });
            with_nodes(&format!(", \"p\": \"{iri}\""), &nodes)
        },
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a key ending in hasUnit through a prefix",
        document: |iri, tokens| with_node(iri, &list(tokens, |i| format!("\"p:k{i}hasUnit\": 0"))),
        outcome: Outcome::RefusedByTheBudget,
    },
    Route {
        name: "a reference whose suffix holds whitespace",
        document: |iri, tokens| {
            let references = list(tokens, |i| format!("{{ \"@id\": \"p:r {i}\" }}"));
            with_node(iri, &format!("\"S231:hasInput\": [ {references} ]"))
        },
        outcome: Outcome::RefusedOnceAToken(DiagCode::RelativeIri),
    },
    Route {
        name: "a node @id whose suffix holds whitespace",
        document: |iri, tokens| {
            with_nodes(
                &format!(", \"p\": \"{iri}\""),
                &list(tokens, |i| format!("{{ \"@id\": \"p:n {i}\" }}")),
            )
        },
        outcome: Outcome::RefusedOnceAToken(DiagCode::RelativeIri),
    },
    Route {
        // The binding is in the S231 namespace, so each token names a term there once trimmed.
        name: "a @type in surrounding whitespace that names an S231 term through a prefix",
        document: |iri, tokens| {
            let namespace = "http://data.ashrae.org/S231#";
            let iri = format!("{namespace}{}", "x".repeat(iri.len() - namespace.len()));
            let types = list(tokens, |_| "\" p:T\"".to_owned());
            with_node(&iri, &format!("\"@type\": [ {types} ]"))
        },
        outcome: Outcome::RefusedOnceAToken(DiagCode::MalformedDocument),
    },
    Route {
        name: "a @type in surrounding whitespace that names no S231 term",
        document: |iri, tokens| {
            let types = list(tokens, |_| "\" p:T\"".to_owned());
            with_node(iri, &format!("\"@type\": [ {types} ]"))
        },
        outcome: Outcome::Loads,
    },
    Route {
        name: "a pass-through key through a prefix",
        document: |iri, tokens| with_node(iri, &list(tokens, |i| format!("\"p:k{i}\": 0"))),
        outcome: Outcome::Loads,
    },
    Route {
        name: "a @context term spelled through a prefix that binds another IRI",
        document: |iri, tokens| {
            let terms: String = (0..tokens)
                .map(|i| format!(", \"p:t{i}\": \"urn:other\""))
                .collect();
            with_nodes(
                &format!(", \"p\": \"{iri}\"{terms}"),
                "{ \"@id\": \"urn:n\" }",
            )
        },
        outcome: Outcome::RefusedUntilTheListEnds(DiagCode::MalformedDocument),
    },
];

/// The binding's length and the number of tokens every route is measured at: a base, the
/// binding four times as long, and four times as many tokens.
const SIZES: [(usize, usize); 3] = [(64 * 1024, 256), (256 * 1024, 256), (64 * 1024, 1_024)];

/// One load of `bytes`: its allocations and its outcome. The unedited model is loaded once
/// first, so what a process sets up on its first load is not counted.
fn load(bytes: &[u8]) -> (AllocationInfo, Result<Vec<Diagnostic>, Vec<Diagnostic>>) {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    let outcome = match result.expect("the load ran") {
        Ok((report, _)) => Ok(report.warnings),
        Err(error) => {
            assert!(matches!(error, OcError::Cxf(_)), "{error}");
            Err(error.diagnostics().to_vec())
        }
    };
    (info, outcome)
}

fn bytes(count: usize) -> u64 {
    u64::try_from(count).expect("fits")
}

/// The `resource-limit-exceeded` refusal of the `@type` token `token` on `urn:n`, expanding to
/// `expanded` bytes after `added` were added, stated by hand.
fn refused_type(token: &str, expanded: usize, added: usize) -> Diagnostic {
    let adds = expanded - token.len();
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "@type `{token}` expands to {expanded} bytes through the document's @context, {adds} \
             more than it is written in; expansion had added {added} bytes, and may add at most \
             8388608 to one document"
        ),
    )
    .with_subject("urn:n".to_owned())
}

#[test]
fn every_route_costs_a_multiple_of_the_document_and_never_tokens_times_the_binding() {
    for route in ROUTES {
        for (iri_bytes, tokens) in SIZES {
            let document = (route.document)(&long_iri(iri_bytes), tokens);
            let (info, outcome) = load(&document);
            let name = format!("{}, {iri_bytes} × {tokens}", route.name);
            // How many times the budget the route may hold at once, and request in all.
            let (held, requested) = match route.outcome {
                Outcome::RefusedByTheBudget => {
                    let refused = outcome.expect_err(&name);
                    assert_eq!(refused.len(), 1, "{name}: {refused:?}");
                    assert_eq!(refused[0].code, DiagCode::ResourceLimitExceeded, "{name}");
                    assert!(refused[0].is_error(), "{name}");
                    assert!(refused[0].subject.is_some(), "{name}");
                    let message = &refused[0].message;
                    assert!(message.len() <= 512, "{name}: {message}");
                    assert!(
                        message.ends_with("and may add at most 8388608 to one document"),
                        "{name}: {message}"
                    );
                    // The working copy holds what was added before the refusal, and a node's own
                    // `@id` is copied once more to name the node. [measured: 8.7 to 23.2 MB
                    // requested; 34 to 411 MB with no budget]
                    (1, 3)
                }
                Outcome::RefusedOnceAToken(code) => {
                    let refused = outcome.expect_err(&name);
                    assert_eq!(refused.len(), tokens, "{name}");
                    assert!(refused.iter().all(|d| d.code == code), "{name}");
                    // No diagnostic holds the binding: each quotes the token as written.
                    let text: usize = refused.iter().map(|d| d.message.len()).sum();
                    assert!(text <= 512 * tokens, "{name}: {text} bytes of messages");
                    (0, 0)
                }
                Outcome::RefusedUntilTheListEnds(code) => {
                    let refused = outcome.expect_err(&name);
                    let (last, listed) = refused.split_last().expect("diagnostics");
                    assert_eq!(last.code, DiagCode::ResourceLimitExceeded, "{name}");
                    assert!(listed.iter().all(|d| d.code == code), "{name}");
                    assert!(!listed.is_empty() && listed.len() < tokens, "{name}");
                    // Each listed refusal names the term's expansion whole, and building one
                    // requests several times its text; past the list's end none is built.
                    // [measured: 59.3 to 59.7 MB requested at every size, 7.1 times the budget;
                    // 160 to 614 MB when every refusal was built]
                    (1, 8)
                }
                Outcome::Loads => {
                    let warnings = outcome.unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
                    assert!(warnings.is_empty(), "{name}: {warnings:?}");
                    (0, 0)
                }
            };
            // `tokens × binding` is 16 MiB at the first size and 64 MiB at the other two. What
            // a route holds beside the budget is the document as parsed, in a few copies: a
            // node costs its record whatever it carries. [measured: 3.4 to 41.4 bytes held and
            // 4.5 to 76.7 requested for each document byte, beside the budget; the routes that
            // only read requested 51 to 208 MB when each joined the binding for every token]
            let size = bytes(document.len());
            assert!(
                info.bytes_max <= held * bytes(BUDGET) + 64 * size,
                "{name}: {} bytes held for a document of {size}",
                info.bytes_max
            );
            assert!(
                info.bytes_total <= requested * bytes(BUDGET) + 128 * size,
                "{name}: {} bytes requested for a document of {size}",
                info.bytes_total
            );
            // A token costs a fixed number of allocations. [measured: 4.4 to 18.0 a token]
            assert!(
                info.count_total <= 2_048 + 24 * bytes(tokens),
                "{name}: {} allocations for {tokens} tokens",
                info.count_total
            );
        }
    }
}

/// The document of issue #709: [`MODEL`] with `p` bound to `urn:long:<64 KiB of x>#` and one
/// more node typed `p:T` `types` times.
fn issue_document(types: usize) -> Vec<u8> {
    let iri = format!("urn:long:{}#", "x".repeat(64 * 1024));
    let types = list(types, |_| "\"p:T\"".to_owned());
    with_node(&iri, &format!("\"@type\": [ {types} ]"))
}

#[test]
fn the_document_of_the_issue_is_refused_and_holds_the_budget_and_a_fixed_overhead() {
    // 1,024 tokens of 65,547 bytes each once expanded: 67 MB in a document of 78 KB.
    let document = issue_document(1_024);
    assert_eq!(document.len(), 78_157);
    let (info, outcome) = load(&document);
    // Each token adds 65,544 bytes. The model's own tokens added 621 before them, so 127 fit.
    let added = MODEL_ADDS + 127 * 65_544;
    assert!(added <= BUDGET && added + 65_544 > BUDGET);
    assert_eq!(outcome, Err(vec![refused_type("p:T", 65_547, added)]));
    // [measured: 8,649,348 held and 8,784,380 requested; with no budget the document loaded,
    // holding 201,749,795 and requesting 269,138,346]
    assert!(info.bytes_max <= 10_000_000, "{} held", info.bytes_max);
    assert!(
        info.bytes_total <= 10_000_000,
        "{} requested",
        info.bytes_total
    );
}

#[test]
fn many_tokens_under_a_shorter_binding_are_refused_at_the_same_cost() {
    // The second measurement on the issue: a 16 KiB binding and 16,384 tokens, a document of
    // 137 KB that loaded and held 807 MB.
    let iri = format!("urn:long:{}#", "x".repeat(16 * 1024));
    let types = list(16_384, |_| "\"p:T\"".to_owned());
    let document = with_node(&iri, &format!("\"@type\": [ {types} ]"));
    let (info, outcome) = load(&document);
    let each = 16 * 1024 + 10 + 1 - 3;
    let fit = (BUDGET - MODEL_ADDS) / each;
    assert_eq!(
        outcome,
        Err(vec![refused_type("p:T", each + 3, MODEL_ADDS + fit * each)])
    );
    assert_eq!(document.len(), 136_525);
    // [measured: 9,383,556 held and 10,502,780 requested; with no budget the document loaded
    // and held 807 MB]
    assert!(info.bytes_max <= 10_500_000, "{} held", info.bytes_max);
    assert!(
        info.bytes_total <= 12_000_000,
        "{} requested",
        info.bytes_total
    );
}

/// [`MODEL`] with one more node whose `@type` tokens, with the model's own, add the budget
/// exactly: 127 that add 64 KiB each and one that adds the rest; then `more`, a further token.
fn document_at_the_budget(more: &str) -> Vec<u8> {
    // A token `x:T` is three bytes and expands to its binding and one byte more.
    let rest = BUDGET - MODEL_ADDS - 127 * 64 * 1024;
    let terms = format!(
        ", \"p\": \"{}\", \"f\": \"{}\", \"ab\": \"a:b\"",
        long_iri(64 * 1024 + 2),
        long_iri(rest + 2)
    );
    let types = list(127, |_| "\"p:T\"".to_owned());
    with_nodes(
        &terms,
        &format!("{{ \"@id\": \"urn:n\", \"@type\": [ {types}, \"f:T\"{more} ] }}"),
    )
}

#[test]
fn a_document_that_adds_the_budget_exactly_loads_and_one_byte_more_is_refused() {
    let (info, outcome) = load(&document_at_the_budget(""));
    assert_eq!(outcome, Ok(Vec::new()), "the budget is met, not passed");
    // What a load that fills the budget holds: the expanded working copy and the copy lowering
    // makes of it, so twice the budget. [measured: 17,295,182 held and 17,569,298 requested,
    // for a document of 136,816 bytes]
    assert!(
        info.bytes_max <= 9 * bytes(BUDGET) / 4,
        "{} held",
        info.bytes_max
    );
    assert!(
        info.bytes_total <= 5 * bytes(BUDGET) / 2,
        "{} requested",
        info.bytes_total
    );

    // The term `ab` is two bytes and is bound to an IRI of three.
    let (_, outcome) = load(&document_at_the_budget(", \"ab\""));
    assert_eq!(outcome, Err(vec![refused_type("ab", 3, BUDGET)]));
}

#[test]
fn a_document_within_the_budget_loads_as_its_spelling_in_absolute_iris_does() {
    // 64 tokens through a 64 KiB binding add half the budget. Written out as absolute IRIs the
    // same document adds nothing, and is the same model.
    let iri = long_iri(64 * 1024);
    let compact = with_node(
        &iri,
        &format!("\"@type\": [ {} ]", list(64, |_| "\"p:T\"".to_owned())),
    );
    let absolute = with_node(
        &iri,
        &format!("\"@type\": [ {} ]", list(64, |_| format!("\"{iri}T\""))),
    );
    let ids: Vec<String> = [compact, absolute]
        .iter()
        .map(|document| {
            let mut engine = Engine::new();
            let (report, _) = engine.load_cxf(document).expect("within the budget");
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            engine.content_id().expect("a content id").to_string()
        })
        .collect();
    assert_eq!(ids[0], ids[1]);
}

#[test]
fn identities_that_fill_the_budget_cost_no_more_than_their_spelling_in_absolute_iris() {
    // The budget is on what expansion adds, and an identity is kept in more than one place once
    // it is read (the node, each reference to it, the resolved model's tables), so a document
    // whose identities fill the budget holds a multiple of it. What bounds that multiple is the
    // same document with every identity written out: expansion makes the first into the second
    // and then costs nothing the second does not.
    let written = "\"@id\": \"http://example.org#";
    let identities = MODEL.matches(written).count();
    assert_eq!(identities, 52);
    // Each `e:…` adds the binding less the two bytes of `e:`.
    let binding = long_iri((BUDGET - MODEL_ADDS) / identities + 2);
    let compact = MODEL
        .replacen(
            "\"base\": \"http://example.org#\"",
            &format!("\"e\": \"{binding}\""),
            1,
        )
        .replace(written, "\"@id\": \"e:");
    let absolute = MODEL.replace(written, &format!("\"@id\": \"{binding}"));
    assert!(compact.len() < 168 * 1024, "{}", compact.len());
    assert!(absolute.len() > BUDGET, "{}", absolute.len());

    let (through_prefix, outcome) = load(compact.as_bytes());
    assert_eq!(outcome, Ok(Vec::new()), "the budget is not passed");
    // [measured: 30,596,310 held and 71,861,922 requested, for a document of 165,804 bytes:
    // 3.6 and 8.6 times the budget]
    assert!(
        through_prefix.bytes_max <= 4 * bytes(BUDGET),
        "{} held",
        through_prefix.bytes_max
    );
    assert!(
        through_prefix.bytes_total <= 10 * bytes(BUDGET),
        "{} requested",
        through_prefix.bytes_total
    );

    // The written-out spelling is past the facade's 8 MiB of admission, so the two are compared
    // on the reader, where they are the same model.
    let read = |document: &str| {
        let options = oce_cxf::ResolveOptions::default();
        let mut model = None;
        let info = measure(|| model = Some(oce_cxf::import_cxf(document.as_bytes(), &options)));
        let (model, report) = model.expect("the read ran").expect("the document reads");
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        (info, format!("{model:?}"))
    };
    let (through_prefix, compact_model) = read(&compact);
    let (written_out, absolute_model) = read(&absolute);
    assert_eq!(compact_model, absolute_model);
    // [measured: 30,596,310 held through the prefix, 38,500,413 written out]
    assert!(
        through_prefix.bytes_max <= written_out.bytes_max,
        "{} held through the prefix, {} written out",
        through_prefix.bytes_max,
        written_out.bytes_max
    );

    // One more byte in the binding and the last identity does not fit.
    let over = compact.replacen(&binding, &format!("{binding}x"), 1);
    let (info, outcome) = load(over.as_bytes());
    let refused = outcome.expect_err("refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].code, DiagCode::ResourceLimitExceeded);
    assert!(
        info.bytes_max <= 9 * bytes(BUDGET) / 4,
        "{}",
        info.bytes_max
    );
}

#[test]
fn what_was_raised_before_the_refusal_is_listed_with_it_and_nothing_after_is() {
    // A relative reference before the token that does not fit, and one after it; each `p:T`
    // adds half the budget, so the second does not fit. The list is
    // sorted by subject, as every list of the reader is.
    let terms = format!(", \"p\": \"{}\"", long_iri(BUDGET / 2 + 2));
    let nodes = "{ \"@id\": \"urn:a\", \"S231:hasInput\": { \"@id\": \"before\" } },
    { \"@id\": \"urn:n\", \"@type\": [ \"p:T\", \"p:T\" ] },
    { \"@id\": \"urn:z\", \"S231:hasInput\": { \"@id\": \"after\" } }";
    let (_, outcome) = load(&with_nodes(&terms, nodes));
    let refused = outcome.expect_err("refused");
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert_eq!(refused[0].code, DiagCode::RelativeIri);
    assert_eq!(refused[0].subject.as_deref(), Some("urn:a"));
    assert_eq!(
        refused[1],
        refused_type("p:T", BUDGET / 2 + 3, MODEL_ADDS + BUDGET / 2)
    );
}
