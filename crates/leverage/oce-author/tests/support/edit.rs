//! Structural edits of a CXF document for the importer's refusal tests: find the root, its
//! members and a block's members by role, and change one node at a time, so a case names what
//! it changes instead of matching pretty-printed text.

use serde_json::{Map, Value};

/// A parsed CXF document under edit.
pub struct Document {
    value: Value,
}

impl Document {
    /// Parse `text`.
    pub fn parse(text: &str) -> Self {
        Self {
            value: serde_json::from_str(text).expect("a JSON document"),
        }
    }

    /// Read the fixture or golden at `path`.
    pub fn read(path: &std::path::Path) -> Self {
        Self::parse(&std::fs::read_to_string(path).expect("read document"))
    }

    fn graph(&self) -> &Vec<Value> {
        self.value["@graph"].as_array().expect("@graph")
    }

    fn graph_mut(&mut self) -> &mut Vec<Value> {
        self.value["@graph"].as_array_mut().expect("@graph")
    }

    /// The root `@id` (the node with `containsBlock`).
    pub fn root(&self) -> String {
        self.graph()
            .iter()
            .find(|n| n.get("S231:containsBlock").is_some())
            .and_then(|n| n["@id"].as_str())
            .expect("a root")
            .to_owned()
    }

    /// The `@id`s a node's relation `key` lists.
    pub fn related(&self, id: &str, key: &str) -> Vec<String> {
        let node = self.node(id);
        match node.get(key) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|r| r["@id"].as_str().map(str::to_owned))
                .collect(),
            Some(one) => one["@id"].as_str().map(str::to_owned).into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// The first `@id` the root's relation `key` lists.
    pub fn first(&self, key: &str) -> String {
        let root = self.root();
        self.related(&root, key)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("the root has no `{key}`"))
    }

    /// The first block that has a parameter, and that parameter's `@id`.
    pub fn first_block_parameter(&self) -> (String, String) {
        let root = self.root();
        self.related(&root, "S231:containsBlock")
            .into_iter()
            .find_map(|block| {
                let params = self.related(&block, "S231:hasParameter");
                params.into_iter().next().map(|p| (block, p))
            })
            .expect("a block with a parameter")
    }

    /// The first block port (an input or output of the first block that has one).
    pub fn first_port(&self) -> String {
        let root = self.root();
        self.related(&root, "S231:containsBlock")
            .into_iter()
            .find_map(|block| {
                let mut ports = self.related(&block, "S231:hasInput");
                ports.extend(self.related(&block, "S231:hasOutput"));
                ports.into_iter().next()
            })
            .expect("a block port")
    }

    /// The node `id`.
    pub fn node(&self, id: &str) -> &Map<String, Value> {
        self.graph()
            .iter()
            .find(|n| n["@id"] == id)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("no node `{id}`"))
    }

    /// Change the node `id` with `change`.
    pub fn set(&mut self, id: &str, change: impl FnOnce(&mut Map<String, Value>)) -> &mut Self {
        let node = self
            .graph_mut()
            .iter_mut()
            .find(|n| n["@id"] == id)
            .and_then(Value::as_object_mut)
            .unwrap_or_else(|| panic!("no node `{id}`"));
        change(node);
        self
    }

    /// Put `key: value` on the node `id`.
    pub fn put(&mut self, id: &str, key: &str, value: Value) -> &mut Self {
        self.set(id, |node| {
            node.insert(key.to_owned(), value);
        })
    }

    /// Remove `key` from the node `id`.
    pub fn remove(&mut self, id: &str, key: &str) -> &mut Self {
        self.set(id, |node| {
            node.remove(key);
        })
    }

    /// Insert `node` at the front of `@graph`.
    pub fn prepend(&mut self, node: Value) -> &mut Self {
        self.graph_mut().insert(0, node);
        self
    }

    /// The document's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.value).expect("write")
    }

    /// The document's text with `"marker"` (a JSON string) replaced by `raw`, for a token no
    /// `serde_json::Value` can hold (`1e999`).
    pub fn bytes_with_raw(&self, marker: &str, raw: &str) -> Vec<u8> {
        let text = serde_json::to_string(&self.value).expect("write");
        let quoted = format!("\"{marker}\"");
        assert!(text.contains(&quoted), "{marker}");
        text.replacen(&quoted, raw, 1).into_bytes()
    }
}

/// A typed literal `{"@value": text, "@type": xsd:<datatype>}`.
pub fn typed(text: &str, datatype: &str) -> Value {
    serde_json::json!({
        "@value": text,
        "@type": format!("http://www.w3.org/2001/XMLSchema#{datatype}"),
    })
}

/// A reference `{"@id": iri}`.
pub fn reference(iri: &str) -> Value {
    serde_json::json!({ "@id": iri })
}
