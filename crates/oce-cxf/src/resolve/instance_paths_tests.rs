//! The instance path budget charged by the reader, at its real size: each test reserves all but
//! what its small document holds (`instance_path_budget::reserving`), so the edges are pinned
//! without a document of 8 MiB of identities.

use oce_diag::{DiagCode, Diagnostic};

use crate::instance_path_budget::{MAX_DOCUMENT_INSTANCE_PATH_BYTES, reserving};
use crate::{CxfError, ResolveOptions, import_cxf};

const S231: &str = "http://data.ashrae.org/S231#";
const CDL: &str = "http://example.org#Buildings.Controls.OBC.CDL";

/// The bytes of CalendarTime's six output names: year, month, day, hour, minute, weekDay.
const CALENDAR_OUTPUT_NAMES: usize = 4 + 5 + 3 + 4 + 6 + 7;

/// A stem of `bytes` bytes for an instance IRI, its last byte `tail`.
fn stem(bytes: usize, tail: char) -> String {
    format!("urn:{}{tail}", "i".repeat(bytes - 5))
}

/// A root holding a `CalendarTime` named by `calendar`, in the `hasInstance` dialect with only
/// its `offset` member listed (so its six outputs are padded) and the shared `zerTim` node, and an
/// `Add` named by `add` whose two inputs the root's two boundary inputs drive. With `prefixed`,
/// each instance IRI is written `c:` or `a:` under a `@context` prefix.
fn mixed(calendar: &str, add: &str, prefixed: bool) -> Vec<u8> {
    let (c, a, context) = if prefixed {
        (
            "c:".to_owned(),
            "a:".to_owned(),
            format!(r#"{{"S231":"{S231}","c":"{calendar}","a":"{add}"}}"#),
        )
    } else {
        (
            calendar.to_owned(),
            add.to_owned(),
            format!(r#"{{"S231":"{S231}"}}"#),
        )
    };
    format!(
        r#"{{"@context":{context},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":[{{"@id":"{c}"}},{{"@id":"{a}"}}],"S231:hasInput":[{{"@id":"urn:M.u1"}},{{"@id":"urn:M.u2"}}]}},
{{"@id":"urn:M.u1","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"{a}.u1"}}}},
{{"@id":"urn:M.u2","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"{a}.u2"}}}},
{{"@id":"{c}","@type":"{CDL}.Reals.Sources.CalendarTime","S231:hasParameter":[{{"@id":"urn:p.zerTim"}}],"S231:hasInstance":[{{"@id":"{c}.offset"}}]}},
{{"@id":"{c}.offset","S231:value":0.0}},
{{"@id":"urn:p.zerTim","S231:value":"Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017"}},
{{"@id":"{a}","@type":"{CDL}.Reals.Add","S231:hasInput":[{{"@id":"{a}.u1"}},{{"@id":"{a}.u2"}}],"S231:hasOutput":{{"@id":"{a}.y"}}}},
{{"@id":"{a}.u1","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"{a}.u2","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"{a}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}}
]}}"#
    )
    .into_bytes()
}

/// The identities of [`mixed`] in the order they are charged, as `(subject, bytes)`.
fn mixed_charges(calendar: &str, add: &str) -> Vec<(String, usize)> {
    let (c, a) = (calendar.len(), add.len());
    vec![
        (calendar.to_owned(), 6 * (c + 1) + CALENDAR_OUTPUT_NAMES),
        ("urn:p.zerTim".to_owned(), c + 1 + "zerTim".len()),
        (format!("{calendar}.offset"), c + 1 + "offset".len()),
        ("urn:M.u1".to_owned(), a + 1 + "in0".len()),
        ("urn:M.u2".to_owned(), a + 1 + "in1".len()),
    ]
}

/// `document` read with all but `left` bytes of its budget taken: `Ok` with its warnings, or the
/// refusal's diagnostics.
fn read_with(left: usize, document: &[u8]) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    reserving(
        MAX_DOCUMENT_INSTANCE_PATH_BYTES - left,
        || match import_cxf(document, &ResolveOptions::default()) {
            Ok((_, report)) => Ok(report.diagnostics),
            Err(CxfError::Validation(diagnostics)) => Err(diagnostics),
            Err(other) => panic!("{other:?}"),
        },
    )
}

/// The subjects of the `resource-limit-exceeded` refusals among `diagnostics`.
fn refused(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .map(|d| d.subject.as_deref().expect("a refusal names its subject"))
        .collect()
}

#[test]
fn a_document_whose_identities_take_the_budget_exactly_reads_and_one_byte_more_is_refused() {
    // Every kind charged at once: six padded outputs, two parameter listings (one shared node,
    // one `hasInstance` member) and two boundary-driven inputs. At exactly their bytes the
    // document reads; a byte short, the last identity charged, the second boundary input, is
    // refused and nothing else is. The prefixed and literal spellings read alike.
    let (calendar, add) = (stem(4_096, 'c'), stem(512, 'a'));
    let charges = mixed_charges(&calendar, &add);
    let total: usize = charges.iter().map(|(_, bytes)| bytes).sum();
    assert_eq!(total, 8 * 4_096 + 2 * 512 + 57);
    let mut outcomes = Vec::new();
    for prefixed in [false, true] {
        let document = mixed(&calendar, &add, prefixed);
        let warnings = read_with(total, &document).expect("exactly at the budget, it reads");
        assert!(refused(&warnings).is_empty());
        let refusal = read_with(total - 1, &document).expect_err("a byte short, it is refused");
        assert_eq!(refused(&refusal), ["urn:M.u2"], "{refusal:?}");
        assert_eq!(refusal.len(), 1, "{refusal:?}");
        outcomes.push((warnings, refusal));
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "a prefix reads as the literal IRI"
    );
}

#[test]
fn each_kind_of_identity_is_refused_where_it_is_charged() {
    let (calendar, add) = (stem(4_096, 'c'), stem(512, 'a'));
    let charges = mixed_charges(&calendar, &add);
    let bytes =
        |range: std::ops::Range<usize>| charges[range].iter().map(|(_, b)| b).sum::<usize>();
    let document = mixed(&calendar, &add, true);
    // A byte short of the padded outputs: the calendar is refused before lowering, and nothing
    // after it is read.
    let refusal = read_with(charges[0].1 - 1, &document).expect_err("refused");
    assert_eq!(refused(&refusal), [calendar.as_str()]);
    assert_eq!(refusal.len(), 1, "{refusal:?}");
    assert!(refusal[0].message.starts_with(&format!(
        "padded output identities of {} byte(s)",
        charges[0].1
    )));
    // A byte short of the outputs and both listings: the member listing is refused and reserves
    // nothing, so both boundary inputs, shorter together than it, still fit.
    assert!(bytes(3..5) < charges[2].1);
    let refusal = read_with(bytes(0..3) - 1, &document).expect_err("refused");
    assert_eq!(refused(&refusal), [charges[2].0.as_str()], "{refusal:?}");
    assert!(
        refusal[0]
            .message
            .starts_with(&format!("parameter path of {} byte(s)", charges[2].1))
    );
    // A byte short of the first boundary input: neither it nor the second, as long, fits.
    let refusal = read_with(bytes(0..4) - 1, &document).expect_err("refused");
    assert_eq!(refused(&refusal), ["urn:M.u1", "urn:M.u2"], "{refusal:?}");
}

/// Two `Constant` leaves named `first` and `second`, in that `containsBlock` order, each listing
/// the one shared parameter node `urn:n.q` its class does not declare, and the second also the
/// array node `urn:n.arr`.
fn sharing(first: &str, second: &str) -> Vec<u8> {
    let leaf = |id: &str, array: bool| {
        let extra = if array { r#",{"@id":"urn:n.arr"}"# } else { "" };
        format!(
            r#"{{"@id":"{id}","@type":"{CDL}.Reals.Sources.Constant","S231:hasParameter":[{{"@id":"urn:n.q"}}{extra}],"S231:hasOutput":{{"@id":"{id}.y"}}}},
{{"@id":"{id}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}}"#
        )
    };
    format!(
        r#"{{"@context":{{"S231":"{S231}"}},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":[{{"@id":"{first}"}},{{"@id":"{second}"}}]}},
{},
{},
{{"@id":"urn:n.q","S231:value":1.0}},
{{"@id":"urn:n.arr","S231:isArray":true,"S231:numberDimensions":1,"S231:sizeOfDimensions":"(2)","S231:value":[1.0,2.0]}}
]}}"#,
        leaf(first, false),
        leaf(second, true)
    )
    .into_bytes()
}

#[test]
fn a_shared_node_is_charged_for_each_listing_and_the_refused_one_follows_the_listing_order() {
    // `urn:n.q` is undeclared and listed by a long and a short leaf: each listing is charged its
    // own path, `<leaf>.q`. Whether the document fits does not depend on the order of the
    // leaves; which listing is refused, a byte short, does: the one charged last. The array node
    // is charged to the array budgets, never here.
    let (long, short) = (stem(2_048, 'l'), stem(64, 's'));
    let total = (long.len() + 2) + (short.len() + 2);
    for (first, second) in [(&long, &short), (&short, &long)] {
        let document = sharing(first, second);
        read_with(total, &document).expect("both listings fit exactly, in either order");
        let refusal = read_with(total - 1, &document).expect_err("one listing does not fit");
        assert_eq!(refused(&refusal), ["urn:n.q"], "{refusal:?}");
        let expected = format!(
            "parameter path of {} byte(s) under `{}",
            second.len() + 2,
            &second[..32]
        );
        assert!(
            refusal[0].message.starts_with(&expected),
            "{}",
            refusal[0].message
        );
    }
    // A byte short of the long listing alone: in either order the long one is refused, and the
    // short one, charged before or after it, fits.
    for (first, second) in [(&long, &short), (&short, &long)] {
        let refusal = read_with(long.len() + 1, &sharing(first, second)).expect_err("refused");
        assert_eq!(refusal.len(), 1, "{refusal:?}");
        let expected = format!("parameter path of {} byte(s)", long.len() + 2);
        assert!(
            refusal[0].message.starts_with(&expected),
            "{}",
            refusal[0].message
        );
    }
}

/// One `Constant` leaf named by an IRI of `iri_bytes` bytes, written once under a `@context`
/// prefix, listing the shared undeclared node `urn:n.q` eight times.
fn eight_listings(iri_bytes: usize) -> Vec<u8> {
    let stem = format!("urn:{}", "i".repeat(iri_bytes - 4));
    let listings = [r#"{"@id":"urn:n.q"}"#; 8].join(",");
    format!(
        r#"{{"@context":{{"S231":"{S231}","x":"{stem}"}},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":{{"@id":"x:"}}}},
{{"@id":"x:","@type":"{CDL}.Reals.Sources.Constant","S231:hasParameter":[{listings}],"S231:hasOutput":{{"@id":"x:.y"}}}},
{{"@id":"x:.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"urn:n.q","S231:value":1.0}}
]}}"#
    )
    .into_bytes()
}

#[test]
fn the_budget_is_eight_mebibytes_of_identities_read_whole() {
    // Nothing reserved: eight listings of `<IRI>.q` under an IRI of 1 MiB less 2 bytes take
    // exactly 8 MiB and read; under one a byte longer they take 8 bytes more, and the eighth
    // listing is refused.
    assert_eq!(MAX_DOCUMENT_INSTANCE_PATH_BYTES, 8 * 1024 * 1024);
    let fits = (1 << 20) - 2;
    assert_eq!(8 * (fits + 2), MAX_DOCUMENT_INSTANCE_PATH_BYTES);
    import_cxf(&eight_listings(fits), &ResolveOptions::default()).expect("exactly 8 MiB reads");
    match import_cxf(&eight_listings(fits + 1), &ResolveOptions::default()) {
        Err(CxfError::Validation(diagnostics)) => {
            assert_eq!(refused(&diagnostics), ["urn:n.q"], "{diagnostics:?}");
            assert!(diagnostics[0].message.contains(&format!(
                "already take {} of the 8388608 bytes",
                7 * (fits + 3)
            )));
        }
        other => panic!("8 bytes past the budget: {other:?}"),
    }
}

#[test]
fn a_document_past_the_expansion_budget_too_is_refused_by_that_budget_alone() {
    // The expansion budget is charged first, at Step 0, and nothing after its refusal is read:
    // with every byte of the instance path budget taken, a document whose prefix also expands
    // past 8 MiB reports the expansion alone, and the same leaf under a prefix that fits reports
    // the identity. Each `x:` spelling expands by 2 MiB less 66; the leaf is spelled four times
    // and its eight listings' node `x:.q` nine times more; under the prefix that fits, the leaf's
    // four spellings take 8 MiB less 256 bytes.
    let stem = format!("urn:{}", "i".repeat((2 << 20) - 68));
    let document = |listed: &str| {
        let listings = vec![format!(r#"{{"@id":"{listed}"}}"#); 8].join(",");
        format!(
            r#"{{"@context":{{"S231":"{S231}","x":"{stem}"}},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":{{"@id":"x:"}}}},
{{"@id":"x:","@type":"{CDL}.Reals.Sources.Constant","S231:hasParameter":[{listings}],"S231:hasOutput":{{"@id":"x:.y"}}}},
{{"@id":"x:.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"{listed}","S231:value":1.0}}
]}}"#
        )
        .into_bytes()
    };
    let refusal = read_with(0, &document("x:.q")).expect_err("refused");
    assert_eq!(refusal.len(), 1, "{refusal:?}");
    assert!(
        refusal[0]
            .message
            .contains("through the document's @context"),
        "{refusal:?}"
    );
    let refusal = read_with(0, &document("urn:n.q")).expect_err("refused");
    assert_eq!(refused(&refusal), vec!["urn:n.q"; 8], "{refusal:?}");
    assert!(
        refusal
            .iter()
            .all(|d| d.message.starts_with("parameter path of"))
    );
}
