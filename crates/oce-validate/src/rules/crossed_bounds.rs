//! Crossed-bound refusal (`min > max`) for connectors and boundary declarations (issue #378).
//!
//! A bound pair whose `min` exceeds its `max` admits no value. Before this rule a boundary input
//! carrying one loaded and then refused every frame with `InputDomain`, so the malformed
//! declaration surfaced only at run time and without its cause. It is now refused at load as
//! [`DiagCode::CrossedBounds`], in the same pass as the non-finite bound rule.
//!
//! - **Equal bounds stay legal.** `min == max` admits exactly one value. The comparison is IEEE
//!   `>`, so `min = +0.0, max = -0.0` is equal (and admits zero of either sign), not crossed.
//! - **Non-finite bounds are skipped.** They are [`DiagCode::NonFiniteBound`]'s to report, so a
//!   `min = +inf` is refused once, as non-finite, never also as crossed.
//! - **Integer bounds compare their effective values.** An undeclared Integer bound means the CDL
//!   Integer limit ([`oce_model::IntAttrs::min`] / [`oce_model::IntAttrs::max`]), which the frame
//!   applies too; a declared `min` above `2_147_483_647` therefore crosses the default `max`.
//! - **A range that §7.10 propagation crosses is refused once per connected cluster.** After
//!   unification every member of a cluster carries the same agreed bounds, so one member's `min`
//!   and another's `max` (neither crossed alone) can leave all of them empty. Such a cluster is
//!   reported once, on its source output, with the propagated range — a member that never declared
//!   one side is not blamed for a "declared" range. Internal connectors used to load and run with
//!   it, because the frame applies bounds to boundary inputs only.
//! - **A boundary input's combined domain is checked as well.** A host-supplied input value must
//!   satisfy the boundary declaration and every child connector it drives (the elided targets that
//!   carry the boundary's path), so a declaration `min` above a target's `max` also empties the
//!   domain. That is reported once, on the boundary path, when no single member is crossed itself.

use std::collections::BTreeMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, Dir, ModelGraph};

use super::subject_of;

/// The effective bounds of one attribute set, with whether each side was declared.
#[derive(Clone, Copy)]
enum Bounds {
    /// Real bounds; `None` = unbounded on that side. Only finite values are kept.
    Real { min: Option<f64>, max: Option<f64> },
    /// Integer bounds after the CDL defaults, plus whether each side was declared.
    Integer {
        min: i64,
        max: i64,
        min_declared: bool,
        max_declared: bool,
    },
}

impl Bounds {
    /// Read the bounds of `attrs`, or `None` for an attribute set that carries none (Boolean,
    /// String, Enum) or holds a non-finite Real bound (reported by the non-finite rule instead).
    fn of(attrs: &Attrs) -> Option<Self> {
        match attrs {
            Attrs::Real(real) => {
                let finite = |bound: Option<f64>| bound.is_none_or(f64::is_finite);
                (finite(real.min) && finite(real.max)).then_some(Bounds::Real {
                    min: real.min,
                    max: real.max,
                })
            }
            Attrs::Integer(int) => Some(Bounds::Integer {
                min: int.min(),
                max: int.max(),
                min_declared: int.min.is_some(),
                max_declared: int.max.is_some(),
            }),
            Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => None,
        }
    }

    /// Whether these bounds admit no value.
    fn crossed(self) -> bool {
        match self {
            Bounds::Real {
                min: Some(min),
                max: Some(max),
            } => min > max,
            Bounds::Real { .. } => false,
            Bounds::Integer { min, max, .. } => min > max,
        }
    }

    /// The intersection of two bound sets of the same type; `None` when the types differ (a
    /// type mismatch is another rule's to report).
    fn intersect(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Bounds::Real { min: a, max: b }, Bounds::Real { min: c, max: d }) => {
                Some(Bounds::Real {
                    min: tighter(a, c, f64::max),
                    max: tighter(b, d, f64::min),
                })
            }
            (
                Bounds::Integer {
                    min: a,
                    max: b,
                    min_declared: e,
                    max_declared: f,
                },
                Bounds::Integer {
                    min: c,
                    max: d,
                    min_declared: g,
                    max_declared: h,
                },
            ) => Some(Bounds::Integer {
                min: a.max(c),
                max: b.min(d),
                min_declared: e || g,
                max_declared: f || h,
            }),
            _ => None,
        }
    }

    /// Render the pair for a diagnostic message, naming a defaulted Integer side.
    fn describe(self) -> (String, String, &'static str) {
        match self {
            Bounds::Real { min, max } => (
                min.map_or_else(String::new, |v| v.to_string()),
                max.map_or_else(String::new, |v| v.to_string()),
                "Real",
            ),
            Bounds::Integer {
                min,
                max,
                min_declared,
                max_declared,
            } => {
                let side = |value: i64, declared: bool| {
                    if declared {
                        value.to_string()
                    } else {
                        format!("{value}, the CDL Integer default")
                    }
                };
                (side(min, min_declared), side(max, max_declared), "Integer")
            }
        }
    }
}

/// The tighter of two optional finite Real bounds, under `pick` (`f64::max` for a lower bound,
/// `f64::min` for an upper one). Both operands are finite, so `pick` never sees a NaN.
fn tighter(a: Option<f64>, b: Option<f64>, pick: fn(f64, f64) -> f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(pick(a, b)),
        (one, None) | (None, one) => one,
    }
}

/// Report every attribute set whose `min` exceeds its `max` — on a connector, a root
/// boundary-input declaration or a root boundary-output declaration — and every boundary input
/// whose combined domain (declaration plus driven child connectors) is empty, as
/// [`DiagCode::CrossedBounds`] errors.
///
/// One diagnostic per crossed attribute set, subject = the connector's or declaration's authored
/// identity; a combined-domain crossing adds one more on the boundary path only when none of its
/// members was already reported. Reads attributes only; total and panic-free on any graph.
pub(crate) fn check_crossed_bounds(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    for connector in &model.connectors {
        report(&connector.attrs, "connector", &subject_of(connector), diags);
    }
    for input in &model.boundary_inputs {
        report(&input.attrs, "boundary input", &input.iri, diags);
    }
    for output in &model.boundary_outputs {
        report(&output.attrs, "boundary output", &output.iri, diags);
    }
    check_combined_input_domains(model, diags);
}

fn report(attrs: &Attrs, owner: &str, subject: &Arc<str>, diags: &mut Vec<Diagnostic>) {
    let Some(bounds) = Bounds::of(attrs).filter(|bounds| bounds.crossed()) else {
        return;
    };
    let (min, max, kind) = bounds.describe();
    diags.push(
        Diagnostic::error(
            DiagCode::CrossedBounds,
            format!(
                "S231:min ({min}) exceeds S231:max ({max}) on {article} {kind} {owner}; the \
                 declared range admits no value",
                article = if kind == "Integer" { "an" } else { "a" },
            ),
        )
        .with_subject(Arc::clone(subject)),
    );
}

/// The combined-domain check: group every input connector by its authored path (an elided
/// boundary target carries the boundary's IRI), keep the groups that a host supplies (an external
/// input among the members), fold in the matching boundary declaration, and refuse an empty
/// intersection. Grouping is by `BTreeMap`, so the emission order is the path order.
fn check_combined_input_domains(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let mut groups: BTreeMap<&str, (bool, Vec<&Attrs>)> = BTreeMap::new();
    for connector in &model.connectors {
        let (Dir::In, Some(iri)) = (connector.dir, connector.iri.as_deref()) else {
            continue;
        };
        let entry = groups.entry(iri).or_default();
        entry.0 |= model.external_inputs.contains(&connector.id);
        entry.1.push(&connector.attrs);
    }
    for input in &model.boundary_inputs {
        if let Some(entry) = groups.get_mut(input.iri.as_ref()) {
            entry.1.push(&input.attrs);
        }
    }
    for (path, (external, members)) in groups {
        if !external || members.len() < 2 {
            continue;
        }
        // A member that is crossed itself is already reported; one carrying a non-finite bound,
        // no bounds, or a mismatched type leaves the combination to other rules.
        let combined = members
            .into_iter()
            .try_fold(None, |acc: Option<Bounds>, attrs| {
                let bounds = Bounds::of(attrs).filter(|bounds| !bounds.crossed())?;
                match acc {
                    None => Some(Some(bounds)),
                    Some(acc) => acc.intersect(bounds).map(Some),
                }
            });
        let Some(bounds) = combined.flatten().filter(|bounds| bounds.crossed()) else {
            continue;
        };
        let (min, max, kind) = bounds.describe();
        diags.push(
            Diagnostic::error(
                DiagCode::CrossedBounds,
                format!(
                    "the combined S231:min ({min}) exceeds the combined S231:max ({max}) across \
                     the {kind} boundary input and the child inputs it drives; no input value \
                     satisfies them all"
                ),
            )
            .with_subject(Arc::from(path)),
        );
    }
}

/// Report a connected cluster whose bounds, after §7.10 propagation, admit no value — one
/// [`DiagCode::CrossedBounds`] error per cluster, subject = the cluster's source output.
///
/// Called by unification only after the declared-attribute check found nothing, so any crossing
/// here was created by propagation. Every member then carries the same agreed bounds (propagation
/// fills unset sides from the one agreed value), so the source output's attributes stand for the
/// whole cluster. Total and panic-free: an out-of-range `root` reports nothing.
pub(crate) fn check_propagated_cluster(
    model: &ModelGraph,
    root: u32,
    connectors: usize,
    boundary_outputs: usize,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(source) = model.connectors.get(root as usize) else {
        return;
    };
    let Some(bounds) = Bounds::of(&source.attrs).filter(|bounds| bounds.crossed()) else {
        return;
    };
    let (min, max, kind) = bounds.describe();
    let members = match connectors {
        1 => "1 connector".to_owned(),
        n => format!("{n} connectors"),
    };
    let outputs = match boundary_outputs {
        0 => String::new(),
        1 => " and 1 boundary output".to_owned(),
        n => format!(" and {n} boundary outputs"),
    };
    diags.push(
        Diagnostic::error(
            DiagCode::CrossedBounds,
            format!(
                "§7.10 propagation across the connected {kind} cluster driven by this output \
                 ({members}{outputs}) gives the effective range S231:min ({min}) \
                 above S231:max ({max}), which admits no value; no single member declares both"
            ),
        )
        .with_subject(subject_of(source)),
    );
}
