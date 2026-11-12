//! The HostTick v1 frame driver exists once, in `oce-graph` (design note decision 11, issue #401).
//!
//! Two kinds of proof. Type identity: the facade's `MAX_FRAME_TIME_MAGNITUDE` and
//! `NonFiniteInputPolicy` are the core's own items, re-exported, not copies. Source structure: no
//! non-test `oce-api` source defines either item, evaluates the schedule, checks time
//! representability, advances a sequence with `checked_add`, or compares a frame time with the
//! magnitude limit; and the facade's frame module calls the core driver's `FrameDriver`,
//! `check_time` and `check_input`, and the slot entry (`frame_slots.rs`) is the only other source
//! that constructs a `FrameDriver` (matched as `FrameDriver` then `{` across any whitespace, and
//! compared by path relative to `src/`). A second implementation of any rule would have to
//! reintroduce one of the refused spellings, which is what this lexical check catches; its control
//! proves the scan reads the sources it claims to. The excluded test files (`*_tests.rs`,
//! `src/tests/`) are checked to be compiled only under `#[cfg(test)]`, so nothing they hold can
//! reach a host build.
//!
//! Residual risk, stated rather than hidden: the scan is lexical. A caller that renamed the type
//! on import (`use oce_graph::frame::FrameDriver as D;`) or reached the driver through a macro would
//! not be counted, and an inline `mod tests { .. }` block in a non-test file is scanned like any
//! other code.

use std::any::TypeId;
use std::path::{Path, PathBuf};

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// `path` relative to `src/`, with `/` separators on every platform.
fn relative(path: &Path) -> String {
    path.strip_prefix(source_root())
        .unwrap()
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `code` constructs a `FrameDriver`: the identifier (not a suffix of a longer one), then
/// `{` after any whitespace.
fn constructs_frame_driver(code: &str) -> bool {
    const NAME: &str = "FrameDriver";
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    code.match_indices(NAME).any(|(at, _)| {
        let before = code[..at].chars().next_back().is_none_or(|c| !ident(c));
        let after = code[at + NAME.len()..].trim_start();
        before && after.starts_with('{')
    })
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
                out.push((relative(&path), text));
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
    assert!(constructs_frame_driver(&frame), "frame.rs drives the core");
    for call in [
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
    assert!(
        constructs_frame_driver(&slots),
        "frame_slots.rs drives the core"
    );
    assert!(slots.contains("oce_graph::frame::output_value("));
    let callers: Vec<&str> = sources
        .iter()
        .filter(|(_, text)| constructs_frame_driver(&code(text)))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(callers, ["frame.rs", "frame_slots.rs"]);
}

#[test]
fn the_frame_driver_matcher_tolerates_layout_and_refuses_longer_names() {
    for planted in [
        "FrameDriver {",
        "FrameDriver{",
        "oce_graph::frame::FrameDriver\n        {",
        "let _ = FrameDriver\t{ model };",
    ] {
        assert!(constructs_frame_driver(planted), "{planted:?}");
    }
    for innocent in [
        "MyFrameDriver {",
        "FrameDriverish {",
        "FrameDriver::new()",
        "use oce_graph::frame::FrameDriver;",
    ] {
        assert!(!constructs_frame_driver(innocent), "{innocent:?}");
    }
}

/// Every `mod` declaration in `text` (a source at `path`, relative to `src/`): the file it loads,
/// and whether `#[cfg(test)]` is among its attributes. Inline `mod name { .. }` blocks load no file.
fn module_declarations(path: &str, text: &str) -> Vec<(String, bool)> {
    let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let stem = path.trim_end_matches(".rs");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(name) = line
            .strip_prefix("pub(crate) mod ")
            .or_else(|| line.strip_prefix("mod "))
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            continue;
        };
        let attributes: Vec<&str> = lines[..index]
            .iter()
            .rev()
            .take_while(|line| line.starts_with("#["))
            .copied()
            .collect();
        let cfg_test = attributes.contains(&"#[cfg(test)]");
        let explicit = attributes.iter().find_map(|attribute| {
            attribute
                .strip_prefix("#[path = \"")
                .and_then(|rest| rest.strip_suffix("\"]"))
        });
        let join = |base: &str, file: &str| {
            if base.is_empty() {
                file.to_owned()
            } else {
                format!("{base}/{file}")
            }
        };
        let file = match explicit {
            Some(explicit) => join(dir, explicit),
            None if path == "lib.rs" || path.ends_with("/mod.rs") || path == "mod.rs" => {
                let local = join(dir, &format!("{name}.rs"));
                if source_root().join(&local).is_file() {
                    local
                } else {
                    join(dir, &format!("{name}/mod.rs"))
                }
            }
            None => join(stem, &format!("{name}.rs")),
        };
        out.push((file, cfg_test));
    }
    out
}

#[test]
fn every_excluded_test_source_compiles_only_under_cfg_test() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&source_root(), &mut files);
    let files: Vec<String> = files.iter().map(|path| relative(path)).collect();
    let excluded = |path: &str| path.ends_with("_tests.rs") || path.starts_with("tests/");
    let mut test_only = std::collections::BTreeSet::new();
    let mut anywhere = std::collections::BTreeSet::new();
    for path in &files {
        let text = std::fs::read_to_string(source_root().join(path)).unwrap();
        for (file, cfg_test) in module_declarations(path, &text) {
            // A declaration inside an excluded file is itself compiled only under cfg(test).
            if cfg_test || excluded(path) {
                test_only.insert(file.clone());
            }
            if !cfg_test && !excluded(path) {
                anywhere.insert(file);
            }
        }
    }
    // Control: the parser sees a test file declared without cfg(test), and one declared with it.
    let planted =
        "#[path = \"x_tests.rs\"]\nmod hidden;\n#[cfg(test)]\n#[path = \"y_tests.rs\"]\nmod seen;";
    assert_eq!(
        module_declarations("frame.rs", planted),
        [
            ("x_tests.rs".to_owned(), false),
            ("y_tests.rs".to_owned(), true)
        ]
    );
    let excluded_files: Vec<&String> = files.iter().filter(|path| excluded(path)).collect();
    assert!(
        excluded_files
            .iter()
            .any(|path| path.as_str() == "frame_slots_tests.rs"),
        "the walk sees the excluded test sources"
    );
    for path in excluded_files {
        assert!(
            !anywhere.contains(path),
            "{path} is declared outside #[cfg(test)]: the frame-driver scan would miss host code"
        );
        // `tests/mod.rs` declares its children by name; every excluded file must be loaded by
        // some cfg(test) declaration, directly or through an excluded parent.
        assert!(
            test_only.contains(path),
            "{path} is not loaded under #[cfg(test)]"
        );
    }
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
