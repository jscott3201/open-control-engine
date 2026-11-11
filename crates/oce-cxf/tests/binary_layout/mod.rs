//! The layout check of `oce-cxf`'s test binaries: which file under `tests/` each `[[test]]`
//! target compiles, read from `Cargo.toml` and the declarations in the sources.
//!
//! Adapted from `oce-author`'s (`crates/leverage/oce-author/tests/authoring.rs`, GitHub #551).
//! Beyond it, this one reads the helper directories a declaration reaches (so a helper that
//! names a test file counts), resolves a plain `mod name;` in a `#[path]` file the way the
//! compiler does, refuses a declaration it cannot place instead of guessing, and reads
//! `autotests` exactly. It does not read `include!`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

mod binary_layout_tests;

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

/// A `mod name;` item, with the `#[path = "..."]` attribute on it, or the reason the check cannot
/// tell which file it compiles.
#[derive(Debug, PartialEq)]
struct Declaration {
    module: String,
    path: Option<String>,
    unreadable: Option<&'static str>,
}

/// The out-of-line modules a file declares: any visibility (`pub`, `pub(crate)`), any attributes,
/// a comment after the semicolon, several to a line.
///
/// A declaration under `#[cfg(..)]` counts as compiled on every target, deliberately: the question
/// is whether a file is in a binary at all, and a gate on the declaration or inside the file (the
/// `#![cfg(windows)]` module of `exchange`) decides only where its tests run. Two forms would
/// need more than a lexer to place, so they are refused, not guessed: a path under `cfg_attr`,
/// whose file depends on the target, and a declaration inside braces (an inline module, or a block
/// holding a `#[path]` module), which the compiler resolves from a directory the lexer does not
/// track.
fn declarations(source: &str) -> Vec<Declaration> {
    let tokens = tokens(source);
    let mut found = Vec::new();
    let mut path = None;
    let mut unreadable = None;
    let mut depth = 0_usize;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Symbol('{') => {
                depth += 1;
                (path, unreadable) = (None, None);
                i += 1;
            }
            Token::Symbol('}') => {
                depth = depth.saturating_sub(1);
                (path, unreadable) = (None, None);
                i += 1;
            }
            Token::Symbol('#') => {
                i += 1;
                let inner = tokens.get(i) == Some(&Token::Symbol('!'));
                if inner {
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
                    let attribute = tokens.get(start..i).unwrap_or_default();
                    match attribute {
                        _ if inner => {}
                        [Token::Word(key), Token::Symbol('='), Token::Text(value)]
                            if key == "path" =>
                        {
                            path = Some(value.clone());
                        }
                        [Token::Word(key), ..]
                            if key == "cfg_attr"
                                && attribute.contains(&Token::Word("path".into())) =>
                        {
                            unreadable = Some("its path is under `cfg_attr`");
                        }
                        _ => {}
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
                    let nested = (depth > 0).then_some("it is declared inside braces");
                    found.push(Declaration {
                        module: module.clone(),
                        path: path.take(),
                        unreadable: unreadable.take().or(nested),
                    });
                    i = item + 3;
                } else {
                    (path, unreadable) = (None, None);
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
/// declaration that names them, and a defect for each declaration the check cannot place.
///
/// The compiler's rules: a `#[path]` is relative to the declaring file's directory; a plain
/// `mod name;` looks for `name.rs` or `name/mod.rs` beside a root, a `mod.rs` or a file reached
/// through `#[path]`, and in a directory named for any other file. Every file in `sources` is read
/// for further declarations, helpers included, so a helper that reaches a test file counts.
fn compiled_files(root: &str, sources: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let mut found = Vec::new();
    let mut defects = Vec::new();
    let mut pending = vec![(root.to_owned(), true)];
    let mut expanded = BTreeSet::new();
    while let Some((file, owns_dir)) = pending.pop() {
        let Some(source) = sources.get(&file) else {
            continue;
        };
        if !expanded.insert(file.clone()) {
            continue;
        }
        let (dir, name) = file.rsplit_once('/').unwrap_or(("", &file));
        let module_dir = if owns_dir || name == "mod.rs" {
            dir.to_owned()
        } else {
            join(dir, name.strip_suffix(".rs").unwrap_or(name))
        };
        for declaration in declarations(source) {
            let module = declaration.module.trim_start_matches("r#");
            if let Some(reason) = declaration.unreadable {
                defects.push(format!(
                    "{file}: `mod {module};` cannot be placed by the layout check: {reason}"
                ));
                continue;
            }
            let target = match &declaration.path {
                Some(path) => join(dir, path),
                None => {
                    let flat = join(&module_dir, &format!("{module}.rs"));
                    let nested = join(&module_dir, &format!("{module}/mod.rs"));
                    if !sources.contains_key(&flat) && sources.contains_key(&nested) {
                        nested
                    } else {
                        flat
                    }
                }
            };
            found.push(target.clone());
            pending.push((target, declaration.path.is_some()));
        }
    }
    (found, defects)
}

/// The `[[test]]` targets of a manifest as `(name, path)`, and whether `[package]` sets
/// `autotests = false`, read exactly (a trailing comment allowed). A line scan of the layout this
/// crate uses, not a TOML parser.
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
                ("[package]", "autotests") => {
                    autotests_off = value.split('#').next().unwrap_or("").trim() == "false";
                }
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

/// Every Rust file under `dir`, keyed by its slash-separated path from the crate root.
pub(crate) fn read_sources(crate_dir: &Path, dir: &str, sources: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(crate_dir.join(dir)).unwrap() {
        let path = entry.unwrap().path();
        let key = format!("{dir}/{}", path.file_name().unwrap().to_string_lossy());
        if path.is_dir() {
            read_sources(crate_dir, &key, sources);
        } else if key.ends_with(".rs") {
            sources.insert(key, fs::read_to_string(path).unwrap());
        }
    }
}

/// Whether Cargo would have discovered `path` as a test target: `tests/name.rs` or
/// `tests/name/main.rs`.
fn discoverable(path: &str) -> bool {
    match path
        .strip_prefix("tests/")
        .map(|rest| rest.split('/').collect::<Vec<_>>())
    {
        Some(parts) => match parts[..] {
            [file] => file.ends_with(".rs"),
            [_, file] => file == "main.rs",
            _ => false,
        },
        None => false,
    }
}

/// What is wrong with the layout: `manifest` is the package's `Cargo.toml` and `sources` every
/// Rust file under `tests/`. Discovery must be off, every declaration must be one the check can
/// place, and every discoverable file must be exactly one `[[test]]` target or be named by exactly
/// one `mod` declaration reachable from a target, with or without `#[path]`.
pub(crate) fn layout_defects(manifest: &str, sources: &BTreeMap<String, String>) -> Vec<String> {
    let mut defects = Vec::new();
    let (autotests_off, targets) = manifest_targets(manifest);
    if !autotests_off {
        defects.push("Cargo.toml: autotests is not false, so Cargo builds every file alone".into());
    }
    if targets.is_empty() {
        defects.push("Cargo.toml: no [[test]] target".into());
    }
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unplaced = BTreeSet::new();
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
        let (files, refused) = compiled_files(path, sources);
        for file in files {
            owners
                .entry(file)
                .or_default()
                .push(format!("a module of {name}"));
        }
        unplaced.extend(refused);
    }
    defects.extend(unplaced);
    for path in sources.keys().filter(|path| discoverable(path)) {
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
