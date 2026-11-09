//! Reading CXF literal values back into the model: typed literals, bare JSON scalars, CDL string
//! expressions, enumeration member paths and bare references (the inverse of the emitter's
//! `params` module).

use serde_json::Value;

use crate::identifier::Identifier;
use crate::model::{Literal, MemberRef, ParamValue, SignalType};

use super::{ImportError, unsupported};

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

/// The package prefix of a full CDL enumeration member path.
const ENUM_PREFIX: &str = "Buildings.Controls.OBC.CDL.Types.";

/// A scalar literal: a typed literal, a JSON number or a Boolean. `None` when `value` is none of
/// those (a string or an array, read elsewhere).
pub(super) fn literal(subject: &str, value: &Value) -> Result<Option<Literal>, ImportError> {
    match value {
        Value::Bool(b) => Ok(Some(Literal::Boolean(*b))),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Ok(Some(Literal::Integer(i))),
            None => n
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| Some(Literal::Real(v)))
                .ok_or_else(|| unsupported(subject, "a number outside the Integer and Real range")),
        },
        Value::Object(map) => {
            let (Some(Value::String(text)), Some(Value::String(datatype))) =
                (map.get("@value"), map.get("@type"))
            else {
                return Err(unsupported(
                    subject,
                    "a value object other than a typed literal",
                ));
            };
            if map.len() != 2 {
                return Err(unsupported(subject, "a typed literal with extra keys"));
            }
            if REAL_TYPES.contains(&datatype.as_str()) {
                return text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| Some(Literal::Real(v)))
                    .ok_or_else(|| {
                        unsupported(subject, "a Real literal that is not a finite number")
                    });
            }
            if datatype == BOOLEAN_TYPE {
                return match text.trim() {
                    "true" => Ok(Some(Literal::Boolean(true))),
                    "false" => Ok(Some(Literal::Boolean(false))),
                    _ => Err(unsupported(
                        subject,
                        "a Boolean literal other than true or false",
                    )),
                };
            }
            if INTEGER_TYPES.contains(&datatype.as_str()) {
                return text
                    .trim()
                    .parse::<i64>()
                    .map(|v| Some(Literal::Integer(v)))
                    .map_err(|_| unsupported(subject, "an Integer literal outside the i64 range"));
            }
            Err(unsupported(
                subject,
                "a typed literal of an unsupported datatype",
            ))
        }
        _ => Ok(None),
    }
}

/// A sequence parameter's literal in its declared type.
pub(super) fn sequence_literal(
    subject: &str,
    value_type: SignalType,
    value: &Value,
) -> Result<Literal, ImportError> {
    let read = literal(subject, value)?;
    match (value_type, read) {
        (SignalType::Real, Some(l @ (Literal::Real(_) | Literal::Integer(_))))
        | (SignalType::Integer, Some(l @ Literal::Integer(_)))
        | (SignalType::Boolean, Some(l @ Literal::Boolean(_))) => Ok(l),
        (_, Some(_)) => Err(unsupported(
            subject,
            "a sequence parameter value whose kind differs from its declared type",
        )),
        (_, None) => Err(unsupported(
            subject,
            "a sequence parameter value that is not a literal (a reference or an expression)",
        )),
    }
}

/// A block parameter value. `params` are the sequence parameter names a bare reference may name.
pub(super) fn param_value(
    subject: &str,
    value: &Value,
    params: &[Identifier],
) -> Result<ParamValue, ImportError> {
    if let Some(read) = literal(subject, value)? {
        return Ok(ParamValue::Literal(read));
    }
    let Value::String(text) = value else {
        return Err(unsupported(
            subject,
            "a parameter value that is not a scalar",
        ));
    };
    if let Some(body) = text.strip_prefix('"') {
        return cdl_string(subject, body).map(|t| ParamValue::Literal(Literal::Text(t)));
    }
    if let Some(path) = text.strip_prefix(ENUM_PREFIX) {
        return match path.split_once('.') {
            Some((type_name, member))
                if !type_name.is_empty() && !member.is_empty() && !member.contains('.') =>
            {
                Ok(ParamValue::Literal(Literal::Member(MemberRef::parse(path))))
            }
            _ => Err(unsupported(
                subject,
                "an enumeration member path that is not `Type.Member`",
            )),
        };
    }
    if text.starts_with("Buildings.") {
        return Err(unsupported(
            subject,
            "an enumeration member outside `CDL.Types` (a library enumeration)",
        ));
    }
    if params.iter().any(|p| p.as_str() == text) {
        return Ok(ParamValue::Ref(Identifier::unchecked(text.as_str())));
    }
    Err(unsupported(
        subject,
        "a parameter value that is an expression or names no sequence parameter",
    ))
}

/// Unescape the body of a CDL string expression (after its opening quote): `\"`, `\\` and `\n`
/// only, then exactly one closing quote.
fn cdl_string(subject: &str, body: &str) -> Result<String, ImportError> {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                return if chars.next().is_none() {
                    Ok(out)
                } else {
                    Err(unsupported(
                        subject,
                        "a string expression with text after its closing quote",
                    ))
                };
            }
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                _ => {
                    return Err(unsupported(
                        subject,
                        "a string escape other than \\\", \\\\ or \\n",
                    ));
                }
            },
            _ => out.push(ch),
        }
    }
    Err(unsupported(subject, "an unterminated string expression"))
}

/// Parse a `sizeOfDimensions` spelling, `(3)` or `(2, 3)`.
pub(super) fn dimensions(subject: &str, spelled: &str) -> Result<Vec<usize>, ImportError> {
    let inner = spelled
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| {
            unsupported(
                subject,
                "a `sizeOfDimensions` that is not `(n)` or `(n, m)`",
            )
        })?;
    let parsed: Option<Vec<usize>> = inner
        .split(',')
        .map(|part| part.trim().parse::<usize>().ok().filter(|&n| n > 0))
        .collect();
    match parsed {
        Some(dims) if (1..=2).contains(&dims.len()) => Ok(dims),
        _ => Err(unsupported(
            subject,
            "a `sizeOfDimensions` that is not one or two positive sizes",
        )),
    }
}
