//! Durable state continuation over two published open-control-library fault rules.
//!
//! AHU-0016 holds a `Logical.TrueDelay` persistence timer (900 s); AHU-0004 holds a
//! `Reals.MovingAverage` trailing window (3600 s) feeding another `TrueDelay` (3600 s). Both CXF
//! graphs and their scenario vectors are byte-copied from the Library (see
//! `fixtures/library_rules/README.md`), never edited.
//!
//! Run A drives one engine uninterrupted across each scenario. Run B captures
//! `Engine::state_snapshot` partway, takes the bytes through a host-envelope stand-in, parses them
//! with `EngineStateSnapshot::from_bytes`, restores into a freshly loaded engine with
//! `restore_state`, and continues. Every observable output (boundary and internal connectors,
//! compared with `Value::bit_eq`) and the full canonical state bytes after every frame must equal
//! Run A's. The Library vectors' own expectations are checked against Run A, and a cold start at
//! the same point must diverge, so equality cannot come from state that was never at risk.

use oce_api::{
    Engine, EngineStateError, EngineStateSnapshot, OcError, PointDirection, StatePortability,
    Value, ValueType,
};

struct Rule {
    id: &'static str,
    cxf: &'static [u8],
    vectors: &'static str,
}

const AHU_0016: Rule = Rule {
    id: "AHU-0016",
    cxf: include_bytes!("fixtures/library_rules/AHU-0016/rule.cxf.jsonld"),
    vectors: include_str!("fixtures/library_rules/AHU-0016/vectors.json"),
};

const AHU_0004: Rule = Rule {
    id: "AHU-0004",
    cxf: include_bytes!("fixtures/library_rules/AHU-0004/rule.cxf.jsonld"),
    vectors: include_str!("fixtures/library_rules/AHU-0004/vectors.json"),
};

/// One Library expectation window: `output` equals `equals` for every tick in `from..=to`.
struct Expect {
    output: String,
    from: f64,
    to: f64,
    equals: bool,
}

/// A Library scenario resolved against a loaded engine's own input definitions.
struct Scenario {
    name: String,
    step: f64,
    ticks: u64,
    /// Per boundary input: its canonical path and its `(time, value)` steps, ascending.
    schedule: Vec<(String, Vec<(f64, Value)>)>,
    expect: Vec<Expect>,
    /// Every output connector of the loaded rule, in the engine's own listing order.
    outputs: Vec<String>,
}

impl Scenario {
    fn time(&self, tick: u64) -> f64 {
        // Multiply-based cadence, as the Library verifier drives it; no accumulated error.
        tick as f64 * self.step
    }

    /// The complete frame for `tick`: every input holds its latest step at or before that time.
    fn frame(&self, tick: u64) -> Vec<(String, Value)> {
        let t = self.time(tick);
        self.schedule
            .iter()
            .map(|(path, steps)| {
                let (_, value) = steps
                    .iter()
                    .rev()
                    .find(|(at, _)| *at <= t)
                    .expect("every Library input is defined from t = 0");
                (path.clone(), value.clone())
            })
            .collect()
    }
}

/// Everything a host can observe after one accepted frame.
#[derive(Debug)]
struct Row {
    time: f64,
    /// Committed boundary outputs from the `CompletedFrame` receipt.
    boundary: Vec<(String, Value)>,
    /// Latest value of every output connector, internal ones included.
    connectors: Vec<(String, Value)>,
    /// Canonical durable state bytes after the frame, when the caller captured them.
    state: Option<Vec<u8>>,
}

fn same_values(left: &[(String, Value)], right: &[(String, Value)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((lp, lv), (rp, rv))| lp == rp && lv.bit_eq(rv))
}

fn same_outputs(left: &Row, right: &Row) -> bool {
    left.time.to_bits() == right.time.to_bits()
        && same_values(&left.boundary, &right.boundary)
        && same_values(&left.connectors, &right.connectors)
}

fn load(rule: &Rule) -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(rule.cxf).unwrap();
    assert!(
        report.warnings.is_empty(),
        "{}: unexpected load warnings {:?}",
        rule.id,
        report.warnings
    );
    engine
}

fn json_value(json: &serde_json::Value, want: &ValueType) -> Value {
    match want {
        ValueType::Real => Value::Real(json.as_f64().unwrap()),
        ValueType::Integer => Value::Integer(json.as_i64().unwrap()),
        ValueType::Boolean => Value::Boolean(json.as_bool().unwrap()),
        other => panic!("Library vectors stage no {other:?} inputs"),
    }
}

/// Resolve a Library point name to the one engine path ending in `.<name>` or `#<name>`.
fn resolve<'a>(paths: impl Iterator<Item = &'a str>, name: &str) -> String {
    let hits: Vec<_> = paths
        .filter(|path| path.ends_with(&format!(".{name}")) || path.ends_with(&format!("#{name}")))
        .collect();
    assert_eq!(hits.len(), 1, "point `{name}` must resolve once: {hits:?}");
    hits[0].to_owned()
}

fn scenarios(rule: &Rule) -> Vec<Scenario> {
    let doc: serde_json::Value = serde_json::from_str(rule.vectors).unwrap();
    assert_eq!(doc["schema"], "cxf-library/vectors/v1");
    let step = doc["clock"]["step_s"].as_f64().unwrap();
    let horizon = doc["clock"]["horizon_s"].as_f64().unwrap();
    let ticks = (horizon / step).floor() as u64;
    assert_eq!(
        ticks as f64 * step,
        horizon,
        "{}: horizon is on the cadence",
        rule.id
    );

    let engine = load(rule);
    let definitions = engine.input_definitions().unwrap();
    let boundary: Vec<String> = engine
        .topology()
        .boundary_outputs
        .into_iter()
        .map(|declared| declared.path)
        .collect();
    doc["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scenario| {
            let mut schedule = Vec::new();
            for (name, spec) in scenario["inputs"].as_object().unwrap() {
                let path = resolve(definitions.iter().map(|d| d.path.as_str()), name);
                let want = &definitions
                    .iter()
                    .find(|d| d.path == path)
                    .unwrap()
                    .value_type;
                let steps = match spec {
                    serde_json::Value::Array(steps) => steps
                        .iter()
                        .map(|s| (s["t"].as_f64().unwrap(), json_value(&s["value"], want)))
                        .collect::<Vec<_>>(),
                    constant => vec![(0.0, json_value(constant, want))],
                };
                assert!(steps.windows(2).all(|w| w[0].0 < w[1].0));
                assert_eq!(steps[0].0, 0.0, "{name} is defined from t = 0");
                schedule.push((path, steps));
            }
            assert_eq!(schedule.len(), definitions.len(), "every frame is complete");
            let expect = scenario["expect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| Expect {
                    output: resolve(
                        boundary.iter().map(String::as_str),
                        e["output"].as_str().unwrap(),
                    ),
                    from: e["from_s"].as_f64().unwrap(),
                    to: e["to_s"].as_f64().unwrap(),
                    equals: e["equals"].as_bool().unwrap(),
                })
                .collect();
            Scenario {
                name: scenario["name"].as_str().unwrap().to_owned(),
                step,
                ticks,
                schedule,
                expect,
                outputs: output_paths(&engine),
            }
        })
        .collect()
}

/// Every output connector the engine lists, in its own deterministic order.
fn output_paths(engine: &Engine) -> Vec<String> {
    engine
        .point_list(None)
        .unwrap()
        .into_iter()
        .filter(|point| point.direction == PointDirection::Out)
        .map(|point| point.path)
        .collect()
}

fn advance(engine: &mut Engine, scenario: &Scenario, tick: u64, capture: bool) -> Row {
    let inputs = scenario.frame(tick);
    let entries: Vec<_> = inputs
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let prepared = engine.prepare_frame(scenario.time(tick), &entries).unwrap();
    let completed = engine.execute_frame(prepared).unwrap();
    let connectors = scenario
        .outputs
        .iter()
        .map(|path| (path.clone(), engine.get_output(path).unwrap()))
        .collect();
    Row {
        time: completed.time(),
        boundary: completed.outputs().to_vec(),
        connectors,
        state: capture.then(|| engine.state_snapshot().unwrap().into_bytes()),
    }
}

/// Stand-in for the host's authenticated sealed envelope (docs/host-responsibilities.md).
///
/// OCE authenticates nothing; the host binds the exact snapshot bytes to its approved
/// build/deployment qualifier and generation and checks them **before** `from_bytes`. This
/// stand-in models that ordering only. It is not an authentication scheme.
struct SealedEnvelope {
    deployment: &'static str,
    generation: u64,
    payload: Vec<u8>,
}

const DEPLOYMENT: &str = "library-rule-continuation-test";

impl SealedEnvelope {
    fn seal(generation: u64, payload: Vec<u8>) -> Self {
        Self {
            deployment: DEPLOYMENT,
            generation,
            payload,
        }
    }

    /// Release the OCE bytes only for the approved deployment and the expected generation.
    fn approve(self, deployment: &str, generation: u64) -> Option<Vec<u8>> {
        (self.deployment == deployment && self.generation == generation).then_some(self.payload)
    }
}

/// Capture, retire the process, approve the envelope, parse and restore into a fresh engine.
fn restart(rule: &Rule, retiring: Engine, generation: u64) -> Engine {
    let captured = retiring.state_snapshot().unwrap().into_bytes();
    drop(retiring);
    resume(rule, captured, generation)
}

/// The restart path from durable bytes: host approval, bounded parse, fresh load, restore.
fn resume(rule: &Rule, captured: Vec<u8>, generation: u64) -> Engine {
    let sealed = SealedEnvelope::seal(generation, captured);
    let approved = sealed.approve(DEPLOYMENT, generation).unwrap();
    let snapshot = EngineStateSnapshot::from_bytes(&approved).unwrap();
    // No target-bound class participates in either rule.
    assert_eq!(snapshot.portability(), &StatePortability::Portable);
    let mut fresh = load(rule);
    fresh.restore_state(&snapshot).unwrap();
    assert_eq!(
        fresh.state_snapshot().unwrap().as_bytes(),
        approved.as_slice(),
        "restore reproduces the captured state exactly"
    );
    fresh
}

/// Run A: one engine, no interruption.
fn uninterrupted(rule: &Rule, scenario: &Scenario) -> Vec<Row> {
    let mut engine = load(rule);
    (0..=scenario.ticks)
        .map(|tick| advance(&mut engine, scenario, tick, true))
        .collect()
}

/// Run B: restart through a durable snapshot after each tick in `restart_after`. `None` is the
/// startup window itself — a snapshot captured after load and before any frame.
fn interrupted(rule: &Rule, scenario: &Scenario, restart_after: &[Option<u64>]) -> Vec<Row> {
    let mut engine = load(rule);
    let mut generation = 0;
    if restart_after.contains(&None) {
        generation += 1;
        engine = restart(rule, engine, generation);
    }
    let mut rows = Vec::new();
    for tick in 0..=scenario.ticks {
        rows.push(advance(&mut engine, scenario, tick, true));
        if restart_after.contains(&Some(tick)) {
            generation += 1;
            engine = restart(rule, engine, generation);
        }
    }
    rows
}

fn assert_identical(rule: &Rule, scenario: &Scenario, label: &str, a: &[Row], b: &[Row]) {
    assert_eq!(a.len(), b.len());
    for (tick, (left, right)) in a.iter().zip(b).enumerate() {
        assert!(
            same_outputs(left, right) && left.state.is_some() && left.state == right.state,
            "{} / {} / {label}: run B diverged at row {tick} (t = {} s)\nA: {:?}\nB: {:?}",
            rule.id,
            scenario.name,
            left.time,
            left.connectors,
            right.connectors
        );
    }
}

fn boundary_bool(row: &Row, path: &str) -> bool {
    match row.boundary.iter().find(|(p, _)| p == path) {
        Some((_, Value::Boolean(value))) => *value,
        other => panic!("{path} is a Boolean boundary output, found {other:?}"),
    }
}

fn connector(row: &Row, suffix: &str) -> Value {
    let path = resolve(row.connectors.iter().map(|(p, _)| p.as_str()), suffix);
    row.connectors
        .iter()
        .find(|(p, _)| *p == path)
        .unwrap()
        .1
        .clone()
}

/// The Library's own expectations hold on Run A, so the fixtures execute as verified upstream.
fn assert_library_expectations(rule: &Rule, scenario: &Scenario, rows: &[Row]) {
    for row in rows {
        for e in &scenario.expect {
            if row.time >= e.from && row.time <= e.to {
                assert_eq!(
                    boundary_bool(row, &e.output),
                    e.equals,
                    "{} / {}: t = {} s",
                    rule.id,
                    scenario.name,
                    row.time
                );
            }
        }
    }
}

fn scenario<'a>(all: &'a [Scenario], name: &str) -> &'a Scenario {
    all.iter().find(|s| s.name == name).unwrap()
}

/// Cold start at `tick + 1`: a fresh engine continues with no restore. Its rows must differ from
/// Run A's, or the restored state at that point was not load-bearing.
fn cold_start_diverges(rule: &Rule, scenario: &Scenario, tick: u64, a: &[Row]) -> bool {
    let mut cold = load(rule);
    ((tick + 1)..=scenario.ticks).any(|next| {
        let row = advance(&mut cold, scenario, next, false);
        let reference = &a[next as usize];
        !same_values(&row.boundary, &reference.boundary)
            || !same_values(&row.connectors, &reference.connectors)
    })
}

fn tick_at(scenario: &Scenario, seconds: f64) -> u64 {
    let tick = (seconds / scenario.step) as u64;
    assert_eq!(scenario.time(tick), seconds);
    tick
}

#[test]
fn library_fixtures_meet_their_own_vectors_uninterrupted() {
    for rule in [&AHU_0016, &AHU_0004] {
        let all = scenarios(rule);
        assert!(!all.is_empty());
        for scenario in &all {
            let first = uninterrupted(rule, scenario);
            assert_library_expectations(rule, scenario, &first);
            // Determinism: a second uninterrupted run is bit-identical, state bytes included.
            assert_identical(
                rule,
                scenario,
                "repeat",
                &first,
                &uninterrupted(rule, scenario),
            );
        }
    }
}

/// Run B for every single restart point: the bytes Run A captured after tick `k` (or after load,
/// before any frame) resume a fresh engine, which must then reproduce Run A's remaining rows.
///
/// To keep the sweep inside the emulated-CI budget, continuation rows compare every boundary and
/// internal output bit-exactly and the final row also compares the full canonical state bytes.
/// The chained and mid-window tests compare state bytes after every frame.
fn assert_single_restart_at_every_tick(rule: &Rule) {
    for scenario in &scenarios(rule) {
        let a = uninterrupted(rule, scenario);
        let at_load = load(rule).state_snapshot().unwrap().into_bytes();
        let points = std::iter::once((None, at_load))
            .chain((0..scenario.ticks).map(|k| (Some(k), a[k as usize].state.clone().unwrap())));
        for (point, captured) in points {
            let mut engine = resume(rule, captured, 1);
            let next = point.map_or(0, |k| k + 1);
            for tick in next..=scenario.ticks {
                let last = tick == scenario.ticks;
                let row = advance(&mut engine, scenario, tick, last);
                let reference = &a[tick as usize];
                assert!(
                    same_outputs(reference, &row) && (!last || reference.state == row.state),
                    "{} / {} / restart after {point:?}: diverged at t = {} s\nA: {:?}\nB: {:?}",
                    rule.id,
                    scenario.name,
                    reference.time,
                    reference.connectors,
                    row.connectors
                );
            }
        }
    }
}

#[test]
fn persistence_rule_single_restart_at_every_tick_continues_identically() {
    assert_single_restart_at_every_tick(&AHU_0016);
}

#[test]
fn moving_window_rule_single_restart_at_every_tick_continues_identically() {
    assert_single_restart_at_every_tick(&AHU_0004);
}

#[test]
fn restart_after_every_tick_continues_identically() {
    for rule in [&AHU_0016, &AHU_0004] {
        for scenario in &scenarios(rule) {
            let a = uninterrupted(rule, scenario);
            let every: Vec<_> = std::iter::once(None)
                .chain((0..=scenario.ticks).map(Some))
                .collect();
            let b = interrupted(rule, scenario, &every);
            assert_identical(rule, scenario, "restart after every tick", &a, &b);
        }
    }
}

#[test]
fn persistence_timer_survives_a_mid_dwell_restart() {
    let rule = &AHU_0016;
    let all = scenarios(rule);
    let both_open = scenario(&all, "both_open");
    let a = uninterrupted(rule, both_open);
    let fault = &both_open.expect[0].output;
    // t = 480 s: both valves have been open since t = 0, the 900 s delay is half elapsed, and the
    // fault is not yet asserted. Same point in `transient_overlap`, whose overlap ends at 600 s.
    let mid = tick_at(both_open, 480.0);
    assert!(!boundary_bool(&a[mid as usize], fault));
    assert!(connector(&a[mid as usize], "bothOpen.y").bit_eq(&Value::Boolean(true)));
    let first_true = a.iter().position(|row| boundary_bool(row, fault)).unwrap();
    assert_eq!(
        a[first_true].time, 900.0,
        "TrueDelay asserts at exactly delayTime"
    );
    let b = interrupted(rule, both_open, &[Some(mid)]);
    assert_identical(rule, both_open, "mid-dwell", &a, &b);
    assert!(
        cold_start_diverges(rule, both_open, mid, &a),
        "a cold start would restart the 900 s clock"
    );

    let transient = scenario(&all, "transient_overlap");
    let a = uninterrupted(rule, transient);
    let b = interrupted(rule, transient, &[Some(mid)]);
    assert_identical(rule, transient, "mid-dwell, released", &a, &b);
}

#[test]
fn moving_window_and_persistence_survive_mid_window_restarts() {
    let rule = &AHU_0004;
    let all = scenarios(rule);
    // (scenario, snapshot time, what the state holds at that instant)
    let points = [
        // Window still filling (t < 3600 s), count above os_max, persist mid-delay.
        ("sustained_thrash", 1800.0),
        // Full window, persist 1800 s into its 3600 s delay after count crossed at 6000 s.
        ("eight_per_hour_trips", 7800.0),
        // Fault asserted, burst over, count decaying as pulses leave the trailing hour.
        ("burst_ages_out_of_window", 11400.0),
        // Partially filled window carrying an extrapolated warm-up rate.
        ("warmup_rate_never_asserts", 900.0),
    ];
    for (name, seconds) in points {
        let scenario = scenario(&all, name);
        let a = uninterrupted(rule, scenario);
        let tick = tick_at(scenario, seconds);
        let count = connector(&a[tick as usize], "count.y");
        assert!(
            matches!(count, Value::Real(value) if value > 0.0),
            "{name}: window holds pulses at {seconds} s, count = {count:?}"
        );
        let b = interrupted(rule, scenario, &[Some(tick)]);
        assert_identical(rule, scenario, &format!("mid-window {seconds} s"), &a, &b);
        assert!(
            cold_start_diverges(rule, scenario, tick, &a),
            "{name}: a cold start at {seconds} s would empty the window"
        );
    }
}

#[test]
fn restore_guards_refuse_advanced_and_foreign_targets_without_mutation() {
    let all = scenarios(&AHU_0016);
    let both_open = scenario(&all, "both_open");
    let a = uninterrupted(&AHU_0016, both_open);
    let mid = tick_at(both_open, 480.0) as usize;
    let snapshot = EngineStateSnapshot::from_bytes(a[mid].state.as_deref().unwrap()).unwrap();

    // A target that already accepted a frame has left the durable restore window.
    let mut advanced = load(&AHU_0016);
    advance(&mut advanced, both_open, 0, false);
    let before = advanced.state_snapshot().unwrap().into_bytes();
    assert!(matches!(
        advanced.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));
    assert_eq!(advanced.state_snapshot().unwrap().as_bytes(), before);

    // The other rule's executable refuses the bytes on manifest comparison.
    let mut foreign = load(&AHU_0004);
    let before = foreign.state_snapshot().unwrap().into_bytes();
    assert!(matches!(
        foreign.restore_state(&snapshot),
        Err(OcError::State(
            EngineStateError::IncompatibleExecution { .. }
        ))
    ));
    assert_eq!(foreign.state_snapshot().unwrap().as_bytes(), before);
    // A refusal leaves the window open: the matching capture still restores afterwards.
    let own = foreign.state_snapshot().unwrap();
    foreign.restore_state(&own).unwrap();
}
