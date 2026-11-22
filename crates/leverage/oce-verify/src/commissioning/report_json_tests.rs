use super::*;
use crate::commissioning::choices::FunnelReference;
use crate::commissioning::report::{OutputStatus, OutputType};
use crate::commissioning::tolerances::PartialTolerances;
use crate::number::Number;

fn report() -> OutputReport {
    OutputReport {
        output: "urn:x#top.y".to_owned(),
        output_type: OutputType::Real,
        status: OutputStatus::Fail,
        compared: 3,
        masked: 1,
        uncovered: 0,
        funnel_reference: Some(FunnelReference::Trend),
        tolerances: Some(PartialTolerances {
            written: [
                Some(Number::Integer(1)),
                Some(Number::Real(0.5)),
                None,
                None,
                None,
                Some(Number::Real(1e-20)),
            ],
        }),
        half_width: Some(HalfSize { min: 1.0, max: 2.5 }),
        half_height: Some(HalfSize {
            min: -0.0,
            max: f64::NAN,
        }),
        worst: Some(Exceedance {
            t: 41235.0,
            exceedance: f64::INFINITY,
        }),
        first_exceedance_t: Some(-0.0),
        exceedance_time_s: 0.0,
        masked_intervals: vec![MaskedInterval { from: 1.0, to: 2.0 }],
        masked_intervals_total: 70,
        masked_exceedance: Vec::new(),
        masked_exceedance_total: 0,
    }
}

#[test]
fn computed_reals_use_the_canonical_spelling_and_odd_values_their_bits() {
    let json = output(&report()).to_document();
    // A computed Real always has a fraction digit.
    assert!(json.contains(r#""t": 41235.0"#), "{json}");
    assert!(json.contains(r#""exceedance_time_s": 0.0"#), "{json}");
    assert!(json.contains(r#""max": 2.5"#), "{json}");
    // Negative zero, NaN and the infinities are the string of their bits.
    assert!(
        json.contains(r#""exceedance": "0x7ff0000000000000""#),
        "{json}"
    );
    assert!(json.contains(r#""min": "0x8000000000000000""#), "{json}");
    assert!(
        json.contains(&format!(r#""max": "0x{:016x}""#, f64::NAN.to_bits())),
        "{json}"
    );
    assert!(
        json.contains(r#""first_exceedance_t": "0x8000000000000000""#),
        "{json}"
    );
    // A tolerance the spec wrote keeps its spelling; one nothing gave is `0`.
    assert!(json.contains(r#""atolx": 1,"#), "{json}");
    assert!(json.contains(r#""atoly": 0.5,"#), "{json}");
    assert!(json.contains(r#""ltolx": 0,"#), "{json}");
    assert!(json.contains(r#""rtoly": 1e-20"#), "{json}");
    assert!(json.contains(r#""funnel_reference": "trend""#), "{json}");
    assert!(json.contains(r#""masked_intervals_total": 70"#), "{json}");
}

#[test]
fn a_discrete_output_writes_no_envelope_field() {
    let mut discrete = report();
    discrete.output_type = OutputType::Boolean;
    discrete.funnel_reference = None;
    discrete.tolerances = None;
    discrete.half_width = None;
    discrete.half_height = None;
    let json = output(&discrete).to_document();
    for key in [
        "funnel_reference",
        "tolerances",
        "half_width",
        "half_height",
    ] {
        assert!(!json.contains(key), "{key}: {json}");
    }
    assert!(json.contains(r#""type": "boolean""#));
}

#[test]
fn a_passing_output_writes_no_worst_and_no_first_exceedance() {
    let mut passing = report();
    passing.status = OutputStatus::Pass;
    passing.worst = None;
    passing.first_exceedance_t = None;
    let json = output(&passing).to_document();
    assert!(
        !json.contains("worst") && !json.contains("first_exceedance_t"),
        "{json}"
    );
}
