//! Census of the registered classes whose feedthrough map is *mixed*: some output does not depend
//! on some input the block reads when it fires.
//!
//! Atomic block firing (one `emit` call produces every output of a block) cannot schedule feedback
//! from such an output into such an input, even when the model is CDL-legal: `oce-graph` refuses
//! it with `BuildError::BlockAlgebraicLoop` (GitHub issue #347). `docs/cdl-coverage.md` lists the
//! classes that can trigger that refusal. This census pins the list mechanically over the grid and
//! probes described in `mixed_feedthrough_grid`, so a block added later with the same shape over
//! that grid fails here until the documentation names it.
//!
//! What the census guarantees is bounded by that grid: exhaustive over structural widths `0..=3`,
//! Booleans, enumerations and structural selector/mask arrays; sampled (by probe values) over the
//! other scalar parameters; and silent on string and non-structural array parameters.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use super::common::*;
use super::mixed_feedthrough_grid::{
    Observation, axes, census, grid, mixed_classes, non_feedthrough_reads, probes, table,
};
use crate::{BlockSignature, RegistryEntry, registry};

/// Exactly the registered classes with mixed feedthrough for some parameterization, sorted.
/// `docs/cdl-coverage.md` ("Atomic block firing refuses some CDL-legal feedback") must name the
/// same set.
const MIXED_FEEDTHROUGH_CLASSES: &[&str] = &[
    "CDL.Reals.MatrixMax",
    "CDL.Reals.MatrixMin",
    "CDL.Reals.PIDWithReset",
    "CDL.Routing.BooleanExtractSignal",
    "CDL.Routing.BooleanVectorFilter",
    "CDL.Routing.BooleanVectorReplicator",
    "CDL.Routing.IntegerExtractSignal",
    "CDL.Routing.IntegerVectorFilter",
    "CDL.Routing.IntegerVectorReplicator",
    "CDL.Routing.RealExtractSignal",
    "CDL.Routing.RealVectorFilter",
    "CDL.Routing.RealVectorReplicator",
];

/// The census over the whole registry, computed once for every test in this module.
fn registry_census() -> &'static [Observation] {
    static CENSUS: OnceLock<Vec<Observation>> = OnceLock::new();
    CENSUS.get_or_init(|| census(registry::all_entries()))
}

fn int_param(point: &[(String, Value)], name: &str, default: i64) -> i64 {
    point
        .iter()
        .find_map(|(n, v)| match v {
            Value::Integer(value) if n == name => Some(*value),
            _ => None,
        })
        .unwrap_or(default)
}

fn bool_param(point: &[(String, Value)], name: &str, default: bool) -> bool {
    point
        .iter()
        .find_map(|(n, v)| match v {
            Value::Boolean(value) if n == name => Some(*value),
            _ => None,
        })
        .unwrap_or(default)
}

fn enum_ordinal(point: &[(String, Value)], name: &str, default: u32) -> u32 {
    point
        .iter()
        .find_map(|(n, v)| match v {
            Value::Enum { ordinal, .. } if n == name => Some(*ordinal),
            _ => None,
        })
        .unwrap_or(default)
}

/// The documented triggering condition for each listed class, as a function of its parameters.
/// Defaults are the registry fallbacks used when the empty table builds the class.
fn documented_mixed(class_path: &str, point: &[(String, Value)], defaults: (i64, i64)) -> bool {
    let (default_a, default_b) = defaults;
    if class_path == "CDL.Reals.PIDWithReset" {
        // In P mode the controller holds no state, so the block is [A] and reads `trigger`, which
        // feeds no output. Every other mode is [S] and `trigger` is a cut (update-only) input.
        // Ordinal 1 is `SimpleController.P`; the registry default is `PI` (ordinal 2).
        return enum_ordinal(point, "controllerType", 2) == 1;
    }
    if let Some(row_flag) = match class_path {
        "CDL.Reals.MatrixMax" => Some("rowMax"),
        "CDL.Reals.MatrixMin" => Some("rowMin"),
        _ => None,
    } {
        // One output per row (rowMax/rowMin = true) or per column; each output reduces only its
        // own row or column, so the map is mixed exactly when there are two or more outputs.
        let (n_row, n_col) = (
            int_param(point, "nRow", default_a),
            int_param(point, "nCol", default_b),
        );
        let outputs = if bool_param(point, row_flag, true) {
            n_row
        } else {
            n_col
        };
        return n_row >= 1 && n_col >= 1 && outputs >= 2;
    }
    // Routing: each output reads exactly one input, so the map is mixed exactly when there is an
    // output and more than one input.
    int_param(point, "nin", default_a) >= 2 && int_param(point, "nout", default_b) >= 1
}

/// Default `(first, second)` widths of each listed width-driven class when the empty table builds
/// it, read from the built block so the predicate needs no copy of the registry fallbacks.
fn default_widths(class_path: &str) -> (i64, i64) {
    let block = (registry::lookup(class_path).expect("listed class").make)(&ParamTable::default());
    let signature = block.resolved_signature();
    let (n_in, n_out) = (
        signature.inputs.len() as i64,
        signature.outputs.len() as i64,
    );
    match class_path {
        // MatrixMax/Min default to rows: nRow = outputs, nCol = inputs / nRow.
        "CDL.Reals.MatrixMax" | "CDL.Reals.MatrixMin" => {
            (n_out, n_in.checked_div(n_out).unwrap_or(0))
        }
        // VectorReplicator emits nin * nout outputs.
        c if c.ends_with("VectorReplicator") => (n_in, n_out.checked_div(n_in).unwrap_or(0)),
        _ => (n_in, n_out),
    }
}

/// The non-swept `(class, parameter)` pairs whose probe values change `kind()`. A comparator with
/// hysteresis `h > 0` keeps the previous output as state, so it becomes `[S]`. Each probe
/// observation is itself in the census, so both kinds are checked for mixed feedthrough; this list
/// only makes every kind-changing parameter explicit, so a new one fails until it is reviewed.
const KIND_CHANGING_PROBED_PARAMETERS: &[(&str, &str)] = &[
    ("CDL.Reals.Greater", "h"),
    ("CDL.Reals.GreaterThreshold", "h"),
    ("CDL.Reals.Less", "h"),
    ("CDL.Reals.LessThreshold", "h"),
];

/// How a probe changed an observation relative to the same grid point without it.
struct ProbeChange {
    class_path: &'static str,
    parameter: &'static str,
    kind_changed: bool,
    mixed_changed: bool,
    detail: String,
}

/// A grid point's identity (class and rendered parameters) and its unprobed kind and mixed pairs.
type Baseline<'a> = BTreeMap<(&'a str, String), (BlockKind, &'a [(usize, usize)])>;

/// Probe observations whose `kind()` or mixed pairs differ from the same grid point without the
/// probe.
fn probe_changes(observations: &[Observation]) -> Vec<ProbeChange> {
    let mut base: Baseline<'_> = BTreeMap::new();
    for o in observations.iter().filter(|o| o.probe.is_none()) {
        base.insert((o.class_path, format!("{:?}", o.point)), (o.kind, &o.mixed));
    }
    observations
        .iter()
        .filter_map(|o| {
            let (name, value) = o.probe.as_ref()?;
            let (kind, mixed) = base[&(o.class_path, format!("{:?}", o.point))];
            let (kind_changed, mixed_changed) = (kind != o.kind, mixed != o.mixed.as_slice());
            (kind_changed || mixed_changed).then(|| ProbeChange {
                class_path: o.class_path,
                parameter: name,
                kind_changed,
                mixed_changed,
                detail: format!(
                    "{} at {:?}: probing {name} = {value:?} changes kind {kind:?} -> {:?}, \
                     mixed pairs {mixed:?} -> {:?}",
                    o.class_path, o.point, o.kind, o.mixed
                ),
            })
        })
        .collect()
}

#[test]
fn registered_mixed_feedthrough_classes_are_exactly_the_documented_twelve() {
    let observations = registry_census();
    let observed = mixed_classes(observations);
    let expected: BTreeSet<&str> = MIXED_FEEDTHROUGH_CLASSES.iter().copied().collect();
    assert_eq!(
        observed, expected,
        "the set of registered classes with mixed feedthrough changed; update \
         docs/cdl-coverage.md (atomic block firing refuses some CDL-legal feedback) and this list"
    );
    assert_eq!(MIXED_FEEDTHROUGH_CLASSES.len(), 12);
    let registered: BTreeSet<&str> = registry::all_entries().map(|e| e.class_path).collect();
    let built: BTreeSet<&str> = observations.iter().map(|o| o.class_path).collect();
    assert_eq!(
        built, registered,
        "every registered class is built at least once"
    );
}

#[test]
fn probed_parameters_never_change_mixed_pairs_and_kind_changes_are_pinned() {
    let changes = probe_changes(registry_census());
    let mixed: Vec<&str> = changes
        .iter()
        .filter(|c| c.mixed_changed)
        .map(|c| c.detail.as_str())
        .collect();
    assert!(
        mixed.is_empty(),
        "non-swept parameters change the mixed pairs; make them swept axes:\n{}",
        mixed.join("\n")
    );
    let kind_changing: BTreeSet<(&str, &str)> = changes
        .iter()
        .filter(|c| c.kind_changed)
        .map(|c| (c.class_path, c.parameter))
        .collect();
    let expected: BTreeSet<(&str, &str)> =
        KIND_CHANGING_PROBED_PARAMETERS.iter().copied().collect();
    assert_eq!(
        kind_changing, expected,
        "the non-swept parameters that change kind() changed; review each for mixed feedthrough"
    );
}

#[test]
fn every_mixed_feedthrough_instance_is_algebraic() {
    // No stateful registered class has an output that skips an input it reads at emit time, so
    // the refusal today never involves a stateful block's own map. A stateful mixed class would
    // need its own row in the coverage table and its own scheduling analysis.
    for observation in registry_census() {
        if !observation.mixed.is_empty() {
            assert_eq!(
                observation.kind,
                BlockKind::Algebraic,
                "{} {:?} {:?}",
                observation.class_path,
                observation.point,
                observation.probe
            );
        }
    }
}

#[test]
fn documented_trigger_condition_matches_the_oracle_at_every_grid_point() {
    let mut checked = BTreeSet::new();
    for observation in registry_census() {
        if !MIXED_FEEDTHROUGH_CLASSES.contains(&observation.class_path) {
            continue;
        }
        let defaults = default_widths(observation.class_path);
        assert_eq!(
            !observation.mixed.is_empty(),
            documented_mixed(observation.class_path, &observation.point, defaults),
            "{} {:?} {:?}: documented trigger condition disagrees with feeds_through",
            observation.class_path,
            observation.point,
            observation.probe
        );
        checked.insert(observation.class_path);
    }
    assert_eq!(checked.len(), MIXED_FEEDTHROUGH_CLASSES.len());
}

/// One concrete parameterization per listed class, with the exact `(input, output)` pairs whose
/// output does not depend on that input (0-based; `u1` is input 0).
type ConcreteCase = (
    &'static str,
    &'static [(&'static str, Value)],
    &'static [(usize, usize)],
);

#[test]
fn each_listed_class_has_a_concrete_mixed_parameterization() {
    let two_by_two: &'static [(usize, usize)] = &[(0, 1), (1, 0)];
    let cases: &[ConcreteCase] = &[
        (
            "CDL.Reals.MatrixMin",
            &[
                ("nRow", Value::Integer(2)),
                ("nCol", Value::Integer(1)),
                ("rowMin", Value::Boolean(true)),
            ],
            two_by_two,
        ),
        (
            "CDL.Reals.MatrixMax",
            &[
                ("nRow", Value::Integer(1)),
                ("nCol", Value::Integer(2)),
                ("rowMax", Value::Boolean(false)),
            ],
            two_by_two,
        ),
        (
            "CDL.Reals.PIDWithReset",
            // controllerType = P (ordinal 1): inputs u_s, u_m, trigger; trigger feeds no output.
            &[(
                "controllerType",
                Value::Enum {
                    class: oce_model::EnumClassId(0),
                    ordinal: 1,
                },
            )],
            &[(2, 0)],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(1))],
            &[(1, 0)],
        ),
        (
            "CDL.Routing.IntegerExtractSignal",
            &[
                ("nin", Value::Integer(2)),
                ("nout", Value::Integer(2)),
                ("extract_1", Value::Integer(2)),
                ("extract_2", Value::Integer(1)),
            ],
            &[(0, 0), (1, 1)],
        ),
        (
            "CDL.Routing.BooleanExtractSignal",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            two_by_two,
        ),
        (
            "CDL.Routing.RealVectorFilter",
            &[
                ("nin", Value::Integer(2)),
                ("nout", Value::Integer(1)),
                ("msk_1", Value::Boolean(false)),
                ("msk_2", Value::Boolean(true)),
            ],
            &[(0, 0)],
        ),
        (
            "CDL.Routing.IntegerVectorFilter",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            two_by_two,
        ),
        (
            "CDL.Routing.BooleanVectorFilter",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            two_by_two,
        ),
        (
            "CDL.Routing.RealVectorReplicator",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(1))],
            two_by_two,
        ),
        (
            "CDL.Routing.IntegerVectorReplicator",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            &[(0, 1), (0, 3), (1, 0), (1, 2)],
        ),
        (
            "CDL.Routing.BooleanVectorReplicator",
            &[("nin", Value::Integer(2)), ("nout", Value::Integer(1))],
            two_by_two,
        ),
    ];
    let named: BTreeSet<&str> = cases.iter().map(|(class, ..)| *class).collect();
    let listed: BTreeSet<&str> = MIXED_FEEDTHROUGH_CLASSES.iter().copied().collect();
    assert_eq!(named, listed, "one concrete case per listed class");
    for (class_path, params, expected) in cases {
        let point: Vec<(String, Value)> = params
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        let block = (lookup(class_path).expect("registered").make)(&table(&point));
        assert_eq!(block.kind(), BlockKind::Algebraic, "{class_path}");
        assert_eq!(
            non_feedthrough_reads(block.as_ref()),
            *expected,
            "{class_path} {params:?}"
        );
    }
}

#[test]
fn boundary_parameterizations_are_not_mixed() {
    // The boundary side of the documented condition: one input, or one output, is never mixed,
    // and a reset PID in any mode other than P (or a plain PID in P mode) is not mixed.
    let mode = |member: &str| {
        Value::String(Arc::from(format!(
            "Buildings.Controls.OBC.CDL.Types.SimpleController.{member}"
        )))
    };
    let pi = [("controllerType", mode("PI"))];
    let pd = [("controllerType", mode("PD"))];
    let pid = [("controllerType", mode("PID"))];
    let p = [("controllerType", mode("P"))];
    let cases: &[(&str, &[(&str, Value)])] = &[
        ("CDL.Reals.PIDWithReset", &pi),
        ("CDL.Reals.PIDWithReset", &pd),
        ("CDL.Reals.PIDWithReset", &pid),
        ("CDL.Reals.PID", &p),
        ("CDL.Reals.PID", &pi),
        (
            "CDL.Routing.RealExtractSignal",
            &[
                ("nin", Value::Integer(1)),
                ("nout", Value::Integer(3)),
                ("extract_1", Value::Integer(1)),
                ("extract_2", Value::Integer(1)),
                ("extract_3", Value::Integer(1)),
            ],
        ),
        (
            "CDL.Routing.RealVectorFilter",
            &[("nin", Value::Integer(1)), ("nout", Value::Integer(1))],
        ),
        (
            "CDL.Routing.RealVectorReplicator",
            &[("nin", Value::Integer(1)), ("nout", Value::Integer(3))],
        ),
        (
            "CDL.Reals.MatrixMin",
            &[
                ("nRow", Value::Integer(1)),
                ("nCol", Value::Integer(3)),
                ("rowMin", Value::Boolean(true)),
            ],
        ),
        (
            "CDL.Reals.MatrixMax",
            &[
                ("nRow", Value::Integer(3)),
                ("nCol", Value::Integer(1)),
                ("rowMax", Value::Boolean(false)),
            ],
        ),
    ];
    for (class_path, params) in cases {
        let point: Vec<(String, Value)> = params
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        let block = (lookup(class_path).expect("registered").make)(&table(&point));
        assert!(
            non_feedthrough_reads(block.as_ref()).is_empty(),
            "{class_path} {params:?}"
        );
    }
}

// ---- mutation controls: the detector and the census must see a mixed block ----------------

/// A synthetic block with a per-pair feedthrough map, for the census's own controls.
struct Synthetic {
    kind: BlockKind,
    pairs: &'static [(usize, usize)],
}

impl Block for Synthetic {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.MixedFeedthroughControl",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real, PortKind::Real],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        self.kind
    }
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        self.pairs.contains(&(in_idx, out_idx))
    }
}

fn make_mixed_control(_: &ParamTable) -> Box<dyn Block> {
    Box::new(Synthetic {
        kind: BlockKind::Algebraic,
        pairs: &[(0, 0), (1, 1)],
    })
}

#[test]
fn detector_flags_partial_maps_and_spares_uniform_and_cut_maps() {
    let algebraic_split = Synthetic {
        kind: BlockKind::Algebraic,
        pairs: &[(0, 0), (1, 1)],
    };
    assert_eq!(non_feedthrough_reads(&algebraic_split), [(0, 1), (1, 0)]);
    // An [A] input that feeds no output is still read at emit time.
    let algebraic_unused = Synthetic {
        kind: BlockKind::Algebraic,
        pairs: &[(0, 0), (0, 1)],
    };
    assert_eq!(non_feedthrough_reads(&algebraic_unused), [(1, 0), (1, 1)]);
    // A stateful block whose fed-through input skips one output (a state-only output) is mixed.
    let stateful_state_only_output = Synthetic {
        kind: BlockKind::Stateful,
        pairs: &[(0, 1)],
    };
    assert_eq!(non_feedthrough_reads(&stateful_state_only_output), [(0, 0)]);
    // A stateful loop-breaker's cut inputs are not read at emit time, so it is not mixed.
    let stateful_cut = Synthetic {
        kind: BlockKind::Stateful,
        pairs: &[],
    };
    assert!(non_feedthrough_reads(&stateful_cut).is_empty());
    let algebraic_full = Synthetic {
        kind: BlockKind::Algebraic,
        pairs: &[(0, 0), (0, 1), (1, 0), (1, 1)],
    };
    assert!(non_feedthrough_reads(&algebraic_full).is_empty());
}

#[test]
fn census_reports_an_added_mixed_class_outside_the_documented_list() {
    static ADDED: RegistryEntry = RegistryEntry {
        class_path: "test.MixedFeedthroughControl",
        make: make_mixed_control,
    };
    let observed = mixed_classes(&census([&ADDED].into_iter()));
    assert!(observed.contains("test.MixedFeedthroughControl"));
    let observed: BTreeSet<&str> = mixed_classes(registry_census())
        .into_iter()
        .chain(observed)
        .collect();
    assert!(observed.contains("test.MixedFeedthroughControl"));
    assert_eq!(observed.len(), MIXED_FEEDTHROUGH_CLASSES.len() + 1);
}

#[test]
fn grid_sweeps_selectors_and_masks_exhaustively() {
    let entry = |class| lookup(class).expect("registered");
    // ExtractSignal: nin, nout over 0..=3 and every extract vector in 1..=nin:
    // sum over nin of sum over nout of nin^nout = 1 + 4 + 15 + 40 = 60.
    assert_eq!(
        grid(&axes(entry("CDL.Routing.RealExtractSignal"))).len(),
        60
    );
    // VectorFilter: masks whose true count equals nout, so sum over nin of 2^nin = 15.
    assert_eq!(grid(&axes(entry("CDL.Routing.RealVectorFilter"))).len(), 15);
    // MatrixMin: nRow, nCol over 1..=3 and rowMin over both values.
    assert_eq!(grid(&axes(entry("CDL.Reals.MatrixMin"))).len(), 18);
}

#[test]
fn grid_sweeps_every_enumeration_member() {
    let entry = |class| lookup(class).expect("registered");
    // PID / PIDWithReset: controllerType over P, PI, PD, PID, times reverseActing.
    for class in ["CDL.Reals.PID", "CDL.Reals.PIDWithReset"] {
        let points = grid(&axes(entry(class)));
        assert_eq!(points.len(), 8, "{class}");
        let ordinals: BTreeSet<String> = points
            .iter()
            .flat_map(|p| p.iter().filter(|(n, _)| n == "controllerType"))
            .map(|(_, v)| format!("{v:?}"))
            .collect();
        assert_eq!(ordinals.len(), 4, "{class}: {ordinals:?}");
    }
    // Real TimeTable: smoothness (2 members) times extrapolation (3 members) from EnumMembers.
    assert_eq!(grid(&axes(entry("CDL.Reals.Sources.TimeTable"))).len(), 6);
    // Probed parameters exclude every swept axis.
    let probed: BTreeSet<&str> = probes(entry("CDL.Reals.PIDWithReset"))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        probed.contains("k") && probed.contains("y_reset"),
        "{probed:?}"
    );
    assert!(!probed.contains("controllerType") && !probed.contains("reverseActing"));
}

#[test]
fn probe_check_reports_a_parameter_that_changes_kind() {
    // Mutation control: an observation whose probe flips kind() is reported, naming the parameter.
    let point = vec![("controllerType".to_owned(), Value::Integer(2))];
    let observation = |probe: Option<(&'static str, Value)>, kind| Observation {
        class_path: "test.ProbeControl",
        point: point.clone(),
        probe,
        kind,
        mixed: Vec::new(),
    };
    let observations = [
        observation(None, BlockKind::Stateful),
        observation(Some(("k", Value::Real(1.0))), BlockKind::Stateful),
        observation(Some(("Ti", Value::Real(0.0))), BlockKind::Algebraic),
    ];
    let changes = probe_changes(&observations);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].class_path, changes[0].parameter),
        ("test.ProbeControl", "Ti")
    );
    assert!(changes[0].kind_changed && !changes[0].mixed_changed);
    assert!(
        changes[0].detail.contains("probing Ti"),
        "{}",
        changes[0].detail
    );
}
