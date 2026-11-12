//! The authoring layer's integration tests, compiled as one test binary.
//!
//! Cargo builds each file under `tests/` as its own binary by default, and under the release
//! profile (`lto = "thin"`, `codegen-units = 1`) every binary links the whole dependency graph
//! again (GitHub #504). Here each file is a module instead. It keeps its file name, so it still
//! mirrors the source module it exercises, and nextest reports its tests as
//! `oce-author::authoring <file>::<test>`.
//!
//! `Cargo.toml` sets `autotests = false` and lists this binary and one other `[[test]]` target,
//! `import_cost`, which measures allocations and so links `allocation-counter`'s counting global
//! allocator; it stays apart so no test here runs under that allocator. The crate's bless command
//! names the package, not a binary, and nothing selects a test binary of this crate by name; a
//! file that needs a binary of its own gets a `[[test]]` entry with the reason. With discovery off
//! a new file under `tests/` would compile into nothing: the layout check at the end of this file
//! refuses that, and a file compiled more than once.
//!
//! The helpers every module shares (`support`) are declared once here and imported with
//! `use crate::support;`, because a second declaration of the same file trips
//! `clippy::duplicate_mod`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

mod support;

mod array_extent;
mod boundary_bounds;
mod builder_errors;
mod emit_bounds;
mod emit_goldens;
mod emit_permutations;
mod emit_values;
mod engine_findings;
mod enum_member_oracle;
mod finding_codes;
mod finding_goldens;
mod finding_order;
mod identifier_rules;
mod import_closed_shapes;
mod import_codes;
mod import_graph_v0;
mod import_graph_v0_refusals;
mod import_hostile;
mod import_namespace_and_label;
mod import_parameter_attributes;
mod import_precedence;
mod import_reference_keys;
mod import_refusals;
mod import_sweep;
mod import_term_controls;
mod import_term_shapes;
mod import_values;
mod library_round_trip;
mod named_types;
mod numeric_boundaries;
mod schema_conformance;
mod schema_pinned;
mod semantic_bounds;
mod semantic_seam;
mod text_bounds;
mod text_cost;
mod text_fmt;
mod text_hostile;
mod text_print;
mod text_refusals;
mod text_round_trip;
mod text_spans;
mod unit_suffixes;
mod validate_cost;
mod wire_closed_shapes;
mod wire_refusals;
mod wire_round_trip;
mod wire_totality;

/// One lexical token of a Rust source file, with comments dropped and a string literal kept as
/// its text, so that a `mod` inside a comment or a string is never read as a declaration.
#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Text(String),
    Symbol(char),
}

/// The text of the quoted literal whose opening quote precedes `start`, and the index after its
/// closing quote.
fn quoted(chars: &[char], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '"' => return (text, i + 1),
            '\\' => {
                text.extend(chars.get(i + 1));
                i += 2;
            }
            c => {
                text.push(c);
                i += 1;
            }
        }
    }
    (text, i)
}

/// The text of the raw string literal whose `r` (or `br`) ended just before `start`, and the index
/// after it; `None` when `start` begins no raw string.
fn raw_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
    let hashes = chars[start..].iter().take_while(|c| **c == '#').count();
    let body = start + hashes;
    if chars.get(body) != Some(&'"') {
        return None;
    }
    let close: Vec<char> = std::iter::once('"')
        .chain(std::iter::repeat_n('#', hashes))
        .collect();
    let mut end = body + 1;
    while end + close.len() <= chars.len() && chars[end..end + close.len()] != close[..] {
        end += 1;
    }
    let end = end.min(chars.len());
    Some((
        chars[body + 1..end].iter().collect(),
        (end + close.len()).min(chars.len()),
    ))
}

/// Lex `source`: line comments and nested block comments vanish, string, raw string and
/// character literals cannot hide or fake a declaration, and everything else is a word or a symbol.
fn tokens(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            let (text, end) = quoted(&chars, i + 1);
            out.push(Token::Text(text));
            i = end;
        } else if c == '\'' {
            // A character literal, or the tick of a lifetime.
            if next == Some('\\') {
                i += 3;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if word == "r" || word == "br" {
                if let Some((text, end)) = raw_quoted(&chars, i) {
                    out.push(Token::Text(text));
                    i = end;
                    continue;
                }
                if word == "r" && chars.get(i) == Some(&'#') {
                    // A raw identifier such as `r#mod` is not the keyword.
                    let name_start = i + 1;
                    i = name_start;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let name: String = chars[name_start..i].iter().collect();
                    out.push(Token::Word(format!("r#{name}")));
                    continue;
                }
            }
            out.push(Token::Word(word));
        } else {
            out.push(Token::Symbol(c));
            i += 1;
        }
    }
    out
}

/// A `mod name;` item at the top level of a file, with the `#[path = "..."]` attribute on it.
#[derive(Debug, PartialEq)]
struct Declaration {
    module: String,
    path: Option<String>,
}

/// The out-of-line modules a file declares at its top level: any visibility (`pub`, `pub(crate)`),
/// any attributes, a comment after the semicolon, several to a line. Inline `mod name { .. }`
/// blocks and everything inside braces are skipped.
fn declarations(source: &str) -> Vec<Declaration> {
    let tokens = tokens(source);
    let mut found = Vec::new();
    let mut path = None;
    let mut depth = 0_usize;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Symbol('{') => {
                depth += 1;
                path = None;
                i += 1;
            }
            Token::Symbol('}') => {
                depth = depth.saturating_sub(1);
                path = None;
                i += 1;
            }
            _ if depth > 0 => i += 1,
            Token::Symbol('#') => {
                i += 1;
                if tokens.get(i) == Some(&Token::Symbol('!')) {
                    i += 1;
                }
                if tokens.get(i) == Some(&Token::Symbol('[')) {
                    let start = i + 1;
                    let mut nesting = 0_usize;
                    while i < tokens.len() {
                        match tokens[i] {
                            Token::Symbol('[') => nesting += 1,
                            Token::Symbol(']') => {
                                nesting -= 1;
                                if nesting == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    if let Some([Token::Word(key), Token::Symbol('='), Token::Text(value)]) =
                        tokens.get(start..i)
                        && key == "path"
                    {
                        path = Some(value.clone());
                    }
                    i += 1;
                }
            }
            _ => {
                let mut item = i;
                if tokens[item] == Token::Word("pub".into()) {
                    item += 1;
                    if tokens.get(item) == Some(&Token::Symbol('(')) {
                        while item < tokens.len() && tokens[item] != Token::Symbol(')') {
                            item += 1;
                        }
                        item += 1;
                    }
                }
                if let Some(
                    [
                        Token::Word(keyword),
                        Token::Word(module),
                        Token::Symbol(';'),
                    ],
                ) = tokens.get(item..item + 3)
                    && keyword == "mod"
                {
                    found.push(Declaration {
                        module: module.clone(),
                        path: path.take(),
                    });
                    i = item + 3;
                } else {
                    path = None;
                    i += 1;
                }
            }
        }
    }
    found
}

/// `relative` resolved against `dir`, both slash-separated and relative to the crate root.
fn join(dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The files the compiler reads when it builds the test target rooted at `root`, one entry per
/// declaration that names them. Only files in `sources` are read for further declarations.
fn compiled_files(root: &str, sources: &BTreeMap<String, String>) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_owned(), true)];
    let mut expanded = BTreeSet::new();
    while let Some((file, is_root)) = pending.pop() {
        let Some(source) = sources.get(&file) else {
            continue;
        };
        if !expanded.insert(file.clone()) {
            continue;
        }
        let (dir, name) = file.rsplit_once('/').unwrap_or(("", &file));
        // A root or a `mod.rs` looks for its modules beside it, any other file in a directory
        // named for it.
        let module_dir = if is_root || name == "mod.rs" {
            dir.to_owned()
        } else {
            join(dir, name.strip_suffix(".rs").unwrap_or(name))
        };
        for declaration in declarations(source) {
            let target = match &declaration.path {
                Some(path) => join(dir, path),
                None => join(
                    &module_dir,
                    &format!("{}.rs", declaration.module.trim_start_matches("r#")),
                ),
            };
            found.push(target.clone());
            pending.push((target, false));
        }
    }
    found
}

/// The `[[test]]` targets of a manifest as `(name, path)`, and whether `autotests = false` is set
/// in `[package]`. A line scan of the layout this crate uses, not a TOML parser.
fn manifest_targets(manifest: &str) -> (bool, Vec<(String, String)>) {
    let mut autotests_off = false;
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    let mut section = String::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.split('#').next().unwrap_or("").trim().to_owned();
            if section == "[[test]]" {
                targets.push((String::new(), None));
            }
        } else if let Some((key, value)) = line.split_once('=') {
            let (key, value) = (key.trim(), value.trim());
            let text = value
                .strip_prefix('"')
                .and_then(|rest| rest.split('"').next())
                .unwrap_or("")
                .to_owned();
            match (section.as_str(), key) {
                ("[package]", "autotests") => autotests_off = value.starts_with("false"),
                ("[[test]]", "name") => targets.last_mut().unwrap().0 = text,
                ("[[test]]", "path") => targets.last_mut().unwrap().1 = Some(text),
                _ => {}
            }
        }
    }
    let targets = targets
        .into_iter()
        .map(|(name, path)| {
            let path = path.unwrap_or_else(|| format!("tests/{name}.rs"));
            (name, path)
        })
        .collect();
    (autotests_off, targets)
}

/// Every file Cargo would have discovered as a test target, keyed by its path from the crate root
/// (`tests/name.rs`, `tests/name/main.rs`), with its text.
fn discoverable_sources(crate_dir: &Path) -> BTreeMap<String, String> {
    let mut sources = BTreeMap::new();
    for entry in fs::read_dir(crate_dir.join("tests")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let (key, file) = if path.is_dir() {
            (format!("tests/{name}/main.rs"), path.join("main.rs"))
        } else if name.ends_with(".rs") {
            (format!("tests/{name}"), path.clone())
        } else {
            continue;
        };
        if file.is_file() {
            sources.insert(key, fs::read_to_string(file).unwrap());
        }
    }
    sources
}

/// What is wrong with the layout: `manifest` is the package's `Cargo.toml` and `sources` the
/// discoverable files under `tests/`. Discovery must be off, and every file must be exactly one
/// `[[test]]` target or be named by exactly one `mod` declaration reachable from a target, with or
/// without `#[path]`.
fn layout_defects(manifest: &str, sources: &BTreeMap<String, String>) -> Vec<String> {
    let mut defects = Vec::new();
    let (autotests_off, targets) = manifest_targets(manifest);
    if !autotests_off {
        defects.push("Cargo.toml: autotests is not false, so Cargo builds every file alone".into());
    }
    if targets.is_empty() {
        defects.push("Cargo.toml: no [[test]] target".into());
    }
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, path) in &targets {
        if !sources.contains_key(path) {
            defects.push(format!(
                "[[test]] {name}: {path} is not a file under tests/"
            ));
        }
        owners
            .entry(path.clone())
            .or_default()
            .push("a [[test]] target".into());
        for file in compiled_files(path, sources) {
            owners
                .entry(file)
                .or_default()
                .push(format!("a module of {name}"));
        }
    }
    for path in sources.keys() {
        let mut owner = owners.remove(path).unwrap_or_default();
        owner.sort();
        match owner.len() {
            0 => defects.push(format!("{path} is never compiled")),
            1 => {}
            times => defects.push(format!(
                "{path} is compiled {times} times: {}",
                owner.join(", ")
            )),
        }
    }
    defects
}

#[test]
fn every_test_file_is_compiled_into_exactly_one_binary() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let sources = discoverable_sources(crate_dir);
    assert!(
        ["tests/authoring.rs", "tests/text_round_trip.rs"]
            .iter()
            .all(|name| sources.contains_key(*name)),
        "the scan found the root and a module, so it cannot pass vacuously: {:?}",
        sources.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "declare a new file as `mod <file>;` in tests/authoring.rs (with `use crate::support;` in \
         the file for the shared helpers), or give it a [[test]] target of its own in Cargo.toml, \
         with the reason, when something selects it by name"
    );
}

/// A manifest with `roots` as `[[test]]` targets, each at `tests/<root>.rs`, discovery off.
fn manifest_of(roots: &[&str]) -> String {
    let mut manifest = String::from("[package]\nautotests = false\n");
    for root in roots {
        manifest.push_str(&format!(
            "\n[[test]]\nname = \"{root}\"\npath = \"tests/{root}.rs\"\n"
        ));
    }
    manifest
}

/// Sources for `files` (each `tests/<name>.rs`), overridden by `texts` as `(name, text)`.
fn sources_of(files: &[&str], texts: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut sources: BTreeMap<String, String> = files
        .iter()
        .map(|name| (format!("tests/{name}.rs"), String::new()))
        .collect();
    for (name, text) in texts {
        sources.insert(format!("tests/{name}.rs"), (*text).to_owned());
    }
    sources
}

#[test]
fn the_layout_check_accepts_a_complete_layout() {
    let sources = sources_of(
        &["root", "member", "alone"],
        &[(
            "root",
            "#[path = \"support/helper.rs\"]\nmod helper;\nmod member;\n",
        )],
    );
    let manifest = format!("{}\n[[test]]\nname = \"alone\"\n", manifest_of(&["root"]));
    assert_eq!(
        layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "a target with no path is tests/<name>.rs"
    );
}

#[test]
fn the_layout_check_refuses_a_file_compiled_into_no_binary() {
    let mut sources = sources_of(&["root", "member", "orphan", "helper"], &[]);
    sources.insert("tests/nested/main.rs".into(), String::new());
    sources.insert(
        "tests/root.rs".into(),
        // A helper at another path does not compile the top-level file of the same name.
        "#[path = \"support/helper.rs\"]\nmod helper;\nmod member;\n".into(),
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        [
            "tests/helper.rs is never compiled",
            "tests/nested/main.rs is never compiled",
            "tests/orphan.rs is never compiled",
        ]
    );
}

#[test]
fn the_layout_check_refuses_a_file_compiled_twice() {
    // A target that another target also declares as a module.
    let sources = sources_of(&["root", "alone"], &[("root", "mod alone;\n")]);
    assert_eq!(
        layout_defects(&manifest_of(&["root", "alone"]), &sources),
        ["tests/alone.rs is compiled 2 times: a [[test]] target, a module of root"]
    );

    // A file compiled through `#[path]` in one root and as a plain module in another.
    let sources = sources_of(
        &["first", "second", "shared"],
        &[
            ("first", "#[path = \"shared.rs\"]\nmod renamed;\n"),
            ("second", "mod shared;\n"),
        ],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["first", "second"]), &sources),
        ["tests/shared.rs is compiled 2 times: a module of first, a module of second"]
    );

    // The same file twice in one root, and by a path that climbs out of `tests/` and back.
    let sources = sources_of(
        &["root", "twice"],
        &[(
            "root",
            "mod twice;\n#[path = \"../tests/twice.rs\"]\nmod again;\n",
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        ["tests/twice.rs is compiled 2 times: a module of root, a module of root"]
    );

    // A target named twice, and a module reached through another module's `#[path]`.
    let sources = sources_of(
        &["root", "inner", "leaf"],
        &[
            ("root", "mod inner;\n"),
            ("inner", "#[path = \"leaf.rs\"]\nmod leaf;\n"),
            ("leaf", ""),
        ],
    );
    let manifest = format!(
        "{}\n[[test]]\nname = \"again\"\npath = \"tests/leaf.rs\"\n",
        manifest_of(&["root"])
    );
    assert_eq!(
        layout_defects(&manifest, &sources),
        ["tests/leaf.rs is compiled 2 times: a [[test]] target, a module of root"]
    );
}

#[test]
fn the_layout_check_reads_declarations_as_the_compiler_does() {
    // Each of these is a declaration the compiler honours, so each file counts as compiled.
    let sources = sources_of(
        &[
            "root",
            "public",
            "restricted",
            "trailing",
            "same_line_a",
            "same_line_b",
            "gated",
            "raw_name",
        ],
        &[(
            "root",
            r#"pub mod public;
pub(crate) mod restricted;
mod trailing; // a note after the semicolon
mod same_line_a; mod same_line_b;
#[cfg(windows)]
mod gated;
mod r#raw_name;
"#,
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        Vec::<String>::new()
    );

    // Each of these is not a declaration, so each file is still in no binary.
    let sources = sources_of(
        &["root", "line", "block", "nested", "string", "raw", "inline"],
        &[(
            "root",
            r##"// mod line;
/* mod block; */
/* outer /* inner */ mod nested; */
const TEXT: &str = "mod string;";
const RAW: &str = r#"mod raw; "quoted""#;
mod inline { mod hidden; }
const QUOTE: char = '"'; const TICK: char = '\'';
fn lifetime<'a>(text: &'a str) -> &'a str { text }
"##,
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        [
            "tests/block.rs is never compiled",
            "tests/inline.rs is never compiled",
            "tests/line.rs is never compiled",
            "tests/nested.rs is never compiled",
            "tests/raw.rs is never compiled",
            "tests/string.rs is never compiled",
        ]
    );
}

#[test]
fn the_layout_check_refuses_discovery_left_on_a_missing_target_and_no_target() {
    let sources = sources_of(&["root"], &[]);
    let discovered = manifest_of(&["root"]).replace("autotests = false\n", "");
    assert_eq!(
        layout_defects(&discovered, &sources),
        ["Cargo.toml: autotests is not false, so Cargo builds every file alone"]
    );
    let other_table = "[package]\nname = \"x\"\n[lints]\nautotests = false\n";
    assert_eq!(layout_defects(other_table, &sources).len(), 3);
    let missing = format!("{}\n[[test]]\nname = \"gone\"\n", manifest_of(&["root"]));
    assert_eq!(
        layout_defects(&missing, &sources),
        ["[[test]] gone: tests/gone.rs is not a file under tests/"]
    );
}
