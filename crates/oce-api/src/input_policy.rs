//! Facade-owned host policy for non-finite Real boundary input values, independent of the loaded
//! model, run state and every serialized artifact.

use oce_model::Value;
use oce_store::Store;

use crate::{Engine, OcError};

/// What frame preparation does with a NaN (either sign, any payload) or infinite value supplied
/// for a Real boundary input.
///
/// The policy governs only which frames the engine accepts; it never changes how an accepted
/// frame evolves state, and blocks compute exactly the same bits under either variant. It is host
/// configuration, like [`Engine::cxf_byte_limit`]: it persists across loads, reloads, parameter
/// resumes and state restores, and no snapshot, checkpoint or replay record carries it.
///
/// The default is [`Self::Admit`], which preserves the existing contract that the bound
/// strict-bit conformance evidence was captured under. **Production hosts are recommended to
/// select [`Self::Refuse`].** The default is planned to become `Refuse` at the next
/// native-evidence refresh (GitHub issue #363). Non-exhaustive so further policies can be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum NonFiniteInputPolicy {
    /// Accept non-finite Real values on unbounded inputs and pass them to blocks unchanged; a
    /// declared comparison bound still refuses NaN and the infinity beyond it with
    /// [`OcError::InputDomain`]. This is the default.
    #[default]
    Admit,
    /// Refuse every non-finite Real input value, bounded or not, with
    /// [`OcError::InputNonFinite`] naming the canonical input. Preparation checks it after the
    /// input's type and before its domain; execution rechecks a plan against the policy in
    /// force, so a plan prepared under `Admit` cannot carry a non-finite value past a later
    /// switch to `Refuse`.
    Refuse,
}

impl<S: Store> Engine<S> {
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
    /// time, accepted-frame sequence, Store, restore readiness or snapshot bytes, and does not
    /// stale an outstanding plan. Infallible and never panics; production hosts are recommended
    /// to select `Refuse`.
    pub fn set_non_finite_input_policy(&mut self, policy: NonFiniteInputPolicy) {
        self.non_finite_input_policy = policy;
    }

    /// Refuse a non-finite Real `value` for the input at `path` when the policy is `Refuse`.
    pub(crate) fn check_input_finite(&self, path: &str, value: &Value) -> Result<(), OcError> {
        match (self.non_finite_input_policy, value) {
            (NonFiniteInputPolicy::Refuse, Value::Real(real)) if !real.is_finite() => {
                Err(OcError::InputNonFinite(path.to_owned()))
            }
            _ => Ok(()),
        }
    }
}
