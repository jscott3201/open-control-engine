use oce_model::ParamTable;

use super::{bool_param, int_param};
use crate::{
    Block, BooleanExtractSignal, BooleanExtractor, BooleanScalarReplicator, BooleanVectorFilter,
    BooleanVectorReplicator, IntegerExtractSignal, IntegerExtractor, IntegerScalarReplicator,
    IntegerVectorFilter, IntegerVectorReplicator, MAX_RESOLVED_PORT_WIDTH, ParamDefault, ParamRule,
    RealExtractSignal, RealExtractor, RealScalarReplicator, RealVectorFilter, RealVectorReplicator,
    RegistryEntry,
};

const EXTRACT_SIGNAL_NIN_DEFAULT: i64 = 1;
const EXTRACT_SIGNAL_NOUT_DEFAULT: i64 = 1;
const EXTRACTOR_NIN_DEFAULT: i64 = 1;
const SCALAR_REPLICATOR_NOUT_DEFAULT: i64 = 1;
const VECTOR_FILTER_NIN_FALLBACK: i64 = 0;
const VECTOR_FILTER_NOUT_FALLBACK: i64 = 0;
const VECTOR_FILTER_MASK_DEFAULT: bool = true;
const VECTOR_REPLICATOR_NIN_DEFAULT: i64 = 1;
const VECTOR_REPLICATOR_NOUT_DEFAULT: i64 = 1;

pub(super) const EXTRACT_SIGNAL_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nin", EXTRACT_SIGNAL_NIN_DEFAULT),
    param_default_integer!("nout", EXTRACT_SIGNAL_NOUT_DEFAULT),
    param_default_derived!("extract_<i>", "i"),
];
pub(super) const EXTRACTOR_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_integer!("nin", EXTRACTOR_NIN_DEFAULT)];
pub(super) const SCALAR_REPLICATOR_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_integer!(
    "nout",
    SCALAR_REPLICATOR_NOUT_DEFAULT
)];
pub(super) const VECTOR_FILTER_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("nin"),
    param_default_required!("nout"),
    param_default_boolean!("msk_<i>", VECTOR_FILTER_MASK_DEFAULT),
];
pub(super) const VECTOR_REPLICATOR_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nin", VECTOR_REPLICATOR_NIN_DEFAULT),
    param_default_integer!("nout", VECTOR_REPLICATOR_NOUT_DEFAULT),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Routing.BooleanExtractSignal",
        make: make_boolean_extract_signal,
    },
    RegistryEntry {
        class_path: "CDL.Routing.BooleanExtractor",
        make: make_boolean_extractor,
    },
    RegistryEntry {
        class_path: "CDL.Routing.BooleanScalarReplicator",
        make: make_boolean_scalar_replicator,
    },
    RegistryEntry {
        class_path: "CDL.Routing.BooleanVectorFilter",
        make: make_boolean_vector_filter,
    },
    RegistryEntry {
        class_path: "CDL.Routing.BooleanVectorReplicator",
        make: make_boolean_vector_replicator,
    },
    RegistryEntry {
        class_path: "CDL.Routing.IntegerExtractSignal",
        make: make_integer_extract_signal,
    },
    RegistryEntry {
        class_path: "CDL.Routing.IntegerExtractor",
        make: make_integer_extractor,
    },
    RegistryEntry {
        class_path: "CDL.Routing.IntegerScalarReplicator",
        make: make_integer_scalar_replicator,
    },
    RegistryEntry {
        class_path: "CDL.Routing.IntegerVectorFilter",
        make: make_integer_vector_filter,
    },
    RegistryEntry {
        class_path: "CDL.Routing.IntegerVectorReplicator",
        make: make_integer_vector_replicator,
    },
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

pub(super) const EXTRACT_SIGNAL_PARAM_RULES: &[ParamRule] = &[
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

pub(super) const EXTRACTOR_PARAM_RULES: &[ParamRule] = &[
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

pub(super) const SCALAR_REPLICATOR_PARAM_RULES: &[ParamRule] = &[
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

pub(super) const VECTOR_FILTER_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "nin",
        kind: oce_model::ValueType::Integer,
    },
    ParamRule::Required {
        name: "nout",
        kind: oce_model::ValueType::Integer,
    },
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

pub(super) const VECTOR_REPLICATOR_PARAM_RULES: &[ParamRule] = &[
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

fn make_boolean_extract_signal(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_usize_param(p, "nin", EXTRACT_SIGNAL_NIN_DEFAULT);
    let nout = bounded_usize_param(p, "nout", EXTRACT_SIGNAL_NOUT_DEFAULT);
    let extract = extract_param_vector(p, nout);
    Box::new(BooleanExtractSignal::new(nin, nout, extract))
}

fn make_boolean_extractor(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanExtractor::new(bounded_usize_param(
        p,
        "nin",
        EXTRACTOR_NIN_DEFAULT,
    )))
}

fn make_boolean_scalar_replicator(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanScalarReplicator::new(bounded_usize_param(
        p,
        "nout",
        SCALAR_REPLICATOR_NOUT_DEFAULT,
    )))
}

fn make_boolean_vector_filter(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout, mask) = vector_filter_params(p);
    Box::new(BooleanVectorFilter::new(nin, nout, mask))
}

fn make_boolean_vector_replicator(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout) = vector_replicator_params(p);
    Box::new(BooleanVectorReplicator::new(nin, nout))
}

fn make_integer_extract_signal(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_usize_param(p, "nin", EXTRACT_SIGNAL_NIN_DEFAULT);
    let nout = bounded_usize_param(p, "nout", EXTRACT_SIGNAL_NOUT_DEFAULT);
    let extract = extract_param_vector(p, nout);
    Box::new(IntegerExtractSignal::new(nin, nout, extract))
}

fn make_integer_extractor(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerExtractor::new(bounded_usize_param(
        p,
        "nin",
        EXTRACTOR_NIN_DEFAULT,
    )))
}

fn make_integer_scalar_replicator(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerScalarReplicator::new(bounded_usize_param(
        p,
        "nout",
        SCALAR_REPLICATOR_NOUT_DEFAULT,
    )))
}

fn make_integer_vector_filter(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout, mask) = vector_filter_params(p);
    Box::new(IntegerVectorFilter::new(nin, nout, mask))
}

fn make_integer_vector_replicator(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout) = vector_replicator_params(p);
    Box::new(IntegerVectorReplicator::new(nin, nout))
}

fn make_real_extract_signal(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_usize_param(p, "nin", EXTRACT_SIGNAL_NIN_DEFAULT);
    let nout = bounded_usize_param(p, "nout", EXTRACT_SIGNAL_NOUT_DEFAULT);
    let extract = extract_param_vector(p, nout);
    Box::new(RealExtractSignal::new(nin, nout, extract))
}

fn make_real_extractor(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealExtractor::new(bounded_usize_param(
        p,
        "nin",
        EXTRACTOR_NIN_DEFAULT,
    )))
}

fn make_real_scalar_replicator(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealScalarReplicator::new(bounded_usize_param(
        p,
        "nout",
        SCALAR_REPLICATOR_NOUT_DEFAULT,
    )))
}

fn make_real_vector_filter(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout, mask) = vector_filter_params(p);
    Box::new(RealVectorFilter::new(nin, nout, mask))
}

fn make_real_vector_replicator(p: &ParamTable) -> Box<dyn Block> {
    let (nin, nout) = vector_replicator_params(p);
    Box::new(RealVectorReplicator::new(nin, nout))
}

fn extract_param_vector(p: &ParamTable, nout: usize) -> Vec<usize> {
    (1..=nout)
        .map(|idx| usize_param(p, &format!("extract_{idx}"), idx as i64))
        .collect()
}

fn vector_filter_params(p: &ParamTable) -> (usize, usize, Vec<bool>) {
    let nin = bounded_usize_param(p, "nin", VECTOR_FILTER_NIN_FALLBACK);
    let nout = bounded_usize_param(p, "nout", VECTOR_FILTER_NOUT_FALLBACK);
    let mask = (1..=nin)
        .map(|idx| bool_param(p, &format!("msk_{idx}"), VECTOR_FILTER_MASK_DEFAULT))
        .collect();
    (nin, nout, mask)
}

fn vector_replicator_params(p: &ParamTable) -> (usize, usize) {
    (
        bounded_usize_param(p, "nin", VECTOR_REPLICATOR_NIN_DEFAULT),
        bounded_usize_param(p, "nout", VECTOR_REPLICATOR_NOUT_DEFAULT),
    )
}

fn bounded_usize_param(params: &ParamTable, name: &str, default: i64) -> usize {
    usize_param(params, name, default).min(MAX_RESOLVED_PORT_WIDTH)
}

fn usize_param(params: &ParamTable, name: &str, default: i64) -> usize {
    usize::try_from(int_param(params, name, default)).unwrap_or(0)
}
