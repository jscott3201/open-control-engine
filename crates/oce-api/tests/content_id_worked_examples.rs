//! The design note's worked examples (`docs/design/content-id.md`), byte for byte: the canonical
//! bytes of each checked-in document equal a hand-assembled vector built here from the note's
//! encoding table, and the ids equal the ones the note prints. The encoder is not the oracle.

use oce_api::{ContentId, Engine};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/content_id/{name}.jsonld",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {path}: {err}"))
}

fn load(name: &str) -> Engine {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(&fixture(name))
        .expect("worked example loads");
    assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
    engine
}

/// Little helpers for writing the note's dump by hand.
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
    /// A form-0 (relative) name.
    fn name(&mut self, text: &str) -> &mut Self {
        self.u8(0).str(text)
    }
    fn present(&mut self, text: &str) -> &mut Self {
        self.u8(1).str(text)
    }
    fn header(&mut self) -> &mut Self {
        self.raw(b"OCECANON").raw(&[1, 0]).raw(&[0, 0])
    }
}

fn class_hash(class_path: &str) -> [u8; 8] {
    oce_blocks::class_signature_hash(class_path)
        .unwrap()
        .to_le_bytes()
}

#[test]
fn offset_gain_has_the_notes_394_bytes_and_id() {
    let engine = load("offset_gain");
    let mut expected = Bytes::default();
    expected.header();
    // inputs
    expected
        .u32(1)
        .name("ex.offset_gain.TOut")
        .u8(0)
        .present("ThermodynamicTemperature")
        .present("K")
        .present("degC")
        .u8(0)
        .u8(0)
        .u32(1)
        .name("ex.offset_gain.off")
        .u32(0);
    // outputs
    expected
        .u32(1)
        .name("ex.offset_gain.y")
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .name("ex.offset_gain.gai")
        .u32(0);
    // blocks: `gai` sorts before `off`, the reverse of authored order
    expected
        .u32(2)
        .name("ex.offset_gain.gai")
        .str("CDL.Reals.MultiplyByParameter")
        .raw(&[0x64, 0xa2, 0x3a, 0x67, 0x75, 0x83, 0xa6, 0x52])
        .u32(1)
        .str("k")
        .u8(0)
        .raw(&[0xcd, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xfc, 0x3f])
        .name("ex.offset_gain.off")
        .str("CDL.Reals.AddParameter")
        .raw(&[0x0d, 0xc3, 0x5f, 0xe4, 0xa5, 0xcf, 0x1f, 0xe7])
        .u32(1)
        .str("p")
        .u8(0)
        .raw(&[0x66, 0x66, 0x66, 0x66, 0x66, 0x12, 0x71, 0xc0]);
    // connections
    expected
        .u32(1)
        .name("ex.offset_gain.gai")
        .u32(0)
        .name("ex.offset_gain.off")
        .u32(0);
    assert_eq!(expected.0.len(), 394);
    // The two class hashes are today's registry values, as the note's dump records them.
    assert_eq!(
        class_hash("CDL.Reals.MultiplyByParameter"),
        expected.0[0xeb..0xf3]
    );
    assert_eq!(
        class_hash("CDL.Reals.AddParameter"),
        expected.0[0x136..0x13e]
    );
    assert_eq!(engine.canonical_content_bytes().unwrap(), expected.0);
    assert_eq!(
        engine.content_id().unwrap().as_str(),
        "oce:canon1:fbbea1ef32cb24797011163d09ead7ee5c17757a4aad551b6989c745740a07a5"
    );
}

/// The bytes shared by both `TrueDelay` documents up to the block's parameter count (`0000` to
/// `00b9` in the note).
fn true_delay_prefix() -> Bytes {
    let mut bytes = Bytes::default();
    bytes.header();
    bytes
        .u32(1)
        .name("ex.true_delay.u")
        .u8(2)
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .u32(1)
        .name("ex.true_delay.del")
        .u32(0);
    bytes
        .u32(1)
        .name("ex.true_delay.y")
        .u8(2)
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .name("ex.true_delay.del")
        .u32(0);
    bytes
        .u32(1)
        .name("ex.true_delay.del")
        .str("CDL.Logical.TrueDelay")
        .raw(&[0x7d, 0x02, 0xd6, 0xfe, 0x4f, 0x11, 0x21, 0xd3]);
    assert_eq!(bytes.0.len(), 0xba);
    assert_eq!(class_hash("CDL.Logical.TrueDelay"), bytes.0[0xb2..0xba]);
    bytes
}

#[test]
fn binding_a_parameter_to_its_default_moves_the_id_as_the_note_shows() {
    let mut a = true_delay_prefix();
    a.u32(1)
        .str("delayTime")
        .u8(0)
        .raw(&60.0_f64.to_bits().to_le_bytes())
        .u32(0);
    assert_eq!(a.0.len(), 216);
    let mut b = true_delay_prefix();
    b.u32(2)
        .str("delayOnInit")
        .u8(2)
        .u8(0)
        .str("delayTime")
        .u8(0)
        .raw(&[0, 0, 0, 0, 0, 0, 0x4e, 0x40])
        .u32(0);
    assert_eq!(b.0.len(), 233);

    let engine_a = load("true_delay");
    let engine_b = load("true_delay_default");
    assert_eq!(engine_a.canonical_content_bytes().unwrap(), a.0);
    assert_eq!(engine_b.canonical_content_bytes().unwrap(), b.0);
    let id_a = engine_a.content_id().unwrap();
    let id_b = engine_b.content_id().unwrap();
    assert_eq!(
        id_a.as_str(),
        "oce:canon1:aff7775cd0e7efb52af23c0347455f5806f789ee719df142c7c33f458ed9e91c"
    );
    assert_eq!(
        id_b.as_str(),
        "oce:canon1:05b0f40f98a38ed7e6dffca3157cae70d5f077c1893cb1587880a33ebc11752b"
    );
    assert_ne!(id_a, id_b);
    assert_eq!(ContentId::parse(id_a.as_str()), Ok(id_a));
}

#[test]
fn every_worked_example_exports_completely() {
    for name in ["offset_gain", "true_delay", "true_delay_default"] {
        load(name).export_cxf().unwrap().complete().unwrap();
    }
}
