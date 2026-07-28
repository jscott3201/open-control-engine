//! Port-identity matching: which document port belongs at which signature position.

use super::port_binding::{Binding, apply, match_names};
use oce_model::ConnectorId;

/// `Permuted(order)` or the test fails naming what it got instead.
fn order_of(iris: &[&str], names: &[&str]) -> Vec<usize> {
    match match_names(iris, names) {
        Binding::Permuted(order) => order,
        Binding::Positional => panic!("expected a name match, got Positional"),
        Binding::Partial { matched, total } => {
            panic!("expected a full name match, got Partial {matched}/{total}")
        }
    }
}

/// The case the whole module exists for: alphabetical order is permuted back to signature order.
///
/// `modelica-json` renders `CDL.Reals.PID` as `u_m` then `u_s`; the class declares `u_s` then
/// `u_m`. Read positionally that swaps setpoint and measurement and inverts the control action.
#[test]
fn alphabetical_document_order_is_permuted_into_signature_order() {
    let order = order_of(
        &["http://ex.org#loop.pid.u_m", "http://ex.org#loop.pid.u_s"],
        &["u_s", "u_m"],
    );
    assert_eq!(order, vec![1, 0]);

    let mut ports = [ConnectorId(70), ConnectorId(71)];
    apply(&order, &mut ports);
    assert_eq!(
        ports,
        [ConnectorId(71), ConnectorId(70)],
        "input 0 must end up holding the connector whose IRI names u_s"
    );
}

/// A document already in declaration order permutes to itself, so the corpus is untouched.
#[test]
fn declaration_order_is_the_identity_permutation() {
    let order = order_of(
        &["http://ex.org#loop.pid.u_s", "http://ex.org#loop.pid.u_m"],
        &["u_s", "u_m"],
    );
    assert_eq!(order, vec![0, 1]);
    let mut ports = [ConnectorId(4), ConnectorId(9)];
    apply(&order, &mut ports);
    assert_eq!(ports, [ConnectorId(4), ConnectorId(9)]);
}

/// This engine's own exporter mints `.in0`/`.in1`, which name positions rather than ports. Those
/// documents must keep binding positionally and must NOT raise a diagnostic — the RT-2 round-trip
/// re-imports exactly those bytes.
#[test]
fn position_named_ports_bind_positionally_without_complaint() {
    assert!(matches!(
        match_names(
            &["http://ex.org#loop.pid.in0", "http://ex.org#loop.pid.in1"],
            &["u_s", "u_m"],
        ),
        Binding::Positional
    ));
}

/// A document naming some ports after declared ports and the rest after something else follows no
/// convention that can be read safely, so it is reported rather than guessed at.
#[test]
fn a_partial_name_match_is_reported_not_guessed() {
    assert!(matches!(
        match_names(
            &["http://ex.org#loop.pid.u_s", "http://ex.org#loop.pid.in1"],
            &["u_s", "u_m"],
        ),
        Binding::Partial {
            matched: 1,
            total: 2
        }
    ));
}

/// Two ports sharing a local name must never both bind to the same signature position.
///
/// Consuming matches one apiece leaves the second `u_s` unmatched, so the side reports `Partial`
/// and nothing is reordered — the alternative is two signature positions pointing at one connector,
/// which would silently drop a wire.
#[test]
fn duplicate_local_names_do_not_double_bind() {
    assert!(matches!(
        match_names(
            &["http://ex.org#a.u_s", "http://ex.org#b.u_s"],
            &["u_s", "u_m"],
        ),
        Binding::Partial {
            matched: 1,
            total: 2
        }
    ));
}

/// Arity disagreement belongs to the arity guard; matching declines so the defect is reported once.
#[test]
fn count_disagreement_defers_to_the_arity_guard() {
    assert!(matches!(
        match_names(&["http://ex.org#loop.pid.u_s"], &["u_s", "u_m"]),
        Binding::Positional
    ));
    assert!(matches!(
        match_names(
            &[
                "http://ex.org#a.u_s",
                "http://ex.org#a.u_m",
                "http://ex.org#a.x"
            ],
            &["u_s", "u_m"],
        ),
        Binding::Positional
    ));
}

/// An IRI with no dotted local segment matches nothing, so an unfamiliar shape degrades to today's
/// positional behavior rather than erroring.
#[test]
fn iris_without_a_local_segment_fall_back_to_position() {
    assert!(matches!(
        match_names(&["urn:a", "urn:b"], &["u_s", "u_m"]),
        Binding::Positional
    ));
}

/// A five-port permutation, so the mapping is exercised beyond a single swap. `Reals.Line` declares
/// `x1, f1, x2, f2, u`; alphabetically that renders `f1, f2, u, x1, x2`.
#[test]
fn a_wide_permutation_maps_every_position() {
    let order = order_of(
        &[
            "http://ex.org#l.f1",
            "http://ex.org#l.f2",
            "http://ex.org#l.u",
            "http://ex.org#l.x1",
            "http://ex.org#l.x2",
        ],
        &["x1", "f1", "x2", "f2", "u"],
    );
    assert_eq!(order, vec![3, 0, 4, 1, 2]);
    let mut ports = [
        ConnectorId(0),
        ConnectorId(1),
        ConnectorId(2),
        ConnectorId(3),
        ConnectorId(4),
    ];
    apply(&order, &mut ports);
    // Signature position 0 is x1, which the document listed fourth.
    assert_eq!(
        ports,
        [
            ConnectorId(3),
            ConnectorId(0),
            ConnectorId(4),
            ConnectorId(1),
            ConnectorId(2)
        ]
    );
    // A permutation moves connectors, never invents or drops one.
    let mut seen: Vec<u32> = ports.iter().map(|c| c.0).collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2, 3, 4]);
}

/// An empty side is trivially bound, and a class with no inputs does not trip the matcher.
#[test]
fn an_empty_side_binds_trivially() {
    assert!(matches!(match_names(&[], &[]), Binding::Positional));
}

/// The reference generator's real output, verbatim.
///
/// These two IRIs are copied from `_research/cxf-structural-diff/reference/CDL.Reals.PID.jsonld`,
/// which `modelica-json` produced: its `S231:hasInput` array lists `u_m` before `u_s`, while
/// `Reals/PID.mo` declares `u_s` at line 48 and `u_m` at line 51. Both ports are `Real`, so nothing
/// else in the workspace could tell the two orders apart. The IRI shape differs from this repo's
/// fixtures — a different host, and a dotted class path rather than an instance path — so this also
/// pins that local-name extraction survives a foreign generator's naming.
#[test]
fn the_reference_generators_own_port_order_binds_correctly() {
    let order = order_of(
        &[
            "http://data.ashrae.org/S231#Buildings.Controls.OBC.CDL.Reals.PID.u_m",
            "http://data.ashrae.org/S231#Buildings.Controls.OBC.CDL.Reals.PID.u_s",
        ],
        &["u_s", "u_m"],
    );
    assert_eq!(
        order,
        vec![1, 0],
        "signature position 0 is u_s, which modelica-json lists second"
    );
}
