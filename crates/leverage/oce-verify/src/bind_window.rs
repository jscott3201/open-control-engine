//! Binding one window expectation, of either form, to the subject (vectors note, "Binding and
//! running" step 3; decisions 7, 39, 43 and 46).
//!
//! In this order, the first refusal winning:
//!
//! 1. **The output**, by the one name rule against the boundary outputs: `unknown-output`,
//!    `ambiguous-output`.
//! 2. **What the window checks.** An `equals` literal is typed by the output's declared type. A
//!    tolerance window's output must be a Real (`tolerance-on-discrete-output` for a Boolean or
//!    an Integer: the envelope is type-blind, so a value between two levels could pass), and
//!    each `of` value is then typed as an `equals` literal is, against that Real.
//! 3. **The mask**, each `when` name in file order, by the one name rule against the boundary
//!    inputs and outputs together (`when-unknown`, `when-ambiguous`), and each must be a Boolean
//!    port (`when-not-boolean`).

use oce_api::ValueType;
use oce_replay::PathIndex;

use crate::bind::{Boundary, OUTPUTS, resolve, typed_at};
use crate::envelope::Tolerances;
use crate::model::{Check, Window};
use crate::receipt::ReceiptValue;
use crate::typing::type_name;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// A window expectation against an output slot.
pub(crate) struct BoundWindow {
    /// The output slot.
    pub(crate) output: usize,
    /// The window as written, seconds: the coverage check reads it.
    pub(crate) from_s: f64,
    /// The window's last second, included.
    pub(crate) to_s: f64,
    /// The first tick in the window.
    pub(crate) first_tick: u64,
    /// The last tick in the window.
    pub(crate) last_tick: u64,
    /// What the window checks at each tick.
    pub(crate) check: BoundCheck,
    /// The window's mask, in file order; empty when it has none.
    pub(crate) when: Vec<Signal>,
}

/// What a bound window checks.
pub(crate) enum BoundCheck {
    /// The output equals this value exactly, a Real by its bits.
    Equals(ReceiptValue),
    /// The output lies inside the envelope built around the expected signal.
    Within {
        /// The six tolerances, an absent name zero.
        tolerances: Tolerances,
        /// The expected signal's steps, `(t, value)` in time order, the first at the window's
        /// `from_s`; each value typed as a Real.
        of: Vec<(f64, f64)>,
    },
}

/// One Boolean signal of a mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    /// A boundary input, by slot: the value a frame submitted.
    Input(usize),
    /// A boundary output, by slot: the value a frame produced.
    Output(usize),
}

/// The boundary inputs and outputs together, sorted by path bytes, with each one's signal and
/// declared type: what a `when` name resolves against.
pub(crate) struct Signals {
    paths: Vec<String>,
    targets: Vec<(Signal, ValueType)>,
}

impl Signals {
    /// The inputs and outputs of a subject, together. An input and an output never share a path
    /// in a loaded subject; if they did, the input is kept.
    pub(crate) fn of(
        inputs: &[oce_api::InputDefinition],
        outputs: &[String],
        output_types: &[ValueType],
    ) -> Self {
        let mut all: Vec<(String, Signal, ValueType)> = inputs
            .iter()
            .enumerate()
            .map(|(slot, d)| (d.path.clone(), Signal::Input(slot), d.value_type))
            .chain(outputs.iter().zip(output_types).enumerate().map(
                |(slot, (path, value_type))| (path.clone(), Signal::Output(slot), *value_type),
            ))
            .collect();
        all.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        all.dedup_by(|later, earlier| later.0 == earlier.0);
        let (paths, targets) = all
            .into_iter()
            .map(|(path, signal, value_type)| (path, (signal, value_type)))
            .unzip();
        Self { paths, targets }
    }

    /// The index the one name rule resolves against.
    pub(crate) fn index(&self) -> PathIndex<'_> {
        PathIndex::new(&self.paths)
    }
}

const WHEN: crate::bind::Names = crate::bind::Names {
    what: "boundary input or output",
    unknown: Kind::WhenUnknown,
    ambiguous: Kind::WhenAmbiguous,
};

/// Bind `window`, the expectation at `place` (`scenarios[i].expect[j]`).
///
/// # Errors
/// The first refusal in the order the module documentation gives.
pub(crate) fn bind_window(
    window: &Window,
    place: &str,
    boundary: &Boundary<'_>,
    signals: &Signals,
    signal_index: &PathIndex<'_>,
) -> Result<BoundWindow, VerifyError> {
    let slot = resolve(
        &boundary.output_index,
        window.output(),
        &OUTPUTS,
        &format!("{place}.output"),
    )?;
    let output = &boundary.outputs[slot];
    let output_type = boundary.output_types[slot];
    let check = match &window.check {
        Check::Equals(literal) => BoundCheck::Equals(typed_at(
            *literal,
            output_type,
            &format!("{place}.equals"),
            output,
        )?),
        Check::Within(within) => {
            if output_type != ValueType::Real {
                return Err(VerifyError::new(
                    Kind::ToleranceOnDiscreteOutput,
                    format!("{place}.within"),
                    format!(
                        "{output} is {}; a tolerance window compares a Real output only",
                        type_name(output_type)
                    ),
                )
                .with_path(output.clone()));
            }
            let mut of = Vec::with_capacity(within.of().len());
            for (index, step) in within.of().iter().enumerate() {
                let subject = if within.of_is_constant() {
                    format!("{place}.of")
                } else {
                    format!("{place}.of[{index}].value")
                };
                // The output is a Real, so every typed value is one.
                if let ReceiptValue::Real(value) =
                    typed_at(step.value(), output_type, &subject, output)?
                {
                    of.push((step.time_s(), value));
                }
            }
            BoundCheck::Within {
                tolerances: within.tolerances(),
                of,
            }
        }
    };
    let mut when = Vec::with_capacity(window.when().len());
    for (index, name) in window.when().iter().enumerate() {
        let subject = format!("{place}.when[{index}]");
        let row = resolve(signal_index, name, &WHEN, &subject)?;
        let (signal, value_type) = signals.targets[row];
        if value_type != ValueType::Boolean {
            return Err(VerifyError::new(
                Kind::WhenNotBoolean,
                subject,
                format!(
                    "`{name}` names {}, which is {}; a when names Boolean ports only",
                    signals.paths[row],
                    type_name(value_type)
                ),
            )
            .with_path(signals.paths[row].clone()));
        }
        when.push(signal);
    }
    Ok(BoundWindow {
        output: slot,
        from_s: window.from_s(),
        to_s: window.to_s(),
        first_tick: window.first_tick(),
        last_tick: window.last_tick(),
        check,
        when,
    })
}
