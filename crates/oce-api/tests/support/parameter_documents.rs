//! One-block CXF documents built in the test, for the parameter-kind tests (issues #734, #735):
//! a root `S` holding one block `blk`, optionally driven by one root input `u`, its output wired
//! to the root output `out`, with block and root parameters each written as the case states.

use oce_api::{Engine, OcError, Value};

/// The XSD namespace a typed literal's datatype is written in.
pub(crate) const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// One parameter node: its local name and the JSON members after `@id`.
pub(crate) struct Param {
    pub(crate) name: String,
    pub(crate) members: String,
}

/// A parameter written `"S231:value": <literal>`.
pub(crate) fn value(name: &str, literal: &str) -> Param {
    Param {
        name: name.to_owned(),
        members: format!(r#""S231:value": {literal}"#),
    }
}

/// A root parameter declared `S231:<datatype>` and written `literal`.
pub(crate) fn declared(name: &str, datatype: &str, literal: &str) -> Param {
    Param {
        name: name.to_owned(),
        members: format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "S231:{datatype}" }}, "S231:value": {literal}"#
        ),
    }
}

/// A preserved array parameter of `size` (one dimension) written `literal`.
pub(crate) fn vector(name: &str, size: usize, literal: &str) -> Param {
    Param {
        name: name.to_owned(),
        members: format!(
            r#""S231:isArray": true, "S231:numberDimensions": 1, "S231:sizeOfDimensions": "({size})", "S231:value": {literal}"#
        ),
    }
}

/// A preserved 2x2 table parameter written `literal`.
pub(crate) fn table(name: &str, literal: &str) -> Param {
    Param {
        name: name.to_owned(),
        members: format!(
            r#""S231:isArray": true, "S231:numberDimensions": 2, "S231:sizeOfDimensions": "(2, 2)", "S231:value": {literal}"#
        ),
    }
}

/// A typed literal `{"@value": lexical, "@type": xsd:datatype}`.
pub(crate) fn typed(lexical: &str, datatype: &str) -> String {
    format!(r#"{{ "@value": "{lexical}", "@type": "{XSD}{datatype}" }}"#)
}

/// The shape of one document: the block's class, its output type and, if it has one input, the
/// input's type.
pub(crate) struct Shape<'a> {
    pub(crate) class: &'a str,
    pub(crate) output: &'a str,
    pub(crate) input: Option<&'a str>,
}

/// A source block of `class` with output type `output` and no input.
pub(crate) fn source<'a>(class: &'a str, output: &'a str) -> Shape<'a> {
    Shape {
        class,
        output,
        input: None,
    }
}

const NS: &str = "http://example.org#";

/// The document of `shape` with block parameters `params` and root parameters `root`.
pub(crate) fn document(shape: &Shape<'_>, params: &[Param], root: &[Param]) -> String {
    let refs = |owner: &str, list: &[Param]| {
        list.iter()
            .map(|param| format!(r#"{{ "@id": "{NS}{owner}.{}" }}"#, param.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let nodes = |owner: &str, list: &[Param]| {
        list.iter()
            .map(|param| {
                format!(
                    "    {{ \"@id\": \"{NS}{owner}.{}\", {} }},\n",
                    param.name, param.members
                )
            })
            .collect::<String>()
    };
    let output = shape.output;
    let (root_input, block_input, input_nodes) = match shape.input {
        None => (String::new(), String::new(), String::new()),
        Some(input) => (
            format!(r#", "S231:hasInput": {{ "@id": "{NS}S.u" }}"#),
            format!(r#", "S231:hasInput": {{ "@id": "{NS}S.blk.u" }}"#),
            format!(
                "    {{ \"@id\": \"{NS}S.u\", \"@type\": \"S231:{input}Input\", \
                 \"S231:isOfDataType\": {{ \"@id\": \"S231:{input}\" }}, \
                 \"S231:isConnectedTo\": {{ \"@id\": \"{NS}S.blk.u\" }} }},\n    \
                 {{ \"@id\": \"{NS}S.blk.u\", \"@type\": \"S231:{input}Input\", \
                 \"S231:isOfDataType\": {{ \"@id\": \"S231:{input}\" }} }},\n"
            ),
        ),
    };
    format!(
        r#"{{
  "@context": {{ "S231": "http://data.ashrae.org/S231#" }},
  "@graph": [
    {{ "@id": "{NS}S", "@type": "S231:Block",
      "S231:containsBlock": {{ "@id": "{NS}S.blk" }},
      "S231:hasParameter": [ {root_refs} ]{root_input},
      "S231:hasOutput": {{ "@id": "{NS}S.out" }} }},
{root_nodes}{input_nodes}    {{ "@id": "{NS}S.blk",
      "@type": "{NS}Buildings.Controls.OBC.{class}",
      "S231:hasParameter": [ {block_refs} ]{block_input},
      "S231:hasOutput": {{ "@id": "{NS}S.blk.y" }} }},
{block_nodes}    {{ "@id": "{NS}S.blk.y", "@type": "S231:{output}Output",
      "S231:isOfDataType": {{ "@id": "S231:{output}" }},
      "S231:isConnectedTo": {{ "@id": "{NS}S.out" }} }},
    {{ "@id": "{NS}S.out", "@type": "S231:{output}Output",
      "S231:isOfDataType": {{ "@id": "S231:{output}" }} }}
  ]
}}"#,
        class = shape.class,
        root_refs = refs("S", root),
        root_nodes = nodes("S", root),
        block_refs = refs("S.blk", params),
        block_nodes = nodes("S.blk", params),
    )
}

/// The engine loaded from `doc`, or the refusal.
pub(crate) fn load(doc: &str) -> Result<Engine, OcError> {
    let mut engine = Engine::new();
    engine.load_cxf(doc.as_bytes()).map(|_| engine)
}

/// The engine loaded from `doc`; panics with the refusal and the document otherwise.
pub(crate) fn loaded(doc: &str) -> Engine {
    load(doc).unwrap_or_else(|error| panic!("expected the document to load: {error:?}\n{doc}"))
}

/// Every diagnostic code the refusal of `doc` carries, in report order; panics if it loads.
pub(crate) fn refusal_codes(doc: &str) -> Vec<String> {
    match load(doc) {
        Ok(_) => panic!("expected a refusal\n{doc}"),
        Err(error) => error
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.as_str().to_owned())
            .collect(),
    }
}

/// The value the loaded block binds to `name`.
pub(crate) fn param(engine: &Engine, name: &str) -> Value {
    engine
        .get_param(&format!("{NS}S.blk.{name}"))
        .unwrap_or_else(|error| panic!("{name}: {error:?}"))
}

/// A run of output values, compared bit for bit (`Value` has no `==`: a Real compares by its
/// bits, so `-0.0` is not `0.0`).
#[derive(Clone, Debug)]
pub(crate) struct Values(pub(crate) Vec<Value>);

impl std::ops::Deref for Values {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        &self.0
    }
}

impl PartialEq<[Value]> for Values {
    fn eq(&self, other: &[Value]) -> bool {
        self.0.len() == other.len() && self.0.iter().zip(other).all(|(a, b)| a.bit_eq(b))
    }
}

impl<const N: usize> PartialEq<[Value; N]> for Values {
    fn eq(&self, other: &[Value; N]) -> bool {
        *self == other[..]
    }
}

impl PartialEq for Values {
    fn eq(&self, other: &Values) -> bool {
        *self == other.0[..]
    }
}

/// The root output after one frame at each time of `times`, driving the input `u` (if any) with
/// `input`.
pub(crate) fn outputs(engine: &mut Engine, times: &[f64], input: Option<Value>) -> Values {
    Values(
        times
            .iter()
            .map(|&time| {
                let inputs: Vec<(&str, Value)> = input
                    .iter()
                    .map(|value| ("http://example.org#S.u", value.clone()))
                    .collect();
                let frame = engine
                    .prepare_frame(time, &inputs)
                    .expect("the frame prepares");
                let done = engine.execute_frame(frame).expect("the frame executes");
                done.outputs()
                    .iter()
                    .find(|(path, _)| path == "http://example.org#S.out")
                    .map(|(_, value)| value.clone())
                    .expect("the root output")
            })
            .collect(),
    )
}
