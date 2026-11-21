//! What a refusal list costs a load, in requested bytes and allocations and never in elapsed time
//! (GitHub issue #695; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A diagnostic copies document text: its subject is a node's `@id`, and most messages name that
//! `@id` again. One long `@id` and many findings about it, each costing the document a few bytes,
//! make a list of `findings × id bytes` unless the list is bounded: 400 padded keys on a node with
//! a 1 MiB `@id`, a document of 2.1 MB, are 1.2 GB of diagnostics. The reader's list holds at most
//! 8 MiB of message and subject text, a subject counted for every diagnostic that carries it; the
//! first diagnostic that does not fit ends the list, and one `resource-limit-exceeded` error
//! counts that diagnostic and every later one and names the first of them.
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
//! Two more measures say what the budget is worth to a host. A receipt, which copies every
//! record's subject, holds no more than the budget again. And what a returned refusal holds is
//! its text and a fixed amount for each diagnostic, with no spare capacity in a message.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets); the headroom is far under the
//! cost of one copied `@id` an item. What this cannot see: work that allocates nothing.

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

/// The closing error of a list that left `unlisted` diagnostics out, the first of them described
/// as `first` (its code and where it is), stated by hand.
fn closing(unlisted: usize, first: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed, the first of them {first}: a \
             diagnostic list holds at most 8388608 bytes of message and subject text"
        ),
    )
}

/// The message and subject text of `diagnostics`, as the reader's budget counts it: a subject
/// for every diagnostic that carries it.
fn text(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic.message.len() + diagnostic.subject.as_deref().map_or(0, str::len)
        })
        .sum()
}

/// `text` as a message quotes authored text of more than 64 bytes: its first 64 and an ellipsis.
fn excerpt(text: &str) -> String {
    format!("{}…", &text[..64])
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
            // It names the first finding left out: one of the route's own.
            let leading = format!(
                "{counted} more diagnostic(s) are not listed, the first of them `{}` on `",
                route.code
            );
            assert!(
                last.message.starts_with(&leading),
                "{name}: {}",
                last.message
            );
            assert!(last.message.len() <= 256, "{name}: {}", last.message);
            assert_eq!(last.subject, None, "{name}");
            assert!(text(listed) <= BUDGET, "{name}");

            // The peak is the budget and what reading the document holds. [measured: 6.1 to
            // 11.7 MB across the cases, for documents of 0.27 to 0.57 MB; 106 to 316 MB with no
            // budget on the list]
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
        // built and dropped requests 262,144 bytes or more]
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
    let fixed = text(&probe) - 2 * short.len();

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
    assert_eq!(text(&whole), BUDGET);
    assert!(
        whole
            .iter()
            .all(|diagnostic| diagnostic.code == DiagCode::RelativeIri
                && diagnostic.subject.as_deref() == Some(id.as_str())),
        "every finding is listed, and nothing else"
    );

    // One more finding of the same size: the same sixteen are listed, and it is counted.
    let (_, one_more) = refused(&relative_references(&id, token, 17));
    assert_eq!(one_more.len(), 17);
    assert_eq!(&one_more[..16], &whole[..]);
    let first = format!("`relative-iri` on `{}`", excerpt(&id));
    assert_eq!(one_more[16], closing(1, &first));

    // The same sixteen findings, each one byte longer in its message: fifteen fit.
    let longer = format!("{token}r");
    let (_, one_byte_more) = refused(&relative_references(&id, &longer, 16));
    assert_eq!(one_byte_more.len(), 16);
    assert_eq!(text(&one_byte_more[..15]), 15 * (each + 1));
    assert_eq!(one_byte_more[15], closing(1, &first));
}

/// [`MODEL`] with one more node of a 64 KiB `@id` whose `S231:hasInput` holds `count`
/// references, each with a `@context` of its own: `count` refusals that share the node's `@id`
/// as one subject allocation.
fn scoped_references(count: usize) -> Vec<u8> {
    let references = list(count, |i| {
        format!(
            "{{ \"@id\": \"local:r{i}\", \"@context\": {{ \"local\": \"http://scoped.example#\" }} }}"
        )
    });
    with_long_node(
        "",
        &long_id(64 * 1024),
        &format!("\"S231:hasInput\": [ {references} ]"),
    )
}

#[test]
fn a_receipt_copies_no_more_text_than_the_list_it_is_taken_from_holds() {
    // 1,024 refusals on one 64 KiB subject, shared as one allocation: 127 fit the budget, which
    // counts the subject for each. A receipt copies every record's subject, so what it holds is
    // the budget again and no more.
    let document = scoped_references(1_024);
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut plain = Engine::new();
    let mut receipted = Engine::new();
    let (mut without, mut with) = (None, None);
    let plain_info = measure(|| without = Some(plain.load_cxf(&document)));
    let receipt_info = measure(|| with = Some(receipted.load_cxf_with_receipt(&document)));
    let error = without.expect("ran").expect_err("refused");
    let failure = with.expect("ran").expect_err("refused");

    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 128);
    assert_eq!(diagnostics[127].code, DiagCode::ResourceLimitExceeded);
    assert!(text(&diagnostics[..127]) <= BUDGET);
    let records = failure.diagnostics().records();
    assert_eq!(records.len(), 128);
    // [measured: a peak of 3.1 MB without a receipt and 8.5 MB with one, for a document of
    // 148,711 bytes; with every one of the 1,024 listed and each subject copied, 67.9 MB]
    assert!(
        plain_info.bytes_max <= 48 * bytes(document.len()),
        "{} bytes held without a receipt",
        plain_info.bytes_max
    );
    assert!(
        receipt_info.bytes_max <= bytes(BUDGET) + 48 * bytes(document.len()),
        "{} bytes held with a receipt",
        receipt_info.bytes_max
    );
}

#[test]
fn a_refusal_list_holds_its_text_and_a_fixed_amount_for_each_diagnostic() {
    // Two full lists: fifteen diagnostics of half a megabyte, and tens of thousands of about a
    // hundred bytes. What the returned error holds, once the load has returned, is the text and
    // a record for each diagnostic.
    for (id_bytes, items) in [(ID_BYTES, 400_usize), (64, 100_000)] {
        let id = long_id(id_bytes);
        let references = list(items, |_| "{ \"@id\": \"r\" }".to_owned());
        let document = with_long_node("", &id, &format!("\"S231:hasInput\": [ {references} ]"));
        Engine::new()
            .load_cxf(MODEL.as_bytes())
            .expect("the model loads");
        let mut engine = Engine::new();
        let mut result = None;
        let info = measure(|| result = Some(engine.load_cxf(&document)));
        let error = result.expect("ran").expect_err("refused");
        let diagnostics = error.diagnostics();
        let held = text(diagnostics);
        // [measured: 1,073 bytes beside 7,866,384 of text in 16 diagnostics, and 2,138,988
        // beside 8,388,638 in 33,421: 64 bytes a diagnostic, 1.26 times the text]
        assert!(
            info.bytes_current
                <= i64::try_from(held + 112 * diagnostics.len() + 4_096).expect("fits"),
            "{} bytes held for {held} of text in {} diagnostics",
            info.bytes_current,
            diagnostics.len()
        );
    }
}
