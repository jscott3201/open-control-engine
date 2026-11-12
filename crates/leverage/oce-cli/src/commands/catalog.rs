//! `oce catalog`: the block classes, their parameters and resolved ports, and the block
//! vocabulary that says what each one means (vocabulary note decision 12, GitHub issue #500).
//!
//! Without a class: every class in catalog order (`oce_api::class_metadata`), each with its ports
//! at the default parameter table where they resolve (`null` where a required parameter has no
//! default); exit `0`. The text listing is one line per class. With a class: that class, its
//! parameters (type, default, settable) and its ports resolved at the default table or at the
//! `--param NAME=VALUE` values (`oce_api::class_ports`, [`super::catalog_params`]). The class
//! rules' refusal of the table is a finding (exit `1`), and so is an unknown class; a malformed
//! `--param` is an error (exit `2`).
//!
//! Every class carries the vocabulary of `oce-vocab` joined in: each parameter's and port's
//! `description`, declared `unit` and `quantity`, each parameter's `range`, the class's typed
//! `notes` and its upstream `links`. The split of who owns what is the vocabulary's: meanings,
//! units, notes and links come from its two data files; **a range is rendered from the
//! catalog's rules** (`oce_vocab::parameter_range`), never from the data. A port's text is its
//! connector's (`u3` of a `MultiSum` reads the vector connector `u`). A note is printed as its
//! `kind`, `subject` and `text`: its `evidence` is for reviewers and is never printed. An
//! engine-reserved lowering identity has no vocabulary: its strings are empty and its `links`
//! are `null`.
//!
//! An enumeration parameter's JSON `enumeration` is the type's authoring class path
//! (`oce_api::enumeration_class_path`), `null` for every other type.

use std::fmt::Write as _;

use oce_api::{
    CatalogDefault, CatalogPortKind, CatalogRule, ClassMetadata, ClassParameter, ClassPort,
    ClassPortsError, Value as EngineValue, ValueType, catalog, class_metadata, class_ports,
    enumeration_class_path,
};
use oce_vocab::{ClassVocab, Side, class_vocab, parameter_base_name, parameter_range};
use serde_json::{Value, json};

use super::Outcome;
use super::catalog_params::{parse_params, text_type_name, type_name};
use crate::report::Report;
use crate::{Exit, codes};

/// Run `catalog`.
pub(crate) fn run(class: Option<&str>, json: bool, params: &[String]) -> Outcome {
    let mut report = Report::new("catalog", None);
    match class {
        None => list(report, json),
        Some(class) => {
            report.set("class", Value::Null);
            report.set("params", params_echo(params));
            describe(report, class, json, params)
        }
    }
}

/// Every class.
fn list(mut report: Report, json: bool) -> Outcome {
    let metadata = class_metadata();
    let classes: Vec<Value> = metadata
        .iter()
        .map(|meta| {
            let ports = class_ports(meta.class_path, &[]).ok();
            let ports = ports.as_ref().map(|p| (&p.inputs[..], &p.outputs[..]));
            ClassView::new(meta).to_json(ports)
        })
        .collect();
    report.set("classes", Value::Array(classes));
    report.verdict(format!("{} classes", metadata.len()));
    if json {
        return Outcome::with_exit(Exit::Yes, report);
    }
    let width = metadata
        .iter()
        .map(|m| m.class_path.len())
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for meta in metadata {
        let _ = writeln!(text, "{:<width$}  {}", meta.class_path, meta.description);
    }
    Outcome::document(report, text.into_bytes())
}

/// One class at a parameter table.
fn describe(mut report: Report, class: &str, json: bool, params: &[String]) -> Outcome {
    let Some(meta) = class_metadata().iter().find(|m| m.class_path == class) else {
        report.finding_raw(
            oce_author::CLASS_NOT_FOUND,
            class,
            &format!("`{class}` is not a registered class"),
            "engine",
        );
        report.verdict("no such class");
        return finish(report, json, None);
    };
    let table = match parse_params(meta, params) {
        Ok(table) => table,
        Err((code, detail)) => {
            report.error(code, detail, None);
            report.verdict("refused: malformed --param");
            return finish(report, json, None);
        }
    };
    let borrowed: Vec<(&str, EngineValue)> = table
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let resolved = class_ports(meta.class_path, &borrowed);
    let ports = match &resolved {
        Ok(ports) => Some((&ports.inputs[..], &ports.outputs[..])),
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
            for diagnostic in diagnostics {
                report.finding_raw(
                    diagnostic.code.as_str(),
                    meta.class_path,
                    &diagnostic.message,
                    "engine",
                );
            }
            None
        }
        Err(error) => {
            report.error(codes::PORTS_UNRESOLVABLE, error.to_string(), None);
            None
        }
    };
    let view = ClassView::new(meta);
    report.set("class", view.to_json(ports));
    report.verdict(if ports.is_some() {
        "ports resolved"
    } else {
        "ports do not resolve at this parameter table"
    });
    let text = ports.map(|ports| view.to_text(ports));
    finish(report, json, text)
}

/// A described class's outcome: the report, or the text when it resolved and `--json` is off.
fn finish(report: Report, json: bool, text: Option<String>) -> Outcome {
    match text {
        Some(text) if !json => Outcome::document(report, text.into_bytes()),
        _ => Outcome::judged(report),
    }
}

/// The `--param` values as given, for the report.
fn params_echo(params: &[String]) -> Value {
    Value::Array(params.iter().map(|p| Value::from(p.as_str())).collect())
}

fn kind_name(kind: CatalogPortKind) -> &'static str {
    match kind {
        CatalogPortKind::Real => "Real",
        CatalogPortKind::Integer => "Integer",
        CatalogPortKind::Boolean => "Boolean",
    }
}

/// A default as `{kind, value}`.
fn default_json(default: &CatalogDefault) -> Value {
    let (kind, value) = match default {
        CatalogDefault::Real(v) => ("real", json!(v)),
        CatalogDefault::Integer(v) => ("integer", json!(v)),
        CatalogDefault::Boolean(v) => ("boolean", json!(v)),
        CatalogDefault::String(v) => ("string", json!(v)),
        CatalogDefault::EnumMember(v) => ("member", json!(v)),
        CatalogDefault::Derived(v) => ("derived", json!(v)),
        CatalogDefault::Required => ("required", Value::Null),
    };
    json!({ "kind": kind, "value": value })
}

/// A default as the text listing spells it.
fn default_text(default: &CatalogDefault) -> String {
    match default {
        CatalogDefault::Real(v) => format!("= {v:?}"),
        CatalogDefault::Integer(v) => format!("= {v}"),
        CatalogDefault::Boolean(v) => format!("= {v}"),
        CatalogDefault::String(v) => format!("= {v:?}"),
        CatalogDefault::EnumMember(v) => format!("= {v}"),
        CatalogDefault::Derived(v) => format!("(derived: {v})"),
        CatalogDefault::Required => "(required)".to_owned(),
    }
}

/// The authoring class path of an enumeration type; `None` for every other type.
fn enumeration_of(value_type: ValueType) -> Option<&'static str> {
    match value_type {
        ValueType::Enum(id) => enumeration_class_path(id),
        _ => None,
    }
}

/// What the vocabulary says about one parameter or port. Every string is `""` where the
/// vocabulary says nothing (an undeclared unit, a reserved class).
#[derive(Debug, Default, PartialEq, Eq)]
struct Meaning<'a> {
    description: &'a str,
    unit: &'a str,
    quantity: &'a str,
}

impl Meaning<'_> {
    /// The indented lines of the text listing: the description, then the declared unit and
    /// quantity on one line. Nothing for an empty field.
    fn write_text(&self, text: &mut String) {
        if !self.description.is_empty() {
            let _ = writeln!(text, "      {}", self.description);
        }
        let declared: Vec<String> = [("unit", self.unit), ("quantity", self.quantity)]
            .iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(label, value)| format!("{label}: {value}"))
            .collect();
        if !declared.is_empty() {
            let _ = writeln!(text, "      {}", declared.join("; "));
        }
    }
}

/// One class as `oce catalog` serves it: the facade's metadata, the catalog's rules and the
/// vocabulary, joined. The JSON and the text listing are both rendered from this one view.
struct ClassView<'a> {
    meta: &'a ClassMetadata,
    /// Whether the class is an engine-reserved lowering identity.
    reserved: bool,
    /// The catalog's parameter rules, which the ranges are rendered from.
    rules: &'static [CatalogRule],
    /// The class's vocabulary; `None` for a reserved class.
    vocab: Option<ClassVocab<'static>>,
}

impl<'a> ClassView<'a> {
    fn new(meta: &'a ClassMetadata) -> Self {
        let entry = catalog()
            .iter()
            .find(|entry| entry.class_path == meta.class_path);
        Self {
            meta,
            reserved: entry.is_some_and(|entry| entry.reserved),
            rules: entry.map_or(&[], |entry| &entry.param_rules),
            vocab: class_vocab(meta.class_path),
        }
    }

    /// The meaning of the parameter at `index` of the class's parameters. The vocabulary lists
    /// them in the catalog's order; a position that names another parameter says nothing.
    fn parameter_meaning(&self, index: usize, parameter: &ClassParameter) -> Meaning<'_> {
        self.vocab
            .as_ref()
            .and_then(|vocab| vocab.parameters.get(index))
            .filter(|found| found.catalog.name == parameter.name)
            .map_or_else(Meaning::default, |found| Meaning {
                description: found.description,
                unit: found.unit,
                quantity: found.quantity,
            })
    }

    /// The constraints the catalog's rules place on the parameter's value.
    fn parameter_range(&self, parameter: &ClassParameter) -> Vec<String> {
        parameter_range(self.rules, parameter_base_name(parameter.name))
    }

    /// The meaning of a resolved port: its connector's.
    fn port_meaning(&self, side: Side, port: &ClassPort) -> Meaning<'_> {
        self.vocab
            .as_ref()
            .and_then(|vocab| vocab.connector_of_port(side, &port.name))
            .map_or_else(Meaning::default, |(connector, _)| Meaning {
                description: connector.description,
                unit: connector.unit,
                quantity: connector.quantity,
            })
    }

    fn ports_json(&self, side: Side, ports: &[ClassPort]) -> Value {
        Value::Array(
            ports
                .iter()
                .map(|port| {
                    let meaning = self.port_meaning(side, port);
                    json!({
                        "description": meaning.description,
                        "kind": kind_name(port.kind),
                        "name": port.name,
                        "quantity": meaning.quantity,
                        "unit": meaning.unit,
                    })
                })
                .collect(),
        )
    }

    fn parameters_json(&self) -> Value {
        Value::Array(
            self.meta
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    let meaning = self.parameter_meaning(index, parameter);
                    json!({
                        "default": default_json(&parameter.default),
                        "description": meaning.description,
                        "enumeration": enumeration_of(parameter.value_type),
                        "name": parameter.name,
                        "quantity": meaning.quantity,
                        "range": self.parameter_range(parameter),
                        "settable": parameter.settable,
                        "type": type_name(parameter.value_type),
                        "unit": meaning.unit,
                    })
                })
                .collect(),
        )
    }

    /// The notes as an agent reads them: `kind`, `subject` and `text`. `evidence` is left out.
    fn notes_json(&self) -> Value {
        let notes = self.vocab.as_ref().map_or(&[][..], |vocab| vocab.notes);
        Value::Array(
            notes
                .iter()
                .map(|note| {
                    json!({
                        "kind": note.kind.as_str(),
                        "subject": note.subject,
                        "text": note.text,
                    })
                })
                .collect(),
        )
    }

    fn links_json(&self) -> Value {
        let links = self.vocab.as_ref().and_then(|vocab| vocab.links.as_ref());
        links.map_or(Value::Null, |links| {
            json!({
                "latest_help": links.latest_help,
                "pinned_source": links.pinned_source,
            })
        })
    }

    /// The class as the report's JSON; `ports` is `None` where they do not resolve.
    fn to_json(&self, ports: Option<(&[ClassPort], &[ClassPort])>) -> Value {
        json!({
            "class_path": self.meta.class_path,
            "description": self.meta.description,
            "inputs": ports.map_or(Value::Null, |(inputs, _)| self.ports_json(Side::Input, inputs)),
            "links": self.links_json(),
            "notes": self.notes_json(),
            "outputs": ports
                .map_or(Value::Null, |(_, outputs)| self.ports_json(Side::Output, outputs)),
            "parameters": self.parameters_json(),
            "reserved": self.reserved,
        })
    }

    /// The text rendering of the resolved class, in the text form's own spelling.
    fn to_text(&self, (inputs, outputs): (&[ClassPort], &[ClassPort])) -> String {
        let meta = self.meta;
        let mut text = format!("{}\n  {}\n", meta.class_path, meta.description);
        text.push_str("\nparameters:\n");
        if meta.parameters.is_empty() {
            text.push_str("  (none)\n");
        }
        for (index, parameter) in meta.parameters.iter().enumerate() {
            let _ = writeln!(
                text,
                "  {} : {} {}{}",
                parameter.name,
                text_type_name(parameter.value_type),
                default_text(&parameter.default),
                if parameter.settable {
                    ""
                } else {
                    " [not settable]"
                }
            );
            self.parameter_meaning(index, parameter)
                .write_text(&mut text);
            let range = self.parameter_range(parameter);
            if !range.is_empty() {
                let _ = writeln!(text, "      range: {}", range.join("; "));
            }
        }
        for (title, side, ports) in [
            ("inputs", Side::Input, inputs),
            ("outputs", Side::Output, outputs),
        ] {
            let _ = writeln!(text, "\n{title}:");
            if ports.is_empty() {
                text.push_str("  (none)\n");
            }
            for port in ports {
                let _ = writeln!(text, "  {} : {}", port.name, kind_name(port.kind));
                self.port_meaning(side, port).write_text(&mut text);
            }
        }
        self.write_notes_and_links(&mut text);
        text
    }

    /// The `notes:` and `links:` sections of the text rendering.
    fn write_notes_and_links(&self, text: &mut String) {
        let vocab = self.vocab.as_ref();
        text.push_str("\nnotes:\n");
        let notes = vocab.map_or(&[][..], |vocab| vocab.notes);
        if notes.is_empty() {
            text.push_str("  (none)\n");
        }
        for note in notes {
            let kind = note.kind.as_str();
            let _ = if note.subject.is_empty() {
                writeln!(text, "  {kind}: {}", note.text)
            } else {
                writeln!(text, "  {kind} ({}): {}", note.subject, note.text)
            };
        }
        text.push_str("\nlinks:\n");
        match vocab.and_then(|vocab| vocab.links.as_ref()) {
            Some(links) => {
                let _ = writeln!(
                    text,
                    "  source at the pinned Buildings commit: {}",
                    links.pinned_source
                );
                let _ = writeln!(
                    text,
                    "  help in the latest Buildings release (may drift from the pin): {}",
                    links.latest_help
                );
            }
            None => text.push_str("  (none)\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(class_path: &str) -> ClassView<'static> {
        let meta = class_metadata()
            .iter()
            .find(|meta| meta.class_path == class_path)
            .unwrap_or_else(|| panic!("{class_path} is a registered class"));
        ClassView::new(meta)
    }

    fn port(name: &str) -> ClassPort {
        ClassPort {
            name: name.to_owned(),
            kind: CatalogPortKind::Real,
        }
    }

    #[test]
    fn only_an_enumeration_type_names_an_enumeration() {
        for row in oce_api::enumeration_types() {
            let class = row.id.expect("an enumeration has an id");
            assert_eq!(enumeration_of(ValueType::Enum(class)), Some(row.class_path));
        }
        for other in [
            ValueType::Real,
            ValueType::Integer,
            ValueType::Boolean,
            ValueType::String,
        ] {
            assert_eq!(enumeration_of(other), None);
        }
    }

    #[test]
    fn a_port_reads_its_connectors_meaning_whichever_element_it_is() {
        let sum = view("CDL.Reals.MultiSum");
        let first = sum.port_meaning(Side::Input, &port("u1"));
        assert!(!first.description.is_empty());
        for element in ["u2", "u3", "u1048576"] {
            assert_eq!(sum.port_meaning(Side::Input, &port(element)), first);
        }
        // Not an element of any input connector: no text, never another port's.
        for stranger in ["u", "u0", "u01", "v1", "y", ""] {
            assert_eq!(
                sum.port_meaning(Side::Input, &port(stranger)),
                Meaning::default(),
                "{stranger:?}"
            );
        }
        // The output of the same name is a different connector on the other side.
        assert_ne!(
            sum.port_meaning(Side::Output, &port("y")),
            Meaning::default()
        );
        assert_eq!(
            sum.port_meaning(Side::Output, &port("u1")),
            Meaning::default()
        );
    }

    #[test]
    fn a_reserved_class_has_no_vocabulary_and_still_renders() {
        let reserved = class_metadata()
            .iter()
            .find(|meta| ClassView::new(meta).reserved)
            .expect("a reserved class");
        let view = ClassView::new(reserved);
        assert!(view.vocab.is_none());
        let json = view.to_json(Some((&[port("u")], &[port("y")])));
        assert_eq!(json["links"], Value::Null);
        assert_eq!(json["notes"], json!([]));
        assert_eq!(json["inputs"][0]["description"], "");
        assert_eq!(json["inputs"][0]["unit"], "");
        assert_eq!(json["inputs"][0]["quantity"], "");
        let text = view.to_text((&[port("u")], &[port("y")]));
        assert!(
            text.ends_with("\nnotes:\n  (none)\n\nlinks:\n  (none)\n"),
            "{text}"
        );
        assert!(text.contains("\ninputs:\n  u : Real\n\noutputs:\n  y : Real\n"));
    }

    #[test]
    fn a_meaning_writes_only_what_it_has() {
        let lines = |description, unit, quantity| {
            let mut text = String::new();
            Meaning {
                description,
                unit,
                quantity,
            }
            .write_text(&mut text);
            text
        };
        assert_eq!(lines("", "", ""), "");
        assert_eq!(lines("Delay time", "", ""), "      Delay time\n");
        assert_eq!(
            lines("Delay time", "s", "Time"),
            "      Delay time\n      unit: s; quantity: Time\n"
        );
        assert_eq!(lines("", "1", ""), "      unit: 1\n");
        assert_eq!(lines("", "", "Time"), "      quantity: Time\n");
    }

    #[test]
    fn the_range_of_an_array_parameter_is_looked_up_by_its_cdl_name() {
        let extract = view("CDL.Routing.BooleanExtractSignal");
        let parameter = extract
            .meta
            .parameters
            .iter()
            .find(|parameter| parameter.name == "extract_<i>")
            .expect("the array template");
        assert_eq!(
            extract.parameter_range(parameter),
            ["1 <= extract_<i> <= nin"]
        );
    }
}
