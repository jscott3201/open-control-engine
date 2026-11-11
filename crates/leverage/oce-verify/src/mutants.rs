//! Mutant mode (vectors note, "Mutant mode"; decisions 27 to 30).
//!
//! A **mutant** is a CXF document that should fail the vectors: a deliberate defect in the
//! subject. The set lists each by path ([`crate::Mutant`]); the caller hands the run each one's
//! bytes under that path ([`crate::VectorInputs::with_mutants`]); `oce-verify` opens no file.
//!
//! **Validation**, at bind, after the subject's parameter edits are tried and before the frame
//! budget, so before any frame runs. In this order, the first refusal winning:
//!
//! 1. the bytes supplied, in the order given: a path the set does not list is `mutant-unlisted`,
//!    a path given twice `duplicate-mutant`;
//! 2. the mutants listed, in file order: one with no bytes is `mutant-missing`;
//! 3. the bytes together against [`crate::VerifyLimits::max_mutant_bytes`]:
//!    `too-many-mutant-bytes`, decided from the lengths before any mutant is loaded;
//! 4. each listed mutant in file order, on a fresh engine: a load refusal is `mutant-load-failed`
//!    (with the engine's `DiagCode` spellings); then its boundary against the subject's, input
//!    paths and types (`Engine::input_definitions`) and output paths and types
//!    (`FrameSlots::outputs`, `output_types`), input bounds not compared:
//!    `mutant-boundary-differs` naming the first port that differs; then its canonical content
//!    id against the subject's as loaded: `mutant-identical` when they are equal; then the
//!    scenarios' parameter edits (owner ruling of 2026-11-11): each distinct effective edit list,
//!    in the order of the first scenario that has it, is applied to a fresh engine of the mutant
//!    by decision 14's passes, so a mutant that refuses an edit the subject admits (its class
//!    rules refuse the value, or it lacks the parameter, or has it with another type, or as a
//!    structural parameter) is `mutant-param-refused`. A set with no edits loads no mutant twice.
//!
//! An invalid mutant is an error and no receipt exists; there is no `invalid` status.
//!
//! **Scoring**, after every scenario of the subject has run, and only when the subject passed:
//! each mutant in file order runs the scenarios in file order, each on a freshly loaded engine of
//! the mutant with the scenario's parameter edits, exactly as the subject ran them, and stops at
//! the first scenario that fails on it (a failed instant, or a frame-class refusal, as for the
//! subject). That mutant is **caught**, and the receipt names the scenario and its first failure
//! by tick then expectation index; a mutant no scenario fails on **survived**. A run-class
//! refusal or a replay error is an error, as for the subject. Against a subject that did not
//! pass, the section is `not_run` and no mutant runs.

use oce_api::{Engine, InputDefinition, ValueType};

use crate::bind::{self, BoundScenario};
use crate::compare::{self, Caps};
use crate::limits::VerifyLimits;
use crate::model::Mutant;
use crate::parameters;
use crate::receipt::{
    MutantCatch, MutantEntry, MutantSection, MutantStatus, ReceiptValue, ScoreStatus,
};
use crate::run;
use crate::typing::type_name;
use crate::vector_set::VectorSet;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// One listed mutant, validated: its bytes and what the receipt records of it.
pub(crate) struct BoundMutant<'a> {
    /// The path, as the set writes it.
    pub(crate) path: String,
    /// `fnv1a128:<32 hex>` of its bytes.
    pub(crate) digest: String,
    /// Its canonical content id as loaded.
    pub(crate) content_id: String,
    /// Its CXF bytes.
    pub(crate) bytes: &'a [u8],
}

/// The subject's boundary and identity, as every mutant is held to it.
pub(crate) struct SubjectBoundary<'b> {
    /// The boundary inputs in slot order.
    pub(crate) inputs: &'b [InputDefinition],
    /// The boundary outputs in output-slot order.
    pub(crate) outputs: &'b [String],
    /// The outputs' declared types, in the same order.
    pub(crate) output_types: &'b [ValueType],
    /// The subject's canonical content id as loaded.
    pub(crate) content_id: &'b str,
}

/// Validate the mutants `listed` by the set against the bytes `supplied` and the subject, in the
/// order the module documentation gives.
///
/// # Errors
/// `mutant-unlisted`, `duplicate-mutant`, `mutant-missing`, `too-many-mutant-bytes`,
/// `mutant-load-failed`, `mutant-boundary-differs` or `mutant-identical`; `replay-failed` with the
/// cause `engine-setup` for a facade call that fails after a mutant loaded.
pub(crate) fn validate<'a>(
    listed: &[Mutant],
    supplied: &[(&'a str, &'a [u8])],
    subject: &SubjectBoundary<'_>,
    set: &VectorSet,
    scenarios: &[BoundScenario],
    limits: &VerifyLimits,
) -> Result<Vec<BoundMutant<'a>>, VerifyError> {
    for (index, (path, _)) in supplied.iter().enumerate() {
        if !listed.iter().any(|mutant| mutant.path() == *path) {
            return Err(VerifyError::new(
                Kind::MutantUnlisted,
                "mutants",
                format!("the run was given mutant bytes for `{path}`, which the set does not list"),
            ));
        }
        if supplied[..index].iter().any(|(earlier, _)| earlier == path) {
            return Err(VerifyError::new(
                Kind::DuplicateMutant,
                "mutants",
                format!("the run was given mutant bytes for `{path}` twice"),
            ));
        }
    }
    let mut bytes_of = Vec::with_capacity(listed.len());
    for (index, mutant) in listed.iter().enumerate() {
        let Some((_, bytes)) = supplied.iter().find(|(path, _)| *path == mutant.path()) else {
            return Err(VerifyError::new(
                Kind::MutantMissing,
                format!("mutants[{index}]"),
                format!("the run was given no bytes for mutant `{}`", mutant.path()),
            ));
        };
        bytes_of.push(*bytes);
    }
    let total = bytes_of.iter().fold(0_u64, |sum, bytes| {
        sum.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    });
    if total > limits.max_mutant_bytes {
        return Err(VerifyError::new(
            Kind::TooManyMutantBytes,
            "mutants",
            format!(
                "the mutants are {total} bytes together; at most {} are allowed",
                limits.max_mutant_bytes
            ),
        ));
    }
    let edit_lists = distinct_edits(set, scenarios);
    let mut bound = Vec::with_capacity(listed.len());
    for (index, (mutant, bytes)) in listed.iter().zip(bytes_of).enumerate() {
        let valid = validate_one(index, mutant, bytes, subject)?;
        for (scenario, edits) in &edit_lists {
            try_edits(index, &valid, scenario, edits)?;
        }
        bound.push(valid);
    }
    Ok(bound)
}

/// Every distinct non-empty effective edit list of the scenarios, in the order of the first
/// scenario that has it, with that scenario's name. Two lists are one when they name the same
/// paths in the same order with values of the same kind and bits.
fn distinct_edits<'s>(
    set: &'s VectorSet,
    scenarios: &'s [BoundScenario],
) -> Vec<(&'s str, &'s [(String, ReceiptValue)])> {
    let mut lists: Vec<(&str, &[(String, ReceiptValue)])> = Vec::new();
    for (scenario, bound) in set.scenarios().iter().zip(scenarios) {
        let edits = bound.edits.as_slice();
        let seen = lists.iter().any(|(_, list)| {
            list.len() == edits.len()
                && list
                    .iter()
                    .zip(edits)
                    .all(|((a, x), (b, y))| a == b && x.bit_eq(y))
        });
        if !edits.is_empty() && !seen {
            lists.push((scenario.name(), edits));
        }
    }
    lists
}

/// Apply one edit list to a fresh engine of a validated mutant.
fn try_edits(
    index: usize,
    mutant: &BoundMutant<'_>,
    scenario: &str,
    edits: &[(String, ReceiptValue)],
) -> Result<(), VerifyError> {
    let place = format!("mutants[{index}]");
    let mut engine = Engine::new();
    engine
        .load_cxf(mutant.bytes)
        .map_err(|error| bind::setup_failed(&place, "a fresh load of the mutant", &error))?;
    parameters::apply_to_mutant(&mut engine, edits, &place, &mutant.path, scenario)
}

fn validate_one<'a>(
    index: usize,
    mutant: &Mutant,
    bytes: &'a [u8],
    subject: &SubjectBoundary<'_>,
) -> Result<BoundMutant<'a>, VerifyError> {
    let place = format!("mutants[{index}]");
    let mut engine = Engine::new();
    if let Err(error) = engine.load_cxf(bytes) {
        return Err(VerifyError::new(
            Kind::MutantLoadFailed,
            place,
            format!(
                "the engine refused to load mutant `{}`: {error}",
                mutant.path()
            ),
        )
        .with_diagnostic_codes(error.all_diagnostics().map(|d| d.code.as_str()).collect()));
    }
    let inputs = engine
        .input_definitions()
        .map_err(|error| bind::setup_failed(&place, "its input definitions", &error))?;
    let slots = engine
        .frame_slots()
        .map_err(|error| bind::setup_failed(&place, "its slot map", &error))?;
    let inputs_of = |definitions: &[InputDefinition]| -> Vec<(String, ValueType)> {
        definitions
            .iter()
            .map(|definition| (definition.path.clone(), definition.value_type))
            .collect()
    };
    let outputs_of = |paths: &[String], types: &[ValueType]| -> Vec<(String, ValueType)> {
        paths.iter().cloned().zip(types.iter().copied()).collect()
    };
    let difference = first_difference("input", &inputs_of(subject.inputs), &inputs_of(&inputs))
        .or_else(|| {
            first_difference(
                "output",
                &outputs_of(subject.outputs, subject.output_types),
                &outputs_of(slots.outputs(), slots.output_types()),
            )
        });
    if let Some((path, detail)) = difference {
        return Err(VerifyError::new(
            Kind::MutantBoundaryDiffers,
            place,
            format!("mutant `{}`: {detail}", mutant.path()),
        )
        .with_path(path));
    }
    let content_id = engine
        .content_id()
        .map_err(|error| bind::setup_failed(&place, "its content id", &error))?
        .to_string();
    if content_id == subject.content_id {
        return Err(VerifyError::new(
            Kind::MutantIdentical,
            place,
            format!(
                "mutant `{}` has the subject's content id {content_id}: it promises the subject's outputs",
                mutant.path()
            ),
        ));
    }
    Ok(BoundMutant {
        path: mutant.path().to_owned(),
        digest: crate::fnv::tagged("", bytes),
        content_id,
        bytes,
    })
}

/// The first port at which two sorted boundaries differ, and a sentence: a path only one of them
/// has, or a path both have with two types.
fn first_difference(
    direction: &str,
    subject: &[(String, ValueType)],
    mutant: &[(String, ValueType)],
) -> Option<(String, String)> {
    let longest = subject.len().max(mutant.len());
    (0..longest).find_map(|slot| match (subject.get(slot), mutant.get(slot)) {
        (Some((left, left_type)), Some((right, right_type))) if left == right => {
            (left_type != right_type).then(|| {
                (
                    left.clone(),
                    format!(
                        "{direction} {left} is {} in the subject and {} in the mutant",
                        type_name(*left_type),
                        type_name(*right_type)
                    ),
                )
            })
        }
        (Some((left, _)), Some((right, _))) if left < right => Some((
            left.clone(),
            format!("the subject's {direction} {left} is not one of the mutant's"),
        )),
        (Some(_), Some((right, _))) | (None, Some((right, _))) => Some((
            right.clone(),
            format!("the mutant's {direction} {right} is not one of the subject's"),
        )),
        (Some((left, _)), None) => Some((
            left.clone(),
            format!("the subject's {direction} {left} is not one of the mutant's"),
        )),
        (None, None) => None,
    })
}

/// What every mutant run shares with the subject's: the set, the subject's boundary and the
/// bound scenarios, and the run's limits and chunk size.
pub(crate) struct Runs<'r> {
    /// The set.
    pub(crate) set: &'r VectorSet,
    /// The subject's boundary inputs, in slot order.
    pub(crate) inputs: &'r [InputDefinition],
    /// The subject's boundary outputs, in output-slot order.
    pub(crate) outputs: &'r [String],
    /// The scenarios, bound once against the subject.
    pub(crate) scenarios: &'r [BoundScenario],
    /// The run's limits, already effective.
    pub(crate) limits: &'r VerifyLimits,
    /// Frames per `Replay::advance`.
    pub(crate) chunk: usize,
}

/// The section of a receipt for the bound mutants, scoring them when the subject passed.
///
/// # Errors
/// Any refusal a run of a scenario on a mutant meets, as for the subject: `run-refused`,
/// `replay-failed`, or a refusal binding has already ruled out (a mutant that no longer loads,
/// an edit it refuses).
pub(crate) fn score(
    runs: &Runs<'_>,
    mutants: &[BoundMutant<'_>],
    subject_passed: bool,
) -> Result<MutantSection, VerifyError> {
    let mut entries = Vec::with_capacity(mutants.len());
    for (index, mutant) in mutants.iter().enumerate() {
        let caught_by = if subject_passed {
            first_catch(runs, index, mutant)?
        } else {
            None
        };
        let status = match (subject_passed, &caught_by) {
            (false, _) => MutantStatus::NotRun,
            (true, Some(_)) => MutantStatus::Caught,
            (true, None) => MutantStatus::Survived,
        };
        entries.push(MutantEntry {
            path: mutant.path.clone(),
            digest: mutant.digest.clone(),
            content_id: mutant.content_id.clone(),
            status,
            caught_by,
        });
    }
    let count = |status: MutantStatus| {
        let found = entries
            .iter()
            .filter(|entry| entry.status == status)
            .count();
        u64::try_from(found).unwrap_or(u64::MAX)
    };
    Ok(MutantSection {
        status: if subject_passed {
            ScoreStatus::Scored
        } else {
            ScoreStatus::NotRun
        },
        total: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        caught: count(MutantStatus::Caught),
        survived: count(MutantStatus::Survived),
        entries,
    })
}

/// Run the scenarios on `mutant` in file order up to the first that fails on it.
fn first_catch(
    runs: &Runs<'_>,
    index: usize,
    mutant: &BoundMutant<'_>,
) -> Result<Option<MutantCatch>, VerifyError> {
    // Two records are enough to name the first failure: with a refused frame, the earliest record
    // keeps the first slot and the refusal the reserved last one.
    let caps = Caps {
        failures: 2,
        warnings: 0,
    };
    for (position, (scenario, bound)) in runs.set.scenarios().iter().zip(runs.scenarios).enumerate()
    {
        let subject = format!("mutants[{index}].scenarios[{position}]");
        let ran = run::run_scenario(
            runs.set,
            &subject,
            mutant.bytes,
            runs.inputs,
            bound,
            runs.limits,
            runs.chunk,
        )?;
        let compared = compare::compare(
            &ran.trace,
            ran.refused,
            &bound.expectations,
            runs.outputs,
            caps,
            &subject,
        )?;
        let failed = ran.refused.is_some() || compared.instants.failed > 0;
        if let (true, Some(failure)) = (failed, compared.failures.into_iter().next()) {
            return Ok(Some(MutantCatch {
                scenario: scenario.name().to_owned(),
                failure,
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "mutants_tests.rs"]
mod tests;
