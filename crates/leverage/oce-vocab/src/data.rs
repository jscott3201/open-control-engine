//! The two vocabulary files as typed data: `generated.json` and `curated.json`.
//!
//! Both are strict JSON under the authoring wire profile (design note `docs/design/vocabulary.md`,
//! "The data"): closed objects, every key present, absent is `""` or `[]`, no `null`, and one
//! canonical spelling (see [`GeneratedVocabulary::to_json`]). A read is bounded like a wire
//! document: [`MAX_VOCAB_BYTES`] and [`MAX_VOCAB_STRING_BYTES`] are the wire reader's limits, the
//! closed shape fixes the nesting depth at five (inside its eight), and its list bound of 65,536
//! items cannot be reached, because the smallest list item is 96 bytes in the canonical spelling
//! and the byte bound allows 64 per item. A read also refuses anything that is not the canonical
//! bytes of what it parsed, so a hand edit with a misplaced key, a stray space or a `\r\n` line
//! ending fails with the line it starts on.

use serde::Deserialize;

use crate::error::VocabError;
use crate::json::Json;

/// The `format` both files carry.
pub const VOCAB_FORMAT: u32 = 1;

/// The largest vocabulary document, in bytes (4 MiB, the wire form's bound).
pub const MAX_VOCAB_BYTES: usize = 4 * 1024 * 1024;

/// The longest string in a vocabulary document, in bytes (the wire form's bound).
pub const MAX_VOCAB_STRING_BYTES: usize = 4_096;

/// `generated.json`: what the regenerate-and-diff test projects from the vendored CXF-Core
/// document through the catalog's names. Never edited by hand.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedVocabulary {
    /// One entry per authorable catalog class, in catalog order.
    pub classes: Vec<GeneratedClass>,
    /// [`VOCAB_FORMAT`].
    pub format: u32,
    /// Where the text came from.
    pub source: Source,
}

/// The vendored document `generated.json` was projected from, and the two upstream pins that
/// produced it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The Modelica Buildings commit, 40 lowercase hexadecimal digits.
    pub buildings_commit: String,
    /// The vendored document's repository-relative path.
    pub document: String,
    /// The modelica-json commit, 40 lowercase hexadecimal digits.
    pub modelica_json_commit: String,
    /// SHA-256 of the vendored document's bytes, 64 lowercase hexadecimal digits.
    pub sha256: String,
}

/// One class of `generated.json`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedClass {
    /// The catalog class path (`CDL.Reals.PID`), relative to `Buildings.Controls.OBC.`.
    #[serde(rename = "class")]
    pub class_path: String,
    /// The class's connectors, inputs then outputs, each side in declaration order.
    pub connectors: Vec<Connector>,
    /// The class's parameters in the catalog's order.
    pub parameters: Vec<Parameter>,
    /// Whether the vendored document has the class. False only for a class the engine carries
    /// and current Buildings does not (`CDL.Logical.TrueHoldWithReset`); such a class has no
    /// upstream links and every description of it is curated.
    pub upstream: bool,
}

/// Which side of a block a connector is on. Inputs order before outputs, as the files list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// An input connector.
    Input,
    /// An output connector.
    Output,
}

impl Side {
    /// The spelling in the data files: `input` or `output`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

/// One connector of a class: a declared CDL connector, not a resolved port. A vector connector
/// `u[nin]` is one entry with `dims == 1`; the engine's ports `u1` … `un` are its elements
/// (see [`crate::port_element`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    /// What the connector carries, as upstream describes it; `""` where the vendored document
    /// has no text for it.
    pub description: String,
    /// 0 for a scalar, 1 for a vector, 2 for a matrix connector.
    pub dims: u8,
    /// The CDL connector name (`u_s`, `u`, `yIdx`).
    pub name: String,
    /// The declared Modelica `quantity` (`Time`, `ThermodynamicTemperature`), or `""`.
    pub quantity: String,
    /// Input or output.
    pub side: Side,
    /// The declared Modelica `unit` as the CDL source spells it (`s`, `K`, `1`), or `""`. Most
    /// CDL connectors declare none.
    pub unit: String,
}

/// One parameter of a class.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// What the parameter means, as upstream describes it; `""` where the vendored document
    /// has no text for it.
    pub description: String,
    /// The CDL parameter name. An array parameter the catalog spells as a template (`k_<i>`,
    /// `K_<row>_<col>`) is listed once under its base name (`k`, `K`); see
    /// [`crate::parameter_base_name`].
    pub name: String,
    /// The declared Modelica `quantity`, or `""`.
    pub quantity: String,
    /// The declared Modelica `unit` as the CDL source spells it, or `""`.
    pub unit: String,
}

/// `curated.json`: what people write. Regeneration never touches it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratedVocabulary {
    /// The curated classes, in catalog order; a class with nothing curated is absent.
    pub classes: Vec<CuratedClass>,
    /// [`VOCAB_FORMAT`].
    pub format: u32,
}

/// One class of `curated.json`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratedClass {
    /// The catalog class path.
    #[serde(rename = "class")]
    pub class_path: String,
    /// Descriptions for connectors and parameters whose generated description is `""`.
    pub descriptions: Vec<CuratedDescription>,
    /// Typed notes about the class or one of its connectors or parameters.
    pub notes: Vec<Note>,
}

/// A curated description of one connector or parameter.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratedDescription {
    /// Where the description comes from. Shown to reviewers, not to agents.
    pub evidence: String,
    /// The connector or parameter name, as `generated.json` lists it.
    pub subject: String,
    /// The description. One sentence, plain text.
    pub text: String,
}

/// What a note is about, so a tool can pick the notes that matter to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteKind {
    /// A library convention that differs from the class default (`delayOnInit = true`).
    Convention,
    /// Something that looks right and is not.
    Pitfall,
    /// Which input is which (`Subtract` gives `u1 - u2`).
    PortOrder,
    /// Whether a feedback loop through the class alone is an algebraic loop.
    Loop,
    /// A unit convention the declarations do not carry.
    Units,
}

impl NoteKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 5] = [
        Self::Convention,
        Self::Pitfall,
        Self::PortOrder,
        Self::Loop,
        Self::Units,
    ];

    /// The spelling in `curated.json`: `convention`, `pitfall`, `port-order`, `loop`, `units`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Convention => "convention",
            Self::Pitfall => "pitfall",
            Self::PortOrder => "port-order",
            Self::Loop => "loop",
            Self::Units => "units",
        }
    }
}

/// A typed, curated note.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    /// Where the claim comes from. Shown to reviewers, not to agents.
    pub evidence: String,
    /// What the note is about.
    pub kind: NoteKind,
    /// The connector or parameter the note is about, or `""` for the class.
    pub subject: String,
    /// The note. Plain text an agent reads.
    pub text: String,
}

/// Parse `text` as `T`, bounded, and require it to be the canonical bytes of what it parsed.
fn read<T: for<'de> Deserialize<'de>>(
    text: &str,
    check: impl Fn(&T) -> Result<(), VocabError>,
    write: impl Fn(&T) -> String,
) -> Result<T, VocabError> {
    if text.starts_with('\u{feff}') {
        return Err(VocabError::ByteOrderMark);
    }
    if text.len() > MAX_VOCAB_BYTES {
        return Err(VocabError::TooLarge {
            bytes: text.len(),
            limit: MAX_VOCAB_BYTES,
        });
    }
    let value: T = serde_json::from_str(text).map_err(|error| VocabError::Json {
        line: error.line(),
        column: error.column(),
        detail: error.to_string(),
    })?;
    check(&value)?;
    let canonical = write(&value);
    if canonical != text {
        // Split on LF alone, so a `\r` counts as a difference on the line it ends.
        let same = canonical
            .split('\n')
            .zip(text.split('\n'))
            .take_while(|(want, got)| want == got)
            .count();
        let line = (same + 1).min(text.split('\n').count());
        return Err(VocabError::NotCanonical { line });
    }
    Ok(value)
}

/// Field checks on a parsed document.
struct Bounds;

impl Bounds {
    /// A string that may be empty.
    fn text(path: &str, text: &str) -> Result<(), VocabError> {
        if text.len() > MAX_VOCAB_STRING_BYTES {
            return Err(VocabError::StringTooLong {
                path: path.to_owned(),
                bytes: text.len(),
                limit: MAX_VOCAB_STRING_BYTES,
            });
        }
        if text.chars().any(char::is_control) {
            return Err(VocabError::ControlCharacter {
                path: path.to_owned(),
            });
        }
        if text.trim() != text {
            return Err(VocabError::SurroundingWhitespace {
                path: path.to_owned(),
            });
        }
        Ok(())
    }

    /// A string that must say something.
    fn required(path: &str, text: &str) -> Result<(), VocabError> {
        Self::text(path, text)?;
        if text.is_empty() {
            return Err(VocabError::EmptyField {
                path: path.to_owned(),
            });
        }
        Ok(())
    }

    fn format(format: u32) -> Result<(), VocabError> {
        if format == VOCAB_FORMAT {
            Ok(())
        } else {
            Err(VocabError::UnsupportedFormat { format })
        }
    }

    fn hex(field: &'static str, text: &str, digits: usize) -> Result<(), VocabError> {
        let lowercase_hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        if text.len() == digits && text.bytes().all(lowercase_hex) {
            Ok(())
        } else {
            Err(VocabError::InvalidSource { field })
        }
    }
}

impl GeneratedVocabulary {
    /// Read `generated.json`.
    ///
    /// # Errors
    ///
    /// A [`VocabError`] of the single-file group: the bounds, the closed shape
    /// ([`VocabError::Json`]), the format, a malformed `source`, a `dims` outside 0 to 2, an
    /// empty name, a string with a control character or surrounding white space, or bytes that
    /// are not the canonical spelling ([`VocabError::NotCanonical`]). Never panics.
    pub fn from_json(text: &str) -> Result<Self, VocabError> {
        read(text, Self::check, Self::to_json)
    }

    fn check(&self) -> Result<(), VocabError> {
        Bounds::format(self.format)?;
        Bounds::hex("buildings_commit", &self.source.buildings_commit, 40)?;
        Bounds::hex(
            "modelica_json_commit",
            &self.source.modelica_json_commit,
            40,
        )?;
        Bounds::hex("sha256", &self.source.sha256, 64)?;
        Bounds::required("source.document", &self.source.document)?;
        for (c, class) in self.classes.iter().enumerate() {
            let at = format!("classes[{c}]");
            Bounds::required(&format!("{at}.class"), &class.class_path)?;
            for (i, connector) in class.connectors.iter().enumerate() {
                let at = format!("{at}.connectors[{i}]");
                Bounds::required(&format!("{at}.name"), &connector.name)?;
                Bounds::text(&format!("{at}.description"), &connector.description)?;
                Bounds::text(&format!("{at}.quantity"), &connector.quantity)?;
                Bounds::text(&format!("{at}.unit"), &connector.unit)?;
                if connector.dims > 2 {
                    return Err(VocabError::InvalidDims {
                        path: format!("{at}.dims"),
                        dims: connector.dims,
                    });
                }
            }
            for (i, parameter) in class.parameters.iter().enumerate() {
                let at = format!("{at}.parameters[{i}]");
                Bounds::required(&format!("{at}.name"), &parameter.name)?;
                Bounds::text(&format!("{at}.description"), &parameter.description)?;
                Bounds::text(&format!("{at}.quantity"), &parameter.quantity)?;
                Bounds::text(&format!("{at}.unit"), &parameter.unit)?;
            }
        }
        Ok(())
    }

    /// The canonical document: keys sorted, two-space indentation, one entry per line, LF, one
    /// trailing newline, ASCII only, lists in the order held. Deterministic; allocates.
    #[must_use]
    pub fn to_json(&self) -> String {
        Json::object([
            (
                "classes",
                Json::array(&self.classes, |class| {
                    Json::object([
                        ("class", Json::str(&class.class_path)),
                        (
                            "connectors",
                            Json::array(&class.connectors, |connector| {
                                Json::object([
                                    ("description", Json::str(&connector.description)),
                                    ("dims", Json::unsigned(u32::from(connector.dims))),
                                    ("name", Json::str(&connector.name)),
                                    ("quantity", Json::str(&connector.quantity)),
                                    ("side", Json::str(connector.side.as_str())),
                                    ("unit", Json::str(&connector.unit)),
                                ])
                            }),
                        ),
                        (
                            "parameters",
                            Json::array(&class.parameters, |parameter| {
                                Json::object([
                                    ("description", Json::str(&parameter.description)),
                                    ("name", Json::str(&parameter.name)),
                                    ("quantity", Json::str(&parameter.quantity)),
                                    ("unit", Json::str(&parameter.unit)),
                                ])
                            }),
                        ),
                        ("upstream", Json::boolean(class.upstream)),
                    ])
                }),
            ),
            ("format", Json::unsigned(self.format)),
            (
                "source",
                Json::object([
                    ("buildings_commit", Json::str(&self.source.buildings_commit)),
                    ("document", Json::str(&self.source.document)),
                    (
                        "modelica_json_commit",
                        Json::str(&self.source.modelica_json_commit),
                    ),
                    ("sha256", Json::str(&self.source.sha256)),
                ]),
            ),
        ])
        .to_document()
    }
}

impl CuratedVocabulary {
    /// Read `curated.json`.
    ///
    /// # Errors
    ///
    /// A [`VocabError`] of the single-file group: the bounds, the closed shape
    /// ([`VocabError::Json`], which includes a note `kind` outside the five), the format, an
    /// empty class path, subject of a description, text or evidence, a string with a control
    /// character or surrounding white space, or bytes that are not the canonical spelling
    /// ([`VocabError::NotCanonical`]). Never panics.
    pub fn from_json(text: &str) -> Result<Self, VocabError> {
        read(text, Self::check, Self::to_json)
    }

    fn check(&self) -> Result<(), VocabError> {
        Bounds::format(self.format)?;
        for (c, class) in self.classes.iter().enumerate() {
            let at = format!("classes[{c}]");
            Bounds::required(&format!("{at}.class"), &class.class_path)?;
            for (i, description) in class.descriptions.iter().enumerate() {
                let at = format!("{at}.descriptions[{i}]");
                Bounds::required(&format!("{at}.subject"), &description.subject)?;
                Bounds::required(&format!("{at}.text"), &description.text)?;
                Bounds::required(&format!("{at}.evidence"), &description.evidence)?;
            }
            for (i, note) in class.notes.iter().enumerate() {
                let at = format!("{at}.notes[{i}]");
                Bounds::text(&format!("{at}.subject"), &note.subject)?;
                Bounds::required(&format!("{at}.text"), &note.text)?;
                Bounds::required(&format!("{at}.evidence"), &note.evidence)?;
            }
        }
        Ok(())
    }

    /// The canonical document, spelled as [`GeneratedVocabulary::to_json`] spells its own.
    #[must_use]
    pub fn to_json(&self) -> String {
        Json::object([
            (
                "classes",
                Json::array(&self.classes, |class| {
                    Json::object([
                        ("class", Json::str(&class.class_path)),
                        (
                            "descriptions",
                            Json::array(&class.descriptions, |description| {
                                Json::object([
                                    ("evidence", Json::str(&description.evidence)),
                                    ("subject", Json::str(&description.subject)),
                                    ("text", Json::str(&description.text)),
                                ])
                            }),
                        ),
                        (
                            "notes",
                            Json::array(&class.notes, |note| {
                                Json::object([
                                    ("evidence", Json::str(&note.evidence)),
                                    ("kind", Json::str(note.kind.as_str())),
                                    ("subject", Json::str(&note.subject)),
                                    ("text", Json::str(&note.text)),
                                ])
                            }),
                        ),
                    ])
                }),
            ),
            ("format", Json::unsigned(self.format)),
        ])
        .to_document()
    }
}
