//! Registry entry for `CDL.Logical.Proof`.

use oce_model::ParamTable;

use super::real_param;
use crate::{Block, ParamDefault, ParamRule, Proof, RegistryEntry};

const DEBOUNCE_FALLBACK: f64 = 0.0;
const FEEDBACK_DELAY_FALLBACK: f64 = 0.0;

pub(super) const PROOF_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("debounce"),
    param_default_required!("feedbackDelay"),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Logical.Proof",
    make: make_proof,
}];

pub(super) const PROOF_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "debounce" },
    ParamRule::Required {
        name: "feedbackDelay",
    },
    ParamRule::RealLessOrEqualWarning {
        lower: "debounce",
        upper: "feedbackDelay",
    },
];

fn make_proof(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Proof {
        debounce: real_param(p, "debounce", DEBOUNCE_FALLBACK),
        feedback_delay: real_param(p, "feedbackDelay", FEEDBACK_DELAY_FALLBACK),
    })
}
