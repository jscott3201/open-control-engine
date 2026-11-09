//! The v0 graph JSON importer (importer note, "The v0 graph JSON importer"; decisions 17 and 18).
//!
//! The frozen prototype's compact block graph is a strict-JSON object with five required keys
//! (`name`, `inputs`, `outputs`, `blocks`, `connections`) and closed objects throughout: an input
//! is `{name, type, unit, description}`, an output `{name, type, description}`, a block
//! `{id, class, params}` and a parameter `{name, value}`. It is read with the strict reader under
//! the wire form's bounds ([`crate::MAX_WIRE_BYTES`], [`crate::MAX_WIRE_DEPTH`],
//! [`crate::MAX_WIRE_LIST_LEN`], [`crate::MAX_WIRE_STRING_BYTES`]).
//!
//! | v0 | Model |
//! | --- | --- |
//! | `name` | `Sequence.name`; `extends` from [`ImportOptions`] |
//! | `inputs[]` / `outputs[]` `name`, `type` | boundaries; `bool`, `real`, `int` to `Boolean`, `Real`, `Integer` |
//! | `inputs[].unit`, `*.description` | comment lines on the boundary and a [`Dropped`] entry |
//! | `blocks[].id`, `class` | block name; class path `CDL.` + `class` |
//! | `blocks[].params[]` | assignments in order: an integer token `Integer`, a fraction or exponent `Real`, `true`/`false` `Boolean` |
//! | `connections[]` | the model's connection parser, as the wire form uses it |
//!
//! Names are carried unchecked, for the validator. A string parameter value is refused
//! (`string-value`: v0 reserved it for enumerations its own validator refused), and so are `null`,
//! an object and a list (`wrong-shape`).

use serde_json::{Map, Value};

use crate::identifier::Identifier;
use crate::model::{Assignment, Block, Boundary, Literal, Sequence, SignalType};
use crate::wire::split_connection;
use crate::wire::{MAX_WIRE_BYTES, MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES};

use super::comments;
use super::error::{ImportError, ImportErrorKind as Kind, SourceForm};
use super::report::{Dropped, DroppedProperty, ImportOptions, ImportReport, Imported};
use super::strict::{self, Bounds};

pub use super::comments::MAX_V0_COMMENT_LINE_BYTES;

/// The comment line (after its `#`) the v0 importer puts on [`Sequence::annotations`], naming
/// the source form; printed as `# imported from v0 graph JSON`.
pub const V0_PROVENANCE_COMMENT: &str = " imported from v0 graph JSON";

fn refuse(subject: &str, kind: Kind) -> ImportError {
    ImportError::new(SourceForm::GraphV0, subject, kind)
}

/// One closed v0 object at `path`: refuse a key outside `keys`, then a missing one, in `keys`
/// order.
fn closed<'a>(
    path: &str,
    value: &'a Value,
    list: &str,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, ImportError> {
    let Value::Object(object) = value else {
        return Err(refuse(
            path,
            Kind::WrongShape {
                key: list.to_owned(),
            },
        ));
    };
    if let Some(key) = object.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(refuse(path, Kind::UnknownKey { key: key.clone() }));
    }
    if let Some(key) = keys.iter().find(|k| !object.contains_key(**k)) {
        return Err(refuse(
            path,
            Kind::MissingKey {
                key: (*key).to_owned(),
            },
        ));
    }
    Ok(object)
}

/// The string under `key` (present: [`closed`] checked).
fn text<'a>(path: &str, object: &'a Map<String, Value>, key: &str) -> Result<&'a str, ImportError> {
    match object.get(key) {
        Some(Value::String(text)) => Ok(text),
        _ => Err(refuse(
            path,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        )),
    }
}

/// The list under `key`.
fn list<'a>(
    path: &str,
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], ImportError> {
    match object.get(key) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(refuse(
            path,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        )),
    }
}

/// Read one v0 graph document (see [`crate::import_graph_v0`]).
pub(super) fn import(bytes: &[u8], options: &ImportOptions) -> Result<Imported, ImportError> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(refuse(
            "document",
            Kind::TooLarge {
                bytes: bytes.len(),
                limit: MAX_WIRE_BYTES,
            },
        ));
    }
    let bounds = Bounds {
        depth: MAX_WIRE_DEPTH,
        list: MAX_WIRE_LIST_LEN,
        string: MAX_WIRE_STRING_BYTES,
    };
    let document =
        strict::parse_bounded(bytes, Some(bounds)).map_err(|kind| refuse("document", kind))?;
    let top = closed(
        "document",
        &document,
        "document",
        &["name", "inputs", "outputs", "blocks", "connections"],
    )?;
    let mut report = ImportReport::new(SourceForm::GraphV0);
    let mut sequence = Sequence::new(Identifier::unchecked(text("document", top, "name")?));
    sequence.extends.clone_from(&options.extends);
    sequence
        .annotations
        .comments
        .push(V0_PROVENANCE_COMMENT.to_owned());
    for (key, output) in [("inputs", false), ("outputs", true)] {
        for (index, item) in list("document", top, key)?.iter().enumerate() {
            let path = format!("{key}[{index}]");
            let boundary = boundary(&path, item, key, output, &mut report)?;
            if output {
                sequence.outputs.push(boundary);
            } else {
                sequence.inputs.push(boundary);
            }
        }
    }
    for (index, item) in list("document", top, "blocks")?.iter().enumerate() {
        sequence
            .blocks
            .push(block(&format!("blocks[{index}]"), item)?);
    }
    for (index, item) in list("document", top, "connections")?.iter().enumerate() {
        let path = format!("connections[{index}]");
        let Value::String(spelling) = item else {
            return Err(refuse(
                &path,
                Kind::WrongShape {
                    key: "connections".to_owned(),
                },
            ));
        };
        let connection =
            split_connection(spelling).ok_or_else(|| refuse(&path, Kind::MalformedConnection))?;
        sequence.connections.push(connection);
    }
    Ok(Imported { sequence, report })
}

fn signal(path: &str, object: &Map<String, Value>) -> Result<SignalType, ImportError> {
    match text(path, object, "type")? {
        "bool" => Ok(SignalType::Boolean),
        "real" => Ok(SignalType::Real),
        "int" => Ok(SignalType::Integer),
        _ => Err(refuse(path, Kind::UnknownDatatype)),
    }
}

fn boundary(
    path: &str,
    item: &Value,
    list_key: &str,
    output: bool,
    report: &mut ImportReport,
) -> Result<Boundary, ImportError> {
    let keys: &[&str] = if output {
        &["name", "type", "description"]
    } else {
        &["name", "type", "unit", "description"]
    };
    let object = closed(path, item, list_key, keys)?;
    let name = text(path, object, "name")?;
    let mut boundary = Boundary::new(Identifier::unchecked(name), signal(path, object)?);
    let description = text(path, object, "description")?;
    let unit = if output {
        None
    } else {
        Some(text(path, object, "unit")?)
    };
    let comment_lines = &mut boundary.annotations.comments;
    if !description.is_empty() {
        comments::append(
            comment_lines,
            description,
            name,
            DroppedProperty::Description,
            &mut report.truncated,
        );
    }
    if let Some(unit) = unit {
        comments::append(
            comment_lines,
            &format!("unit: {unit}"),
            name,
            DroppedProperty::Unit,
            &mut report.truncated,
        );
    }
    for (property, value) in [
        (DroppedProperty::Description, Some(description)),
        (DroppedProperty::Unit, unit),
    ] {
        if let Some(value) = value {
            report.dropped.push(Dropped {
                subject: name.to_owned(),
                property,
                value: value.to_owned(),
            });
        }
    }
    Ok(boundary)
}

fn block(path: &str, item: &Value) -> Result<Block, ImportError> {
    let object = closed(path, item, "blocks", &["id", "class", "params"])?;
    let class = text(path, object, "class")?;
    let mut block = Block::new(
        Identifier::unchecked(text(path, object, "id")?),
        format!("CDL.{class}"),
    );
    for (index, param) in list(path, object, "params")?.iter().enumerate() {
        let param_path = format!("{path}.params[{index}]");
        let param = closed(&param_path, param, "params", &["name", "value"])?;
        let name = text(&param_path, param, "name")?;
        let literal = match param.get("value") {
            Some(Value::Bool(b)) => Literal::Boolean(*b),
            Some(Value::Number(n)) => match n.as_i64() {
                Some(i) => Literal::Integer(i),
                None => n
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .map(Literal::Real)
                    .ok_or_else(|| refuse(&param_path, Kind::MalformedLiteral))?,
            },
            Some(Value::String(_)) => return Err(refuse(&param_path, Kind::StringValue)),
            _ => {
                return Err(refuse(
                    &param_path,
                    Kind::WrongShape {
                        key: "value".to_owned(),
                    },
                ));
            }
        };
        block
            .assignments
            .push(Assignment::new(Identifier::unchecked(name), literal));
    }
    Ok(block)
}
