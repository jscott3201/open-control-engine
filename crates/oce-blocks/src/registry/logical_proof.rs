//! Registry entry for `CDL.Logical.Proof`.

use oce_model::ParamTable;

use super::real_param;
use crate::{Block, ParamRule, Proof, RegistryEntry};

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
        debounce: real_param(p, "debounce", 0.0),
        feedback_delay: real_param(p, "feedbackDelay", 0.0),
    })
}
