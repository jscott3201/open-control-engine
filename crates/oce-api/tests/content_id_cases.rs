//! Canonical content id edge cases (`docs/design/content-id.md`, test plan): the pass-through
//! output, names that collide or end in a delimiter, a boundary type change, the refusals, and
//! the largest corpus encoding.

// Shared helper, declared once for this binary in its root `facade.rs`.
use crate::content_id_corpus as corpus;

use corpus::*;
use oce_api::{Engine, EngineStateError, OcError, Value};

fn pass_through() -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/content_id/pass_through.jsonld",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }
    fn u8(&mut self, value: u8) -> &mut Self {
        self.raw(&[value])
    }
    fn u32(&mut self, value: u32) -> &mut Self {
        self.raw(&value.to_le_bytes())
    }
    fn str(&mut self, text: &str) -> &mut Self {
        self.u32(text.len() as u32).raw(text.as_bytes())
    }
    fn name(&mut self, text: &str) -> &mut Self {
        self.u8(0).str(text)
    }
}

#[test]
fn a_pass_through_output_is_driven_by_its_input_and_is_no_block() {
    let engine = load(pass_through().as_bytes());
    let class = "CDL.Reals.MultiplyByParameter";
    let mut expected = Bytes::default();
    expected.raw(b"OCECANON").raw(&[1, 0, 0, 0]);
    // Inputs: `a` feeds only the pass-through, so it has a record with no targets.
    expected.u32(2);
    expected
        .name("ex.pass.a")
        .u8(0)
        .raw(&[0, 0, 0, 0, 0])
        .u32(0);
    expected
        .name("ex.pass.u")
        .u8(0)
        .raw(&[0, 0, 0, 0, 0])
        .u32(1)
        .name("ex.pass.gai")
        .u32(0);
    // Outputs: `b` carries the declared node's unit through the minted connector and is driven
    // by input `a` (driver tag 1); `y` is driven by `gai` port 0.
    expected.u32(2);
    expected
        .name("ex.pass.b")
        .u8(0)
        .u8(0)
        .u8(1)
        .str("K")
        .u8(0)
        .u8(1)
        .name("ex.pass.a");
    expected
        .name("ex.pass.y")
        .u8(0)
        .raw(&[0, 0, 0])
        .u8(0)
        .name("ex.pass.gai")
        .u32(0);
    // Blocks: only `gai`; the reserved pass-through block is not one.
    expected
        .u32(1)
        .name("ex.pass.gai")
        .str(class)
        .raw(
            &oce_blocks::class_signature_hash(class)
                .unwrap()
                .to_le_bytes(),
        )
        .u32(1)
        .str("k")
        .u8(0)
        .raw(&2.0_f64.to_bits().to_le_bytes());
    expected.u32(0);
    assert_eq!(engine.canonical_content_bytes().unwrap(), expected.0);
    assert_eq!(engine.topology().pass_through.len(), 1);

    let original = engine.content_id().unwrap();
    let moved = |from: &str, to: &str| {
        let changed = pass_through().replace(from, to);
        assert_ne!(changed, pass_through(), "{from}");
        id_of(changed.as_bytes())
    };
    // `b`'s unit is in the id.
    assert_ne!(
        moved(r#""S231:unit": "K""#, r#""S231:unit": "degC""#),
        original
    );
    // Rewiring `b` to `gai`'s output: `a` is then unused and leaves, `b` is driven by a block.
    let mut rewired: serde_json::Value = serde_json::from_str(&pass_through()).unwrap();
    for node in rewired["@graph"].as_array_mut().unwrap() {
        match node["@id"].as_str().unwrap() {
            "http://example.org#ex.pass.a" => {
                node.as_object_mut().unwrap().remove("S231:isConnectedTo");
            }
            "http://example.org#ex.pass.gai.y" => node["S231:isConnectedTo"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({ "@id": "http://example.org#ex.pass.b" })),
            _ => {}
        }
    }
    let rewired_bytes = to_bytes(&rewired);
    let rewired_engine = load(&rewired_bytes);
    assert!(rewired_engine.topology().pass_through.is_empty());
    assert_ne!(rewired_engine.content_id().unwrap(), original);
}

#[test]
fn a_boundary_type_change_moves_the_id() {
    let integer = pass_through()
        .replace(
            r#""@id": "http://example.org#ex.pass.a", "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" },"#,
            r#""@id": "http://example.org#ex.pass.a", "@type": "S231:IntegerInput",
      "S231:isOfDataType": { "@id": "S231:Integer" },"#,
        )
        .replace(
            r#""@id": "http://example.org#ex.pass.b", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" }, "S231:unit": "K" }"#,
            r#""@id": "http://example.org#ex.pass.b", "@type": "S231:IntegerOutput",
      "S231:isOfDataType": { "@id": "S231:Integer" } }"#,
        );
    let unit_dropped = pass_through().replace(r#", "S231:unit": "K" }"#, " }");
    assert_ne!(integer, unit_dropped);
    assert_ne!(id_of(integer.as_bytes()), id_of(unit_dropped.as_bytes()));
}

/// The offset-gain worked example with `off` moved to `namespace`, which may collide with `gai`.
fn two_namespaces(off_iri: &str, gai_iri: &str) -> Vec<u8> {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/content_id/offset_gain.jsonld",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .replace("http://example.org#ex.offset_gain.off", off_iri)
    .replace("http://example.org#ex.offset_gain.gai", gai_iri)
    .into_bytes()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn absolute(iri: &str) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(iri.len() as u32).to_le_bytes());
    bytes.extend_from_slice(iri.as_bytes());
    bytes
}

#[test]
fn colliding_names_and_empty_remainders_keep_the_whole_iri() {
    // `urn:other:ex.blk` and `http://example.org#ex.blk` both reduce to `ex.blk`.
    let collide = load(&two_namespaces(
        "urn:other:ex.blk",
        "http://example.org#ex.blk",
    ));
    let bytes = collide.canonical_content_bytes().unwrap();
    assert!(contains(&bytes, &absolute("urn:other:ex.blk")));
    assert!(contains(&bytes, &absolute("http://example.org#ex.blk")));
    // Without the collision both are relative again, and the id differs.
    let apart = load(&two_namespaces(
        "urn:other:ex.off",
        "http://example.org#ex.blk",
    ));
    let apart_bytes = apart.canonical_content_bytes().unwrap();
    assert!(!contains(
        &apart_bytes,
        &absolute("http://example.org#ex.blk")
    ));
    assert_ne!(collide.content_id().unwrap(), apart.content_id().unwrap());
    // An IRI whose remainder is empty (it ends in a delimiter) is kept whole.
    let empty = load(&two_namespaces(
        "urn:other:ex:",
        "http://example.org#ex.gai",
    ));
    assert!(contains(
        &empty.canonical_content_bytes().unwrap(),
        &absolute("urn:other:ex:")
    ));
}

#[test]
fn content_id_refuses_before_a_load_while_edits_are_pending_and_after_a_failed_load() {
    let mut engine = Engine::new();
    assert!(matches!(
        engine.content_id(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    assert!(matches!(
        engine.canonical_content_bytes(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    assert!(engine.load_cxf(b"{ not json").is_err());
    assert!(matches!(
        engine.content_id(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    engine.load_cxf(pass_through().as_bytes()).unwrap();
    let loaded = engine.content_id().unwrap();
    // A failed reload keeps the loaded model, and so its id.
    assert!(engine.load_cxf(b"{ not json").is_err());
    assert_eq!(engine.content_id().unwrap(), loaded);
    engine.halt().unwrap();
    engine
        .set_param("http://example.org#ex.pass.gai.k", Value::Real(3.0))
        .unwrap();
    assert!(matches!(
        engine.content_id(),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    assert!(matches!(
        engine.compile_blob(),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    engine.resume().unwrap();
    assert_ne!(engine.content_id().unwrap(), loaded);
}

#[test]
fn the_largest_corpus_encoding_is_pinned() {
    let (size, name) = corpus()
        .into_iter()
        .map(|name| {
            (
                load(&fixture_bytes(&name))
                    .canonical_content_bytes()
                    .unwrap()
                    .len(),
                name,
            )
        })
        .max()
        .unwrap();
    assert_eq!(
        (size, name.as_str()),
        (64_491, "multizone_vav_relief_fan_group")
    );
}
