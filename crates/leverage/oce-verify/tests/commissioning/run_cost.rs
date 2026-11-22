//! What a commissioning run costs, in allocations, never in wall-clock time (the review of
//! GitHub #744): the whole run's peak at a middle size, per frame.
//!
//! Measured with `allocation-counter` on the running thread, over `run_commissioning` itself:
//! the trend read, the merged inputs, the replay's trace (four outputs and five retained inputs,
//! 67 bytes a row), the instants and one output's held series, mask and curves at a time. The
//! ceiling, 480 bytes a frame, carries headroom over the 365 measured at 20,000 frames (the
//! review of GitHub #744 measured about 250 at 2^20 frames, where vector growth amortises).

use oce_verify::CommissionOptions;

use super::{body_of, commission, run_inputs, run_spec_edited};

const FRAMES: u32 = 20_000;

/// The fixture's sequence over `FRAMES` frames a second apart, compared every second.
fn long_run() -> (allocation_counter::AllocationInfo, u64) {
    let mut inputs = String::from("time,Supply Temp,Offset,Enable,Stage,Limit\n");
    let mut outputs = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for t in 0..FRAMES {
        let warm = t % 7 == 0;
        inputs.push_str(&format!(
            "{t},{},1,true,{},50\n",
            if warm { 21 } else { 20 },
            t % 3
        ));
        outputs.push_str(&format!(
            "{t},{},true,{},50\n",
            if warm { 22 } else { 21 },
            t % 3
        ));
    }
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        &format!(
            r#""window": {{ "start": 0, "stop": {} }}, "sampling": 1,"#,
            FRAMES - 1
        ),
    )]);
    let files = run_inputs(&[
        ("inputs.csv", inputs.as_bytes()),
        ("outputs.csv", outputs.as_bytes()),
    ]);
    let mut compared = 0;
    let info = allocation_counter::measure(|| {
        let run = commission(&spec, &files, &CommissionOptions::default()).expect("runs");
        compared = body_of(&run.receipt).outputs[0].compared;
    });
    (info, compared)
}

#[test]
fn a_runs_peak_is_bounded_per_frame() {
    let (info, compared) = long_run();
    assert_eq!(compared, u64::from(FRAMES));
    let per_frame = info.bytes_max / u64::from(FRAMES);
    assert!(
        per_frame <= 480,
        "{per_frame} bytes a frame at peak: {info:?}"
    );
}
