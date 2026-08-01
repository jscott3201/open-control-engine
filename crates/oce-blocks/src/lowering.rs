//! Reserved identity blocks used only by importer lowering.
//!
//! These classes are not authored CDL. The CXF resolver synthesizes them to preserve a composite
//! boundary input connected directly to a boundary output. Each block is stateless, parameterless,
//! fully feed-through, and copies its scalar `u` value to `y` without conversion.
//!
//! The `#` namespace is intentionally unsmuggleable through authored CXF: the bridge keeps only
//! the fragment after the last `#`, so an authored `urn:oce:lowering#PassThrough.*` reaches lookup
//! as `PassThrough.*`, never as one of these full reserved class paths. Only internal lowering can
//! construct these registry identities.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind};

macro_rules! pass_through {
    ($name:ident, $class:literal, $kind:ident, $variant:ident) => {
        #[doc = concat!(
                    "`",
                    $class,
                    "` — reserved scalar identity used by CXF boundary pass-through lowering."
                )]
        ///
        /// It has one input `u`, one output `y`, no parameters or state, and never panics for a
        /// resolver-validated input slice.
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name;

        impl Block for $name {
            fn signature(&self) -> &'static BlockSignature {
                static SIG: BlockSignature = BlockSignature {
                    class_path: $class,
                    inputs: &[PortKind::$kind],
                    outputs: &[PortKind::$kind],
                    stateful: false,
                };
                &SIG
            }

            fn kind(&self) -> BlockKind {
                BlockKind::Algebraic
            }

            fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
                true
            }

            fn step_algebraic(
                &self,
                _ctx: &Ctx<'_>,
                inputs: &[Value],
                emit: &mut dyn FnMut(usize, Value),
            ) {
                let value = match inputs.first() {
                    Some(Value::$variant(value)) => Value::$variant(*value),
                    _ => Value::$variant(Default::default()),
                };
                emit(0, value);
            }
        }
    };
}

pass_through!(
    RealPassThrough,
    "urn:oce:lowering#PassThrough.Real",
    Real,
    Real
);
pass_through!(
    IntegerPassThrough,
    "urn:oce:lowering#PassThrough.Integer",
    Integer,
    Integer
);
pass_through!(
    BooleanPassThrough,
    "urn:oce:lowering#PassThrough.Boolean",
    Boolean,
    Boolean
);

#[cfg(test)]
mod tests {
    use super::{BooleanPassThrough, IntegerPassThrough, RealPassThrough};
    use crate::{Block, Ctx, NoopDiagnostics};
    use oce_model::Value;

    fn step(block: &dyn Block, input: Value) -> Value {
        let diagnostics = NoopDiagnostics;
        let context = Ctx::new(0.0, &diagnostics);
        let mut output = None;
        block.step_algebraic(&context, &[input], &mut |_, value| output = Some(value));
        output.expect("identity block must emit")
    }

    #[test]
    fn scalar_values_are_copied_bit_exactly() {
        let real = Value::Real(f64::from_bits(0x7ff8_0000_0000_1234));
        assert!(step(&RealPassThrough, real.clone()).bit_eq(&real));
        assert!(
            step(&IntegerPassThrough, Value::Integer(i64::MIN)).bit_eq(&Value::Integer(i64::MIN))
        );
        assert!(step(&BooleanPassThrough, Value::Boolean(true)).bit_eq(&Value::Boolean(true)));
    }

    #[test]
    fn every_reserved_identity_is_full_feedthrough() {
        for block in [
            &RealPassThrough as &dyn Block,
            &IntegerPassThrough,
            &BooleanPassThrough,
        ] {
            assert!(block.feeds_through(0, 0));
            assert_eq!(block.state_len(), 0);
        }
    }
}
