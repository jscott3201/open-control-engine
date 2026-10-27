//! `CDL.Utilities.Assert` engine-observation reference.
//!
//! Buildings `CDL/Utilities/Assert.mo` line 11 is the stateless equation
//! `assert(u, message, AssertionLevel.warning);`. The block has no output connector, so its only
//! observable is the warning record. Lines 1-12 hold no `pre`, `edge`, latch or warn-once state,
//! so each evaluation with `u = false` writes the message once and each evaluation with
//! `u = true` writes nothing. Modelica leaves the repetition rate of a warning-level assert to the
//! tool; this reference pins the per-evaluation reading, one evaluation per host tick.
//!
//! The emitted `warning` column is therefore not a CDL output: `1.0` means "exactly one warning
//! record with this block's `message` and this row's time", `0.0` means "no record". The consumer
//! compares it against the facade's completed-frame diagnostics, not against an output point.
//! Derived from the source only — never from `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
const SOURCE_FILE: &str =
    "third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Utilities/Assert.mo";
/// The `message` parameter the reference was derived for; consumers must bind the same text.
const MESSAGE: &str = "supply air temperature out of range";

/// Build the `CDL.Utilities.Assert` warning-observation golden.
pub fn goldens() -> Vec<Golden> {
    // False on the first tick (warns at t = 0, no initialisation grace), a repeated false (no
    // warn-once latch), true ticks (silent), a single-tick false between trues, and a final false
    // after a held true.
    let u = [false, false, true, false, true, true, false];
    let time: Vec<f64> = (0..u.len()).map(|k| (k as f64) * 60.0).collect();
    let warning: Vec<Sample> = u.iter().map(|&ok| Sample::Boolean(!ok)).collect();
    vec![
        Golden::new(
            "CDL.Utilities.Assert",
            "warning",
            ValueKind::Boolean,
            time,
            warning,
            format!("message=\"{MESSAGE}\"; u=[F,F,T,F,T,T,F] at 60 s ticks; u is false on the first tick"),
            "Buildings CDL/Utilities/Assert.mo line 11 assert(u, message, AssertionLevel.warning): warning[k] = NOT u[k]; 1.0 = exactly one Warning record carrying message at time[k], 0.0 = none",
        )
        .with_inputs(vec![InputSeries::new(
            "u",
            ValueKind::Boolean,
            u.iter().copied().map(Sample::Boolean).collect(),
        )])
        .with_provenance("source_commit", SOURCE_COMMIT)
        .with_provenance("source_files", SOURCE_FILE)
        .with_provenance("message", MESSAGE)
        .with_provenance(
            "observation_channel",
            "warning-record count per evaluation (CDL Assert has no output connector); compare against the completed frame's Warning diagnostics: source CDL.Utilities.Assert, text = message, time = row time",
        ),
    ]
}
