//! The vendored `CXF-Core.jsonld` as an index from `@id` to node.
//!
//! The document is read as data: one JSON-LD object with an `@graph` array of nodes, each with a
//! string `@id`. Nothing here follows a relation list (`hasInput`, `hasParameter`): the catalog
//! names what to look up (design note decision 4).

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use super::GenerateError;

/// The largest CXF-Core document the generator reads (the vendored one is 683 KB).
pub const MAX_CORE_BYTES: usize = 4 * 1024 * 1024;

/// The `@id` prefix of every class and member node.
pub const ID_BASE: &str = "S231:Buildings.Controls.OBC.";

/// The parsed document.
#[derive(Debug)]
pub struct Core {
    nodes: BTreeMap<String, Map<String, Value>>,
    sha256: String,
}

impl Core {
    /// Index `bytes`, refusing a document that is too large, not JSON, not an object with an
    /// `@graph` array of objects, or that gives one `@id` to two nodes.
    pub fn parse(bytes: &[u8]) -> Result<Self, GenerateError> {
        if bytes.len() > MAX_CORE_BYTES {
            return Err(GenerateError::CoreTooLarge {
                bytes: bytes.len(),
                limit: MAX_CORE_BYTES,
            });
        }
        let document: Value = serde_json::from_slice(bytes)
            .map_err(|error| GenerateError::CoreNotJson(error.to_string()))?;
        let Value::Object(mut document) = document else {
            return Err(GenerateError::CoreShape("the document is not an object"));
        };
        let Some(Value::Array(graph)) = document.remove("@graph") else {
            return Err(GenerateError::CoreShape("`@graph` is not an array"));
        };
        let mut nodes = BTreeMap::new();
        for node in graph {
            let Value::Object(node) = node else {
                return Err(GenerateError::CoreShape("a graph node is not an object"));
            };
            let Some(Value::String(id)) = node.get("@id") else {
                return Err(GenerateError::CoreShape("a graph node has no string `@id`"));
            };
            let id = id.clone();
            if nodes.insert(id.clone(), node).is_some() {
                return Err(GenerateError::DuplicateNode { id });
            }
        }
        let digest = Sha256::digest(bytes);
        let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self { nodes, sha256 })
    }

    /// SHA-256 of the bytes that were parsed, lowercase hexadecimal.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Whether the document has a node for the class.
    pub fn has_class(&self, class_path: &str) -> bool {
        self.nodes.contains_key(&format!("{ID_BASE}{class_path}"))
    }

    /// The node of a class's connector or parameter.
    pub fn member(&self, class_path: &str, name: &str) -> Option<&Map<String, Value>> {
        self.nodes.get(&format!("{ID_BASE}{class_path}.{name}"))
    }

    /// Every `@id` a node anywhere in the document names under `key` (`qudt:hasUnit`,
    /// `qudt:hasQuantityKind`), with the `@id` of the node that carries it.
    pub fn identifiers(&self, key: &str) -> Vec<(&str, &str)> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| Some((id.as_str(), reference(node.get(key)?)?)))
            .collect()
    }
}

/// The `@id` of a `{"@id": "..."}` reference.
pub fn reference(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.get("@id")?.as_str()
}
