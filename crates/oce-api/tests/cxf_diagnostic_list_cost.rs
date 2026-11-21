//! What a refusal list costs a load, in requested bytes and allocations and never in elapsed time
//! (GitHub issue #695; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A diagnostic copies document text: its subject is a node's `@id`, and most messages name that
//! `@id` again. One long `@id` and many findings about it, each costing the document a few bytes,
//! used to build a list of `findings × id bytes`: 400 padded keys on a node with a 1 MiB `@id`, a
//! document of 2.1 MB, held 1.2 GB. The reader's list holds at most 8 MiB of message and subject
//! text; the first diagnostic that does not fit ends it, and one `resource-limit-exceeded` error
//! counts that diagnostic and every later one.
//!
//! Each case of [`ROUTES`] is one place the reader raises a diagnostic per item of a list the
//! document sets (the keys of a node, its `@type` tokens, its references), about one string the
//! document also sets. Every case is loaded through `Engine::load_cxf` at 400 and at 1,600 items
//! on a 256 KiB `@id` and held to:
//!
//! - **a peak** of the list's budget plus a fixed multiple of the document's bytes;
//! - **a total** of requested bytes that grows by a fixed amount an item between the two sizes,
//!   never by the string's length: once the list has ended, a finding is counted and nothing is
//!   built for it;
//! - **allocations** that grow by a fixed number an item;
//! - **the count**: every finding is listed or counted in the closing error.
//!
//! The budget itself is pinned at its edge by a case of known size: the findings that fit are
//! listed whole with no closing error, and one more finding adds only the closing error.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets); the headroom is far under the
//! cost of one copied `@id` an item. What this cannot see: work that allocates nothing.

use std::collections::HashSet;
use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_diag::{DiagCode, Diagnostic};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The reader's budget of message and subject text (`oce-cxf`'s `MAX_DIAGNOSTIC_TEXT_BYTES`),
/// restated so this test fails if the budget moves without review.
const BUDGET: usize = 8 * 1024 * 1024;

/// The length of the long `@id` every case is measured on.
const ID_BYTES: usize = 256 * 1024;

/// The item counts every case is measured at.
const ITEMS: [usize; 2] = [400, 1_600];

/// A node `@id` of `bytes` bytes.
fn long_id(bytes: usize) -> String {
    let stem = "urn:long:";
    format!("{stem}{}", "x".repeat(bytes - stem.len()))
}

/// [`MODEL`] with `terms` added to its `@context` and `node` appended to its `@graph`.
fn with_node(terms: &str, node: &str) -> String {
    let context = "\"base\": \"http://example.org#\"";
    let last = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";
    assert_eq!(MODEL.matches(context).count(), 1);
    assert_eq!(MODEL.matches(last).count(), 1);
    MODEL
        .replacen(context, &format!("{context}{terms}"), 1)
        .replacen(
            last,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }},\n{node}\n  ]"),
            1,
        )
}

/// [`MODEL`] with one more node, `@id` `id`, carrying `members` (JSON members, comma separated).
fn with_long_node(terms: &str, id: &str, members: &str) -> Vec<u8> {
    with_node(terms, &format!("{{ \"@id\": \"{id}\", {members} }}")).into_bytes()
}

/// `count` strings from `item`, joined with commas.
fn list(count: usize, item: impl Fn(usize) -> String) -> String {
    (0..count).map(item).collect::<Vec<_>>().join(", ")
}

/// `count` distinct spellings of `key` that differ only in surrounding spaces.
fn padded(key: &str, count: usize) -> String {
    list(count, |i| {
        let (before, after) = (i / 40 + 1, i % 40);
        format!("\"{}{key}{}\": 0", " ".repeat(before), " ".repeat(after))
    })
}

/// `count` `@context` terms `<stem><i>` bound to `namespace`, each led by a comma.
fn prefixes(stem: &str, namespace: &str, count: usize) -> String {
    (0..count)
        .map(|i| format!(", \"{stem}{i}\": \"{namespace}\""))
        .collect()
}

/// `model` with a root `S231:hasParameter` list of `<id>.k` and then `items` more declarations
/// that end in `.k`, each of which re-binds the local name `k`.
fn redeclarations(model: &str, id: &str, items: usize) -> Vec<u8> {
    let root = "\"S231:label\": \"MinLoop\",";
    assert_eq!(model.matches(root).count(), 1);
    let again = list(items, |i| format!("{{ \"@id\": \"urn:d{i}.k\" }}"));
    model
        .replacen(
            root,
            &format!("{root} \"S231:hasParameter\": [ {{ \"@id\": \"{id}.k\" }}, {again} ],"),
            1,
        )
        .into_bytes()
}

/// One place the reader raises a diagnostic per item, about one long string.
struct Route {
    /// What repeats.
    name: &'static str,
    /// The code of the route's own diagnostics.
    code: DiagCode,
    /// The document with `items` items about the `@id` `id`.
    document: fn(id: &str, items: usize) -> Vec<u8>,
    /// How many diagnostics of the route `items` items raise.
    raised: fn(items: usize) -> usize,
}

fn every_item(items: usize) -> usize {
    items
}

const ROUTES: &[Route] = &[
    Route {
        name: "a key written twice",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let keys = list(items, |i| format!("\"k{i:05}\": 0, \"k{i:05}\": 1"));
            with_long_node("", id, &keys)
        },
        raised: every_item,
    },
    Route {
        name: "a modeled key in surrounding whitespace",
        code: DiagCode::MalformedDocument,
        document: |id, items| with_long_node("", id, &padded("S231:min", items)),
        raised: every_item,
    },
    Route {
        name: "an unread hasUnit key in surrounding whitespace",
        code: DiagCode::MalformedDocument,
        document: |id, items| with_long_node("", id, &padded("qudt:hasUnit", items)),
        raised: every_item,
    },
    Route {
        name: "a modeled term keyed in a foreign namespace",
        code: DiagCode::VocabularyNamespaceMismatch,
        document: |id, items| {
            let terms = prefixes("x", "https://data.ashrae.org/S231#", items);
            with_long_node(&terms, id, &list(items, |i| format!("\"x{i}:min\": 0")))
        },
        raised: every_item,
    },
    Route {
        name: "hasUnit keyed in a near miss of a QUDT namespace",
        code: DiagCode::VocabularyNamespaceMismatch,
        document: |id, items| {
            let terms = prefixes("q", "https://qudt.org/schema/qudt#", items);
            with_long_node(&terms, id, &list(items, |i| format!("\"q{i}:hasUnit\": 0")))
        },
        raised: every_item,
    },
    Route {
        name: "a second spelling of one property",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let terms = prefixes("s", "http://data.ashrae.org/S231#", items + 1);
            let keys = list(items + 1, |i| format!("\"s{i}:min\": 0"));
            with_long_node(&terms, id, &keys)
        },
        raised: every_item,
    },
    Route {
        name: "a relative reference",
        code: DiagCode::RelativeIri,
        document: |id, items| {
            let references = list(items, |_| "{ \"@id\": \"r\" }".to_owned());
            with_long_node("", id, &format!("\"S231:hasInput\": [ {references} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "a type carrying a control character",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let types = list(items, |_| "\"\\u200bT\"".to_owned());
            with_long_node("", id, &format!("\"@type\": [ {types} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "an S231 type in surrounding whitespace",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let types = list(items, |_| "\" S231:Real\"".to_owned());
            with_long_node("", id, &format!("\"@type\": [ {types} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "a type in a foreign namespace",
        code: DiagCode::VocabularyNamespaceMismatch,
        document: |id, items| {
            let types = list(items, |_| {
                "\"https://data.ashrae.org/S231#Real\"".to_owned()
            });
            with_long_node("", id, &format!("\"@type\": [ {types} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "a type that is no S231 term",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let types = list(items, |_| "\"S231:NoSuchTerm\"".to_owned());
            with_long_node("", id, &format!("\"@type\": [ {types} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "a reference carrying a key beside its @id",
        code: DiagCode::NonSubsetConstruct,
        document: |id, items| {
            let references = list(items, |_| "{ \"@id\": \"urn:r\", \"x\": 0 }".to_owned());
            with_long_node("", id, &format!("\"S231:hasInput\": [ {references} ]"))
        },
        raised: every_item,
    },
    Route {
        name: "a Modelica construct that survived lowering",
        code: DiagCode::NonSubsetConstruct,
        document: |id, items| {
            with_long_node("", id, &list(items, |i| format!("\"a{i}:extends\": 0")))
        },
        raised: every_item,
    },
    Route {
        name: "a declaration that re-binds a name first declared under a long @id",
        code: DiagCode::MalformedDocument,
        document: |id, items| redeclarations(MODEL, id, items),
        raised: every_item,
    },
    Route {
        // A conditional child makes the specialization pass read the root's declarations too,
        // and withhold what it finds until lowering has read them.
        name: "the same on a composite with a conditional child",
        code: DiagCode::MalformedDocument,
        document: |id, items| {
            let child = "\"S231:label\": \"con\",";
            assert_eq!(MODEL.matches(child).count(), 1);
            let conditional = MODEL.replacen(
                child,
                &format!(
                    "{child} \"S231:isConditionalComponent\": true, \
                     \"S231:conditionalExpression\": \"unknownFlag\","
                ),
                1,
            );
            redeclarations(&conditional, id, items)
        },
        raised: every_item,
    },
    Route {
        name: "a child listed again by a composite with a long @id",
        code: DiagCode::SharedInstance,
        document: |id, items| {
            let root = "\"@id\": \"http://example.org#MinLoop\",";
            let child = "{ \"@id\": \"http://example.org#MinLoop.con\" },";
            assert_eq!(MODEL.matches(root).count(), 1);
            assert_eq!(MODEL.matches(child).count(), 1);
            let again = list(items, |_| child.trim_end_matches(',').to_owned());
            MODEL
                .replacen(root, &format!("\"@id\": \"{id}\","), 1)
                .replacen(child, &format!("{child} {again},"), 1)
                .into_bytes()
        },
        raised: every_item,
    },
    Route {
        name: "a member outside its owner",
        code: DiagCode::NonSubsetConstruct,
        document: |id, items| {
            // The gain block, declaring no port list of its own, lists its members instead.
            let gain = "\"http://example.org#MinLoop.gain\"";
            let ports = "\"S231:hasInput\": { \"@id\": \"http://example.org#MinLoop.gain.u\" },\n      \"S231:hasOutput\": { \"@id\": \"http://example.org#MinLoop.gain.y\" }";
            assert_eq!(MODEL.matches(gain).count(), 2);
            assert_eq!(MODEL.matches(ports).count(), 1);
            let members = list(items, |i| format!("{{ \"@id\": \"urn:m{i}\" }}"));
            MODEL
                .replacen(ports, &format!("\"S231:hasInstance\": [ {members} ]"), 1)
                .replace(gain, &format!("\"{id}\""))
                .into_bytes()
        },
        raised: every_item,
    },
];

/// One load of `bytes`, which must be refused: its allocations and its diagnostics. The unedited
/// model is loaded once first, so what a process sets up on its first load is not counted.
fn refused(bytes: &[u8]) -> (AllocationInfo, Vec<Diagnostic>) {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    let error = result
        .expect("the load ran")
        .expect_err("the document is refused");
    (info, error.diagnostics().to_vec())
}

/// The closing error of a list that left `unlisted` diagnostics out, stated by hand.
fn closing(unlisted: usize) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed: a diagnostic list holds at most \
             8388608 bytes of message and subject text"
        ),
    )
}

/// The message and subject text `diagnostics` hold, each subject allocation counted once.
fn held_text(diagnostics: &[Diagnostic]) -> usize {
    let mut counted = HashSet::new();
    diagnostics
        .iter()
        .map(|diagnostic| {
            let subject = diagnostic
                .subject
                .as_ref()
                .filter(|subject| counted.insert(Arc::as_ptr(subject).cast::<u8>()))
                .map_or(0, |subject| subject.len());
            diagnostic.message.len() + subject
        })
        .sum()
}

fn bytes(count: usize) -> u64 {
    u64::try_from(count).expect("fits")
}

#[test]
fn every_route_holds_a_bounded_list_and_builds_nothing_once_it_has_ended() {
    let id = long_id(ID_BYTES);
    for route in ROUTES {
        let mut costs = Vec::new();
        for items in ITEMS {
            let document = (route.document)(&id, items);
            let (info, diagnostics) = refused(&document);
            let name = format!("{}, {items} items", route.name);

            // The list ends with the closing error, and the route's diagnostics before it name
            // the long `@id`. Listed and counted, every finding is accounted for.
            let (last, listed) = diagnostics.split_last().expect("diagnostics");
            let own: Vec<&Diagnostic> = listed
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == route.code
                        && (diagnostic.message.contains(&id)
                            || diagnostic
                                .subject
                                .as_deref()
                                .is_some_and(|s| s.contains(&id)))
                })
                .collect();
            assert!(
                !own.is_empty(),
                "{name}: {:?}",
                listed.first().map(|d| d.code)
            );
            let raised = (route.raised)(items);
            let unlisted = raised.checked_sub(own.len()).expect("no more than raised");
            assert!(unlisted > 0, "{name}: the list did not end");
            assert_eq!(last.code, DiagCode::ResourceLimitExceeded, "{name}");
            let counted: usize = last
                .message
                .split(' ')
                .next()
                .and_then(|count| count.parse().ok())
                .expect("the closing error starts with its count");
            assert!(
                counted >= unlisted && counted - unlisted <= 8,
                "{name}: {counted} counted, {unlisted} of the route's own left out"
            );
            assert_eq!(last, &closing(counted), "{name}");
            assert!(held_text(listed) <= BUDGET, "{name}");

            // The peak is the budget and what reading the document holds. [measured: 9.7 to
            // 19.6 MB across the cases, for documents of 0.27 to 0.57 MB; 106 to 316 MB before
            // the list had a budget]
            assert!(
                info.bytes_max <= bytes(BUDGET) + 48 * bytes(document.len()),
                "{name}: {} bytes held for a document of {}",
                info.bytes_max,
                document.len()
            );
            costs.push((info, document.len()));
        }
        // Between the two sizes every further item is one the list has no room for. It costs
        // what reading it costs: a fixed number of bytes and allocations, never the `@id` again.
        // [measured: 142 to 1,966 bytes and 2.0 to 15.2 allocations a further item; a diagnostic
        // built and dropped would request 262,144 bytes or more]
        let (small, large) = (&costs[0], &costs[1]);
        let further = bytes(ITEMS[1] - ITEMS[0]);
        assert!(
            large.0.bytes_total - small.0.bytes_total <= 4_096 * further,
            "{}: {} more bytes requested for {further} more items",
            route.name,
            large.0.bytes_total - small.0.bytes_total
        );
        assert!(
            large.0.count_total - small.0.count_total <= 24 * further,
            "{}: {} more allocations for {further} more items",
            route.name,
            large.0.count_total - small.0.count_total
        );
    }
}

/// [`MODEL`] with one more node, `@id` `id`, whose `S231:hasInput` holds `count` references to
/// `token`, which names nothing: every one is a relative reference.
fn relative_references(id: &str, token: &str, count: usize) -> Vec<u8> {
    let references = list(count, |_| format!("{{ \"@id\": \"{token}\" }}"));
    with_long_node("", id, &format!("\"S231:hasInput\": [ {references} ]"))
}

#[test]
fn a_list_that_fills_the_budget_exactly_is_whole_and_one_byte_more_ends_it() {
    // A relative reference's diagnostic names its node twice, in the message and as a subject
    // of its own. What else its message holds is measured on a short `@id`.
    let short = long_id(64);
    let (_, probe) = refused(&relative_references(&short, "r", 1));
    assert_eq!(probe.len(), 1, "{probe:?}");
    assert_eq!(probe[0].code, DiagCode::RelativeIri);
    let fixed = held_text(&probe) - 2 * short.len();

    // Sixteen diagnostics of exactly a sixteenth of the budget each. The token is in the message
    // once, so a second character makes an odd remainder even.
    let each = BUDGET / 16;
    let token = if (each - fixed).is_multiple_of(2) {
        "r"
    } else {
        "rr"
    };
    let id = long_id((each - fixed - (token.len() - 1)) / 2);

    let (_, whole) = refused(&relative_references(&id, token, 16));
    assert_eq!(whole.len(), 16);
    assert_eq!(held_text(&whole), BUDGET);
    assert!(
        whole
            .iter()
            .all(|diagnostic| diagnostic.code == DiagCode::RelativeIri
                && diagnostic.subject.as_deref() == Some(id.as_str())),
        "every finding is listed, and nothing else"
    );

    // One more finding of the same size: the sixteen are listed as they were, and it is counted.
    let (_, one_more) = refused(&relative_references(&id, token, 17));
    assert_eq!(one_more.len(), 17);
    assert_eq!(&one_more[..16], &whole[..]);
    assert_eq!(one_more[16], closing(1));

    // The same sixteen findings, each one byte longer in its message: fifteen fit.
    let longer = format!("{token}r");
    let (_, one_byte_more) = refused(&relative_references(&id, &longer, 16));
    assert_eq!(one_byte_more.len(), 16);
    assert_eq!(held_text(&one_byte_more[..15]), 15 * (each + 1));
    assert_eq!(one_byte_more[15], closing(1));
}
