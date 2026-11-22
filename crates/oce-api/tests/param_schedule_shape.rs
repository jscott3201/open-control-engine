//! `set_param` refuses a value that changes what the frozen schedule was derived from (issue #768).
//!
//! `resume` re-instantiates the edited blocks but never recomputes the schedule. In the PI and PID
//! modes `PIDWithReset.trigger` feeds through to `y` (the reset applies at the frame of the event);
//! in the P and PD modes it does not (decision 10: no integrator, no reset), and P mode, holding no
//! state, reads it when it fires while PD never reads it. So an edit of `controllerType` across
//! those groups, or between P and PD, would run a schedule built for another block shape: a model
//! loaded as PD with a late-declared trigger producer ran a stale one-frame-late reset after an
//! edit to PI, and a feedback model a fresh PI load refuses ran. Each such edit is refused with
//! `ParamStructural`; the engine stays halted, its pending edits unchanged and usable.
//!
//! Compiled-schedule blobs have no parameter edit (`oce_graph::blob::Executable` exposes none), so
//! the hole exists only on a source-loaded engine.

use oce_api::{Engine, EnumClassId, OcError, RunMode, Value};

use crate::event_documents::{Blk, Doc, iri};

const MODES: [&str; 4] = ["P", "PI", "PD", "PID"];

fn mode(member: &str) -> Value {
    let ordinal = match member {
        "P" => 1,
        "PI" => 2,
        "PD" => 3,
        "PID" => 4,
        other => panic!("{other} is no SimpleController member"),
    };
    Value::Enum {
        class: EnumClassId::SIMPLE_CONTROLLER,
        ordinal,
    }
}

fn controller_type() -> String {
    iri("pid.controllerType")
}

/// A `PIDWithReset` (`k = Ti = 1`, `y_reset = 5`, `e = 1`) whose `trigger` comes from
/// `CivilTime > 0.5`, both declared AFTER the controller; with `feedback`, `y -> GreaterThreshold
/// (2.5) -> trigger` instead.
fn reset_pid(member: &'static str, feedback: bool) -> Doc {
    let mut doc = Doc {
        blocks: vec![Blk {
            name: "pid",
            class: "Reals.PIDWithReset",
            params: vec![
                ("controllerType", "SimpleController", member),
                ("k", "Real", "1"),
                ("Ti", "Real", "1"),
                ("Td", "Real", "0.5"),
                ("y_reset", "Real", "5"),
                ("yMax", "Real", "100"),
                ("yMin", "Real", "-100"),
            ],
            ins: vec![("u_s", "Real"), ("u_m", "Real"), ("trigger", "Boolean")],
            outs: vec![("y", "Real")],
        }],
        inputs: vec![
            ("us", "Real", vec!["pid.u_s"]),
            ("um", "Real", vec!["pid.u_m"]),
        ],
        outputs: vec![("y", "Real")],
        wires: vec![("pid.y", "y")],
    };
    if feedback {
        doc.blocks.push(Blk {
            name: "thr",
            class: "Reals.GreaterThreshold",
            params: vec![("t", "Real", "2.5")],
            ins: vec![("u", "Real")],
            outs: vec![("y", "Boolean")],
        });
        doc.wires
            .extend([("pid.y", "thr.u"), ("thr.y", "pid.trigger")]);
    } else {
        doc.blocks.push(Blk {
            name: "clock",
            class: "Reals.Sources.CivilTime",
            params: vec![],
            ins: vec![],
            outs: vec![("y", "Real")],
        });
        doc.blocks.push(Blk {
            name: "thr",
            class: "Reals.GreaterThreshold",
            params: vec![("t", "Real", "1.5")],
            ins: vec![("u", "Real")],
            outs: vec![("y", "Boolean")],
        });
        doc.wires
            .extend([("clock.y", "thr.u"), ("thr.y", "pid.trigger")]);
    }
    doc
}

fn load(doc: &Doc) -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(&doc.bytes()).expect("loads");
    engine
}

/// Frames at t = 0..5 with `u_s = 1`, `u_m = 0`; the first boundary output of each.
fn run_frames(engine: &mut Engine) -> Vec<u64> {
    let (us, um) = (iri("us"), iri("um"));
    (0..5)
        .map(|t| {
            let entries = [
                (us.as_str(), Value::Real(1.0)),
                (um.as_str(), Value::Real(0.0)),
            ];
            let prepared = engine.prepare_frame(f64::from(t), &entries).unwrap();
            let done = engine.execute_frame(prepared).unwrap();
            done.outputs()[0].1.as_real().unwrap().to_bits()
        })
        .collect()
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

fn assert_structural(result: Result<(), OcError>, what: &str) {
    match result {
        Err(OcError::ParamStructural { path }) => assert_eq!(path, controller_type(), "{what}"),
        other => panic!("{what}: expected ParamStructural, got {other:?}"),
    }
}

#[test]
fn reviewer_scenario_pd_to_pi_with_a_late_declared_trigger_is_refused_and_the_engine_stays_usable()
{
    // A fresh PI load resets at the frame the trigger rises (t = 2): 1, 1, 5, 5, 6.
    let fresh_pi = run_frames(&mut load(&reset_pid("PI", false)));
    assert_eq!(fresh_pi, bits(&[1.0, 1.0, 5.0, 5.0, 6.0]));

    let mut engine = load(&reset_pid("PD", false));
    engine.halt().unwrap();
    let snapshot = |engine: &Engine| -> Vec<(String, String)> {
        engine
            .params()
            .iter()
            .map(|(path, value, _)| (path, format!("{value:?}")))
            .collect()
    };
    let before = snapshot(&engine);
    assert_structural(engine.set_param(&controller_type(), mode("PI")), "PD -> PI");
    // Refused: still halted, the table (pending edits included) unchanged.
    assert_eq!(engine.mode(), RunMode::Halted);
    assert_eq!(snapshot(&engine), before);
    assert!(
        engine
            .get_param(&controller_type())
            .unwrap()
            .bit_eq(&mode("PD"))
    );

    // Usable: a later valid edit and resume behave as on an engine that never saw the refusal.
    engine.set_param(&iri("pid.k"), Value::Real(2.0)).unwrap();
    engine.resume().unwrap();
    let mut never_refused = load(&reset_pid("PD", false));
    never_refused.halt().unwrap();
    never_refused
        .set_param(&iri("pid.k"), Value::Real(2.0))
        .unwrap();
    never_refused.resume().unwrap();
    assert_eq!(run_frames(&mut engine), run_frames(&mut never_refused));
}

#[test]
fn reviewer_scenario_feedback_loaded_as_pd_cannot_be_switched_to_a_mode_that_refuses_it() {
    let fresh_pi = Engine::new().load_cxf(&reset_pid("PI", true).bytes());
    assert!(
        fresh_pi.is_err(),
        "a fresh PI load of the feedback model is an algebraic loop"
    );
    let mut engine = load(&reset_pid("PD", true));
    engine.halt().unwrap();
    for target in ["PI", "PID", "P"] {
        assert_structural(
            engine.set_param(&controller_type(), mode(target)),
            &format!("PD -> {target} with feedback"),
        );
        assert!(
            engine
                .get_param(&controller_type())
                .unwrap()
                .bit_eq(&mode("PD"))
        );
    }
    engine.resume().unwrap();
    run_frames(&mut engine);
}

#[test]
fn every_mode_switch_across_the_trigger_shape_is_refused_and_pi_pid_is_accepted() {
    // The shape groups: P (reads trigger at emit), PD (never reads it), PI and PID (feed it
    // through). Only PI <-> PID stays inside one group.
    for from in MODES {
        for to in MODES {
            let mut engine = load(&reset_pid(from, false));
            engine.halt().unwrap();
            let result = engine.set_param(&controller_type(), mode(to));
            let same_shape = from == to || matches!((from, to), ("PI", "PID") | ("PID", "PI"));
            if same_shape {
                result.unwrap_or_else(|err| panic!("{from} -> {to}: {err:?}"));
            } else {
                assert_structural(result, &format!("{from} -> {to}"));
            }
        }
    }
}

#[test]
fn pi_to_pid_and_back_is_accepted_and_runs_like_a_fresh_load() {
    // resume re-seeds the state (the run restarts), so the edited engine equals a fresh load of
    // the target mode bit for bit.
    for (from, to) in [("PI", "PID"), ("PID", "PI")] {
        let mut engine = load(&reset_pid(from, false));
        run_frames(&mut engine);
        engine.halt().unwrap();
        engine.set_param(&controller_type(), mode(to)).unwrap();
        engine.resume().unwrap();
        let mut fresh = load(&reset_pid(to, false));
        assert_eq!(
            run_frames(&mut engine),
            run_frames(&mut fresh),
            "{from} -> {to}"
        );
    }
}

#[test]
fn the_guard_judges_the_cumulative_pending_edits_of_the_block() {
    let mut engine = load(&reset_pid("PI", false));
    engine.halt().unwrap();
    // PI -> PID (same shape), then PID -> PI (back): both accepted.
    engine.set_param(&controller_type(), mode("PID")).unwrap();
    engine.set_param(&controller_type(), mode("PI")).unwrap();
    engine.set_param(&controller_type(), mode("PID")).unwrap();
    // PID (pending) -> PD changes the shape against the scheduled PI instance: refused, and the
    // pending PID survives.
    assert_structural(
        engine.set_param(&controller_type(), mode("PD")),
        "pending PID -> PD",
    );
    assert!(
        engine
            .get_param(&controller_type())
            .unwrap()
            .bit_eq(&mode("PID"))
    );
    // An edit after the refused one is judged with the pending PID: another parameter is fine.
    engine.set_param(&iri("pid.Td"), Value::Real(0.25)).unwrap();
    engine.resume().unwrap();
    assert_eq!(engine.mode(), RunMode::Running);
}

#[test]
fn pid_controller_type_edits_stay_settable() {
    // `CDL.Reals.PID` has no trigger: every mode reads `u_s` and `u_m` when it fires (P as an
    // algebraic block, the others because both feed through), so no mode edit changes its shape.
    let doc = Doc {
        blocks: vec![Blk {
            name: "pid",
            class: "Reals.PID",
            params: vec![("controllerType", "SimpleController", "PI")],
            ins: vec![("u_s", "Real"), ("u_m", "Real")],
            outs: vec![("y", "Real")],
        }],
        inputs: vec![
            ("us", "Real", vec!["pid.u_s"]),
            ("um", "Real", vec!["pid.u_m"]),
        ],
        outputs: vec![("y", "Real")],
        wires: vec![("pid.y", "y")],
    };
    for to in MODES {
        let mut engine = load(&doc);
        engine.halt().unwrap();
        engine
            .set_param(&controller_type(), mode(to))
            .unwrap_or_else(|err| panic!("PI -> {to}: {err:?}"));
        engine.resume().unwrap();
    }
}
