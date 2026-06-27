use oce_model::ParamTable;

use super::{bool_param, int_param};
use crate::{
    Block, MAX_RESOLVED_PORT_WIDTH, ParamRule, RealExtractSignal, RealExtractor,
    RealScalarReplicator, RealVectorFilter, RealVectorReplicator, RegistryEntry,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Routing.RealExtractSignal",
        make: make_real_extract_signal,
    },
    RegistryEntry {
        class_path: "CDL.Routing.RealExtractor",
        make: make_real_extractor,
    },
    RegistryEntry {
        class_path: "CDL.Routing.RealScalarReplicator",
        make: make_real_scalar_replicator,
    },
    RegistryEntry {
        class_path: "CDL.Routing.RealVectorFilter",
        make: make_real_vector_filter,
    },
    RegistryEntry {
        class_path: "CDL.Routing.RealVectorReplicator",
        make: make_real_vector_replicator,
    },
];

pub(super) const REAL_EXTRACT_SIGNAL_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::Structural { name: "nout" },
    ParamRule::StructuralArrayElements { base: "extract" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nout",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerArrayElementsInRange {
        base: "extract",
        len: "nout",
        len_default: 1,
        min: 1,
        max: "nin",
        max_default: 1,
        default_to_index: true,
    },
];

pub(super) const REAL_EXTRACTOR_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 1,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

pub(super) const REAL_SCALAR_REPLICATOR_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nout" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nout",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

pub(super) const REAL_VECTOR_FILTER_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "nin" },
    ParamRule::Required { name: "nout" },
    ParamRule::Structural { name: "nin" },
    ParamRule::Structural { name: "nout" },
    ParamRule::StructuralArrayElements { base: "msk" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nout",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::BooleanArrayElements {
        base: "msk",
        len: "nin",
    },
    ParamRule::BooleanArrayTrueCountEquals {
        base: "msk",
        len: "nin",
        count: "nout",
        default: true,
    },
];

pub(super) const REAL_VECTOR_REPLICATOR_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::Structural { name: "nout" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nout",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerProductLessOrEqualConstant {
        left: "nin",
        right: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

fn make_real_extract_signal(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_usize_param(p, "nin", 1);
    let nout = bounded_usize_param(p, "nout", 1);
    let extract = (1..=nout)
        .map(|idx| usize_param(p, &format!("extract_{idx}"), idx as i64))
        .collect();
    Box::new(RealExtractSignal::new(nin, nout, extract))
}

fn make_real_extractor(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealExtractor::new(bounded_usize_param(p, "nin", 1)))
}

fn make_real_scalar_replicator(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealScalarReplicator::new(bounded_usize_param(p, "nout", 1)))
}

fn make_real_vector_filter(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_usize_param(p, "nin", 0);
    let nout = bounded_usize_param(p, "nout", 0);
    let mask = (1..=nin)
        .map(|idx| bool_param(p, &format!("msk_{idx}"), true))
        .collect();
    Box::new(RealVectorFilter::new(nin, nout, mask))
}

fn make_real_vector_replicator(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealVectorReplicator::new(
        bounded_usize_param(p, "nin", 1),
        bounded_usize_param(p, "nout", 1),
    ))
}

fn bounded_usize_param(params: &ParamTable, name: &str, default: i64) -> usize {
    usize_param(params, name, default).min(MAX_RESOLVED_PORT_WIDTH)
}

fn usize_param(params: &ParamTable, name: &str, default: i64) -> usize {
    usize::try_from(int_param(params, name, default)).unwrap_or(0)
}
