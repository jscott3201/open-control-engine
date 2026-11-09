//! CPU bounds on long authored connection sources in composite lowering (GitHub issue #480).
//!
//! Lowering once classified both ends of every authored `isConnectedTo` edge by string content,
//! so one long authored source shared by a whole fan-out cost fan-out × source length: a leaf
//! output with a 1 MiB `@id` wired 65,535 times to one top boundary output took more than three
//! minutes of release CPU before any diagnostic. A leaf input wired 65,535 times to a top boundary
//! input is re-anchored (swapped) onto the input, so the lowered list also copied the 1 MiB IRI
//! once per edge — 64 GiB. Lowering now interns each distinct endpoint once and shares one
//! allocation per lowered IRI, so these documents import in time and memory linear in their size.
//!
//! The 15 s wall-clock bound per import is deliberately generous — it guards the complexity
//! class, not a speed. With the fix one 10 MB import takes well under a second in debug and under
//! 0.1 s in release on a developer machine; allowing roughly 3× for a Windows cell or a loaded CI
//! runner still leaves more than 5× headroom. Before the fix the same import took several minutes
//! in release (and about half an hour in debug), so the pre-fix code fails the bound by more than
//! an order of magnitude. Documents are generated straight to bytes, and each test builds at most
//! three 10 MB documents, so each stays under 10 s in debug.

use std::time::{Duration, Instant};

use super::*;

/// One mebibyte of `@id` padding.
const MIB: usize = 1024 * 1024;

/// Edges authored on the long source: one below the boundary walk's examined-target limit.
const FANOUT: usize = 65_535;

/// Generous bound on one import; see the module docs.
const IMPORT_BOUND: Duration = Duration::from_secs(15);

const MODEL: &str = "http://example.org#fan";

/// Which leaf port carries the long `@id` and which top boundary port it is wired to.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Leaf output → Boolean top output: kept, one `TypeMismatch` per edge.
    OutputToTopOutput,
    /// Leaf input → top output: contradictory, one `DirectionMismatch` per edge.
    InputToTopOutput,
    /// Leaf input → top input: swapped onto the input, one multiply-driven input.
    InputToTopInput,
}

const SHAPES: [Shape; 3] = [
    Shape::OutputToTopOutput,
    Shape::InputToTopOutput,
    Shape::InputToTopInput,
];

/// A flat top composite with one `MultiplyByParameter` leaf whose `@id` carries `padding` bytes,
/// a Real top input and a Boolean top output, and `fanout` copies of one edge from the shape's
/// leaf port to its top port. Written straight to bytes; returns the document and the long port.
fn flat_fanout(shape: Shape, padding: usize, fanout: usize) -> (Vec<u8>, String) {
    let leaf = format!("{MODEL}.leaf{}", "x".repeat(padding));
    let (source, other, source_type, other_type, target) = match shape {
        Shape::OutputToTopOutput => ("y", "u", "RealOutput", "RealInput", "yb"),
        Shape::InputToTopOutput => ("u", "y", "RealInput", "RealOutput", "yb"),
        Shape::InputToTopInput => ("u", "y", "RealInput", "RealOutput", "ub"),
    };
    let real = r#""S231:isOfDataType":{"@id":"S231:Real"}"#;
    let mut out = String::with_capacity(8 * padding + fanout * (MODEL.len() + 12) + 1024);
    out.push_str(r#"{"@context":{"S231":"http://data.ashrae.org/S231P#"},"@graph":["#);
    out.push_str(&format!(
        r#"{{"@id":"{MODEL}","@type":"S231:Block","S231:containsBlock":{{"@id":"{leaf}"}},"S231:hasInput":{{"@id":"{MODEL}.ub"}},"S231:hasOutput":{{"@id":"{MODEL}.yb"}}}},"#
    ));
    out.push_str(&format!(
        r#"{{"@id":"{MODEL}.ub","@type":"S231:RealInput",{real}}},{{"@id":"{MODEL}.yb","@type":"S231:BooleanOutput","S231:isOfDataType":{{"@id":"S231:Boolean"}}}},"#
    ));
    out.push_str(&format!(
        r#"{{"@id":"{leaf}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter","S231:hasParameter":{{"@id":"{leaf}.k"}},"S231:hasInput":{{"@id":"{leaf}.u"}},"S231:hasOutput":{{"@id":"{leaf}.y"}}}},"#
    ));
    out.push_str(&format!(
        r#"{{"@id":"{leaf}.k","@type":"S231:Parameter",{real},"S231:value":1.0}},{{"@id":"{leaf}.{other}","@type":"S231:{other_type}",{real}}},"#
    ));
    out.push_str(&format!(
        r#"{{"@id":"{leaf}.{source}","@type":"S231:{source_type}",{real},"S231:isConnectedTo":["#
    ));
    let edge = format!(r#"{{"@id":"{MODEL}.{target}"}}"#);
    for index in 0..fanout {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&edge);
    }
    out.push_str("]}]}");
    (out.into_bytes(), format!("{leaf}.{source}"))
}

/// Import `bytes`, returning its refusal and how long the import took.
fn timed_refusal(bytes: &[u8], label: &str) -> (Vec<Diagnostic>, Duration) {
    let started = Instant::now();
    let result = import_cxf(bytes, &ResolveOptions::default());
    let elapsed = started.elapsed();
    match result {
        Err(CxfError::Validation(diagnostics)) => (diagnostics, elapsed),
        Ok(_) => panic!("{label}: the document must be refused"),
        Err(error) => panic!("{label}: unexpected import error: {error:?}"),
    }
}

/// Assert the shape's ordinary verdict over the long source, and that no resource bound fired.
fn assert_verdict(shape: Shape, source: &str, fanout: usize, diagnostics: &[Diagnostic]) {
    let (code, message, count) = match shape {
        Shape::OutputToTopOutput => (
            DiagCode::TypeMismatch,
            "connected types differ: Real → Boolean".to_owned(),
            fanout,
        ),
        Shape::InputToTopOutput => (
            DiagCode::DirectionMismatch,
            "boundary output is driven by a non-output connector".to_owned(),
            fanout,
        ),
        Shape::InputToTopInput => (
            DiagCode::SingleAssignment,
            format!("input is multiply driven (in-degree {fanout})"),
            1,
        ),
    };
    let matching = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code && diagnostic.message == message)
        .collect::<Vec<_>>();
    assert_eq!(
        matching.len(),
        count,
        "{shape:?}: {} diagnostics",
        diagnostics.len()
    );
    // Comparing a 1 MiB subject per diagnostic would itself cost fan-out × IRI length, so the
    // content is compared once and every other subject must be that same shared allocation.
    let subject = matching[0]
        .subject
        .as_ref()
        .expect("a verdict names its subject");
    assert_eq!(
        &**subject, source,
        "{shape:?}: the verdict names the long source"
    );
    assert!(
        matching.iter().all(|diagnostic| diagnostic
            .subject
            .as_ref()
            .is_some_and(|other| std::sync::Arc::ptr_eq(other, subject))),
        "{shape:?}: every verdict shares one subject allocation"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::MalformedDocument),
        "{shape:?}: no resource bound applies to this document"
    );
}

/// A 1 MiB authored source fanned out 65,535 times lowers and resolves to its ordinary verdict
/// within the bound, for every shape. Before the fix each import took minutes in release.
#[test]
fn mebibyte_authored_source_fanout_lowers_within_the_cpu_bound() {
    for shape in SHAPES {
        let (bytes, source) = flat_fanout(shape, MIB, FANOUT);
        let (diagnostics, elapsed) = timed_refusal(&bytes, &format!("{shape:?}"));
        assert_verdict(shape, &source, FANOUT, &diagnostics);
        assert!(
            elapsed < IMPORT_BOUND,
            "{shape:?}: import took {elapsed:?}, bound {IMPORT_BOUND:?}"
        );
    }
}

/// A one-byte-padded source keeps the verdict the 1 MiB one gets, for every shape.
#[test]
fn short_authored_source_keeps_the_same_verdict() {
    for shape in SHAPES {
        let (bytes, source) = flat_fanout(shape, 1, FANOUT);
        let (diagnostics, elapsed) = timed_refusal(&bytes, &format!("{shape:?}"));
        assert_verdict(shape, &source, FANOUT, &diagnostics);
        assert!(elapsed < IMPORT_BOUND, "{shape:?}: {elapsed:?}");
    }
}

/// Importing one fan-out document twice in one process yields identical diagnostic vectors, for
/// every shape: endpoint ids and allocation-keyed memos never leak into the output.
#[test]
fn repeated_lowering_import_reports_identical_diagnostics() {
    for shape in SHAPES {
        let (bytes, source) = flat_fanout(shape, 64, 16);
        let (first, _) = timed_refusal(&bytes, &format!("{shape:?}"));
        let (second, _) = timed_refusal(&bytes, &format!("{shape:?}"));
        assert_verdict(shape, &source, 16, &first);
        assert_eq!(
            first, second,
            "{shape:?}: repeated import changed the diagnostics"
        );
    }
}
