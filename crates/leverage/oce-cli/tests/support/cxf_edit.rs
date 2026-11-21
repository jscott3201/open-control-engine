//! Hand edits of one small flat CXF document, for the `oce import` tests: the emitter's golden
//! of the worked example (`oce-author`'s `emit/econ_enable.cxf.json`, read in place) parsed as
//! JSON, one node changed, written back. A sequence parameter (`dTHys`), two boundary inputs
//! with units, one Boolean output and two blocks (`sub`, `hys`), one of whose parameters
//! (`hys.uHigh`) references the sequence parameter.

use serde_json::Value;

/// The root `@id` of the worked example.
pub const ROOT: &str = "urn:oce:author:econ_enable";
/// The XSD datatype of a Real literal.
pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
/// The XSD datatype of an Integer literal.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// The worked example's CXF bytes.
pub fn econ() -> Vec<u8> {
    std::fs::read(super::crate_dir().join(super::author_fixture("emit/econ_enable.cxf.json")))
        .expect("the emitter's golden")
}

/// The node whose `@id` is the root's plus `suffix` (`""` for the root, `".hys.uLow"`).
pub fn node<'a>(document: &'a mut Value, suffix: &str) -> &'a mut Value {
    let id = format!("{ROOT}{suffix}");
    document["@graph"]
        .as_array_mut()
        .expect("a graph")
        .iter_mut()
        .find(|node| node["@id"] == id.as_str())
        .unwrap_or_else(|| panic!("no node {id}"))
}

/// The worked example after `edit`, as bytes.
pub fn edited(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut document: Value = serde_json::from_slice(&econ()).expect("JSON");
    edit(&mut document);
    serde_json::to_vec_pretty(&document).expect("JSON")
}

/// A typed Real literal with the lexical form `text`.
pub fn real(text: &str) -> Value {
    serde_json::json!({ "@value": text, "@type": XSD_DOUBLE })
}
