//! Registry and feedthrough contract tests for logical timing blocks.

use oce_model::ParamTable;

use super::{
    Block, BlockKind, FallingEdge, IntegerChange, Latch, LogicalChange, OnCounter, Timer,
    TimerAccumulating, Toggle, TrueDelay, TrueFalseHold, TrueHoldWithReset, lookup,
};

#[test]
fn registry_paths_and_feedthrough_classification_are_complete() {
    let paths = [
        "CDL.Logical.FallingEdge",
        "CDL.Logical.Change",
        "CDL.Logical.Latch",
        "CDL.Logical.Toggle",
        "CDL.Logical.Timer",
        "CDL.Logical.TimerAccumulating",
        "CDL.Logical.TrueDelay",
        "CDL.Logical.TrueFalseHold",
        "CDL.Logical.TrueHoldWithReset",
        "CDL.Integers.OnCounter",
        "CDL.Integers.Change",
    ];
    for path in paths {
        let block = (lookup(path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .make)(&ParamTable::default());
        assert_eq!(block.signature().class_path, path);
        assert_eq!(block.kind(), BlockKind::Stateful, "{path}");
    }

    assert!(FallingEdge::default().feeds_through(0, 0));
    assert!(LogicalChange::default().feeds_through(0, 0));
    assert!(Latch.feeds_through(0, 0) && Latch.feeds_through(1, 0));
    assert!(Toggle.feeds_through(0, 0) && Toggle.feeds_through(1, 0));
    assert!(Timer::default().feeds_through(0, 0) && Timer::default().feeds_through(0, 1));
    assert!(
        TimerAccumulating::default().feeds_through(0, 0)
            && TimerAccumulating::default().feeds_through(1, 0)
            && TimerAccumulating::default().feeds_through(0, 1)
            && TimerAccumulating::default().feeds_through(1, 1)
    );
    assert!(TrueDelay::default().feeds_through(0, 0));
    assert!(TrueFalseHold::default().feeds_through(0, 0));
    assert!(
        TrueHoldWithReset::default().feeds_through(0, 0)
            && TrueHoldWithReset::default().feeds_through(1, 0)
    );
    assert!(
        IntegerChange::default().feeds_through(0, 0)
            && IntegerChange::default().feeds_through(0, 1)
            && IntegerChange::default().feeds_through(0, 2)
    );
    assert!(!OnCounter::default().feeds_through(0, 0));
    assert!(!OnCounter::default().feeds_through(1, 0));
}
