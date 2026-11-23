//! What the event plan decides for every event consumer of every loadable document in the crates
//! (issue #800, the second review of GitHub #805), pinned in
//! `fixtures/settled_rule_census.txt` so the design note's per-fixture statements cannot drift.
//!
//! The unit is the consumer: one block that declares settled event inputs, in one document that
//! loads and compiles (every `.jsonld` and `.cxf.json` under `crates/`). Each takes one path:
//! `settled` (it receives settled values), `cone-fallback` (an event input's cone holds a fallback
//! leaf, so every event input keeps the rising edge), `data-fallback` (an event input's cone is
//! active, holding a pulse or a sample, but a data input is tainted, so the block keeps the rising
//! edge: the data fallback changed its path) or `rising` (no event input's cone holds a pulse or a
//! sample: the rising edge, bit for bit, whatever else; `rising, data tainted` marks one whose
//! data input is tainted all the same, which the data fallback does not affect). A consumer this round's per-consumer fallback
//! moved off the rule is marked. Re-bless with `OCE_BLESS=1`.

use std::path::{Path, PathBuf};

use oce_api::Engine;
use oce_graph::EventPlan;
use oce_graph::blob::decode;

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

/// One line per consumer of `bytes`, or nothing when it does not load or compile.
fn census(bytes: &[u8]) -> Vec<String> {
    let mut engine = Engine::new();
    if engine.load_cxf(bytes).is_err() {
        return Vec::new();
    }
    let Ok(blob) = engine.compile_blob() else {
        return Vec::new();
    };
    let decoded = decode(&blob.bytes).expect("a compiled blob decodes");
    EventPlan::census(&decoded.model, &decoded.blocks, &decoded.schedule)
        .into_iter()
        .map(|c| {
            let block = &decoded.model.blocks[c.block];
            let path = if c.settled {
                "settled"
            } else if c.cone_fallback {
                "cone-fallback"
            } else if c.active && c.tainted_data {
                "data-fallback"
            } else if c.tainted_data {
                "rising, data tainted"
            } else {
                "rising"
            };
            // A consumer with an active input that a fallback co-input now keeps on the rising
            // edge took the rule on its active input before the second review of GitHub #805.
            let moved = if c.active && c.cone_fallback && !c.tainted_data {
                " (an active input; settled before the per-consumer fallback)"
            } else {
                ""
            };
            // The blob keeps no instance names: the class and the declaration index name it.
            format!(
                "{path} {} #{}{moved}",
                block.class_iri.rsplit('#').next().unwrap_or("?"),
                block.decl_order
            )
        })
        .collect()
}

#[test]
fn every_event_consumer_in_the_crates_takes_the_pinned_path() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    documents(&root, &mut found);
    let mut text = String::new();
    let mut totals = std::collections::BTreeMap::<&str, usize>::new();
    for path in &found {
        let lines = census(&std::fs::read(path).unwrap());
        if lines.is_empty() {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for line in lines {
            let kind = line.split([' ', ',']).next().unwrap_or("?");
            *totals
                .entry(match kind {
                    "settled" => "settled",
                    "cone-fallback" => "cone-fallback",
                    "data-fallback" => "data-fallback",
                    _ => "rising",
                })
                .or_default() += 1;
            text += &format!("{rel}: {line}\n");
        }
    }
    let summary: Vec<String> = totals.iter().map(|(k, v)| format!("{k} {v}")).collect();
    text = format!("# totals: {}\n{text}", summary.join(", "));
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/settled_rule_census.txt");
    if oce_bless::enabled("OCE_BLESS") {
        std::fs::write(&fixture, &text).unwrap();
        return;
    }
    let pinned = std::fs::read_to_string(&fixture).expect("the census fixture; OCE_BLESS=1");
    assert_eq!(
        text, pinned,
        "the event-plan census moved; re-bless with OCE_BLESS=1"
    );
}
