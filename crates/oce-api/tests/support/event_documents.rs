//! A small flat-document builder for the event-input tests (issue #768): blocks with typed
//! parameters and ports, boundary inputs and outputs, and connections, emitted as a CXF document
//! the facade loads; plus a runner that ticks frames at t = 0, 1, 2, ... s.

#![allow(dead_code)]

use oce_api::{Engine, Value};
use serde_json::{Value as Json, json};

pub const PREFIX: &str = "http://example.org#evt.";

pub fn iri(local: &str) -> String {
    format!("{PREFIX}{local}")
}

/// One block: name, class under `CDL.`, `(parameter, data type, literal)`, inputs, outputs.
pub struct Blk {
    pub name: &'static str,
    pub class: &'static str,
    pub params: Vec<(&'static str, &'static str, &'static str)>,
    pub ins: Vec<(&'static str, &'static str)>,
    pub outs: Vec<(&'static str, &'static str)>,
}

/// A flat document: blocks, boundary inputs `(name, type, targets)`, boundary outputs and
/// connections `from -> to` (a port `block.port` or a boundary output name).
#[derive(Default)]
pub struct Doc {
    pub blocks: Vec<Blk>,
    pub inputs: Vec<(&'static str, &'static str, Vec<&'static str>)>,
    pub outputs: Vec<(&'static str, &'static str)>,
    pub wires: Vec<(&'static str, &'static str)>,
}

fn data_type(kind: &str) -> Json {
    json!({ "@id": format!("S231:{kind}") })
}

fn literal(kind: &str, value: &str) -> (Json, Json) {
    let xsd = |t: &str| format!("http://www.w3.org/2001/XMLSchema#{t}");
    match kind {
        "SimpleController" => (
            json!({ "@id": "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController" }),
            json!(format!(
                "Buildings.Controls.OBC.CDL.Types.SimpleController.{value}"
            )),
        ),
        "Real" => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("double") }),
        ),
        "Integer" => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("integer") }),
        ),
        _ => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("boolean") }),
        ),
    }
}

impl Doc {
    pub fn bytes(&self) -> Vec<u8> {
        let ids = |names: &mut dyn Iterator<Item = String>| {
            names.map(|n| json!({ "@id": iri(&n) })).collect::<Vec<_>>()
        };
        let targets = |port: &str| {
            let mut to: Vec<_> = self
                .wires
                .iter()
                .filter(|(from, _)| *from == port)
                .map(|(_, to)| json!({ "@id": iri(to) }))
                .collect();
            for (name, _, wired) in &self.inputs {
                if *name == port {
                    to.extend(wired.iter().map(|t| json!({ "@id": iri(t) })));
                }
            }
            to
        };
        let mut graph = vec![json!({
            "@id": "http://example.org#evt",
            "@type": "S231:Block",
            "S231:containsBlock": ids(&mut self.blocks.iter().map(|b| b.name.to_owned())),
            "S231:hasInput": ids(&mut self.inputs.iter().map(|i| i.0.to_owned())),
            "S231:hasOutput": ids(&mut self.outputs.iter().map(|o| o.0.to_owned())),
        })];
        for (name, kind, _) in &self.inputs {
            graph.push(json!({
                "@id": iri(name), "@type": format!("S231:{kind}Input"),
                "S231:isOfDataType": data_type(kind), "S231:isConnectedTo": targets(name),
            }));
        }
        for (name, kind) in &self.outputs {
            graph.push(json!({
                "@id": iri(name), "@type": format!("S231:{kind}Output"),
                "S231:isOfDataType": data_type(kind),
            }));
        }
        for block in &self.blocks {
            let port = |p: &str| format!("{}.{p}", block.name);
            let mut node = json!({
                "@id": iri(block.name),
                "@type": format!("http://example.org#Buildings.Controls.OBC.CDL.{}", block.class),
                "S231:hasInput": ids(&mut block.ins.iter().map(|(p, _)| port(p))),
                "S231:hasOutput": ids(&mut block.outs.iter().map(|(p, _)| port(p))),
            });
            if !block.params.is_empty() {
                node["S231:hasParameter"] =
                    json!(ids(&mut block.params.iter().map(|(p, _, _)| port(p))));
            }
            graph.push(node);
            for (name, kind, value) in &block.params {
                let (data, value) = literal(kind, value);
                graph.push(json!({
                    "@id": iri(&port(name)), "@type": "S231:Parameter",
                    "S231:isOfDataType": data, "S231:value": value,
                }));
            }
            for (name, kind) in &block.ins {
                graph.push(json!({
                    "@id": iri(&port(name)), "@type": format!("S231:{kind}Input"),
                    "S231:isOfDataType": data_type(kind),
                }));
            }
            for (name, kind) in &block.outs {
                let mut node = json!({
                    "@id": iri(&port(name)), "@type": format!("S231:{kind}Output"),
                    "S231:isOfDataType": data_type(kind),
                });
                let to = targets(&port(name));
                if !to.is_empty() {
                    node["S231:isConnectedTo"] = json!(to);
                }
                graph.push(node);
            }
        }
        serde_json::to_vec(&json!({
            "@context": { "S231": "http://data.ashrae.org/S231#" },
            "@graph": graph,
        }))
        .expect("serialize document")
    }

    pub fn pre(&mut self, name: &'static str) {
        self.blocks.push(Blk {
            name,
            class: "Logical.Pre",
            params: vec![],
            ins: vec![("u", "Boolean")],
            outs: vec![("y", "Boolean")],
        });
    }
}

/// Run frames at t = 0, 1, 2, ... and return the first boundary output of each.
pub fn run(doc: &Doc, frames: &[Vec<(&str, Value)>]) -> Vec<Value> {
    let mut engine = Engine::new();
    engine
        .load_cxf(&doc.bytes())
        .expect("the delayed model loads");
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let names: Vec<String> = frame.iter().map(|(n, _)| iri(n)).collect();
            let entries: Vec<_> = names
                .iter()
                .zip(frame)
                .map(|(n, (_, v))| (n.as_str(), v.clone()))
                .collect();
            let prepared = engine.prepare_frame(index as f64, &entries).unwrap();
            let done = engine.execute_frame(prepared).unwrap();
            done.outputs()[0].1.clone()
        })
        .collect()
}
