//! Reading CXF literal values back into the model: typed literals, bare JSON scalars, CDL string
//! expressions, member paths of the facade's named types, bare references and boundary bounds
//! (the inverse of the emitter's `params` module), and the check of each value against its
//! node's `isOfDataType`.
//!
//! Numbers are converted exactly as the engine's loader converts them (`oce_cxf::ground`): a
//! typed literal's lexical form with `str::parse` and no trimming, a bare JSON number as
//! `serde_json` reads it, an Integer on a Real connector with `as f64`. A bound is therefore bit
//! for bit the engine's `InputDefinition` bound.

use oce_api::{EnumerationClass, EnumerationKind, enumeration};
use serde_json::{Map, Value};

use crate::identifier::Identifier;
use crate::model::{Assignment, Bound, Literal, MemberRef, ParamValue, ParameterType, SignalType};

use super::error::{ImportError, ImportErrorKind as Kind};
use super::nodes::{DataType, OBC_PACKAGE, data_type, declared_type, refuse};

/// The XML Schema datatypes a Real typed literal may carry.
const REAL_TYPES: [&str; 3] = [
    "http://www.w3.org/2001/XMLSchema#double",
    "http://www.w3.org/2001/XMLSchema#decimal",
    "http://www.w3.org/2001/XMLSchema#float",
];

/// The XML Schema datatypes an Integer typed literal may carry.
const INTEGER_TYPES: [&str; 3] = [
    "http://www.w3.org/2001/XMLSchema#integer",
    "http://www.w3.org/2001/XMLSchema#int",
    "http://www.w3.org/2001/XMLSchema#long",
];

/// The XML Schema datatype of a Boolean typed literal.
const BOOLEAN_TYPE: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// A scalar literal under `key`: a typed literal, a JSON number or a Boolean. `None` when `value`
/// is none of those (a string, a list or `null`, read elsewhere).
pub(super) fn literal(
    subject: &str,
    key: &str,
    value: &Value,
) -> Result<Option<Literal>, ImportError> {
    let malformed = || refuse(subject, Kind::MalformedLiteral);
    match value {
        Value::Bool(b) => Ok(Some(Literal::Boolean(*b))),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Ok(Some(Literal::Integer(i))),
            None => n
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| Some(Literal::Real(v)))
                .ok_or_else(malformed),
        },
        Value::Object(map) => {
            let (Some(Value::String(text)), Some(Value::String(datatype)), 2) =
                (map.get("@value"), map.get("@type"), map.len())
            else {
                return Err(refuse(
                    subject,
                    Kind::WrongShape {
                        key: key.to_owned(),
                    },
                ));
            };
            if REAL_TYPES.contains(&datatype.as_str()) {
                return text
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| Some(Literal::Real(v)))
                    .ok_or_else(malformed);
            }
            if INTEGER_TYPES.contains(&datatype.as_str()) {
                return text
                    .parse::<i64>()
                    .map(|v| Some(Literal::Integer(v)))
                    .map_err(|_| malformed());
            }
            if datatype == BOOLEAN_TYPE {
                return match text.as_str() {
                    "true" => Ok(Some(Literal::Boolean(true))),
                    "false" => Ok(Some(Literal::Boolean(false))),
                    _ => Err(malformed()),
                };
            }
            Err(malformed())
        }
        _ => Ok(None),
    }
}

/// A full member path `Buildings.Controls.OBC.<class path>.<member>` resolved through the facade
/// table: the authored [`MemberRef`] and its row. `Ok(None)` when `text` is no member path (does
/// not start with `Buildings.`); [`Kind::UnknownMemberType`] when the table lacks its type. A
/// member the row does not have is carried for the validator (`unknown-enum-literal`).
pub(super) fn member_path(
    subject: &str,
    text: &str,
) -> Result<Option<(MemberRef, &'static EnumerationClass)>, ImportError> {
    if !text.starts_with("Buildings.") {
        return Ok(None);
    }
    let unknown = || refuse(subject, Kind::UnknownMemberType);
    let (class_path, member) = text
        .strip_prefix(OBC_PACKAGE)
        .and_then(|path| path.rsplit_once('.'))
        .ok_or_else(unknown)?;
    let row = enumeration(class_path).ok_or_else(unknown)?;
    if member.is_empty() {
        return Err(unknown());
    }
    let member = MemberRef::parse(&format!("{}.{member}", row.type_name()));
    Ok(Some((member, row)))
}

/// A sequence parameter's literal in its declared type. `package` is the constant package an
/// Integer parameter is typed by, when it is.
pub(super) fn sequence_literal(
    subject: &str,
    value_type: &ParameterType,
    package: Option<&'static EnumerationClass>,
    value: &Value,
) -> Result<Literal, ImportError> {
    let mismatch = || refuse(subject, Kind::DatatypeMismatch);
    if let Value::String(text) = value {
        let Some((member, row)) = member_path(subject, text)? else {
            // A quoted string is a String, which no sequence parameter type holds; anything else
            // is an expression or a reference, which a sequence parameter value never is.
            return Err(if text.starts_with('"') {
                mismatch()
            } else {
                refuse(subject, Kind::ExpressionValue)
            });
        };
        let fits = match (value_type, row.kind) {
            (ParameterType::Enum(type_name), EnumerationKind::Enumeration) => {
                row.type_name() == type_name
            }
            (ParameterType::Integer, EnumerationKind::ConstantPackage) => {
                package.is_none_or(|declared| std::ptr::eq(declared, row))
            }
            _ => false,
        };
        return if fits {
            Ok(Literal::Member(member))
        } else {
            Err(mismatch())
        };
    }
    if matches!(value, Value::Array(_)) {
        return Err(refuse(subject, Kind::ArrayShape));
    }
    match (value_type, literal(subject, "S231:value", value)?) {
        (ParameterType::Real, Some(l @ (Literal::Real(_) | Literal::Integer(_))))
        | (ParameterType::Integer, Some(l @ Literal::Integer(_)))
        | (ParameterType::Boolean, Some(l @ Literal::Boolean(_))) => Ok(l),
        (_, Some(_)) => Err(mismatch()),
        (_, None) => Err(refuse(
            subject,
            Kind::WrongShape {
                key: "S231:value".to_owned(),
            },
        )),
    }
}

/// A block parameter value. `params` are the sequence parameter names a bare reference may name.
pub(super) fn param_value(
    subject: &str,
    value: &Value,
    params: &[Identifier],
) -> Result<ParamValue, ImportError> {
    if let Some(read) = literal(subject, "S231:value", value)? {
        return Ok(ParamValue::Literal(read));
    }
    let text = match value {
        Value::String(text) => text,
        Value::Array(_) => return Err(refuse(subject, Kind::ArrayShape)),
        _ => {
            return Err(refuse(
                subject,
                Kind::WrongShape {
                    key: "S231:value".to_owned(),
                },
            ));
        }
    };
    if let Some(body) = text.strip_prefix('"') {
        return cdl_string(subject, body).map(|t| ParamValue::Literal(Literal::Text(t)));
    }
    if let Some((member, _)) = member_path(subject, text)? {
        return Ok(ParamValue::Literal(Literal::Member(member)));
    }
    if params.iter().any(|p| p.as_str() == text) {
        return Ok(ParamValue::Ref(Identifier::unchecked(text.as_str())));
    }
    Err(refuse(subject, Kind::ExpressionValue))
}

/// Whether `text`, a string `S231:value`, is an expression: not a quoted string, not a member
/// path (judged when read) and not the name of a sequence parameter. Used by the refusal
/// precedence pass, before any value is read.
pub(super) fn is_expression(text: &str, params: &[&str]) -> bool {
    !(text.starts_with('"') || text.starts_with("Buildings.") || params.contains(&text))
}

/// A block parameter's `isOfDataType`, when written, agrees with the shape of every value read
/// for it: a reference agrees with any type; otherwise Real takes a Real or Integer literal,
/// Integer an Integer or a constant member, Boolean a Boolean, String a string, an enumeration a
/// member of that enumeration and a constant package an Integer or one of its members.
pub(super) fn check_value_shapes(
    iri: &str,
    node: &Map<String, Value>,
    assignments: &[Assignment],
) -> Result<(), ImportError> {
    let Some(declared) = declared_type(iri, node)? else {
        return Ok(());
    };
    let declared = data_type(iri, declared)?;
    let is_constant = |member: &MemberRef| {
        oce_api::enumerations().iter().any(|row| {
            row.kind == EnumerationKind::ConstantPackage && row.type_name() == member.type_name
        })
    };
    let fits = |literal: &Literal| match (declared, literal) {
        (DataType::Scalar(SignalType::Real), Literal::Real(_) | Literal::Integer(_))
        | (DataType::Scalar(SignalType::Integer), Literal::Integer(_))
        | (DataType::Scalar(SignalType::Boolean), Literal::Boolean(_))
        | (DataType::String, Literal::Text(_)) => true,
        (DataType::Scalar(SignalType::Integer), Literal::Member(member)) => is_constant(member),
        (DataType::Named(row), Literal::Member(member)) => row.type_name() == member.type_name,
        (DataType::Named(row), Literal::Integer(_)) => row.kind == EnumerationKind::ConstantPackage,
        _ => false,
    };
    for assignment in assignments {
        if let ParamValue::Literal(literal) = &assignment.value
            && !fits(literal)
        {
            return Err(refuse(iri, Kind::DatatypeMismatch));
        }
    }
    Ok(())
}

/// A boundary bound under `key` on a connector of type `signal`: a number literal, never a
/// reference or an expression. On a Real boundary every bound is read as the engine reads it, a
/// Real (an Integer token converted with `as f64`); on an Integer boundary it must be an Integer;
/// a Boolean boundary takes none.
pub(super) fn bound(
    subject: &str,
    key: &str,
    signal: SignalType,
    value: &Value,
) -> Result<Bound, ImportError> {
    if matches!(value, Value::String(_)) {
        return Err(refuse(subject, Kind::ExpressionValue));
    }
    let read = literal(subject, key, value)?.ok_or_else(|| {
        refuse(
            subject,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        )
    })?;
    match (signal, read) {
        (SignalType::Real, Literal::Real(v)) => Ok(Bound::Real(v)),
        (SignalType::Real, Literal::Integer(v)) => Ok(Bound::Real(v as f64)),
        (SignalType::Integer, Literal::Integer(v)) => Ok(Bound::Integer(v)),
        _ => Err(refuse(subject, Kind::DatatypeMismatch)),
    }
}

/// Unescape the body of a CDL string expression (after its opening quote): `\"`, `\\` and `\n`
/// only, then exactly one closing quote. Text after the closing quote makes it an expression.
fn cdl_string(subject: &str, body: &str) -> Result<String, ImportError> {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                return if chars.next().is_none() {
                    Ok(out)
                } else {
                    Err(refuse(subject, Kind::ExpressionValue))
                };
            }
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                _ => return Err(refuse(subject, Kind::MalformedLiteral)),
            },
            _ => out.push(ch),
        }
    }
    Err(refuse(subject, Kind::MalformedLiteral))
}

/// Parse a `sizeOfDimensions` spelling, `(3)` or `(2, 3)`. Every size is a run of ASCII digits:
/// a sign (`(+3)`), which `usize::parse` would accept, is refused as `array-shape`.
pub(super) fn dimensions(subject: &str, spelled: &str) -> Result<Vec<usize>, ImportError> {
    let shape = || refuse(subject, Kind::ArrayShape);
    let inner = spelled
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(shape)?;
    let parsed: Option<Vec<usize>> = inner
        .split(',')
        .map(|part| {
            let part = part.trim();
            part.bytes()
                .next()
                .filter(u8::is_ascii_digit)
                .and_then(|_| part.parse::<usize>().ok())
                .filter(|&n| n > 0)
        })
        .collect();
    match parsed {
        Some(dims) if (1..=2).contains(&dims.len()) => Ok(dims),
        _ => Err(shape()),
    }
}
