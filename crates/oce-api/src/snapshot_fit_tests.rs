//! The snapshot a load counts is the snapshot the engine writes, and the load refuses a model whose
//! snapshot would not fit: exactly at the cap loads, one byte past it is refused.

use std::path::{Path, PathBuf};

use oce_diag::DiagCode;

use super::counted_snapshot_bytes;
use crate::state::MAX_SNAPSHOT_BYTES;
use crate::{Engine, OcError};

/// Every `.jsonld` fixture under the facade's, the reader's and the conformance crate's
/// `tests/fixtures`, sorted.
fn fixtures() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "jsonld") {
                out.push(path);
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for krate in ["oce-api", "oce-cxf", "oce-conformance"] {
        walk(&crates.join(krate).join("tests").join("fixtures"), &mut out);
    }
    out.sort();
    out
}

/// The snapshot `engine`'s load counted.
fn counted(engine: &Engine) -> usize {
    counted_snapshot_bytes(&engine.manifest_source(), engine.model_id.as_str())
        .expect("a durable snapshot can be taken")
}

const T: &str = "http://example.org#Buildings.Controls.OBC.CDL.";

fn port(id: &str, kind: &str, wired: &[String]) -> String {
    let targets = if wired.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = wired
            .iter()
            .map(|target| format!("{{ \"@id\": \"{target}\" }}"))
            .collect();
        format!(", \"S231:isConnectedTo\": [ {} ]", items.join(", "))
    };
    format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"S231:Real{kind}\", \"S231:isOfDataType\": \
         {{ \"@id\": \"S231:Real\" }}{targets} }}"
    )
}

/// A root `root` whose input `p:u` (a path `path_bytes` longer than written) drives `targets`
/// `Abs` blocks, and, when `constant`, a `Constant` that drives one more.
fn boundary_fan_out(root: &str, path_bytes: usize, targets: usize, constant: bool) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut blocks = Vec::new();
    let fed: Vec<String> = (0..targets).map(|i| format!("urn:a{i}.u")).collect();
    nodes.push(port("p:u", "Input", &fed));
    for i in 0..=targets {
        if i == targets && !constant {
            break;
        }
        let id = format!("urn:a{i}");
        nodes.push(format!(
            "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.Abs\", \"S231:hasInput\": \
             {{ \"@id\": \"{id}.u\" }}, \"S231:hasOutput\": {{ \"@id\": \"{id}.y\" }} }}"
        ));
        nodes.push(port(&format!("{id}.u"), "Input", &[]));
        nodes.push(port(&format!("{id}.y"), "Output", &[]));
        blocks.push(id);
    }
    if constant {
        nodes.push(format!(
            "{{ \"@id\": \"urn:c\", \"@type\": \"{T}Reals.Sources.Constant\", \
             \"S231:hasParameter\": {{ \"@id\": \"urn:c.k\" }}, \"S231:hasOutput\": \
             {{ \"@id\": \"urn:c.y\" }} }}"
        ));
        nodes.push("{ \"@id\": \"urn:c.k\", \"S231:value\": 1.0 }".to_owned());
        nodes.push(port("urn:c.y", "Output", &[format!("urn:a{targets}.u")]));
        blocks.push("urn:c".to_owned());
    }
    let list = |ids: &[String]| {
        ids.iter()
            .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\", \"p\": \"urn:x:{}\" }}, \
         \"@graph\": [ {{ \"@id\": \"{root}\", \"@type\": \"S231:Block\", \
         \"S231:containsBlock\": [ {} ], \"S231:hasInput\": [ {{ \"@id\": \"p:u\" }} ] }},\n{} ] }}",
        "k".repeat(path_bytes),
        list(&blocks),
        nodes.join(",\n")
    )
    .into_bytes()
}

#[test]
fn the_counted_snapshot_is_the_snapshot_the_engine_writes() {
    let mut loaded = 0;
    for path in fixtures() {
        let document = std::fs::read(&path).expect("readable");
        let mut engine = Engine::new();
        if engine.load_cxf(&document).is_err() {
            continue;
        }
        let Ok(snapshot) = engine.state_snapshot() else {
            continue;
        };
        assert_eq!(
            counted(&engine),
            snapshot.as_bytes().len(),
            "{}",
            path.display()
        );
        loaded += 1;
    }
    assert!(loaded >= 100, "only {loaded} fixtures snapshot");
    for (path_bytes, targets) in [(1, 1), (8, 3), (300, 17), (5_000, 40)] {
        let mut engine = Engine::new();
        engine
            .load_cxf(&boundary_fan_out("urn:top", path_bytes, targets, true))
            .expect("loads");
        let snapshot = engine.state_snapshot().expect("a snapshot");
        assert_eq!(counted(&engine), snapshot.as_bytes().len());
    }
}

#[test]
fn a_frame_adds_the_previous_time_the_check_already_counts() {
    // A model with no boundary input runs a frame with no inputs; its snapshot then carries the
    // previous frame's time, 8 bytes the load counted as present.
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!(
            "../../oce-cxf/tests/fixtures/minimal_loop.jsonld"
        ))
        .expect("loads");
    let before = counted(&engine);
    assert_eq!(
        engine
            .state_snapshot()
            .expect("a snapshot")
            .as_bytes()
            .len(),
        before
    );
    let inputs = engine.input_definitions().expect("definitions");
    let values: Vec<(&str, oce_model::Value)> = inputs
        .iter()
        .map(|input| (input.path.as_str(), oce_model::Value::Real(0.0)))
        .collect();
    let frame = engine.prepare_frame(1.0, &values).expect("a frame");
    engine.execute_frame(frame).expect("runs");
    let frame = engine.prepare_frame(2.0, &values).expect("a frame");
    engine.execute_frame(frame).expect("runs");
    assert_eq!(
        engine
            .state_snapshot()
            .expect("a snapshot")
            .as_bytes()
            .len(),
        before + 8
    );
}

#[test]
fn a_snapshot_at_the_cap_loads_and_one_byte_more_is_refused() {
    // A 1 MiB boundary path fanned out to 62 inputs: the snapshot spells it once per input, about
    // 62 MiB, and the root's `@id` (written once in the snapshot) makes up the rest a byte at a
    // time. The snapshot is counted, never written: written, it would peak at four times the cap.
    let cap = usize::try_from(MAX_SNAPSHOT_BYTES).expect("fits");
    let root = |pad: usize| format!("urn:top:{}", "z".repeat(pad));
    let document = |pad: usize| boundary_fan_out(&root(pad), 1 << 20, 62, false);
    let mut engine = Engine::new();
    engine.load_cxf(&document(1)).expect("under the cap");
    let short = counted(&engine);
    let pad = 1 + (cap - 8) - short;
    let mut engine = Engine::new();
    engine.load_cxf(&document(pad)).expect("exactly at the cap");
    assert_eq!(counted(&engine) + 8, cap);

    let error = Engine::new()
        .load_cxf(&document(pad + 1))
        .map(drop)
        .expect_err("one byte past the cap");
    let refusals: Vec<_> = error
        .diagnostics()
        .iter()
        .filter(|d| d.is_error())
        .collect();
    assert_eq!(refusals.len(), 1, "{error:?}");
    assert_eq!(refusals[0].code, DiagCode::ResourceLimitExceeded);
    assert!(
        refusals[0]
            .message
            .contains(&format!("would take {} bytes", cap + 1)),
        "{}",
        refusals[0].message
    );
    assert!(matches!(
        error,
        OcError::Validate(_) | OcError::LoadContext(_)
    ));
}
