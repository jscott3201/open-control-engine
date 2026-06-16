//! Per-block tests for the M0 starter catalog: feedthrough classification against the `03` §3/§5
//! table, algebraic step semantics, the loop-breakers' emit-then-latch cycle, and registry
//! resolution.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Add, And, Block, BlockKind, Constant, Greater, Limiter, MultiplyByParameter, Not, Pre,
    Subtract, Switch, UnitDelay, lookup,
};

/// Run an `[A]` block's `step_algebraic` and collect outputs in port-index order.
fn outs(b: &dyn Block, inputs: &[Value]) -> Vec<Value> {
    let mut v = Vec::new();
    b.step_algebraic(inputs, 0.0, &mut |idx, val| {
        assert_eq!(idx, v.len(), "outputs must be emitted in port-index order");
        v.push(val);
    });
    v
}

/// Run an `[S]` block's `emit_from_state` and collect outputs.
fn emit(b: &dyn Block, inputs: &[Value], region: &[u64]) -> Vec<Value> {
    let mut v = Vec::new();
    b.emit_from_state(inputs, 0.0, region, &mut |idx, val| {
        assert_eq!(idx, v.len());
        v.push(val);
    });
    v
}

#[test]
fn feedthrough_classification_matches_spec() {
    // [A] math/logic blocks feed through every (in, out) pair; the two loop-breakers cut.
    assert!(Add.feeds_through(0, 0) && Add.feeds_through(1, 0));
    assert!(Subtract.feeds_through(0, 0) && Subtract.feeds_through(1, 0));
    assert!(MultiplyByParameter { k: 1.0 }.feeds_through(0, 0));
    assert!(
        Limiter {
            u_min: 0.0,
            u_max: 1.0
        }
        .feeds_through(0, 0)
    );
    assert!(Greater.feeds_through(0, 0) && Greater.feeds_through(1, 0));
    assert!(And.feeds_through(0, 0) && And.feeds_through(1, 0));
    assert!(Not.feeds_through(0, 0));
    assert!(Switch.feeds_through(0, 0) && Switch.feeds_through(1, 0) && Switch.feeds_through(2, 0));

    assert!(!Constant { k: 0.0 }.feeds_through(0, 0)); // no inputs
    assert!(!Pre::default().feeds_through(0, 0)); // THE cut
    assert!(!UnitDelay::default().feeds_through(0, 0)); // discrete cut
    assert_eq!(Pre::default().kind(), BlockKind::Stateful);
    assert_eq!(UnitDelay::default().kind(), BlockKind::Stateful);
    assert_eq!(Add.kind(), BlockKind::Algebraic);
}

#[test]
fn arithmetic_blocks() {
    assert!(outs(&Constant { k: 3.5 }, &[])[0].bit_eq(&Value::Real(3.5)));
    assert!(outs(&Add, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(7.0)));
    assert!(outs(&Subtract, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(-3.0)));
    assert!(
        outs(&MultiplyByParameter { k: 2.0 }, &[Value::Real(4.0)])[0].bit_eq(&Value::Real(8.0))
    );
}

#[test]
fn limiter_clips_and_tolerates_inverted_bounds() {
    let lim = Limiter {
        u_min: 0.0,
        u_max: 10.0,
    };
    assert!(outs(&lim, &[Value::Real(-5.0)])[0].bit_eq(&Value::Real(0.0)));
    assert!(outs(&lim, &[Value::Real(15.0)])[0].bit_eq(&Value::Real(10.0)));
    assert!(outs(&lim, &[Value::Real(5.0)])[0].bit_eq(&Value::Real(5.0)));
    // Inverted bounds must not panic; they degrade to u_max deterministically.
    let inv = Limiter {
        u_min: 10.0,
        u_max: 0.0,
    };
    assert!(outs(&inv, &[Value::Real(5.0)])[0].bit_eq(&Value::Real(0.0)));
}

#[test]
fn comparison_and_logical_blocks() {
    assert!(outs(&Greater, &[Value::Real(3.0), Value::Real(2.0)])[0].bit_eq(&Value::Boolean(true)));
    assert!(
        outs(&Greater, &[Value::Real(2.0), Value::Real(2.0)])[0].bit_eq(&Value::Boolean(false))
    );
    assert!(
        outs(&And, &[Value::Boolean(true), Value::Boolean(false)])[0]
            .bit_eq(&Value::Boolean(false))
    );
    assert!(
        outs(&And, &[Value::Boolean(true), Value::Boolean(true)])[0].bit_eq(&Value::Boolean(true))
    );
    assert!(outs(&Not, &[Value::Boolean(false)])[0].bit_eq(&Value::Boolean(true)));
}

#[test]
fn switch_selects_on_the_middle_boolean() {
    let inputs_true = [Value::Real(1.0), Value::Boolean(true), Value::Real(9.0)];
    assert!(outs(&Switch, &inputs_true)[0].bit_eq(&Value::Real(1.0)));
    let inputs_false = [Value::Real(1.0), Value::Boolean(false), Value::Real(9.0)];
    assert!(outs(&Switch, &inputs_false)[0].bit_eq(&Value::Real(9.0)));
}

#[test]
fn pre_emits_prior_then_latches_current() {
    let pre = Pre { y_start: true };
    assert_eq!(pre.state_len(), 1);
    let mut region = vec![0u64; pre.state_len()];
    pre.init_state(&mut region, &ParamTable::default());

    // Emit returns the prior (seed) value before any update this tick.
    assert!(emit(&pre, &[Value::Boolean(false)], &region)[0].bit_eq(&Value::Boolean(true)));
    pre.update_state(&[Value::Boolean(false)], 0.0, &mut region);
    // Next tick: the latched `false` is emitted.
    assert!(emit(&pre, &[Value::Boolean(true)], &region)[0].bit_eq(&Value::Boolean(false)));
}

#[test]
fn unit_delay_holds_prior_sample() {
    let ud = UnitDelay { y_start: 2.5 };
    assert_eq!(ud.state_len(), 1);
    let mut region = vec![0u64; ud.state_len()];
    ud.init_state(&mut region, &ParamTable::default());

    assert!(emit(&ud, &[Value::Real(99.0)], &region)[0].bit_eq(&Value::Real(2.5))); // seed
    ud.update_state(&[Value::Real(99.0)], 0.0, &mut region);
    assert!(emit(&ud, &[Value::Real(7.0)], &region)[0].bit_eq(&Value::Real(99.0))); // prior sample
}

#[test]
fn registry_resolves_canonical_paths() {
    const PATHS: &[&str] = &[
        "CDL.Reals.Sources.Constant",
        "CDL.Reals.Add",
        "CDL.Reals.Subtract",
        "CDL.Reals.MultiplyByParameter",
        "CDL.Reals.Limiter",
        "CDL.Reals.Greater",
        "CDL.Reals.Switch",
        "CDL.Logical.And",
        "CDL.Logical.Not",
        "CDL.Logical.Pre",
        "CDL.Discrete.UnitDelay",
    ];
    for path in PATHS {
        let entry = lookup(path).unwrap_or_else(|| panic!("missing catalog entry: {path}"));
        assert_eq!(entry.class_path, *path);
        // The constructor builds the matching class.
        let blk = (entry.make)(&ParamTable::default());
        assert_eq!(blk.signature().class_path, *path);
    }
    assert!(lookup("CDL.Reals.Nonexistent").is_none());
}

#[test]
fn registry_make_resolves_parameters() {
    let params = ParamTable {
        values: vec![(Arc::from("k"), Value::Real(4.0))],
    };
    let constant = (lookup("CDL.Reals.Sources.Constant").unwrap().make)(&params);
    assert!(outs(constant.as_ref(), &[])[0].bit_eq(&Value::Real(4.0)));

    let delay_params = ParamTable {
        values: vec![(Arc::from("y_start"), Value::Real(1.25))],
    };
    let delay = (lookup("CDL.Discrete.UnitDelay").unwrap().make)(&delay_params);
    let mut region = vec![0u64; delay.state_len()];
    delay.init_state(&mut region, &delay_params);
    assert!(emit(delay.as_ref(), &[Value::Real(0.0)], &region)[0].bit_eq(&Value::Real(1.25)));
}

#[test]
fn real_param_promotes_integer_to_real() {
    // Modelica/CDL Int→Real promotion: an integer literal bound to a `Real` parameter is its real
    // value, NOT silently dropped to the constructor default. CXF can carry a bare integer for a
    // Real parameter (no `isOfDataType` re-types it), so a non-zero integer `y_start`/`k` must reach
    // the block. Tripwire for the silent-wrong-initial-state hole (M1-PR-5 review C-3).
    let k_int = ParamTable {
        values: vec![(Arc::from("k"), Value::Integer(5))],
    };
    let constant = (lookup("CDL.Reals.Sources.Constant").unwrap().make)(&k_int);
    assert!(
        outs(constant.as_ref(), &[])[0].bit_eq(&Value::Real(5.0)),
        "Integer(5) bound to Real param k must promote to 5.0, not default to 0.0"
    );

    // A non-zero integer UnitDelay.y_start must seed the loop-breaker's initial output to 5.0.
    let y_int = ParamTable {
        values: vec![(Arc::from("y_start"), Value::Integer(5))],
    };
    let delay = (lookup("CDL.Discrete.UnitDelay").unwrap().make)(&y_int);
    let mut region = vec![0u64; delay.state_len()];
    delay.init_state(&mut region, &y_int);
    assert!(
        emit(delay.as_ref(), &[Value::Real(0.0)], &region)[0].bit_eq(&Value::Real(5.0)),
        "Integer(5) y_start must seed the initial output to 5.0, not silently default to 0.0"
    );
}

/// Compile-time guard (R-API-PY-2): the `Block` trait object is `Send + Sync`, localized to
/// oce-blocks' own boundary. The `Block: Send + Sync` supertrait already forces every `impl Block`
/// to be `Send + Sync` at its impl site, so a future non-`Send` block class fails to compile; this
/// also pins the **trait object** (`dyn Block` / `Box<dyn Block>`) so the engine's
/// `Vec<Box<dyn Block>>` stays shareable. Never called — its compilation IS the assertion.
#[allow(dead_code)]
fn _assert_block_object_send_sync() {
    fn needs<T: Send + Sync + ?Sized>() {}
    needs::<dyn Block>();
    needs::<Box<dyn Block>>();
}
