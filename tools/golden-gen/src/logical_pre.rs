//! `CDL.Logical.Pre` standalone references under the HostTick v1 execution profile.
//!
//! Buildings `CDL/Logical/Pre.mo` declares `initial equation pre(u)=pre_u_start;` (line 14) and
//! `equation y=pre(u);` (line 17): a delay of one Modelica event iteration without advancing
//! time, where event iteration continues until `u == pre(u)`. HostTick v1 performs no same-time
//! event iteration, so these references implement the documented one-transition projection
//! instead (`docs/execution-profile.md`, `CDL.Logical.Pre`):
//!
//! - the first successful tick emits `pre_u_start` (the `initial equation`), then latches `u`;
//! - each later tick emits the `u` latched by the preceding tick, then latches the current `u`.
//!
//! A converged Modelica event iteration would instead report `y == u` after every input event.
//! The references therefore pin the HostTick v1 profile, not Modelica event-iteration equivalence.
//! Derived from the source and the profile text only — never from `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const CLASS_PATH: &str = "CDL.Logical.Pre";
const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
const SOURCE_FILE: &str =
    "third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Logical/Pre.mo";
const EXECUTION_PROFILE: &str = "HostTick v1";
const EVENT_ITERATION_DIVERGENCE: &str = "Pre.mo line 17 y=pre(u) is a one-event-iteration delay; Modelica iterates at each input event until u == pre(u), so a converged trace reports y == u after every event. HostTick v1 performs no same-time event iteration: y at tick k is u at tick k-1 and y at the first tick is pre_u_start. This reference pins the HostTick v1 projection and claims no Modelica event-iteration equivalence.";

/// Build the standalone `CDL.Logical.Pre` HostTick v1 goldens.
pub fn goldens() -> Vec<Golden> {
    vec![
        // Default `pre_u_start=false` with `u` already true on the first tick: the first tick must
        // emit the parameter, not the input. Then a held true, a single-tick false, single-tick
        // toggles, a held false and a final rise, so every transition kind is delayed once.
        pre_golden(
            None,
            false,
            &[true, true, false, true, false, false, true, true, false],
            "pre_u_start=false (default); u=[T,T,F,T,F,F,T,T,F] at 60 s ticks; u is true on the first tick",
        ),
        // `pre_u_start=true` with `u` false on the first tick: the first tick emits `true` from
        // the parameter, so the default `false` cannot pass by accident.
        pre_golden(
            Some("pre_u_start_true"),
            true,
            &[false, false, true, false, true],
            "pre_u_start=true; u=[F,F,T,F,T] at 60 s ticks; u is false on the first tick",
        ),
    ]
}

fn pre_golden(
    scenario: Option<&'static str>,
    pre_u_start: bool,
    u: &[bool],
    input_desc: &'static str,
) -> Golden {
    let time: Vec<f64> = (0..u.len()).map(|k| (k as f64) * 60.0).collect();
    let y = host_tick_pre(pre_u_start, u);
    let golden = Golden::new(
        CLASS_PATH,
        "y",
        ValueKind::Boolean,
        time,
        y.into_iter().map(Sample::Boolean).collect(),
        input_desc,
        "HostTick v1 projection of Buildings CDL/Logical/Pre.mo: y[0] = pre_u_start (initial equation pre(u)=pre_u_start, line 14); y[k] = u[k-1] for k >= 1 (y=pre(u), line 17, one transition per tick)",
    )
    .with_inputs(vec![InputSeries::new(
        "u",
        ValueKind::Boolean,
        u.iter().copied().map(Sample::Boolean).collect(),
    )])
    .with_provenance("source_commit", SOURCE_COMMIT)
    .with_provenance("source_files", SOURCE_FILE)
    .with_provenance("execution_profile", EXECUTION_PROFILE)
    .with_provenance("diverges_from_modelica_event_iteration", EVENT_ITERATION_DIVERGENCE)
    .with_provenance(
        "parameters",
        format!("pre_u_start={pre_u_start}"),
    );
    match scenario {
        Some(scenario) => golden.with_scenario(scenario),
        None => golden,
    }
}

/// One-transition Boolean memory: emit the held value, then latch the current input.
fn host_tick_pre(pre_u_start: bool, u: &[bool]) -> Vec<bool> {
    let mut held = pre_u_start;
    u.iter()
        .map(|&input| {
            let y = held;
            held = input;
            y
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::host_tick_pre;

    #[test]
    fn first_tick_emits_the_start_value_and_later_ticks_lag_the_input_by_one() {
        assert_eq!(
            host_tick_pre(false, &[true, true, false, true]),
            [false, true, true, false]
        );
        assert_eq!(host_tick_pre(true, &[false, false]), [true, false]);
    }

    #[test]
    fn empty_input_emits_nothing() {
        assert!(host_tick_pre(true, &[]).is_empty());
    }
}
