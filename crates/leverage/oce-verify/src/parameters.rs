//! A scenario's parameter edits applied on a fresh engine, in passes (vectors note, decision 14):
//! tried at bind for every scenario before any frame runs, then applied again to the scenario's
//! own engine when it runs, by the same deterministic passes.
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

use oce_api::{Engine, OcError};

use crate::bind::setup_failed;
use crate::receipt::ReceiptValue;
use crate::refusal_classes::{mutant_param_refusal_cause, param_refusal_class};
use crate::verify_error::{VerifyError, VerifyErrorKind};

/// Apply `edits` (canonical path and typed value, sorted by path bytes) to the subject's
/// `engine` by the passes in the module documentation; nothing happens for no edits. `subject`
/// names the scenario in a refusal.
///
/// # Errors
/// `param-refused` with the class of the first waiting edit's last refusal when a pass applies
/// nothing; `replay-failed` with the cause `engine-setup` when the halt, the resume or a
/// `set_param` refusal not about the edit's value fails.
pub(crate) fn apply(
    engine: &mut Engine,
    edits: &[(&str, ReceiptValue)],
    subject: &str,
) -> Result<(), VerifyError> {
    passes(engine, edits, subject, param_refusal_class).map_err(|refusal| match refusal {
        Refusal::Setup(error) => error,
        Refusal::Edit {
            path,
            cause,
            detail,
        } => VerifyError::new(
            VerifyErrorKind::ParamRefused,
            subject,
            format!(
                "no pass admits the edit of {path}: {detail}; the edits are applied in passes in path order (decision 14)"
            ),
        )
        .with_path(path)
        .with_cause(cause),
    })
}

/// Apply `edits`, the effective edits of the scenario `scenario`, to the `engine` of the mutant
/// `mutant` (its listed path) by the same passes. The edits were resolved against the subject's
/// parameters, so the mutant may lack one: that is a refusal too.
///
/// # Errors
/// `mutant-param-refused` with the cause of the first waiting edit's last refusal (one of
/// [`crate::MUTANT_PARAM_REFUSAL_CAUSES`]) when a pass applies nothing; `replay-failed` with the
/// cause `engine-setup` as for [`apply`].
pub(crate) fn apply_to_mutant(
    engine: &mut Engine,
    edits: &[(&str, ReceiptValue)],
    subject: &str,
    mutant: &str,
    scenario: &str,
) -> Result<(), VerifyError> {
    passes(engine, edits, subject, mutant_param_refusal_cause).map_err(|refusal| match refusal {
        Refusal::Setup(error) => error,
        Refusal::Edit {
            path,
            cause,
            detail,
        } => VerifyError::new(
            VerifyErrorKind::MutantParamRefused,
            subject,
            format!(
                "mutant `{mutant}` refuses the edit of {path} in every pass, where the subject admits it (scenario `{scenario}`): {detail}"
            ),
        )
        .with_path(path)
        .with_cause(cause),
    })
}

/// Why the passes stopped short.
enum Refusal {
    /// A pass applied nothing; the first edit still waiting, the class of its last refusal and
    /// the facade's sentence.
    Edit {
        path: String,
        cause: &'static str,
        detail: String,
    },
    /// A facade call refused for a reason that is not about an edit.
    Setup(VerifyError),
}

/// The passes of the module documentation. `classify` names a refusal of `set_param` that is
/// about the edit, which then waits for the next pass; any other refusal stops them as `Setup`.
fn passes(
    engine: &mut Engine,
    edits: &[(&str, ReceiptValue)],
    subject: &str,
    classify: fn(&OcError) -> Option<&'static str>,
) -> Result<(), Refusal> {
    if edits.is_empty() {
        return Ok(());
    }
    engine
        .halt()
        .map_err(|error| Refusal::Setup(setup_failed(subject, "the parameter halt", &error)))?;
    let mut waiting: Vec<&(&str, ReceiptValue)> = edits.iter().collect();
    loop {
        let mut refused = Vec::new();
        let mut first_refusal = None;
        for edit in &waiting {
            let (path, value) = *edit;
            if let Err(error) = engine.set_param(path, value.engine_value()) {
                let Some(cause) = classify(&error) else {
                    return Err(Refusal::Setup(setup_failed(
                        subject,
                        &format!("the edit of {path}"),
                        &error,
                    )));
                };
                if first_refusal.is_none() {
                    first_refusal = Some((path, cause, error.to_string()));
                }
                refused.push(*edit);
            }
        }
        if refused.is_empty() {
            break;
        }
        if refused.len() == waiting.len()
            && let Some((path, cause, detail)) = first_refusal
        {
            return Err(Refusal::Edit {
                path: (*path).to_owned(),
                cause,
                detail,
            });
        }
        waiting = refused;
    }
    engine
        .resume()
        .map_err(|error| Refusal::Setup(setup_failed(subject, "the parameter resume", &error)))
}
