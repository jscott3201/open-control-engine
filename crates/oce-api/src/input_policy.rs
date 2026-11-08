//! Facade-owned host policy for non-finite Real boundary input values, independent of the loaded
//! model, run state and every serialized artifact.
//!
//! The policy type and the rule it selects live in the core frame driver
//! ([`oce_graph::frame::NonFiniteInputPolicy`], re-exported as [`crate::NonFiniteInputPolicy`]);
//! the facade stores the host's choice and passes it to every preparation and execution.

use crate::{Engine, NonFiniteInputPolicy};

impl Engine {
    /// Effective policy for non-finite Real boundary input values.
    ///
    /// Defaults to [`NonFiniteInputPolicy::Admit`]; persists across successful and failed loads,
    /// parameter resumes and state restores. Never panics.
    #[must_use]
    pub fn non_finite_input_policy(&self) -> NonFiniteInputPolicy {
        self.non_finite_input_policy
    }

    /// Select the policy for non-finite Real boundary input values.
    ///
    /// Takes effect for every later [`Engine::prepare_frame`] and [`Engine::execute_frame`] call,
    /// including the execution of a plan prepared before the change: under
    /// [`NonFiniteInputPolicy::Refuse`] execution refuses such a plan if it carries a non-finite
    /// Real input. Changing the policy does not touch the loaded executable, run state, model
    /// time, accepted-frame sequence, restore readiness or snapshot bytes, and does not
    /// stale an outstanding plan. Infallible and never panics; production hosts are recommended
    /// to select `Refuse`.
    pub fn set_non_finite_input_policy(&mut self, policy: NonFiniteInputPolicy) {
        self.non_finite_input_policy = policy;
    }
}
