//! Text validation cost stays near-linear (issue #482 review, ruling C): placing findings on the
//! text indexes the subjects once, so a document at the statement cap whose every block carries a
//! finding costs about what the validator itself costs, not findings × declarations (the review
//! measured 17.8 s for 64,000 blocks before the index, against 31 ms for `validate`).

use std::time::{Duration, Instant};

use oce_author::{MAX_STATEMENTS_PER_KIND, MAX_TEXT_BYTES, validate_text_author_side};

/// `n` blocks of an unregistered class: one `class-not-found` finding each.
fn document(n: usize) -> String {
    let mut text = String::from("sequence s\nderived_from none\n");
    for i in 0..n {
        text.push_str(&format!("block b{i} = CDL.Reals.Nope\n"));
    }
    text
}

fn timed(text: &str, n: usize) -> Duration {
    let start = Instant::now();
    let findings = validate_text_author_side(text).expect("parses");
    let elapsed = start.elapsed();
    assert_eq!(findings.len(), n);
    assert!(findings.iter().all(|f| f.span.is_some()));
    elapsed
}

#[test]
fn a_document_at_the_statement_cap_places_every_finding() {
    let n = MAX_STATEMENTS_PER_KIND;
    let text = document(n);
    assert!(text.len() <= MAX_TEXT_BYTES);
    let findings = validate_text_author_side(&text).expect("parses");
    assert_eq!(findings.len(), n);
    let last = findings.last().expect("findings");
    assert_eq!(last.subject, format!("b{}", n - 1));
    assert_eq!(
        last.span.map(|at| (at.line, at.column)),
        Some((u32::try_from(n + 2).expect("fits"), 7))
    );
}

#[test]
fn sixteen_times_the_findings_costs_well_under_a_quadratic_factor() {
    // 16x the blocks and findings: linear work ~16x, findings × declarations ~256x; the bound
    // sits at 48x. The quietest of five interleaved pairs is judged, as in `validate_cost.rs`.
    let (small_n, large_n) = (MAX_STATEMENTS_PER_KIND / 16, MAX_STATEMENTS_PER_KIND);
    let (small, large) = (document(small_n), document(large_n));
    let mut best = (f64::INFINITY, Duration::ZERO, Duration::ZERO);
    for _ in 0..5 {
        let s = timed(&small, small_n);
        let l = timed(&large, large_n);
        let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
        if ratio < best.0 {
            best = (ratio, s, l);
        }
    }
    let (ratio, s, l) = best;
    assert!(
        ratio < 48.0,
        "16x the findings took {ratio:.1}x the time in the quietest pair ({s:?} vs {l:?})"
    );
}
