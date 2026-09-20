//! Latest-state inspection and warning collection for complete-frame execution.

use std::cell::RefCell;

use oce_blocks::Diagnostics;
use oce_model::Value;
use oce_store::Store;

use crate::{Engine, OcError};

/// An owned warning emitted during a completed frame, not an equipment interlock.
#[derive(Clone, Debug)]
pub struct AssertEvent {
    /// Producer-supplied diagnostic source, currently class-level rather than instance identity.
    pub block: String,
    /// Producer display text (`message` for CDL Assert).
    pub message: String,
    /// Model time in seconds at which the warning was emitted.
    pub t: f64,
    /// Advisory severity; execution continues.
    pub level: AssertLevel,
}

/// Supported warning-only severity; not Modelica AssertionLevel or a safety interlock.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum AssertLevel {
    /// Advisory warning; the transition completes.
    #[default]
    Warning,
}

#[derive(Default)]
pub(crate) struct AssertCollector {
    pub(crate) events: RefCell<Vec<AssertEvent>>,
}

impl Diagnostics for AssertCollector {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.events.borrow_mut().push(AssertEvent {
            block: source.to_string(),
            message: message.to_string(),
            t,
            level: AssertLevel::Warning,
        });
    }
}

impl<S: Store> Engine<S> {
    /// Inspect the latest output value by connector path or declared boundary-output alias.
    ///
    /// This is a latest-state, non-receipt view. Load, restore and parameter resume can replace
    /// the observed state without executing a frame. Use [`crate::CompletedFrame`] for immutable,
    /// correlated boundary outputs and warnings from an accepted transition. Internal connector
    /// observations do not enlarge that receipt's executable boundary output set.
    ///
    /// Values retain native types, declared units and exact Real bits. No Store calls occur.
    ///
    /// # Errors
    /// Returns [`OcError::UnknownPoint`] if the name is not a known output. Does not panic.
    pub fn get_output(&self, point: &str) -> Result<Value, OcError> {
        let id = self
            .io
            .resolve_output(point)
            .ok_or_else(|| OcError::UnknownPoint(point.to_owned()))?;
        Ok(self.state.values[id.0 as usize].clone())
    }
}
