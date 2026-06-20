//! `CDL.Utilities` blocks that interact with the diagnostics seam instead of signal outputs.

use std::sync::Arc;

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool};

/// `CDL.Utilities.Assert` — warning sink with no signal outputs.
///
/// The upstream implementation is `Buildings/Controls/OBC/CDL/Utilities/Assert.mo`: line 11 is
/// the stateless equation `assert(u, message, AssertionLevel.warning)`. There is no `pre`, `edge`,
/// latch, or warn-once state in lines 1-12, so the engine emits one warning on every evaluation
/// whose Boolean input is `false`. The warning is routed only through [`Ctx::warn`]; the block has
/// no output connector and does not mutate scheduler state.
#[derive(Clone, Debug)]
pub struct Assert {
    pub(crate) message: Arc<str>,
}

impl Default for Assert {
    fn default() -> Self {
        Self {
            message: Arc::from(""),
        }
    }
}

impl Block for Assert {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Utilities.Assert",
            inputs: &[PortKind::Boolean],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], _emit: &mut dyn FnMut(usize, Value)) {
        if !read_bool(inputs, 0) {
            ctx.warn(self.signature().class_path, &self.message);
        }
    }
}
