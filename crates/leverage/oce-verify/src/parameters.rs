//! A scenario's parameter edits applied on its fresh engine, in passes (vectors note, decision
//! 14).
//!
//! `set_param` checks each class's rules against the table as it stands (`Reals.Hysteresis`
//! requires `uLow <= uHigh`), so the result of a list of edits depends on their order. The edits
//! are applied inside one halt, in passes: each pass tries the edits still waiting in ascending
//! canonical-path byte order; a refused edit leaves the table unchanged and waits for the next
//! pass; a pass that applies nothing ends it, and the first edit still waiting is
//! `param-refused`. Then `resume`. The order is a function of the resolved paths and values
//! alone.
//!
//! Lowering both hysteresis thresholds therefore succeeds in two passes (`uHigh` sorts first and
//! would drop below the old `uLow`, so it waits for `uLow`), and raising both in one.
//!
//! **The documented limitation.** The passes can refuse a table that only another order of edits
//! reaches: an edit applied in an early pass can make a later one invalid where the reverse order
//! would not. That is `param-refused` too. It is kept, not worked around: such a vector splits
//! the edits across scenarios, or the subject is emitted with the values.

use oce_api::Engine;

use crate::bind::setup_failed;
use crate::receipt::ReceiptValue;
use crate::refusal_classes::param_refusal_class;
use crate::verify_error::{VerifyError, VerifyErrorKind};

/// Apply `edits` (canonical path and typed value, sorted by path bytes) to `engine` by the passes
/// in the module documentation; nothing happens for no edits. `subject` names the scenario in a
/// refusal.
///
/// # Errors
/// `param-refused` with the class of the first waiting edit's last refusal when a pass applies
/// nothing; `replay-failed` with the cause `engine-setup` when the halt, the resume or a
/// `set_param` refusal not about the edit's value fails.
pub(crate) fn apply(
    engine: &mut Engine,
    edits: &[(String, ReceiptValue)],
    subject: &str,
) -> Result<(), VerifyError> {
    if edits.is_empty() {
        return Ok(());
    }
    engine
        .halt()
        .map_err(|error| setup_failed(subject, "the parameter halt", &error))?;
    let mut waiting: Vec<&(String, ReceiptValue)> = edits.iter().collect();
    loop {
        let mut refused = Vec::new();
        let mut first_refusal = None;
        for edit in &waiting {
            let (path, value) = *edit;
            match engine.set_param(path, value.engine_value()) {
                Ok(()) => {}
                Err(error) => {
                    let Some(class) = param_refusal_class(&error) else {
                        return Err(setup_failed(
                            subject,
                            &format!("the edit of {path}"),
                            &error,
                        ));
                    };
                    if first_refusal.is_none() {
                        first_refusal = Some((path, class, error.to_string()));
                    }
                    refused.push(*edit);
                }
            }
        }
        if refused.is_empty() {
            break;
        }
        if refused.len() == waiting.len()
            && let Some((path, class, detail)) = first_refusal
        {
            return Err(VerifyError::new(
                VerifyErrorKind::ParamRefused,
                subject,
                format!(
                    "no pass admits the edit of {path}: {detail}; the edits are applied in passes in path order (decision 14)"
                ),
            )
            .with_path(path.clone())
            .with_cause(class));
        }
        waiting = refused;
    }
    engine
        .resume()
        .map_err(|error| setup_failed(subject, "the parameter resume", &error))
}
