use serde_json::Value as JsonValue;

pub(crate) fn node_mut<'document>(
    document: &'document mut JsonValue,
    suffix: &str,
) -> &'document mut JsonValue {
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|node| node["@id"].as_str().is_some_and(|id| id.ends_with(suffix)))
        .unwrap_or_else(|| panic!("node ending in `{suffix}`"))
}

pub(crate) fn attr_input_document(kind: &str, class_path: &str, attrs: JsonValue) -> JsonValue {
    let input_type = format!("S231:{kind}Input");
    let output_type = format!("S231:{kind}Output");
    let data_type = format!("S231:{kind}");
    let mut document = serde_json::json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            {
                "@id": "http://example.org#AttrInput",
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": "http://example.org#AttrInput.add" },
                "S231:hasInput": [
                    { "@id": "http://example.org#AttrInput.uExt" },
                    { "@id": "http://example.org#AttrInput.uOther" }
                ]
            },
            {
                "@id": "http://example.org#AttrInput.add",
                "@type": format!("http://example.org#Buildings.Controls.OBC.{class_path}"),
                "S231:hasInput": [
                    { "@id": "http://example.org#AttrInput.add.u1" },
                    { "@id": "http://example.org#AttrInput.add.u2" }
                ],
                "S231:hasOutput": { "@id": "http://example.org#AttrInput.add.y" }
            },
            {
                "@id": "http://example.org#AttrInput.add.u1",
                "@type": input_type,
                "S231:isOfDataType": { "@id": data_type }
            },
            {
                "@id": "http://example.org#AttrInput.add.u2",
                "@type": input_type,
                "S231:isOfDataType": { "@id": data_type }
            },
            {
                "@id": "http://example.org#AttrInput.add.y",
                "@type": output_type,
                "S231:isOfDataType": { "@id": data_type }
            },
            {
                "@id": "http://example.org#AttrInput.uExt",
                "@type": input_type,
                "S231:isOfDataType": { "@id": data_type },
                "S231:isConnectedTo": { "@id": "http://example.org#AttrInput.add.u1" }
            },
            {
                "@id": "http://example.org#AttrInput.uOther",
                "@type": input_type,
                "S231:isOfDataType": { "@id": data_type },
                "S231:isConnectedTo": { "@id": "http://example.org#AttrInput.add.u2" }
            }
        ]
    });
    let boundary = node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object");
    for (key, value) in attrs.as_object().expect("attrs object") {
        boundary.insert(key.clone(), value.clone());
    }
    document
}
