//! The canonical printer (authoring note, "Canonical printer"; decisions 6 and 12).
//!
//! Section order is the `file` production's: header comments, `sequence`, `extends`, then
//! parameters, inputs, outputs, blocks and connections, authored order kept inside each section.
//! One space around `=`, `->` and `:`. Reals in Rust's shortest round-trip spelling (`{:?}`),
//! Integers bare, suffixes after one space. A statement's blank line and comments are printed
//! above it, the blank line first. A block prints on one line when it has at most two assignments,
//! none annotated, and the line fits in [`MAX_ONE_LINE_COLUMNS`]; otherwise one assignment per line,
//! indented by two spaces. The trailing comments follow one blank line. The output ends with one
//! line feed.
//!
//! Arrays. The model holds an array as one assignment per element (decision 15), so the printer
//! regroups a run into `[..]` through the catalog: a consecutive run of elements of one array
//! template, starting at index 1, in row-major order, filling a vector or whole matrix rows, with no
//! annotations after the first element. Anything else (a gap, another order, an annotated element,
//! an unknown class) prints one assignment per element under its flattened name. A regrouped array
//! that does not fit in [`MAX_ONE_LINE_COLUMNS`] at its indent prints one element (a vector) or one
//! row (a matrix) per line, so formatting a long table never produces a line past the bound.

use crate::model::{Annotations, Assignment, Block, Boundary, Literal, ParamValue, Sequence};
use crate::validate::catalog::{is_template, lookup_class, lookup_parameter, match_template};

use super::parser::{TAGS, template_base};

/// The widest one-line block, in columns (characters).
pub const MAX_ONE_LINE_COLUMNS: usize = 100;

/// Print a sequence canonically.
pub(super) fn print(sequence: &Sequence) -> String {
    let mut out = String::new();
    comments(&mut out, &sequence.annotations.comments, "");
    out.push_str(&format!("sequence {}\n", sequence.name));
    match &sequence.extends {
        Some(id) => out.push_str(&format!("extends {id}\n")),
        None => out.push_str("extends none\n"),
    }
    for param in &sequence.params {
        annotations(&mut out, &param.annotations, "");
        out.push_str(&format!(
            "param {} : {} = {}\n",
            param.name,
            param.value_type.as_str(),
            with_suffix(&literal(&param.value), param.suffix.as_deref())
        ));
    }
    for input in &sequence.inputs {
        boundary(&mut out, "in", input);
    }
    for output in &sequence.outputs {
        boundary(&mut out, "out", output);
    }
    for block in &sequence.blocks {
        print_block(&mut out, block);
    }
    for connection in &sequence.connections {
        annotations(&mut out, &connection.annotations, "");
        out.push_str(&connection.spelling());
        out.push('\n');
    }
    if !sequence.trailing_comments.is_empty() {
        out.push('\n');
        comments(&mut out, &sequence.trailing_comments, "");
    }
    out
}

fn comments(out: &mut String, lines: &[String], indent: &str) {
    for line in lines {
        out.push_str(indent);
        out.push('#');
        out.push_str(line);
        out.push('\n');
    }
}

fn annotations(out: &mut String, annotations: &Annotations, indent: &str) {
    if annotations.blank_line_before {
        out.push('\n');
    }
    comments(out, &annotations.comments, indent);
}

fn is_plain(annotations: &Annotations) -> bool {
    annotations.comments.is_empty() && !annotations.blank_line_before
}

/// A string literal: quoted, with `"`, `\` and line feed escaped.
pub(super) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn literal(value: &Literal) -> String {
    match value {
        Literal::Integer(v) => v.to_string(),
        Literal::Real(v) => format!("{v:?}"),
        Literal::Boolean(v) => v.to_string(),
        Literal::Text(text) => quoted(text),
        Literal::EnumMember(member) => member.spelling(),
    }
}

fn with_suffix(value: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(suffix) => format!("{value} {suffix}"),
        None => value.to_owned(),
    }
}

fn element(assignment: &Assignment) -> String {
    let value = match &assignment.value {
        ParamValue::Literal(lit) => literal(lit),
        ParamValue::Ref(name) => name.as_str().to_owned(),
    };
    with_suffix(&value, assignment.suffix.as_deref())
}

fn boundary(out: &mut String, keyword: &str, b: &Boundary) {
    annotations(out, &b.annotations, "");
    let s = &b.semantic;
    let mut line = format!("{keyword} {} : {}", b.name, b.value_type.as_str());
    for tag in TAGS {
        let atoms: Vec<String> = match *tag {
            "unit" => s.unit.iter().cloned().collect(),
            "quantity" => b.quantity.iter().cloned().collect(),
            "displayUnit" => b.display_unit.iter().cloned().collect(),
            "brick" => s.brick.iter().cloned().collect(),
            "s223" => s.s223.iter().cloned().collect(),
            "qudt" => s.qudt.iter().cloned().collect(),
            "haystack" => s.haystack.clone(),
            "name" => s.name.iter().map(|t| quoted(t)).collect(),
            "label" => s.label.iter().map(|t| quoted(t)).collect(),
            _ => {
                if s.provisional {
                    line.push_str(" @provisional");
                }
                Vec::new()
            }
        };
        for atom in atoms {
            line.push_str(&format!(" @{tag} {atom}"));
        }
    }
    out.push_str(&line);
    out.push('\n');
}

/// One printed item of a block's list: the annotations above it and `name = value`.
struct Item<'a> {
    annotations: &'a Annotations,
    text: String,
}

fn print_block(out: &mut String, block: &Block) {
    annotations(out, &block.annotations, "");
    let head = format!("block {} = {}", block.name, block.class_path);
    let items = items(block);
    if items.is_empty() {
        out.push_str(&head);
        out.push('\n');
        return;
    }
    if items.len() <= 2
        && items
            .iter()
            .all(|item| is_plain(item.annotations) && !item.text.contains('\n'))
    {
        let texts: Vec<&str> = items.iter().map(|item| item.text.as_str()).collect();
        let line = format!("{head}({})", texts.join(", "));
        if line.chars().count() <= MAX_ONE_LINE_COLUMNS {
            out.push_str(&line);
            out.push('\n');
            return;
        }
    }
    out.push_str(&head);
    out.push_str("(\n");
    let last = items.len() - 1;
    for (i, item) in items.iter().enumerate() {
        annotations(out, item.annotations, "  ");
        out.push_str("  ");
        out.push_str(&item.text);
        if i != last {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(")\n");
}

fn items(block: &Block) -> Vec<Item<'_>> {
    let list = &block.assignments;
    let mut items = Vec::with_capacity(list.len());
    let mut j = 0;
    while j < list.len() {
        if let Some((len, base, parts)) = array_run(block, j) {
            items.push(Item {
                annotations: &list[j].annotations,
                text: list_text(base, &parts),
            });
            j += len;
        } else {
            items.push(Item {
                annotations: &list[j].annotations,
                text: format!("{} = {}", list[j].name, element(&list[j])),
            });
            j += 1;
        }
    }
    items
}

/// `base = [..]` on one line when it fits in [`MAX_ONE_LINE_COLUMNS`] at its two-space indent;
/// otherwise one part (a vector element or a matrix row) per line, indented by four spaces, so a
/// long array never formats past the line bound.
fn list_text(base: &str, parts: &[String]) -> String {
    let one = format!("{base} = [{}]", parts.join(", "));
    if one.chars().count() + 2 <= MAX_ONE_LINE_COLUMNS {
        return one;
    }
    format!("{base} = [\n    {}\n  ]", parts.join(",\n    "))
}

/// The array run starting at assignment `j` of `block`, if any: how many assignments it covers,
/// the array's base name, and its parts — the element texts of a vector, or the `[..]` row texts
/// of a matrix (see the module documentation).
fn array_run(block: &Block, j: usize) -> Option<(usize, &'static str, Vec<String>)> {
    let list = &block.assignments;
    let class = lookup_class(&block.class_path)?;
    let parameter = lookup_parameter(class.metadata, list[j].name.as_str())?;
    if !is_template(parameter.name) {
        return None;
    }
    let template = parameter.name;
    let start = match_template(template, list[j].name.as_str())?;
    if start.iter().any(|&i| i != 1) {
        return None;
    }
    // Whether assignment `at` is element `indices` and carries no annotations (the first may).
    let fits = |at: usize, indices: &[usize]| -> bool {
        list.get(at).is_some_and(|a| {
            (at == j || is_plain(&a.annotations))
                && oce_api::param_element_name(template, indices).as_deref()
                    == Some(a.name.as_str())
        })
    };
    let base = template_base(template);
    match start.len() {
        1 => {
            let len = (0..).take_while(|&k| fits(j + k, &[k + 1])).count();
            if len == 0 {
                return None;
            }
            Some((len, base, list[j..j + len].iter().map(element).collect()))
        }
        2 => {
            let cols = (0..).take_while(|&c| fits(j + c, &[1, c + 1])).count();
            if cols == 0 {
                return None;
            }
            let mut rows = 1;
            while (0..cols).all(|c| fits(j + rows * cols + c, &[rows + 1, c + 1])) {
                rows += 1;
            }
            let text: Vec<String> = (0..rows)
                .map(|r| {
                    let cells: Vec<String> = list[j + r * cols..j + (r + 1) * cols]
                        .iter()
                        .map(element)
                        .collect();
                    format!("[{}]", cells.join(", "))
                })
                .collect();
            Some((rows * cols, base, text))
        }
        _ => None,
    }
}
