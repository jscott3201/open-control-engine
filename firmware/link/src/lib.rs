#![forbid(unsafe_code)]
#![no_std]
//! `oce-firmware-link` — the run a bare-metal firmware image performs (issue #402): one checked-in
//! compiled-schedule blob, decoded by the `no_std` engine entry ([`oce_graph::blob::Executable`])
//! and driven through [`TICKS`] HostTick v1 frames on the host schedule's deterministic inputs.
//!
//! The library is `no_std` on every target and links no `core::fmt`: its output is ASCII lines
//! built byte by byte ([`line`](mod@line)), so an image's size measures the engine rather than a
//! formatter.
//! The `blob-frames` bin runs [`run`] on a bare-metal target, writing each line to semihosting
//! stdout and exiting with the [`Verdict`]'s code; on a host the same bin is the reference runner
//! and writes the same lines to stdout. `.github/workflows/qemu.yml` runs the images under QEMU and
//! diffs their lines against the host runner's.
//!
//! What the lines carry, in order:
//!
//! 1. `fixture <name> frames <n>`;
//! 2. one `frame <k> seq <sequence> digest <hex>` per frame: the run digest
//!    ([`schedule::RunDigest`]) after frame `k`, so a divergence shows at the first frame it
//!    happens in;
//! 3. `digest <hex> expected <hex>`, the final digest against the fixture's line in
//!    `crates/oce-graph/tests/fixtures/blob_frame_digests.tsv`, the table the facade's dual-path
//!    test and `oce-graph`'s `blob_execution` test both check;
//! 4. the verdict: `PASS`, `MISMATCH`, or `FAULT <what>`.
//!
//! A run allocates what [`Executable::from_blob`] allocates and one input vector; the frames
//! allocate nothing more unless the fixture has a String input.

extern crate alloc;

pub mod line;
pub mod schedule;

use alloc::vec::Vec;

use oce_blocks::NoopDiagnostics;
use oce_graph::blob::Executable;

pub use schedule::TICKS;
use schedule::{RunDigest, input_value, time_at};

/// The checked-in golden the image embeds: `multizone_vav_supply_fan`, the G36 multi-zone VAV
/// supply fan sequence (a `PIDWithReset` loop, thresholds with hysteresis and an `Assert`), so the
/// run carries integrator state across frames.
pub const FIXTURE: &str = "multizone_vav_supply_fan";

/// The fixture's compiled-schedule blob, linked into flash as `.rodata`.
pub static BLOB: &[u8] =
    include_bytes!("../../../crates/oce-api/tests/fixtures/blob/multizone_vav_supply_fan.oceb");

/// The run digest table, one `name<TAB>digest` line per golden, as the host tests check it.
pub const DIGEST_TABLE: &str =
    include_str!("../../../crates/oce-graph/tests/fixtures/blob_frame_digests.tsv");

/// How a run ended, and the exit code the image reports it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every frame ran and the final digest equals the table's.
    Pass,
    /// Every frame ran, but the final digest differs from the table's.
    Mismatch,
    /// The engine refused: the blob did not decode, a frame was refused, or the table has no
    /// digest for the fixture.
    Fault,
}

impl Verdict {
    /// The process exit code: 0, 1 and 3, as rusty-bacnet's emulation images use them.
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Pass => 0,
            Self::Mismatch => 1,
            Self::Fault => 3,
        }
    }
}

/// The exit code of a panic, the status a Rust program exits with after one on a host.
pub const PANIC_EXIT_CODE: i32 = 101;

/// Where a run's output goes.
pub trait Sink {
    /// One output line, newline included.
    fn line(&mut self, bytes: &[u8]);

    /// Called once after the last frame, while the executable is still alive, so an image can
    /// measure what the run holds (its allocator's use). Does nothing by default.
    fn after_frames(&mut self) {}
}

impl<F: FnMut(&[u8])> Sink for F {
    fn line(&mut self, bytes: &[u8]) {
        self(bytes);
    }
}

/// The digest `table` lists for fixture `name`, or `None` when it lists none or its digest is not
/// sixteen lowercase hex digits.
#[must_use]
pub fn expected_digest(table: &str, name: &str) -> Option<u64> {
    table.lines().find_map(|row| {
        let (row_name, digest) = row.split_once('\t')?;
        if row_name != name || digest.len() != 16 {
            return None;
        }
        digest.bytes().try_fold(0_u64, |acc, byte| {
            let nibble = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => return None,
            };
            Some(acc << 4 | u64::from(nibble))
        })
    })
}

/// Run the embedded [`FIXTURE`] for [`TICKS`] frames against its line in [`DIGEST_TABLE`],
/// handing each output line, newline included, to `out`.
pub fn run(out: &mut impl Sink) -> Verdict {
    run_blob(FIXTURE, BLOB, expected_digest(DIGEST_TABLE, FIXTURE), out)
}

/// Run `blob`, named `name`, for [`TICKS`] frames and compare the final run digest with
/// `expected`, handing each output line to `out`. [`run`] is this on the embedded fixture; the
/// host tests call it on every golden.
pub fn run_blob(name: &str, blob: &[u8], expected: Option<u64>, out: &mut impl Sink) -> Verdict {
    let mut text = line::Line::new();
    text.push_str("fixture ")
        .push_str(name)
        .push_str(" frames ")
        .push_dec(u64::from(TICKS));
    out.line(text.finish());

    let Some(expected) = expected else {
        out.line(
            line::Line::new()
                .push_str("FAULT no digest for the fixture")
                .finish(),
        );
        return Verdict::Fault;
    };
    let Ok(mut executable) = Executable::from_blob(blob) else {
        out.line(
            line::Line::new()
                .push_str("FAULT the blob did not decode")
                .finish(),
        );
        return Verdict::Fault;
    };

    let mut inputs = Vec::with_capacity(executable.inputs().len());
    let mut digest = RunDigest::default();
    for k in 0..TICKS {
        inputs.clear();
        inputs.extend(executable.inputs().iter().enumerate().map(|(slot, input)| {
            input_value(
                input.value_type,
                input.min.as_ref(),
                input.max.as_ref(),
                slot,
                k,
            )
        }));
        let time = time_at(k);
        let Ok(sequence) = executable.execute_frame(time, &inputs, &NoopDiagnostics) else {
            out.line(
                line::Line::new()
                    .push_str("FAULT frame ")
                    .push_dec(u64::from(k))
                    .push_str(" refused")
                    .finish(),
            );
            return Verdict::Fault;
        };
        let state = &executable.decoded().state;
        digest.frame(
            time,
            sequence,
            executable.output_values(),
            &state.values,
            &state.words,
        );
        out.line(
            line::Line::new()
                .push_str("frame ")
                .push_dec(u64::from(k))
                .push_str(" seq ")
                .push_dec(sequence)
                .push_str(" digest ")
                .push_hex(digest.get())
                .finish(),
        );
    }

    out.after_frames();
    out.line(
        line::Line::new()
            .push_str("digest ")
            .push_hex(digest.get())
            .push_str(" expected ")
            .push_hex(expected)
            .finish(),
    );
    let verdict = if digest.get() == expected {
        Verdict::Pass
    } else {
        Verdict::Mismatch
    };
    out.line(
        line::Line::new()
            .push_str(match verdict {
                Verdict::Pass => "PASS",
                _ => "MISMATCH",
            })
            .finish(),
    );
    verdict
}
