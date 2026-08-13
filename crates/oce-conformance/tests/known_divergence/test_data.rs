//! Values used only to test the register schema and validation policy.

use serde_json::{Value, json};

pub(crate) const ARTIFACT_PATH: &str =
    "crates/oce-conformance/tests/fixtures/known_divergence/schema_test_artifact.txt";
pub(crate) const ARTIFACT_SHA256: &str =
    "210e6fadba176fbd5027a0d7d2062d758c01d06ebc2e87152734d3730aaa9419";
const SYNTHETIC_REGISTER: &str =
    include_str!("../fixtures/known_divergence/synthetic_schema_test_register.json");

pub(crate) fn entry(id: &str, suffix: &str) -> Value {
    let mut value: Value = serde_json::from_str(SYNTHETIC_REGISTER)
        .expect("checked-in synthetic schema-test register parses as JSON");
    let mut entry = value["entries"]
        .as_array_mut()
        .and_then(Vec::pop)
        .expect("synthetic schema-test register has one entry");
    entry["id"] = json!(id);
    entry["subject"]["scenario"] = json!(format!("schema_test_{suffix}"));
    entry["producer_cases"][0]["case_id"] =
        json!(format!("clean-room:CDL.Logical.Nand:schema_test:{suffix}"));
    entry["producer_cases"][1]["case_id"] = json!(format!("tier-a-schema-{suffix}"));
    entry["producer_cases"][2]["case_id"] = json!(format!("engine-schema-{suffix}"));
    entry
}

pub(crate) fn register(entries: Vec<Value>) -> Value {
    json!({
        "format": "oce-known-divergence-register-v1",
        "entries": entries
    })
}

pub(crate) fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("schema test value serializes")
}

pub(crate) fn code(value: &Value) -> super::reader::ValidationCode {
    super::reader::read_register(&bytes(value))
        .expect_err("schema mutation must be rejected")
        .code
}
