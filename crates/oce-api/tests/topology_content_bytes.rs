//! The canonical content bytes rebuilt from a topology snapshot alone (issue #701).
//!
//! `Topology` is documented to hold every fact the canonical content id encodes, so that two
//! loaded documents with equal topologies have equal ids. This file holds the claim itself, for
//! the whole id and not only for the boundary: [`rebuild`] writes the `OCECANON` v1 bytes
//! (`docs/design/content-id.md`, "Canonical bytes") from nothing but a [`Topology`] and the
//! class registry, and every loadable CXF document under `crates/` must give exactly
//! [`Engine::canonical_content_bytes`]. Equal topologies then give equal bytes, and equal ids.
//!
//! The rebuild is a second implementation, typed from the design note. It shares no code with
//! the encoder: the naming rule, the collision rule, the sort orders and the tag tables are all
//! written again here. Where a topology did not say which port an edge ends at, or which input
//! a pass-through reads, [`rebuild`] would panic and that would be a finding about the view.
//!
//! The documents are every `.jsonld` and `.cxf.json` file under `crates/` that loads: the
//! fixtures of `oce-cxf`, `oce-api` and `oce-conformance`, whose count is pinned, and those of
//! the leverage crates, which are held to a floor only. A leverage-only pull request does not
//! run this suite, so its fixtures must not move a count here. `third_party/` is not read: a
//! pin advance must not have to move a count in this crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oce_api::{Engine, EnumClassId, Topology, Value, ValueType};
use serde_json::json;

/// A reduced IRI: form 0 and the text after the namespace, or form 1 and the whole IRI.
type Name = (u8, String);

/// The canonical bytes, appended to.
#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn count(&mut self, value: usize) {
        self.u32(u32::try_from(value).expect("a count fits u32"));
    }

    fn str(&mut self, text: &str) {
        self.count(text.len());
        self.0.extend_from_slice(text.as_bytes());
    }

    fn optional_str(&mut self, text: Option<&str>) {
        match text {
            None => self.u8(0),
            Some(text) => {
                self.u8(1);
                self.str(text);
            }
        }
    }

    fn name(&mut self, name: &Name) {
        self.u8(name.0);
        self.str(&name.1);
    }

    /// 0 Real, 1 Integer, 2 Boolean, 3 String, 4 Enum and its class path.
    fn value_type(&mut self, value_type: ValueType) {
        match value_type {
            ValueType::Real => self.u8(0),
            ValueType::Integer => self.u8(1),
            ValueType::Boolean => self.u8(2),
            ValueType::String => self.u8(3),
            ValueType::Enum(class) => {
                self.u8(4);
                self.str(durable_class_path(class));
            }
        }
    }

    /// 0 Real bits, 1 Integer, 2 Boolean, 3 String, 4 Enum class path and ordinal.
    fn value(&mut self, value: &Value) {
        match value {
            Value::Real(real) => {
                self.u8(0);
                self.0.extend_from_slice(&real.to_bits().to_le_bytes());
            }
            Value::Integer(integer) => {
                self.u8(1);
                self.0.extend_from_slice(&integer.to_le_bytes());
            }
            Value::Boolean(boolean) => {
                self.u8(2);
                self.u8(u8::from(*boolean));
            }
            Value::String(text) => {
                self.u8(3);
                self.str(text);
            }
            Value::Enum { class, ordinal } => {
                self.u8(4);
                self.str(durable_class_path(*class));
                self.u32(*ordinal);
            }
        }
    }

    fn optional_value(&mut self, value: Option<&Value>) {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.value(value);
            }
        }
    }
}

/// The class path a durable record writes for an enumeration: relative to
/// `Buildings.Controls.OBC.` for a CDL enumeration and the full Modelica path for a G36 one. It
/// is a function of the class identity a topology carries, not the authoring path
/// `oce_api::enumeration_class_path` gives.
fn durable_class_path(class: EnumClassId) -> &'static str {
    oce_model::enum_descriptor(class)
        .expect("a registered enumeration")
        .class_path
}

/// What follows the last `#`, `/` or `:` of an IRI, when anything does.
fn remainder(iri: &str) -> Option<&str> {
    let start = iri.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    iri.get(start..).filter(|rest| !rest.is_empty())
}

/// Name the IRIs of one kind: the remainder, unless it is empty or another IRI of the kind has
/// the same one, in which case each of those keeps its whole IRI.
fn names<'a>(iris: impl Iterator<Item = &'a str> + Clone) -> BTreeMap<&'a str, Name> {
    let mut uses: BTreeMap<&str, usize> = BTreeMap::new();
    for rest in iris.clone().filter_map(remainder) {
        *uses.entry(rest).or_default() += 1;
    }
    iris.map(|iri| {
        let name = match remainder(iri) {
            Some(rest) if uses.get(rest) == Some(&1) => (0, rest.to_owned()),
            _ => (1, iri.to_owned()),
        };
        (iri, name)
    })
    .collect()
}

/// The one port a connector path names; a path that names none or several is something the
/// topology does not say.
fn the_port<'a>(ports: &'a BTreeMap<&str, Vec<(Name, u32)>>, path: &str) -> &'a (Name, u32) {
    match ports.get(path).map(Vec::as_slice) {
        Some([port]) => port,
        other => panic!("`{path}` names {} ports", other.map_or(0, <[_]>::len)),
    }
}

/// The `OCECANON` v1 bytes of the document `topology` was taken from, from the topology alone.
fn rebuild(topology: &Topology) -> Vec<u8> {
    let block_names = names(topology.blocks.iter().map(|b| b.instance_path.as_str()));
    let input_names = names(topology.boundary_inputs.iter().map(|i| i.path.as_str()));
    let output_names = names(topology.boundary_outputs.iter().map(|o| o.path.as_str()));

    // Every block port by the connector path it carries, with its index in the block's list.
    let mut fed: BTreeMap<&str, Vec<(Name, u32)>> = BTreeMap::new();
    let mut driven: BTreeMap<&str, Vec<(Name, u32)>> = BTreeMap::new();
    for block in &topology.blocks {
        let name = &block_names[block.instance_path.as_str()];
        for (ports, paths) in [(&mut fed, &block.inputs), (&mut driven, &block.outputs)] {
            for (port, path) in paths.iter().enumerate() {
                ports
                    .entry(path.as_str())
                    .or_default()
                    .push((name.clone(), u32::try_from(port).unwrap()));
            }
        }
    }

    let mut bytes = Bytes::default();
    bytes.0.extend_from_slice(b"OCECANON");
    bytes.0.extend_from_slice(&[1, 0, 0, 0]);

    // Inputs, by name; the fan-out is every block input that carries the input's path.
    let mut inputs: Vec<_> = topology.boundary_inputs.iter().collect();
    inputs.sort_by_key(|input| &input_names[input.path.as_str()]);
    bytes.count(inputs.len());
    for input in inputs {
        bytes.name(&input_names[input.path.as_str()]);
        bytes.value_type(input.value_type);
        bytes.optional_str(input.quantity.as_deref());
        bytes.optional_str(input.unit.as_deref());
        bytes.optional_str(input.display_unit.as_deref());
        bytes.optional_value(input.min.as_ref());
        bytes.optional_value(input.max.as_ref());
        let mut targets = fed.get(input.path.as_str()).cloned().unwrap_or_default();
        targets.sort();
        bytes.count(targets.len());
        for (block, port) in &targets {
            bytes.name(block);
            bytes.u32(*port);
        }
    }

    // Outputs, by name; a pass-through output is driven by its pair's input.
    let mut outputs: Vec<_> = topology.boundary_outputs.iter().collect();
    outputs.sort_by_key(|output| &output_names[output.path.as_str()]);
    bytes.count(outputs.len());
    for output in outputs {
        bytes.name(&output_names[output.path.as_str()]);
        bytes.value_type(output.value_type);
        bytes.optional_str(output.quantity.as_deref());
        bytes.optional_str(output.unit.as_deref());
        bytes.optional_str(output.display_unit.as_deref());
        let passed: Vec<_> = topology
            .pass_through
            .iter()
            .filter(|pair| pair.output == output.path)
            .collect();
        match passed.as_slice() {
            [] => {
                let (block, port) = the_port(&driven, &output.driver_path);
                bytes.u8(0);
                bytes.name(block);
                bytes.u32(*port);
            }
            [pair] => {
                assert_eq!(output.driver_path, output.path);
                bytes.u8(1);
                bytes.name(&input_names[pair.input.as_str()]);
            }
            several => panic!("`{}` has {} pass-through pairs", output.path, several.len()),
        }
    }

    // Blocks, by name; parameters by name bytes.
    let mut blocks: Vec<_> = topology.blocks.iter().collect();
    blocks.sort_by_key(|block| &block_names[block.instance_path.as_str()]);
    bytes.count(blocks.len());
    for block in blocks {
        bytes.name(&block_names[block.instance_path.as_str()]);
        bytes.str(&block.class_iri);
        let signature = oce_blocks::class_signature_hash(&block.class_iri).expect("registered");
        bytes.0.extend_from_slice(&signature.to_le_bytes());
        let mut params: Vec<_> = block.params.iter().collect();
        params.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        bytes.count(params.len());
        for (name, value) in params {
            bytes.str(name);
            bytes.value(value);
        }
    }

    // Connections: target block and input port, then source block and output port.
    let mut edges: Vec<(Name, u32, Name, u32)> = topology
        .connections
        .iter()
        .map(|edge| {
            let (target, input) = the_port(&fed, &edge.to).clone();
            let (source, output) = the_port(&driven, &edge.from).clone();
            (target, input, source, output)
        })
        .collect();
    edges.sort();
    bytes.count(edges.len());
    for (target, input, source, output) in &edges {
        bytes.name(target);
        bytes.u32(*input);
        bytes.name(source);
        bytes.u32(*output);
    }
    bytes.0
}

/// Every `.jsonld` and `.cxf.json` file under `directory`, sorted.
fn documents(directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "target" {
                documents(&path, found);
            }
        } else if name.ends_with(".jsonld") || name.ends_with(".cxf.json") {
            found.push(path);
        }
    }
}

/// Load `bytes` and require the rebuilt bytes to be the engine's; `false` when refused.
fn rebuilds(what: &str, bytes: &[u8]) -> bool {
    let mut engine = Engine::new();
    if engine.load_cxf(bytes).is_err() {
        return false;
    }
    let expected = engine.canonical_content_bytes().unwrap();
    let rebuilt = rebuild(&engine.topology());
    assert!(
        rebuilt == expected,
        "{what}: the bytes rebuilt from the topology are not the content id's"
    );
    true
}

#[test]
fn the_content_bytes_of_every_loadable_document_are_rebuilt_from_its_topology() {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let (mut engine_side, mut leverage, mut refused) = (0, 0, 0);
    let mut found = Vec::new();
    documents(&crates, &mut found);
    for path in &found {
        let what = path.display().to_string();
        let above_the_facade = path.components().any(|part| part.as_os_str() == "leverage");
        if !rebuilds(&what, &std::fs::read(path).unwrap()) {
            refused += usize::from(!above_the_facade);
        } else if above_the_facade {
            leverage += 1;
        } else {
            engine_side += 1;
        }
    }
    // The engine crates' documents: those that load, and those the loader refuses (the
    // refusal fixtures). The leverage crates' are a floor, so a fixture added there moves
    // nothing here: `oce-author`'s six emit goldens load by that crate's own contract.
    assert_eq!((engine_side, refused), (117, 74));
    assert!(leverage >= 6, "{leverage} leverage documents loaded");
}

/// A root whose one input `u` reaches four leaf ports through two levels of composites, both
/// ports of a block beside them, and a pass-through output.
fn nested_fan_out(namespace_of_direct: &str) -> serde_json::Value {
    let root = "http://example.org#ex.fan";
    let at = |local: &str| json!({ "@id": format!("{root}.{local}") });
    let direct = format!("{namespace_of_direct}ex.fan.direct");
    let class = |path: &str| format!("http://example.org#Buildings.Controls.OBC.{path}");
    let real = |id: String, kind: &str, to: Vec<serde_json::Value>| {
        let mut node = json!({
            "@id": id,
            "@type": format!("S231:Real{kind}"),
            "S231:isOfDataType": { "@id": "S231:Real" },
        });
        if !to.is_empty() {
            node["S231:isConnectedTo"] = json!(to);
        }
        node
    };
    let input =
        |local: &str, to: Vec<serde_json::Value>| real(format!("{root}.{local}"), "Input", to);
    let output =
        |local: &str, to: Vec<serde_json::Value>| real(format!("{root}.{local}"), "Output", to);
    let gain = |local: &str, k: &str| {
        vec![
            json!({
                "@id": format!("{root}.{local}"),
                "@type": class("CDL.Reals.MultiplyByParameter"),
                "S231:hasParameter": [at(&format!("{local}.k"))],
                "S231:hasInput": [at(&format!("{local}.u"))],
                "S231:hasOutput": [at(&format!("{local}.y"))],
            }),
            json!({
                "@id": format!("{root}.{local}.k"),
                "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:value": { "@value": k, "@type": "http://www.w3.org/2001/XMLSchema#double" },
            }),
        ]
    };
    let mut graph = vec![
        json!({
            "@id": root,
            "@type": "S231:Block",
            "S231:hasInput": [at("u")],
            "S231:containsBlock": [at("outer"), { "@id": direct }],
            "S231:hasOutput": [at("ySum"), at("yDirect"), at("yPass")],
        }),
        input(
            "u",
            vec![
                at("outer.u"),
                json!({ "@id": format!("{direct}.u1") }),
                json!({ "@id": format!("{direct}.u2") }),
                at("yPass"),
            ],
        ),
        output("ySum", vec![]),
        output("yDirect", vec![]),
        output("yPass", vec![]),
        json!({
            "@id": direct,
            "@type": class("CDL.Reals.Add"),
            "S231:hasInput": [{ "@id": format!("{direct}.u1") }, { "@id": format!("{direct}.u2") }],
            "S231:hasOutput": [{ "@id": format!("{direct}.y") }],
        }),
        real(format!("{direct}.u1"), "Input", vec![]),
        real(format!("{direct}.u2"), "Input", vec![]),
        real(format!("{direct}.y"), "Output", vec![at("yDirect")]),
        json!({
            "@id": format!("{root}.outer"),
            "@type": "http://example.org#Vendor.Sequences.Outer",
            "S231:containsBlock": [at("outer.inner")],
            "S231:hasInput": [at("outer.u")],
            "S231:hasOutput": [at("outer.y")],
        }),
        input("outer.u", vec![at("outer.inner.u")]),
        output("outer.y", vec![at("ySum")]),
        json!({
            "@id": format!("{root}.outer.inner"),
            "@type": "http://example.org#Vendor.Sequences.Inner",
            "S231:containsBlock": [
                at("outer.inner.g1"), at("outer.inner.g2"), at("outer.inner.direct")
            ],
            "S231:hasInput": [at("outer.inner.u")],
            "S231:hasOutput": [at("outer.inner.y")],
        }),
        input(
            "outer.inner.u",
            vec![at("outer.inner.g1.u"), at("outer.inner.g2.u")],
        ),
        output("outer.inner.y", vec![at("outer.y")]),
        input("outer.inner.g1.u", vec![]),
        output("outer.inner.g1.y", vec![at("outer.inner.direct.u1")]),
        input("outer.inner.g2.u", vec![]),
        output("outer.inner.g2.y", vec![at("outer.inner.direct.u2")]),
        json!({
            "@id": format!("{root}.outer.inner.direct"),
            "@type": class("CDL.Reals.Add"),
            "S231:hasInput": [at("outer.inner.direct.u1"), at("outer.inner.direct.u2")],
            "S231:hasOutput": [at("outer.inner.direct.y")],
        }),
        input("outer.inner.direct.u1", vec![]),
        input("outer.inner.direct.u2", vec![]),
        output("outer.inner.direct.y", vec![at("outer.inner.y")]),
    ];
    graph.extend(gain("outer.inner.g1", "2.0"));
    graph.extend(gain("outer.inner.g2", "3.0"));
    json!({ "@context": { "S231": "http://data.ashrae.org/S231#" }, "@graph": graph })
}

#[test]
fn a_nested_composite_fan_out_is_rebuilt_from_its_topology() {
    let document = nested_fan_out("http://example.org#");
    let bytes = serde_json::to_vec(&document).unwrap();
    assert!(rebuilds("nested fan-out", &bytes), "the document loads");

    let mut engine = Engine::new();
    engine.load_cxf(&bytes).unwrap();
    let topology = engine.topology();
    // One input, four leaf ports behind two composite boundaries, and a pass-through.
    assert_eq!(topology.boundary_inputs.len(), 1);
    let input = &topology.boundary_inputs[0].path;
    let fan_out: usize = topology
        .blocks
        .iter()
        .map(|block| block.inputs.iter().filter(|path| *path == input).count())
        .sum();
    assert_eq!(fan_out, 4);
    assert_eq!(topology.pass_through.len(), 1);
    assert_eq!(topology.boundary_outputs.len(), 3);
    assert_eq!(topology.connections.len(), 2);
}

#[test]
fn two_blocks_whose_names_collide_across_namespaces_are_rebuilt_with_their_whole_iris() {
    // `direct` moved to a second namespace: its name after the namespace is still
    // `ex.fan.direct`, which no other block has, so nothing collides yet.
    let moved = nested_fan_out("urn:other:");
    assert!(rebuilds("moved", &serde_json::to_vec(&moved).unwrap()));

    // Now rename the inner `direct` so that both blocks reduce to `ex.fan.direct`: the id keeps
    // the whole IRI of each, and so must the rebuild.
    let text = serde_json::to_string(&moved).unwrap().replace(
        "http://example.org#ex.fan.outer.inner.direct",
        "http://example.org#ex.fan.direct",
    );
    assert!(rebuilds("collision", text.as_bytes()), "the document loads");
    let mut engine = Engine::new();
    engine.load_cxf(text.as_bytes()).unwrap();
    let bytes = engine.canonical_content_bytes().unwrap();
    let holds = |needle: &str| {
        bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    assert!(holds("urn:other:ex.fan.direct"));
    assert!(holds("http://example.org#ex.fan.direct"));
    // A block the collision does not touch keeps its short name.
    assert!(!holds("http://example.org#ex.fan.outer.inner.g1"));
}

#[test]
fn a_rebuild_from_an_edited_topology_is_not_the_documents_bytes() {
    // The control: the rebuild reads every part of a topology, so changing one part of a
    // snapshot changes the bytes it gives.
    let bytes = serde_json::to_vec(&nested_fan_out("http://example.org#")).unwrap();
    let mut engine = Engine::new();
    engine.load_cxf(&bytes).unwrap();
    let expected = engine.canonical_content_bytes().unwrap();
    let reference = engine.topology();
    assert_eq!(rebuild(&reference), expected);
    type Edit = (&'static str, fn(&mut Topology));
    let edits: [Edit; 10] = [
        ("input unit", |t| {
            t.boundary_inputs[0].unit = Some("K".into())
        }),
        ("input bound", |t| {
            t.boundary_inputs[0].max = Some(Value::Real(1.0));
        }),
        ("input type", |t| {
            t.boundary_inputs[0].value_type = ValueType::Integer;
        }),
        ("output quantity", |t| {
            t.boundary_outputs[0].quantity = Some("Q".into());
        }),
        ("output type", |t| {
            t.boundary_outputs[0].value_type = ValueType::Boolean;
        }),
        ("output driver", |t| {
            let other = t.boundary_outputs[1].driver_path.clone();
            t.boundary_outputs[0].driver_path = other;
        }),
        ("block name", |t| t.blocks[0].instance_path.push('x')),
        ("block class", |t| {
            t.blocks[0].class_iri = "CDL.Reals.Subtract".into();
        }),
        ("parameter", |t| {
            let gain = t.blocks.iter_mut().find(|b| !b.params.is_empty()).unwrap();
            gain.params[0].1 = Value::Real(2.5);
        }),
        ("fan-out port", |t| {
            let input = t.boundary_inputs[0].path.clone();
            let fed = t.blocks.iter_mut().find(|b| b.inputs.contains(&input));
            fed.unwrap()
                .inputs
                .iter_mut()
                .for_each(|path| path.push('x'));
        }),
    ];
    for (what, edit) in edits {
        let mut edited = reference.clone();
        edit(&mut edited);
        assert_ne!(rebuild(&edited), expected, "{what}");
    }
    // The order of the connection list is outside the id, so it is outside the rebuild.
    let mut reordered = reference;
    reordered.connections.swap(0, 1);
    assert_eq!(rebuild(&reordered), expected);
}
