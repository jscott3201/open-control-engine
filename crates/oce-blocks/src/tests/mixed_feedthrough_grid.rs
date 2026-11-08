//! Parameter grid and probes for the mixed-feedthrough census (`mixed_feedthrough_census`).
//!
//! A block's feedthrough map is *mixed* when some output does not depend on some input the block
//! reads when it fires. "Reads when it fires" mirrors `oce-graph`'s emit-before rule: an `[A]`
//! block reads every input at emit time; an `[S]` block reads only the inputs that feed through to
//! some output (a fully cut input is read only by the deferred state update).
//!
//! # What is swept exhaustively
//!
//! Each registry entry is built from the empty parameter table (registry defaults) and from every
//! point of a grid derived from the class's own parameter rules and defaults:
//!
//! - every `ParamRule::Structural` integer width over `0..=3`, dropping values below the class's
//!   `IntegerGreaterOrEqual` bound;
//! - every Boolean parameter (from `ParamRule::Boolean`, a structural Boolean, or a Boolean
//!   literal default) over `{false, true}`;
//! - every enumeration parameter over every member: members come from a `ParamRule::EnumMembers`
//!   rule, or, for an enum-literal default without one, from [`KNOWN_ENUM_DOMAINS`];
//! - every `ParamRule::StructuralArrayElements` array for the swept widths: selector arrays over
//!   every in-range value vector, Boolean masks over every vector whose true count matches a
//!   `BooleanArrayTrueCountEquals` rule.
//!
//! A structural parameter, structural array or enumeration whose domain the census cannot derive
//! panics instead of being skipped.
//!
//! # What is probed, not swept
//!
//! Every other scalar parameter with a registry default (Real, Integer, derived or required) is
//! *probed*: at every grid point it is set alone to each value in [`REAL_PROBES`] or
//! [`INTEGER_PROBES`]. Every probe observation joins the census, so a probe that makes a class
//! mixed is caught by the class-set check. The census also asserts that no probe changes the mixed
//! pairs, and that the parameters whose probes change `kind()` are exactly a pinned, reviewed list
//! (comparator hysteresis `h`, which makes the comparators stateful). This is a sampled check, not
//! a proof: a parameter that changed the map only outside the probe values would be missed.
//!
//! # What is not covered
//!
//! String-literal defaults, non-structural array parameters (gains, tables, `base_<i>` members
//! other than structural selectors and masks) and widths above 3 are not varied. Widths stop at 3
//! because every listed class's map is a closed-form function of its widths whose shape is settled
//! once a width reaches 2; the census checks each listed class's documented trigger condition at
//! every grid point to pin that shape.

use std::collections::BTreeSet;

use oce_model::EnumClassId;

use super::common::*;
use crate::{DefaultLiteral, DefaultSource, ParamRule, RegistryEntry, registry};

/// Largest structural width the grid sweeps.
pub(super) const MAX_SWEPT_WIDTH: i64 = 3;

/// Member counts of enumeration parameters whose registry entry carries an enum-literal default
/// but no `ParamRule::EnumMembers` rule. `controllerType` is `CDL.Types.SimpleController`
/// `{P, PI, PD, PID}`, read by ordinal 1..=4. An enum default not listed here and without an
/// `EnumMembers` rule fails the census.
pub(super) const KNOWN_ENUM_DOMAINS: &[(&str, u32)] = &[("controllerType", 4)];

/// Values each probed Real (or derived / required) parameter takes, one at a time.
pub(super) const REAL_PROBES: &[f64] = &[-1.0, 0.0, 0.5, 1.0, 1.0e6];

/// Values each probed Integer parameter takes, one at a time.
pub(super) const INTEGER_PROBES: &[i64] = &[-1, 0, 1, 2, 3];

/// One parameter assignment list.
pub(super) type Point = Vec<(String, Value)>;

/// The `(input, output)` pairs that make `block` mixed: the input is read when the block fires,
/// and the output does not depend on it. Empty means the block can always emit all its outputs
/// at one position in the schedule.
pub(super) fn non_feedthrough_reads(block: &dyn Block) -> Vec<(usize, usize)> {
    let signature = block.resolved_signature();
    let (n_in, n_out) = (signature.inputs.len(), signature.outputs.len());
    drop(signature);
    let reads = |i: usize| {
        block.kind() == BlockKind::Algebraic || (0..n_out).any(|j| block.feeds_through(i, j))
    };
    (0..n_in)
        .filter(|&i| reads(i))
        .flat_map(|i| (0..n_out).map(move |j| (i, j)))
        .filter(|&(i, j)| !block.feeds_through(i, j))
        .collect()
}

/// One sweepable dimension of a class's parameter space.
#[derive(Debug)]
pub(super) enum Axis {
    /// An integer width with its inclusive lower bound.
    Width { name: &'static str, min: i64 },
    /// A Boolean flag.
    Flag { name: &'static str },
    /// An enumeration, swept by ordinal `1..=members`.
    Enum { name: &'static str, members: u32 },
    /// `base_1..=base_len` selectors, each in `min..=value(max)`.
    Selector {
        base: &'static str,
        len: &'static str,
        min: i64,
        max: &'static str,
    },
    /// `base_1..=base_len` Boolean mask, optionally with `count` true members.
    Mask {
        base: &'static str,
        len: &'static str,
        count: Option<&'static str>,
    },
}

fn integer_width(rules: &[ParamRule], name: &str) -> bool {
    rules.iter().any(|rule| {
        matches!(rule, ParamRule::IntegerGreaterOrEqual { name: n, .. } if *n == name)
            || matches!(
                rule,
                ParamRule::Required { name: n, kind: oce_model::ValueType::Integer } if *n == name
            )
    })
}

fn array_axis(entry: &RegistryEntry, base: &'static str) -> Axis {
    let rules = entry.param_rules();
    let selector = rules.iter().find_map(|rule| match *rule {
        ParamRule::IntegerArrayElementsInRange {
            base: b,
            len,
            min,
            max,
            ..
        } if b == base => Some(Axis::Selector {
            base,
            len,
            min,
            max,
        }),
        _ => None,
    });
    let mask = rules.iter().find_map(|rule| match *rule {
        ParamRule::BooleanArrayElements { base: b, len } if b == base => {
            let count = rules.iter().find_map(|rule| match *rule {
                ParamRule::BooleanArrayTrueCountEquals { base: b, count, .. } if b == base => {
                    Some(count)
                }
                _ => None,
            });
            Some(Axis::Mask { base, len, count })
        }
        _ => None,
    });
    selector.or(mask).unwrap_or_else(|| {
        panic!(
            "{}: structural array {base:?} is neither a ranged selector nor a Boolean mask; \
             extend the mixed-feedthrough census grid",
            entry.class_path
        )
    })
}

/// Derive the sweep axes for `entry` from its parameter rules and defaults. Panics on a structural
/// or enumeration parameter whose domain cannot be derived, so it is never silently skipped.
pub(super) fn axes(entry: &RegistryEntry) -> Vec<Axis> {
    let rules = entry.param_rules();
    let mut scalars: Vec<Axis> = Vec::new();
    let mut arrays: Vec<Axis> = Vec::new();
    let mut flags: BTreeSet<&'static str> = BTreeSet::new();
    let mut enums: Vec<(&'static str, u32)> = Vec::new();
    for rule in rules {
        match *rule {
            ParamRule::Structural { name }
                if rules
                    .iter()
                    .any(|r| matches!(r, ParamRule::Boolean { name: n } if *n == name)) =>
            {
                flags.insert(name);
            }
            ParamRule::Structural { name } => {
                assert!(
                    integer_width(rules, name),
                    "{}: structural parameter {name:?} is neither Boolean nor an integer width; \
                     extend the mixed-feedthrough census grid",
                    entry.class_path
                );
                let min = rules
                    .iter()
                    .filter_map(|rule| match *rule {
                        ParamRule::IntegerGreaterOrEqual { name: n, min } if n == name => Some(min),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0)
                    .max(0);
                scalars.push(Axis::Width { name, min });
            }
            ParamRule::StructuralArrayElements { base } => arrays.push(array_axis(entry, base)),
            ParamRule::Boolean { name } => {
                flags.insert(name);
            }
            ParamRule::EnumMembers { name, members } => {
                enums.push((name, u32::try_from(members.len()).expect("small enum")));
            }
            _ => {}
        }
    }
    for default in registry::param_defaults(entry.class_path) {
        // `base_<i>` defaults describe array members, never a scalar axis.
        if default.name.contains('<') {
            continue;
        }
        match default.default {
            DefaultSource::Literal(DefaultLiteral::Boolean(_)) => {
                flags.insert(default.name);
            }
            DefaultSource::Literal(DefaultLiteral::EnumMember(_))
                if !enums.iter().any(|(n, _)| *n == default.name) =>
            {
                let members = KNOWN_ENUM_DOMAINS
                    .iter()
                    .find_map(|(n, members)| (*n == default.name).then_some(*members))
                    .unwrap_or_else(|| {
                        panic!(
                            "{}: enumeration {:?} has no EnumMembers rule and no known domain; \
                             add it to KNOWN_ENUM_DOMAINS",
                            entry.class_path, default.name
                        )
                    });
                enums.push((default.name, members));
            }
            _ => {}
        }
    }
    scalars.extend(flags.into_iter().map(|name| Axis::Flag { name }));
    scalars.extend(
        enums
            .into_iter()
            .map(|(name, members)| Axis::Enum { name, members }),
    );
    scalars.extend(arrays);
    scalars
}

/// The scalar parameters the grid does not sweep, each with the probe values it takes.
pub(super) fn probes(entry: &RegistryEntry) -> Vec<(&'static str, Vec<Value>)> {
    let swept: BTreeSet<&str> = axes(entry)
        .iter()
        .filter_map(|axis| match *axis {
            Axis::Width { name, .. } | Axis::Flag { name } | Axis::Enum { name, .. } => Some(name),
            _ => None,
        })
        .collect();
    let rules = entry.param_rules();
    registry::param_defaults(entry.class_path)
        .iter()
        .filter(|d| !d.name.contains('<') && !swept.contains(d.name))
        .filter_map(|d| {
            let integer = matches!(
                d.default,
                DefaultSource::Literal(DefaultLiteral::Integer(_))
            ) || integer_width(rules, d.name);
            let values: Vec<Value> = match d.default {
                // String literals have no ordered domain to sample; documented as not covered.
                DefaultSource::Literal(DefaultLiteral::Str(_)) => return None,
                _ if integer => INTEGER_PROBES.iter().map(|&v| Value::Integer(v)).collect(),
                _ => REAL_PROBES.iter().map(|&v| Value::Real(v)).collect(),
            };
            Some((d.name, values))
        })
        .collect()
}

fn int_of(point: &[(String, Value)], name: &str) -> i64 {
    point
        .iter()
        .find_map(|(n, v)| match v {
            Value::Integer(value) if n == name => Some(*value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("array axis reads unswept width {name:?}"))
}

fn selector_vectors(len: i64, min: i64, max: i64) -> Vec<Vec<i64>> {
    let mut vectors: Vec<Vec<i64>> = vec![Vec::new()];
    for _ in 0..len {
        vectors = vectors
            .into_iter()
            .flat_map(|v| {
                (min..=max).map(move |value| {
                    let mut v = v.clone();
                    v.push(value);
                    v
                })
            })
            .collect();
    }
    vectors
}

/// The values `axis` takes at `point`, each as the parameters it adds.
fn extensions(axis: &Axis, point: &[(String, Value)]) -> Vec<Point> {
    match *axis {
        Axis::Width { name, min } => (min..=MAX_SWEPT_WIDTH)
            .map(|w| vec![(name.to_owned(), Value::Integer(w))])
            .collect(),
        Axis::Flag { name } => [false, true]
            .map(|b| vec![(name.to_owned(), Value::Boolean(b))])
            .to_vec(),
        Axis::Enum { name, members } => (1..=members)
            .map(|ordinal| vec![(name.to_owned(), enum_value(ordinal))])
            .collect(),
        Axis::Selector {
            base,
            len,
            min,
            max,
        } => selector_vectors(int_of(point, len), min, int_of(point, max))
            .into_iter()
            .map(|v| {
                v.into_iter()
                    .enumerate()
                    .map(|(k, value)| (format!("{base}_{}", k + 1), Value::Integer(value)))
                    .collect()
            })
            .collect(),
        Axis::Mask { base, len, count } => {
            let len = int_of(point, len) as u32;
            let count = count.map(|name| int_of(point, name) as u32);
            (0u32..(1 << len))
                .filter(|bits| count.is_none_or(|c| bits.count_ones() == c))
                .map(|bits| {
                    (0..len)
                        .map(|k| {
                            (
                                format!("{base}_{}", k + 1),
                                Value::Boolean(bits >> k & 1 == 1),
                            )
                        })
                        .collect()
                })
                .collect()
        }
    }
}

/// An enumeration value by 1-based source ordinal, as the resolver grounds it.
pub(super) fn enum_value(ordinal: u32) -> Value {
    Value::Enum {
        class: EnumClassId(0),
        ordinal,
    }
}

/// Every grid point of `axes`. Scalars are expanded before arrays, so an array's length and bound
/// read the width already chosen at that point.
pub(super) fn grid(axes: &[Axis]) -> Vec<Point> {
    let mut points: Vec<Point> = vec![Vec::new()];
    for axis in axes {
        points = points
            .iter()
            .flat_map(|point| {
                extensions(axis, point).into_iter().map(move |extension| {
                    let mut grown = point.clone();
                    grown.extend(extension);
                    grown
                })
            })
            .collect();
    }
    points
}

pub(super) fn table(point: &[(String, Value)]) -> ParamTable {
    ParamTable {
        values: point
            .iter()
            .map(|(name, value)| (Arc::from(name.as_str()), value.clone()))
            .collect(),
    }
}

/// One built instance in the census.
pub(super) struct Observation {
    pub(super) class_path: &'static str,
    /// The grid point (empty for the registry defaults).
    pub(super) point: Point,
    /// The single probed parameter added on top of `point`, if any.
    pub(super) probe: Option<(&'static str, Value)>,
    pub(super) kind: BlockKind,
    pub(super) mixed: Vec<(usize, usize)>,
}

/// Build every entry at its default table and at every grid point, and at every grid point again
/// with each probed parameter set alone to each of its probe values.
pub(super) fn census<'a>(entries: impl Iterator<Item = &'a RegistryEntry>) -> Vec<Observation> {
    let mut observations = Vec::new();
    for entry in entries {
        let probes = probes(entry);
        let mut points = vec![Vec::new()];
        points.extend(grid(&axes(entry)));
        for point in points {
            let mut observe = |probe: Option<(&'static str, Value)>| {
                let mut full = point.clone();
                full.extend(probe.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
                let block = (entry.make)(&table(&full));
                observations.push(Observation {
                    class_path: entry.class_path,
                    point: point.clone(),
                    probe,
                    kind: block.kind(),
                    mixed: non_feedthrough_reads(block.as_ref()),
                });
            };
            observe(None);
            for (name, values) in &probes {
                for value in values {
                    observe(Some((name, value.clone())));
                }
            }
        }
    }
    observations
}

pub(super) fn mixed_classes(observations: &[Observation]) -> BTreeSet<&'static str> {
    observations
        .iter()
        .filter(|o| !o.mixed.is_empty())
        .map(|o| o.class_path)
        .collect()
}
