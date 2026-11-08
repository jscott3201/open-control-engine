//! Frame-rate-dependence labels on the G36 Tier-A sequence references, derived from the fixtures.
//!
//! golden-gen integrates every CDL `PID`/`PIDWithReset` integrator with the forward-Euler
//! recurrence the engine also uses: `xI += (k/Ti)*(e - (yU - y)/(k*Ni))*dt`, with `dt` the golden's
//! frame step. Applied to the integrator's own time constant `Ti/k`, forward Euler has the
//! amplification factor `1 - k*dt/Ti`; at `k*dt/Ti >= 2` that factor reaches `-1` or below, so
//! the discrete recurrence no longer tracks the continuous PI and the values (typically a
//! saturate-every-frame alternation) depend on the frame step. Such signals must carry a
//! `frame_rate_dependence` note. Below the threshold the same recurrence is used but is treated
//! as frame-step-insensitive at the golden's step; no current fixture lies in `[0.2, 2)`.
//!
//! A golden scenario is surveyed against the fixture of the same name, or, for an additional
//! scenario of an existing fixture (such as a finer frame step), against the fixture named in
//! `SCENARIO_FIXTURES`; every golden scenario must resolve to a fixture.
//!
//! This suite reads each fixture's PI blocks, resolves `controllerType`, `k` and `Ti` through the
//! fixture's parameter bindings (falling back to the vendored CDL defaults only where the fixture
//! leaves a parameter unset), takes the largest step of the golden's time grid, and follows the
//! fixture's connections to find the root outputs each PI reaches. A PI scenario that is not in
//! the documented ratio table, an unresolvable parameter, or a reached output without a known
//! golden signal fails the suite rather than passing silently.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `k*dt/Ti` at or above which forward Euler leaves its stability region for the integrator.
const FRAME_RATE_THRESHOLD: f64 = 2.0;
const FIXTURE_DIR: &str = "crates/oce-cxf/tests/fixtures/g36";
const GOLDEN_DIR: &str = "tools/golden-gen/goldens/G36";
const CDL_REALS: &str = "third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Reals";
const SIMPLE_CONTROLLER: &str = "Buildings.Controls.OBC.CDL.Types.SimpleController.";

/// Golden scenarios that reuse another scenario's fixture: `(golden scenario, fixture name)`.
const SCENARIO_FIXTURES: [(&str, &str); 1] = [(
    "multizone_vav_freeze_protection_fine_step",
    "multizone_vav_freeze_protection",
)];

/// The largest `k*dt/Ti` of every G36 golden scenario with an integrating PI, as documented in
/// `docs/verification-evidence.md` and the `frame_rate_dependence` field docs.
const DOCUMENTED_RATIOS: [(&str, f64); 12] = [
    ("cooling_only_controller", 0.1),
    ("cooling_only_dampers", 0.2),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        120.0,
    ),
    ("multizone_vav_economizer_limits_common", 0.05 / 120.0),
    ("multizone_vav_freeze_protection", 120.0),
    ("multizone_vav_freeze_protection_fine_step", 0.125),
    ("multizone_vav_return_fan_airflow_tracking", 2.0),
    ("multizone_vav_return_fan_direct_pressure", 2.0),
    ("multizone_vav_supply_fan", 0.1 / 60.0),
    ("multizone_vav_supply_signals", 0.05 / 600.0),
    ("thermal_zones_control_loops", 0.1 * 60.0 / 900.0),
    ("vav_single_zone", 0.25 / 20.0),
];

/// `(scenario, fixture root output, golden signal)` for outputs reached by an at-threshold PI.
const OUTPUT_SIGNALS: [(&str, &str, &str); 8] = [
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "yOutDam_min",
        "outdoor_damper_min_limit",
    ),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "yOutDam",
        "outdoor_damper_command",
    ),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "yRetDam",
        "return_damper_command",
    ),
    (
        "multizone_vav_freeze_protection",
        "yHeaCoi",
        "heating_coil_command",
    ),
    (
        "multizone_vav_return_fan_airflow_tracking",
        "yRetFan",
        "return_fan_speed",
    ),
    (
        "multizone_vav_return_fan_direct_pressure",
        "dpDisSet",
        "discharge_pressure_setpoint",
    ),
    (
        "multizone_vav_return_fan_direct_pressure",
        "yRelDam",
        "relief_damper_command",
    ),
    (
        "multizone_vav_return_fan_direct_pressure",
        "yRetFan",
        "return_fan_speed",
    ),
];

/// Vendored source declarations behind each labelled scenario's `Ti = 0.5 s`.
const LABELLED_SOURCE_DECLARATIONS: [(&str, &str, &[&str]); 4] = [
    (
        "multizone_vav_freeze_protection",
        "AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo",
        &["parameter Real Ti(unit=\"s\")=0.5"],
    ),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "AHUs/MultiZone/VAV/Economizers/Controller.mo",
        &["parameter Real TiMinOA(unit=\"s\")=0.5", "final Ti=TiMinOA"],
    ),
    (
        "multizone_vav_return_fan_airflow_tracking",
        "AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo",
        &["parameter Real Ti( final unit=\"s\", final quantity=\"Time\")=0.5"],
    ),
    (
        "multizone_vav_return_fan_direct_pressure",
        "AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo",
        &["parameter Real Ti( final unit=\"s\", final quantity=\"Time\")=0.5"],
    ),
];

/// One integrating PI block in a fixture.
struct IntegratingPi {
    block: String,
    ratio: f64,
    reached_outputs: BTreeSet<String>,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root resolves")
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read {} failed: {err}", path.display()))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&read_text(path))
        .unwrap_or_else(|err| panic!("parse JSON {} failed: {err}", path.display()))
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn as_list(value: &Value) -> Vec<&Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().collect(),
        item => vec![item],
    }
}

fn parent(id: &str) -> Option<&str> {
    let (scope, _) = id.rsplit_once('.')?;
    scope.contains('#').then_some(scope)
}

fn type_name(node: &Value) -> &str {
    node["@type"].as_str().unwrap_or("")
}

/// A parameter value: a number, or a `SimpleController` literal.
#[derive(Debug, PartialEq)]
enum Resolved {
    Number(f64),
    Controller(String),
}

struct Fixture {
    scenario: String,
    root: String,
    nodes: BTreeMap<String, Value>,
    edges: BTreeMap<String, Vec<String>>,
}

impl Fixture {
    fn load(path: &Path) -> Self {
        let document = read_json(path);
        let graph = document["@graph"].as_array().expect("fixture graph");
        let scenario = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("fixture name")
            .to_string();
        let root = graph[0]["@id"].as_str().expect("root id").to_string();
        assert_eq!(
            graph[0]["S231:label"],
            scenario,
            "{} must start with its root block",
            path.display()
        );
        let mut nodes = BTreeMap::new();
        let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in graph {
            let Some(id) = node["@id"].as_str() else {
                continue;
            };
            for target in as_list(&node["S231:isConnectedTo"]) {
                let target = target["@id"].as_str().expect("connection target id");
                edges
                    .entry(id.to_string())
                    .or_default()
                    .push(target.to_string());
            }
            nodes.insert(id.to_string(), node.clone());
        }
        Self {
            scenario,
            root,
            nodes,
            edges,
        }
    }

    /// Resolve `block.name`, following name references outward through enclosing scopes.
    fn parameter(&self, block: &str, name: &str) -> Option<Resolved> {
        let mut node = self.nodes.get(&format!("{block}.{name}"))?;
        for _ in 0..16 {
            let value = &node["S231:value"];
            let text = value
                .as_str()
                .or_else(|| value["@value"].as_str())
                .unwrap_or_else(|| panic!("{}.{name} has no value", block));
            if let Ok(number) = text.parse::<f64>() {
                return Some(Resolved::Number(number));
            }
            if let Some(literal) = text.strip_prefix(SIMPLE_CONTROLLER) {
                return Some(Resolved::Controller(literal.to_string()));
            }
            // Search the parameter's own scope (its siblings) and then each enclosing scope,
            // never matching the parameter itself (a block's `k` bound to an outer `k`).
            let id = node["@id"].as_str().expect("parameter id");
            let mut scope = parent(id);
            let mut found = None;
            while let Some(candidate) = scope {
                let target_id = format!("{candidate}.{text}");
                if target_id != id
                    && let Some(target) = self.nodes.get(&target_id)
                {
                    found = Some(target);
                    break;
                }
                scope = parent(candidate);
            }
            node = found.unwrap_or_else(|| {
                panic!(
                    "{}: unresolvable parameter reference `{text}` for {block}.{name}",
                    self.scenario
                )
            });
        }
        panic!(
            "{}: parameter chain for {block}.{name} is too deep",
            self.scenario
        )
    }

    fn outputs_of(&self, block: &str) -> Vec<String> {
        let prefix = format!("{block}.");
        self.nodes
            .iter()
            .filter(|(id, node)| {
                id.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains('.'))
                    && type_name(node).ends_with("Output")
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Root output names reachable from `block`'s outputs through connections and blocks.
    fn reached_root_outputs(&self, block: &str) -> BTreeSet<String> {
        let mut stack = self.outputs_of(block);
        let mut seen = BTreeSet::new();
        let mut reached = BTreeSet::new();
        while let Some(connector) = stack.pop() {
            if !seen.insert(connector.clone()) {
                continue;
            }
            let node = self.nodes.get(&connector);
            let kind = node.map_or("", type_name);
            let owner = parent(&connector).unwrap_or("");
            if owner == self.root && kind.ends_with("Output") {
                reached.insert(connector[self.root.len() + 1..].to_string());
            }
            if owner != self.root && kind.ends_with("Input") {
                stack.extend(self.outputs_of(owner));
            }
            stack.extend(self.edges.get(&connector).into_iter().flatten().cloned());
        }
        reached
    }

    fn integrating_pis(&self, frame_step: f64) -> Vec<IntegratingPi> {
        let mut out = Vec::new();
        for (id, node) in &self.nodes {
            let kind = type_name(node);
            if !(kind.ends_with("CDL.Reals.PID") || kind.ends_with("CDL.Reals.PIDWithReset")) {
                continue;
            }
            let controller = match self.parameter(id, "controllerType") {
                None => "PI".to_string(),
                Some(Resolved::Controller(literal)) => literal,
                Some(other) => panic!("{id} controllerType resolved to {other:?}"),
            };
            if !matches!(controller.as_str(), "PI" | "PID") {
                assert!(
                    matches!(controller.as_str(), "P" | "PD"),
                    "{id}: unrecognised controllerType {controller}"
                );
                continue;
            }
            let number = |name: &str, default: f64| match self.parameter(id, name) {
                None => default,
                Some(Resolved::Number(value)) => value,
                Some(other) => panic!("{id}.{name} resolved to {other:?}"),
            };
            let k = number("k", 1.0);
            let ti = number("Ti", 0.5);
            assert!(k > 0.0 && ti > 0.0, "{id}: non-positive k={k} or Ti={ti}");
            out.push(IntegratingPi {
                block: id.clone(),
                ratio: k * frame_step / ti,
                reached_outputs: self.reached_root_outputs(id),
            });
        }
        out
    }
}

/// Largest step of a golden's time grid; zero for a single-row golden.
fn frame_step(scenario_dir: &Path) -> f64 {
    let csv = read_text(&scenario_dir.join("reference.csv"));
    let times = csv
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("double"))
        .map(|line| {
            line.split_whitespace()
                .next()
                .and_then(|cell| cell.parse::<f64>().ok())
                .unwrap_or_else(|| panic!("unparseable row {line} in {}", scenario_dir.display()))
        })
        .collect::<Vec<_>>();
    times
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .fold(0.0, f64::max)
}

struct Survey {
    ratios: BTreeMap<String, f64>,
    required_labels: BTreeSet<(String, String)>,
}

fn survey() -> Survey {
    let root = repository_root();
    let mut goldens = std::fs::read_dir(root.join(GOLDEN_DIR))
        .expect("enumerate G36 goldens")
        .map(|entry| entry.expect("golden entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    goldens.sort();
    let mut ratios = BTreeMap::new();
    let mut required_labels = BTreeSet::new();
    let mut surveyed = 0;
    for golden in goldens {
        let scenario = golden
            .file_name()
            .and_then(|name| name.to_str())
            .expect("golden scenario name")
            .to_string();
        let fixture_name = SCENARIO_FIXTURES
            .iter()
            .find(|(golden_scenario, _)| *golden_scenario == scenario)
            .map_or(scenario.as_str(), |(_, fixture)| fixture);
        let path = root
            .join(FIXTURE_DIR)
            .join(format!("{fixture_name}.jsonld"));
        assert!(
            path.is_file(),
            "golden scenario {scenario} has no fixture {}; add it to SCENARIO_FIXTURES",
            path.display()
        );
        let mut fixture = Fixture::load(&path);
        fixture.scenario = scenario;
        surveyed += 1;
        let pis = fixture.integrating_pis(frame_step(&golden));
        if let Some(max) = pis.iter().map(|pi| pi.ratio).reduce(f64::max) {
            ratios.insert(fixture.scenario.clone(), max);
        }
        for pi in pis.iter().filter(|pi| pi.ratio >= FRAME_RATE_THRESHOLD) {
            assert!(
                !pi.reached_outputs.is_empty(),
                "{} reaches no root output",
                pi.block
            );
            for output in &pi.reached_outputs {
                let signal = OUTPUT_SIGNALS
                    .iter()
                    .find(|(scenario, name, _)| *scenario == fixture.scenario && name == output)
                    .map(|(_, _, signal)| *signal)
                    .unwrap_or_else(|| {
                        panic!(
                            "{} (k*dt/Ti={}) reaches root output {output} with no known golden \
                             signal; add it to OUTPUT_SIGNALS and label it",
                            pi.block, pi.ratio
                        )
                    });
                required_labels.insert((fixture.scenario.clone(), signal.to_string()));
            }
        }
    }
    assert!(
        surveyed >= 40,
        "expected the full G36 fixture set, surveyed {surveyed}"
    );
    Survey {
        ratios,
        required_labels,
    }
}

fn frame_rate_notes() -> BTreeMap<(String, String), (String, String)> {
    let root = repository_root().join(GOLDEN_DIR);
    let mut notes = BTreeMap::new();
    let mut dirs = std::fs::read_dir(&root)
        .expect("enumerate G36 goldens")
        .map(|entry| entry.expect("golden entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    for dir in dirs {
        for entry in std::fs::read_dir(&dir).expect("enumerate golden dir") {
            let path = entry.expect("golden file").path();
            let Some(signal) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".prov.json"))
                .map(str::to_string)
            else {
                continue;
            };
            let record = read_json(&path);
            if let Some(note) = record["frame_rate_dependence"].as_str() {
                let scenario = record["scenario"].as_str().expect("scenario").to_string();
                let source = record["source"].as_str().expect("source").to_string();
                notes.insert((scenario, signal), (note.to_string(), source));
            }
        }
    }
    notes
}

fn stated_ratio(ratio: f64) -> String {
    assert_eq!(ratio.fract(), 0.0, "labelled ratios are stated as integers");
    format!("k*dt/Ti={ratio:.0}")
}

#[test]
fn every_integrating_pi_ratio_is_documented() {
    let ratios = survey().ratios;
    let documented = DOCUMENTED_RATIOS
        .iter()
        .map(|(scenario, ratio)| (scenario.to_string(), *ratio))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        ratios.keys().collect::<Vec<_>>(),
        documented.keys().collect::<Vec<_>>(),
        "a G36 fixture gained or lost an integrating PI; review its frame-rate dependence and \
         update DOCUMENTED_RATIOS and the docs"
    );
    for (scenario, ratio) in &ratios {
        let expected = documented[scenario];
        assert!(
            (ratio - expected).abs() <= expected * 1e-12,
            "{scenario}: k*dt/Ti is {ratio}, documented {expected}"
        );
        assert!(
            *ratio >= FRAME_RATE_THRESHOLD || *ratio <= 0.2,
            "{scenario}: k*dt/Ti={ratio} lies between 0.2 and the threshold; decide whether it \
             is frame-step sensitive before relying on the threshold"
        );
    }
}

#[test]
fn every_scenario_fixture_mapping_names_a_golden_and_a_distinct_fixture() {
    let root = repository_root();
    for (scenario, fixture) in SCENARIO_FIXTURES {
        assert_ne!(scenario, fixture, "{scenario} maps to itself");
        assert!(
            root.join(GOLDEN_DIR).join(scenario).is_dir(),
            "{scenario} has no golden; drop it from SCENARIO_FIXTURES"
        );
        assert!(
            !root
                .join(FIXTURE_DIR)
                .join(format!("{scenario}.jsonld"))
                .exists(),
            "{scenario} has its own fixture; drop it from SCENARIO_FIXTURES"
        );
    }
}

#[test]
fn signals_reached_by_an_at_threshold_pi_are_exactly_the_labelled_signals() {
    let required = survey().required_labels;
    let labelled = frame_rate_notes().into_keys().collect::<BTreeSet<_>>();
    assert_eq!(
        labelled, required,
        "frame_rate_dependence labels must be exactly the signals an integrating PI with \
         k*dt/Ti >= {FRAME_RATE_THRESHOLD} reaches"
    );
}

#[test]
fn frame_rate_notes_state_their_ratio_and_disclaim_independence() {
    let ratios = survey().ratios;
    for ((scenario, signal), (note, source)) in frame_rate_notes() {
        let ratio = stated_ratio(ratios[&scenario]);
        assert!(
            note.contains(&ratio),
            "{scenario}/{signal} note must state {ratio}: {note}"
        );
        assert!(
            source.contains("frame-rate dependent")
                && !source.contains("independent re-derivation"),
            "{scenario}/{signal} source must not claim an independent derivation: {source}"
        );
    }
}

#[test]
fn labelled_integral_times_match_the_vendored_sources_and_cdl_defaults() {
    let root = repository_root();
    let g36 = root.join("third_party/modelica-buildings-cdl/Buildings/Controls/OBC/ASHRAE/G36");
    let labelled = frame_rate_notes()
        .into_keys()
        .map(|(scenario, _)| scenario)
        .collect::<BTreeSet<_>>();
    let explained = LABELLED_SOURCE_DECLARATIONS
        .iter()
        .map(|(scenario, _, _)| scenario.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        labelled, explained,
        "every labelled scenario needs its source declaration"
    );
    for (scenario, source, declarations) in LABELLED_SOURCE_DECLARATIONS {
        let text = normalized(&read_text(&g36.join(source)));
        for declaration in declarations {
            assert!(
                text.contains(declaration),
                "vendored {source} no longer contains `{declaration}`; revisit the \
                 {scenario} frame_rate_dependence notes"
            );
        }
    }

    // The fallbacks used for parameters a fixture leaves unset.
    for block in ["PID.mo", "PIDWithReset.mo"] {
        let text = normalized(&read_text(&root.join(CDL_REALS).join(block)));
        for declaration in [
            "controllerType = Buildings.Controls.OBC.CDL.Types.SimpleController.PI",
            "min=100*Constants.eps)=1 \"Gain of controller\"",
            "min=100*Constants.eps)=0.5",
        ] {
            assert!(
                text.contains(declaration),
                "vendored CDL {block} no longer declares `{declaration}`; update the defaults \
                 in this suite"
            );
        }
    }
}
