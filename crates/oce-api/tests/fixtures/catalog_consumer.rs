//! Studio-shaped compile fixture: every import comes from the host facade.
use oce_api::{CatalogRule, CatalogValueKind, CatalogPortKind, CatalogPortNaming, CatalogDefault};

#[derive(Debug)]
pub enum Field {
    Text(String), Strings(Vec<String>), Optional(Option<String>), Integer(i64),
    RealBits(u64), Boolean(bool), Kind(CatalogValueKind), PortKind(CatalogPortKind),
}
#[derive(Debug)]
pub struct HostEntry {
    pub class_path: String,
    pub inputs: Vec<(Option<String>, CatalogPortKind)>,
    pub outputs: Vec<(Option<String>, CatalogPortKind)>,
    pub naming: CatalogPortNaming,
    pub rules: Vec<(&'static str, Vec<(&'static str, Field)>)>,
    pub defaults: Vec<(String, CatalogDefault)>,
    pub width_driven: bool,
    pub stateful: bool,
    pub reserved: bool,
}

fn rule(rule: &CatalogRule) -> (&'static str, Vec<(&'static str, Field)>) {
    match rule {
        CatalogRule::Required { name, kind } => ("Required", vec![
            ("name", Field::Text((*name).into())),
            ("kind", Field::Kind(kind.clone())),
        ]),
        CatalogRule::Structural { name } => ("Structural", vec![
            ("name", Field::Text((*name).into())),
        ]),
        CatalogRule::StructuralArrayElements { base } => ("StructuralArrayElements", vec![
            ("base", Field::Text((*base).into())),
        ]),
        CatalogRule::Boolean { name } => ("Boolean", vec![
            ("name", Field::Text((*name).into())),
        ]),
        CatalogRule::Real { name } => ("Real", vec![
            ("name", Field::Text((*name).into())),
        ]),
        CatalogRule::RealFinite { name } => ("RealFinite", vec![
            ("name", Field::Text((*name).into())),
        ]),
        CatalogRule::RealGreaterThan { name, min } => ("RealGreaterThan", vec![
            ("name", Field::Text((*name).into())),
            ("min", Field::RealBits(min.to_bits())),
        ]),
        CatalogRule::RealFiniteGreaterThan { name, min } => ("RealFiniteGreaterThan", vec![
            ("name", Field::Text((*name).into())),
            ("min", Field::RealBits(min.to_bits())),
        ]),
        CatalogRule::IntegerGreaterOrEqual { name, min } => ("IntegerGreaterOrEqual", vec![
            ("name", Field::Text((*name).into())),
            ("min", Field::Integer(*min)),
        ]),
        CatalogRule::IntegerLessOrEqualConstant { name, max } => ("IntegerLessOrEqualConstant", vec![
            ("name", Field::Text((*name).into())),
            ("max", Field::Integer(*max)),
        ]),
        CatalogRule::IntegerArrayElements { base, len } => ("IntegerArrayElements", vec![
            ("base", Field::Text((*base).into())),
            ("len", Field::Text((*len).into())),
        ]),
        CatalogRule::IntegerArrayElementsInRange { base, len, len_default, min, max, max_default, default_to_index } => ("IntegerArrayElementsInRange", vec![
            ("base", Field::Text((*base).into())),
            ("len", Field::Text((*len).into())),
            ("len_default", Field::Integer(*len_default)),
            ("min", Field::Integer(*min)),
            ("max", Field::Text((*max).into())),
            ("max_default", Field::Integer(*max_default)),
            ("default_to_index", Field::Boolean(*default_to_index)),
        ]),
        CatalogRule::RealArrayElements { base, len } => ("RealArrayElements", vec![
            ("base", Field::Text((*base).into())),
            ("len", Field::Text((*len).into())),
        ]),
        CatalogRule::RealMatrixElements { base, rows, default_rows, cols, default_cols } => ("RealMatrixElements", vec![
            ("base", Field::Text((*base).into())),
            ("rows", Field::Text((*rows).into())),
            ("default_rows", Field::Integer(*default_rows)),
            ("cols", Field::Text((*cols).into())),
            ("default_cols", Field::Integer(*default_cols)),
        ]),
        CatalogRule::TimeTableMatrix { base, values, time_scale, period, extrapolation } => ("TimeTableMatrix", vec![
            ("base", Field::Text((*base).into())),
            ("values", Field::PortKind(*values)),
            ("time_scale", Field::Text((*time_scale).into())),
            ("period", Field::Optional(period.map(Into::into))),
            ("extrapolation", Field::Optional(extrapolation.map(Into::into))),
        ]),
        CatalogRule::TimeTableOffset { base, table } => ("TimeTableOffset", vec![
            ("base", Field::Text((*base).into())),
            ("table", Field::Text((*table).into())),
        ]),
        CatalogRule::BooleanArrayElements { base, len } => ("BooleanArrayElements", vec![
            ("base", Field::Text((*base).into())),
            ("len", Field::Text((*len).into())),
        ]),
        CatalogRule::BooleanArrayTrueCountEquals { base, len, count, default } => ("BooleanArrayTrueCountEquals", vec![
            ("base", Field::Text((*base).into())),
            ("len", Field::Text((*len).into())),
            ("count", Field::Text((*count).into())),
            ("default", Field::Boolean(*default)),
        ]),
        CatalogRule::EnumMembers { name, members } => ("EnumMembers", vec![
            ("name", Field::Text((*name).into())),
            ("members", Field::Strings(members.iter().map(|s| (*s).into()).collect())),
        ]),
        CatalogRule::RealGreaterOrEqual { name, min } => ("RealGreaterOrEqual", vec![
            ("name", Field::Text((*name).into())),
            ("min", Field::RealBits(min.to_bits())),
        ]),
        CatalogRule::RealLessOrEqualConstant { name, max } => ("RealLessOrEqualConstant", vec![
            ("name", Field::Text((*name).into())),
            ("max", Field::RealBits(max.to_bits())),
        ]),
        CatalogRule::RealTimesIntegerInclusiveRange { real, integer, min, max } => ("RealTimesIntegerInclusiveRange", vec![
            ("real", Field::Text((*real).into())),
            ("integer", Field::Text((*integer).into())),
            ("min", Field::RealBits(min.to_bits())),
            ("max", Field::RealBits(max.to_bits())),
        ]),
        CatalogRule::IntegerProductLessOrEqualConstant { left, right, max } => ("IntegerProductLessOrEqualConstant", vec![
            ("left", Field::Text((*left).into())),
            ("right", Field::Text((*right).into())),
            ("max", Field::Integer(*max)),
        ]),
        CatalogRule::RealLessOrEqual { lower, upper } => ("RealLessOrEqual", vec![
            ("lower", Field::Text((*lower).into())),
            ("upper", Field::Text((*upper).into())),
        ]),
        CatalogRule::RealLessOrEqualWarning { lower, upper } => ("RealLessOrEqualWarning", vec![
            ("lower", Field::Text((*lower).into())),
            ("upper", Field::Text((*upper).into())),
        ]),
        CatalogRule::RealGreaterOrEqualScaledWarning { left, right, factor } => ("RealGreaterOrEqualScaledWarning", vec![
            ("left", Field::Text((*left).into())),
            ("right", Field::Text((*right).into())),
            ("factor", Field::RealBits(factor.to_bits())),
        ]),
        CatalogRule::RealEqualWarning { left, right } => ("RealEqualWarning", vec![
            ("left", Field::Text((*left).into())),
            ("right", Field::Text((*right).into())),
        ]),
    }
}

pub fn catalog_adapter() -> Vec<HostEntry> {
    oce_api::catalog().iter().map(|entry| HostEntry {
        class_path: entry.class_path.into(),
        inputs: entry.inputs.iter().map(|port| (port.name.map(Into::into), port.kind)).collect(),
        outputs: entry.outputs.iter().map(|port| (port.name.map(Into::into), port.kind)).collect(),
        naming: entry.naming,
        rules: entry.param_rules.iter().map(rule).collect(),
        defaults: entry.param_defaults.iter().map(|param| (param.name.into(), param.default.clone())).collect(),
        width_driven: entry.width_driven, stateful: entry.stateful, reserved: entry.reserved,
    }).collect()
}

pub fn receipts(bytes: &[u8]) -> Result<(), oce_api::OperationFailure> {
    let mut engine = oce_api::Engine::in_memory();
    let receipt = engine.load_cxf_with_receipt(bytes)?;
    let (_legacy, diagnostics) = receipt.into_parts();
    for diagnostic in diagnostics.records() {
        let _: &str = diagnostic.key().code();
        let _: &oce_api::DiagnosticSubject = diagnostic.key().subject();
        let _ = diagnostic.key().stage().rank();
    }
    let _ = engine.export_cxf_with_receipt()?;
    let _ = oce_api::catalog_content_id(oce_api::catalog());
    let _ = oce_api::contract_descriptors();
    Ok(())
}
