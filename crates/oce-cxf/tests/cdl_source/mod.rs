//! Derive each CDL class's interface, in upstream declaration order, from vendored Modelica
//! source (`third_party/modelica-buildings-cdl/`).
//!
//! # The seam
//!
//! This module knows nothing about `oce-blocks`, the CXF fixtures, or the resolver, and it must
//! stay that way. It produces the *reference data* that `fixture_port_order.rs` then checks the
//! shipping registry and the fixture corpus against, so letting it read either would make the
//! comparison answer to itself — the same anti-tautology rule that keeps `tools/golden-gen` off
//! the workspace and away from `oce-blocks`.
//!
//! # Reading Modelica without a Modelica parser
//!
//! Source is blanked of comments and string bodies, then scanned as `;`-terminated statements.
//! Both steps are load-bearing and the order matters: statement scanning is only sound once every
//! remaining `;` is real code, and blanking is what guarantees that. A hand-rolled scanner is
//! justified here because the corpus is pinned, 132 files wide, locally byte-pinned by the hash
//! manifest, and spot-checkable against upstream; a full parser is not, and a dependency would
//! have to be audited against the same standard.
//!
//! This scanner's predecessor shipped **five** defects before it was correct, and every one of
//! them presented as *finding less than reality*, never as finding something wrong. That is why
//! [`derive_table`] pins volumes rather than trusting a clean run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Ports of one class, in upstream declaration order.
pub struct ClassPorts {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub input_kinds: Vec<String>,
    pub output_kinds: Vec<String>,
    /// Upstream declares an array-valued connector (`u[nin]`) on that side. We flatten those into
    /// N scalars, so names cannot correspond 1:1.
    ///
    /// Recorded **per direction and as a declared fact**. Inferring it from an arity mismatch made
    /// every genuine arity disagreement look like an array port and hid 21 real wirings from an
    /// earlier revision. Marking the whole class exempt instead discarded checkable coverage:
    /// `MultiSum` has an array input but a scalar output `y`, which is verifiable.
    pub inputs_are_array: bool,
    pub outputs_are_array: bool,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/oce-cxf.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

/// Directory holding the vendored upstream sources, at their upstream-relative paths.
fn vendored_cdl_root() -> PathBuf {
    repo_root()
        .join("third_party")
        .join("modelica-buildings-cdl")
        .join("Buildings")
        .join("Controls")
        .join("OBC")
        .join("CDL")
}

/// Every `.mo` under `dir`, recursively, sorted so the walk is deterministic.
fn mo_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            mo_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "mo") {
            out.push(path);
        }
    }
}

/// Consume a Modelica identifier from the front of `s`, returning it and the remainder.
///
/// Modelica identifiers are `[A-Za-z_][A-Za-z0-9_]*`. Quoted identifiers (`'x y'`) are legal in
/// the language but appear nowhere in CDL, and accepting them here would let a quoted string be
/// read as a declaration.
fn take_ident(s: &str) -> Option<(&str, &str)> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let end = chars
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map_or(s.len(), |(i, _)| i);
    Some((&s[..end], &s[end..]))
}

/// Blank comment bodies and quoted-string bodies, preserving line count and quote delimiters.
///
/// Declaration-shaped text is only a declaration when it is *code*. Left readable, a comment or a
/// documentation string can forge a port, and a forged port offsets a real one that was lost — so
/// the port totals, which catch any lone loss, go blind. Combining a multi-component clause with
///
/// ```text
/// /*
/// Buildings.Controls.OBC.CDL.Interfaces.IntegerInput ghost[nin]
/// */
/// ```
///
/// turned `Routing.IntegerExtractor`'s inputs into `[index, ghost]` with totals unmoved and the
/// array exemption intact, so neither the registry cross-check nor the fixture comparison ran.
///
/// String-awareness is not optional here. 80 of the 132 vendored files carry a `//` inside an HTML
/// documentation string — `https://` in 70 of them, `modelica://` resource URIs in the rest — and
/// those strings escape their own quotes as `\"`. A stripper that mishandles the escape ends the
/// string early and reads the rest of the line as a line comment.
fn blank_comments_and_strings(src: &str) -> String {
    enum State {
        Code,
        Str,
        LineComment,
        BlockComment,
    }
    // Newlines survive so diagnostics still cite the real line; everything else becomes a space.
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut out = String::with_capacity(src.len());
    let mut state = State::Code;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match state {
            State::Code => match (c, chars.peek()) {
                ('/', Some('/')) => {
                    chars.next();
                    out.push_str("  ");
                    state = State::LineComment;
                }
                ('/', Some('*')) => {
                    chars.next();
                    out.push_str("  ");
                    state = State::BlockComment;
                }
                ('"', _) => {
                    out.push('"');
                    state = State::Str;
                }
                _ => out.push(c),
            },
            State::Str => match c {
                '\\' => {
                    out.push(' ');
                    if let Some(escaped) = chars.next() {
                        out.push(blank(escaped));
                    }
                }
                '"' => {
                    out.push('"');
                    state = State::Code;
                }
                _ => out.push(blank(c)),
            },
            State::LineComment => {
                out.push(blank(c));
                if c == '\n' {
                    state = State::Code;
                }
            }
            State::BlockComment => {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("  ");
                    state = State::Code;
                } else {
                    out.push(blank(c));
                }
            }
        }
    }
    out
}

/// Skip a balanced `open`..`close` run at the head of `s`.
///
/// `Some(s)` unchanged when `s` does not start with `open`; `None` when the run never closes on
/// this line, which is input this scanner cannot read and must therefore refuse.
fn skip_balanced(s: &str, open: char, close: char) -> Option<&str> {
    if !s.starts_with(open) {
        return Some(s);
    }
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&s[i + c.len_utf8()..]);
            }
        }
    }
    None
}

/// True if the line opens a class whose body we should descend into.
///
/// CDL classes are spelled `block` **or** `model` — `CalendarTime` and `CivilTime` are models, and
/// matching only `block` leaves their depth at zero, so every declaration is skipped and the class
/// silently reports no ports at all.
///
/// The class name must be followed by a description string or end-of-line. Without that check,
/// documentation prose is read as a class opening: `Reals/Ramp.mo` line 127 begins "block has the
/// boolean input ..." inside an `<html>` section, which pushes depth to 2 and hides every
/// declaration below it.
fn opens_class(line: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        let stripped = ["partial ", "protected ", "encapsulated "]
            .iter()
            .find_map(|kw| rest.strip_prefix(kw));
        match stripped {
            Some(r) => rest = r.trim_start(),
            None => break,
        }
    }
    let Some(rest) = rest
        .strip_prefix("block ")
        .or_else(|| rest.strip_prefix("model "))
    else {
        return false;
    };
    let Some((_, tail)) = take_ident(rest.trim_start()) else {
        return false;
    };
    let tail = tail.trim();
    tail.is_empty() || tail.starts_with('"')
}

/// True if the line closes a class.
///
/// `end <Ident>;` closes a class, but Modelica spells `end if;` / `end when;` / `end for;` /
/// `end while;` the same way inside algorithm and equation sections. Counting those as class
/// closings drives depth below the class body and drops every later declaration —
/// `CalendarTime` lost all six of its outputs that way.
fn closes_class(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("end ") else {
        return false;
    };
    let Some((ident, tail)) = take_ident(rest.trim_start()) else {
        return false;
    };
    tail.trim_start().starts_with(';') && !matches!(ident, "if" | "when" | "for" | "while")
}

/// A connector declaration: `(name, kind, is_output, is_array)`.
///
/// Accepts the bare `Interfaces.RealInput` spelling and the fully qualified
/// `Buildings.Controls.OBC.CDL.Interfaces.RealInput`, with an optional `discrete` or `parameter`
/// prefix. A side is an **array** only when the subscript sits on this very declaration, which is
/// why array-ness is read here rather than inferred from the class.
fn port_decl(line: &str) -> Option<(String, &'static str, bool, bool)> {
    let mut rest = line.trim_start();
    // A class's description string sits between its header and its body and is terminated by
    // neither `;` nor a keyword, so it lands at the head of the first statement in the body. Skip
    // it. Blanking leaves the delimiters and empties the body, so the next `"` is the closer.
    while let Some(after) = rest.strip_prefix('"') {
        rest = after[after.find('"')? + 1..].trim_start();
    }
    for kw in ["discrete ", "parameter "] {
        if let Some(r) = rest.strip_prefix(kw) {
            rest = r.trim_start();
        }
    }
    rest = rest
        .strip_prefix("Buildings.Controls.OBC.CDL.")
        .unwrap_or(rest);
    let rest = rest.strip_prefix("Interfaces.")?;
    let (kind, rest) = ["Real", "Integer", "Boolean"]
        .iter()
        .find_map(|k| rest.strip_prefix(k).map(|r| (*k, r)))?;
    let (is_output, rest) = if let Some(r) = rest.strip_prefix("Output") {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("Input") {
        (false, r)
    } else {
        return None;
    };
    // A space is required: `RealInputFoo` is a different type, not a declaration of `Foo`.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let (name, tail) = take_ident(rest.trim_start())?;
    let tail = tail.trim_start();
    let is_array = tail.starts_with('[');
    // One component clause may declare SEVERAL components — `IntegerInput index, u[nin];` is legal
    // and this scanner reads only the first. Reading one of many is a silent loss that leaves no
    // unparsed text behind, so refuse the whole line instead: the caller records it and the corpus
    // is rejected by name. Skip the subscript and any modification first, since both carry commas
    // of their own.
    let after = skip_balanced(tail, '[', ']')?.trim_start();
    if skip_balanced(after, '(', ')')?
        .trim_start()
        .starts_with(',')
    {
        return None;
    }
    Some((name.to_owned(), kind, is_output, is_array))
}

/// Bare spellings of every connector type this scanner recognizes.
const CONNECTOR_TYPES: [&str; 6] = [
    "RealInput",
    "RealOutput",
    "IntegerInput",
    "IntegerOutput",
    "BooleanInput",
    "BooleanOutput",
];

/// Is this statement connector-shaped — does it mention the `Interfaces` package or a connector
/// type?
///
/// Paired with [`port_decl`] it separates two things the `Option` alone cannot: a statement that
/// is not a declaration, and a declaration the scanner failed to understand. The second is an
/// error, and treating it as the first is how a port disappears without a trace. Every one of
/// this extractor's five predecessor defects had that shape.
///
/// The multi-component clause is the case that makes it necessary. `IntegerInput index, u[nin];`
/// declares two connectors, [`port_decl`] can read one, so it reads neither and the statement
/// lands here instead — a silent loss otherwise, since dropping `u` leaves nothing behind for
/// [`derive_table`]'s port totals to notice if a forged port offsets it.
///
/// The predicate over-approximates on purpose: it fires on the `Interfaces.` prefix by itself.
fn mentions_connector(line: &str) -> bool {
    line.contains("Interfaces.") || CONNECTOR_TYPES.iter().any(|ty| line.contains(ty))
}

/// Derive one class's interface from vendored upstream source, in declaration order.
///
/// **Only depth-1 declarations count.** CDL sources define nested helper blocks that declare their
/// own connectors: `Logical/VariablePulse.mo` opens `block Cycle` at line 69 whose `BooleanInput
/// go` sits at line 82. A scope-blind scan leaks `go` into the parent interface, yielding inputs
/// `[u, go]` where upstream — and our own shipping registry — carry only `[u]`.
///
/// Connector-shaped statements that do not parse are pushed onto `unparsed` as
/// `<label>:<line>: <text>` rather than skipped, and [`derive_table`] rejects the corpus if any
/// survive.
fn parse_class(path: &Path, label: &str, unparsed: &mut Vec<String>) -> ClassPorts {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let src = blank_comments_and_strings(&raw);
    let mut ports = ClassPorts {
        inputs: Vec::new(),
        outputs: Vec::new(),
        input_kinds: Vec::new(),
        output_kinds: Vec::new(),
        inputs_are_array: false,
        outputs_are_array: false,
    };
    let mut depth = 0usize;
    let mut stmt = String::new();
    let mut stmt_line = 1usize;
    for (lineno, line) in src.lines().enumerate() {
        if opens_class(line) {
            depth += 1;
            stmt.clear();
            continue;
        }
        if closes_class(line) {
            depth = depth.saturating_sub(1);
            stmt.clear();
            continue;
        }
        // Statements, not lines. A declaration may span lines, and 31 of the vendored ones do —
        // their modification clause opens on the head line and closes below it. Every `;` left in
        // blanked source terminates a statement, which is precisely what blanking bought.
        let mut rest = line;
        while let Some(end) = rest.find(';') {
            if stmt.trim().is_empty() {
                stmt_line = lineno + 1;
            }
            stmt.push(' ');
            stmt.push_str(&rest[..end]);
            match port_decl(&stmt) {
                // The unparsed check runs at EVERY depth on purpose. Scope tracking is the part of
                // this scanner with the worst history — three of the five predecessor defects were
                // miscounted depth — so a backstop must not consult the thing it backstops.
                None => {
                    if mentions_connector(&stmt) {
                        unparsed.push(format!("{label}:{stmt_line}: {}", stmt.trim()));
                    }
                }
                Some((name, kind, is_output, is_array)) if depth == 1 => {
                    let (names, kinds, flag) = if is_output {
                        (
                            &mut ports.outputs,
                            &mut ports.output_kinds,
                            &mut ports.outputs_are_array,
                        )
                    } else {
                        (
                            &mut ports.inputs,
                            &mut ports.input_kinds,
                            &mut ports.inputs_are_array,
                        )
                    };
                    names.push(name);
                    kinds.push(kind.to_owned());
                    *flag |= is_array;
                }
                Some(_) => {}
            }
            stmt.clear();
            rest = &rest[end + 1..];
        }
        if stmt.trim().is_empty() && !rest.trim().is_empty() {
            stmt_line = lineno + 1;
        }
        stmt.push(' ');
        stmt.push_str(rest);
    }
    ports
}

/// Derive the whole name->position table from vendored upstream source.
///
/// This replaces a checked-in JSON catalog, and the difference is the point. A catalog is an
/// artifact someone must review entry by entry, and reviewing 132 entries against upstream by hand
/// is a task nobody performs reliably — the generator that produced it shipped five defects before
/// it was right. The hash-manifest gate checks local byte integrity, while the bucket-scoped
/// upstream spot-check in `third_party/modelica-buildings-cdl/README.md` checks fidelity at the
/// pinned commit; the derivation runs here in the open where a reviewer reads code rather than
/// data.
///
/// # Panics
///
/// On any corpus-integrity failure: an unreadable declaration, a class that yields no ports, a
/// changed class count, or changed port totals.
pub fn derive_table() -> BTreeMap<String, ClassPorts> {
    let root = vendored_cdl_root();
    let mut paths = Vec::new();
    mo_files(&root, &mut paths);

    let mut table = BTreeMap::new();
    let mut unparsed = Vec::new();
    for path in &paths {
        let rel = path.strip_prefix(&root).expect("path under CDL root");
        // `Reals/PID.mo` -> `CDL.Reals.PID`. The directory layout mirrors upstream, so this
        // mapping is also what makes the vendored tree diffable against a clone.
        let mut segments = vec!["CDL".to_owned()];
        segments.extend(
            rel.with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned()),
        );
        let label = rel.to_string_lossy().into_owned();
        table.insert(segments.join("."), parse_class(path, &label, &mut unparsed));
    }

    assert!(
        unparsed.is_empty(),
        "{} connector-shaped statement(s) in vendored source did not parse as a declaration:\n  \
         {}\nEach is a port this scanner would otherwise drop in silence.",
        unparsed.len(),
        unparsed.join("\n  ")
    );

    assert_eq!(
        table.len(),
        132,
        "vendored upstream corpus changed size. It is the 132 CDL classes reachable from the 46 \
         G36 fixtures; adding or removing source files changes what this gate covers."
    );
    let portless: Vec<&str> = table
        .iter()
        .filter(|(_, p)| p.inputs.is_empty() && p.outputs.is_empty())
        .map(|(c, _)| c.as_str())
        .collect();
    assert!(
        portless.is_empty(),
        "{} vendored class(es) parsed to zero ports: {:?}. Every CDL block has at least one \
         connector, so this means the scanner failed to find the class body — the exact failure \
         mode that made a scope bug look like a clean run.",
        portless.len(),
        portless
    );

    // A class losing SOME of its ports is the dangerous case the portless check above cannot see,
    // and `Reals.Sort` is the worked example: drop `yIdx` and the class still has ports, still
    // carries an array flag from `y[nin]`, and is skipped by both the registry cross-check and the
    // fixture comparison. Totals are blind to none of that.
    let inputs: usize = table.values().map(|p| p.inputs.len()).sum();
    let outputs: usize = table.values().map(|p| p.outputs.len()).sum();
    assert_eq!(
        (inputs, outputs),
        (175, 144),
        "vendored corpus port totals changed. Upstream declares 175 inputs and 144 outputs across \
         the 132 classes; a drop means the scanner stopped seeing declarations it used to see, \
         which is invisible to every per-class check here."
    );
    table
}
