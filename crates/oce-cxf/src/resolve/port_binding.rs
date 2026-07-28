//! Agreement between a document's declared block interface and the resolved class signature:
//! arity, and then port identity.
//!
//! # Why identity is not the same question as order
//!
//! Port position comes from `hasInput`/`hasOutput` array order, so whoever wrote the document
//! decided which connector lands at which index. Two guards sit downstream and neither can see a
//! wrong decision: [`check_and_bind`] compares counts, and `oce-validate`'s `check_ports_dir` compares
//! each position's kind. A transposition between two ports of the **same kind** passes both and
//! computes a different answer — `CDL.Reals.PID` with `u_s` and `u_m` exchanged inverts the control
//! action, and `CDL.Logical.Latch` with `u` and `clr` exchanged inverts the latch.
//!
//! That is not a hypothetical about malformed input. The reference CDL toolchain, `modelica-json`,
//! renders a class's connectors sorted **alphabetically by label** rather than in declaration
//! order. Its `S231:hasInput` array for `CDL.Reals.PID` — the very key this resolver reads — lists
//! `u_m` before `u_s`, while `Reals/PID.mo` declares `u_s` first. Nine registered class sides
//! reorder under an alphabetical sort without changing their kind sequence, so the kind guard is
//! blind to exactly the documents a conforming generator produces: eight on the input side, and
//! `Integers.Change` on the output side, whose `y, up, down` are all `Boolean` and render as
//! `down, up, y` — a rising edge reported on the falling-edge wire.
//!
//! So the answer is to **bind**, not to reject. Ordering is a renderer's choice; identity is not.
//! When a document names its ports the way the class does, the ports are permuted into signature
//! order and the model is wired correctly. Rejecting instead would refuse valid input.
//!
//! # When binding does not apply
//!
//! [`match_names`] answers [`Binding::Positional`] whenever no port IRI names a declared port, and
//! that is the common, correct case rather than a failure:
//!
//! - this engine's own exporter mints `…​.in0` / `…​.out0`, which name positions, not ports;
//! - 29 registered classes declare no names at all (`oce_blocks::port_names`), width-driven classes
//!   among them, because a flattened `u[nin]` has no 1:1 correspondence to record;
//! - hand-built models never went through a document.
//!
//! A *partial* match is the one shape that is neither: a document that names most of a class's
//! ports and then does not is following no convention this resolver can safely read, so it earns a
//! [`DiagCode::PortNameMismatch`] rather than a guess.

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockInstance, ConnectorId};

use super::local_name;

/// How a document's port array corresponds to its class's declared ports.
pub(super) enum Binding {
    /// No port IRI names a declared port. Bind by array position, as before.
    Positional,
    /// `order[i]` is the document-array index of the port belonging at signature position `i`.
    /// Identity when the document already listed ports in declaration order.
    Permuted(Vec<usize>),
    /// Some ports name declared ports and some do not, so neither reading is safe.
    Partial {
        /// How many document ports named a declared port.
        matched: usize,
        /// How many ports the class declares on this side.
        total: usize,
    },
}

/// Match one side's port IRIs against the class's declared port names for that side.
///
/// Identity comes from the IRI's local name — the segment after the last `.`, the same rule
/// `docs/cxf-composite-subset.md` already uses for parameter binding references. A document whose
/// IRIs carry no such segment simply matches nothing and binds positionally, which is why an
/// unfamiliar IRI shape degrades to today's behavior instead of erroring.
///
/// Returns [`Binding::Positional`] when the two sides disagree on count: arity is
/// [`check_and_bind`]'s diagnostic to emit, and a second one for the same defect would be noise.
pub(super) fn match_names(port_iris: &[&str], names: &[&str]) -> Binding {
    if port_iris.len() != names.len() {
        return Binding::Positional;
    }
    let leaves: Vec<&str> = port_iris.iter().map(|iri| local_name(iri)).collect();
    let mut order = Vec::with_capacity(names.len());
    let mut taken = vec![false; leaves.len()];
    let mut matched = 0usize;
    for name in names {
        // Duplicate leaves are consumed one apiece rather than both resolving to the first, so a
        // document with two ports of the same local name can never bind two signature positions to
        // one connector; the leftover falls through as unmatched and the side reports Partial.
        let found = leaves
            .iter()
            .enumerate()
            .find(|(k, leaf)| *leaf == name && !taken[*k])
            .map(|(k, _)| k);
        match found {
            Some(k) => {
                taken[k] = true;
                order.push(k);
                matched += 1;
            }
            None => order.push(usize::MAX),
        }
    }
    match matched {
        0 => Binding::Positional,
        n if n == names.len() => Binding::Permuted(order),
        n => Binding::Partial {
            matched: n,
            total: names.len(),
        },
    }
}

/// Reorder `ports` so position `i` holds the connector named by signature position `i`.
///
/// `order` must be a total permutation of `ports`' indices — [`Binding::Permuted`] is the only
/// thing that produces one, and it does so only when every signature name matched a distinct port.
pub(super) fn apply(order: &[usize], ports: &mut [ConnectorId]) {
    let source: Vec<ConnectorId> = ports.to_vec();
    for (i, &k) in order.iter().enumerate() {
        ports[i] = source[k];
    }
}

/// Check one instance's declared interface against its resolved class signature, then bind its
/// ports.
///
/// Arity is checked first and a disagreement stops there: the engine emits by port index, so a
/// document declaring fewer ports than the class resolves to would later index past
/// `inputs`/`outputs` and **panic** on the tick, and binding a known-wrong interface by name would
/// only add noise. Connector `value_type` against `PortKind` is the deeper §7.10 check and belongs
/// to `oce-validate`.
pub(super) fn check_and_bind(
    block: &mut BlockInstance,
    class_path: &str,
    subject: &str,
    want: (usize, usize),
    port_iris: (&[&str], &[&str]),
    diags: &mut Vec<Diagnostic>,
) {
    let got = (block.inputs.len(), block.outputs.len());
    if got != want {
        let ((got_in, got_out), (want_in, want_out)) = (got, want);
        diags.push(
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "block interface mismatch for `{class_path}`: declared {got_in} \
                     input(s)/{got_out} output(s), class requires {want_in}/{want_out}"
                ),
            )
            .with_subject(subject.to_owned()),
        );
        return;
    }
    let Some(named) = oce_blocks::port_names::port_names(class_path) else {
        return;
    };
    for (iris, names, ports, side) in [
        (port_iris.0, named.inputs, &mut block.inputs, "input"),
        (port_iris.1, named.outputs, &mut block.outputs, "output"),
    ] {
        match match_names(iris, names) {
            Binding::Positional => {}
            Binding::Permuted(order) => apply(&order, ports),
            Binding::Partial { matched, total } => diags.push(
                Diagnostic::error(
                    DiagCode::PortNameMismatch,
                    format!(
                        "block `{class_path}`: {matched} of {total} {side} port(s) name a declared \
                         port of the class and the rest do not, so the port list can be read \
                         neither by name nor by position"
                    ),
                )
                .with_subject(subject.to_owned()),
            ),
        }
    }
}
