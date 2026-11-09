//! The HostTick v1 frame driver exists once, in `oce-graph` (design note decision 11, issue #401).
//!
//! Two kinds of proof. Type identity: the facade's `MAX_FRAME_TIME_MAGNITUDE` and
//! `NonFiniteInputPolicy` are the core's own items, re-exported, not copies. Source structure: no
//! non-test `oce-api` source defines either item, evaluates the schedule, checks time
//! representability, advances a sequence with `checked_add`, or compares a frame time with the
//! magnitude limit; and the facade's frame module calls the core driver's `FrameDriver`,
//! `check_time` and `check_input`, and the slot entry (`frame_slots.rs`) is the only other caller
//! of `FrameDriver`. A second implementation of any rule would have to reintroduce
//! one of the refused spellings, which is what this lexical check catches; its control proves the
//! scan reads the sources it claims to.

use std::any::TypeId;
use std::path::{Path, PathBuf};

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every non-test Rust source under `src/`: test modules (`*_tests.rs`, `src/tests/`) may name
/// internals to inject faults and are not a second implementation.
fn facade_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" {
                    walk(&path, out);
                }
            } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&source_root(), &mut out);
    out
}

/// Text outside `//` comments, so documentation may name the rules without tripping the scan.
fn code(text: &str) -> String {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Spellings only a second frame driver would need in the facade.
const REFUSED: &[&str] = &[
    "const MAX_FRAME_TIME_MAGNITUDE",
    "enum NonFiniteInputPolicy",
    "f64::MAX / 4.0",
    "eval_tick",
    "run_tick",
    "EvalContext",
    "time_is_representable",
    "checked_add(1)",
    "> MAX_FRAME_TIME_MAGNITUDE",
    "fn accepts_type",
    "fn validate_frame_time",
    "fn transition_host_tick",
];

#[test]
fn the_facade_reexports_the_core_frame_items_instead_of_copying_them() {
    assert_eq!(
        TypeId::of::<oce_api::NonFiniteInputPolicy>(),
        TypeId::of::<oce_graph::frame::NonFiniteInputPolicy>()
    );
    // A `const` has no identity to compare; equal bits plus the re-export line and the source scan
    // below (no `const MAX_FRAME_TIME_MAGNITUDE` anywhere in the facade) are the proof.
    assert_eq!(
        oce_api::MAX_FRAME_TIME_MAGNITUDE.to_bits(),
        oce_graph::frame::MAX_FRAME_TIME_MAGNITUDE.to_bits()
    );
    let lib = std::fs::read_to_string(source_root().join("lib.rs")).unwrap();
    assert!(
        lib.contains("pub use oce_graph::frame::{MAX_FRAME_TIME_MAGNITUDE, NonFiniteInputPolicy};"),
        "lib.rs re-exports both items from the core driver"
    );
}

#[test]
fn no_facade_source_holds_a_second_frame_driver() {
    let sources = facade_sources();
    assert!(
        sources.iter().any(|(path, _)| path.ends_with("frame.rs")),
        "the scan reads the facade's frame module"
    );
    for (path, text) in &sources {
        let code = code(text);
        for refused in REFUSED {
            assert!(
                !code.contains(refused),
                "{path} contains `{refused}`: the HostTick v1 rules belong to oce_graph::frame"
            );
        }
    }
    let frame = code(&std::fs::read_to_string(source_root().join("frame.rs")).unwrap());
    for call in [
        "FrameDriver {",
        "oce_graph::frame::check_time(",
        "oce_graph::frame::check_input(",
        "oce_graph::frame::target_bound(",
        "oce_graph::frame::output_value(",
    ] {
        assert!(
            frame.contains(call),
            "the facade frame module calls `{call}`"
        );
    }
    // The slot entry (issue #444) drives the same core driver; the named and slot entries are
    // the only two callers in the facade, so a third execution path would show up here.
    let slots = code(&std::fs::read_to_string(source_root().join("frame_slots.rs")).unwrap());
    for call in ["FrameDriver {", "oce_graph::frame::output_value("] {
        assert!(slots.contains(call), "the slot entry calls `{call}`");
    }
    let callers: Vec<&str> = sources
        .iter()
        .filter(|(_, text)| code(text).contains("FrameDriver {"))
        .map(|(path, _)| path.rsplit(['/', '\\']).next().unwrap_or(path))
        .collect();
    assert_eq!(callers, ["frame.rs", "frame_slots.rs"]);
}

#[test]
fn the_scan_catches_a_reintroduced_rule() {
    // Control: the scan's matcher fires on each refused spelling in code and ignores comments.
    for refused in REFUSED {
        let planted = format!("fn f() {{ let _ = {refused}; }}");
        assert!(code(&planted).contains(refused), "{refused}");
        let commented = format!("// {refused}");
        assert!(!code(&commented).contains(refused), "{refused}");
    }
}
