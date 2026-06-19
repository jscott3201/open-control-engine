//! `CDL.Discrete` starter block (`03` §4.6): `UnitDelay`, the discrete one-sample loop-breaker.

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_real};

/// `CDL.Discrete.UnitDelay` — `y(k) = u(k−1)`, a one-sample `Real` delay and a discrete
/// loop-breaker (`03` §4.6): stateful with `feeds_through == false`, so it cuts the
/// direct-feedthrough DAG. The held value lives in one `[S]` state word (`f64::to_bits`), seeded
/// from the `y_start` parameter (default `0.0`).
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitDelay {
    pub(crate) y_start: f64,
}

impl Block for UnitDelay {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.UnitDelay",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // the discrete loop cut: output is the prior sample, not the current input
    }
    fn state_len(&self) -> usize {
        1 // one word holds the one-sample-delayed Real (as f64 bits)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = self.y_start.to_bits();
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Real(f64::from_bits(region[0]))); // the prior sample, held since last tick
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = read_real(inputs, 0).to_bits(); // latch the current input for next tick
    }
}
