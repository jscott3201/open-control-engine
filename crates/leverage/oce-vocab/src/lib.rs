#![forbid(unsafe_code)]
//! `oce-vocab`: the block vocabulary (design note `docs/design/vocabulary.md`, GitHub issue
//! #499).
//!
//! Reference text and unit facts about the 133 authorable block classes, served beside the
//! engine's catalog: for each class its upstream links, for each connector and parameter a
//! meaning, the unit and quantity the CDL source declares, and curated notes for agents. The
//! readers are agents (tool descriptions, repair messages), editor hover text and card
//! generators.
//!
//! It is **not engine behaviour**. Nothing on the load path, the emitter, the compiled-schedule
//! blob, the content id or a class signature hash reads it, and `oce-api` is unchanged by it. It
//! never restates what the engine owns: port names and kinds, parameter types, defaults and
//! ranges are read from `oce_api::catalog()`, `class_metadata()` and `class_ports()` when a view
//! is built, never copied into the data.
//!
//! # The data
//!
//! Two checked-in files, embedded with `include_str!` and parsed once:
//!
//! - [`GENERATED_JSON`] (`data/generated.json`): projected from the vendored modelica-json
//!   `CXF-Core.jsonld` through the catalog's own names by the crate's regenerate-and-diff test.
//!   Never edited by hand; `OCE_BLESS=1` rewrites it.
//! - [`CURATED_JSON`] (`data/curated.json`): written by people. Additive in v1: typed notes
//!   ([`NoteKind`]), and a description only where the generated one is empty.
//!
//! Both are strict JSON in one canonical spelling ([`GeneratedVocabulary::to_json`]); a read
//! refuses anything else with a typed [`VocabError`].
//!
//! # The view
//!
//! [`vocabulary()`] is the merged, checked pair, and [`class_vocab`] one class of it joined with
//! the catalog: [`ClassVocab`]. A resolved port name maps to its connector with
//! [`ClassVocab::connector_of_port`] (`u3` is element 3 of the vector connector `u`), and a
//! catalog parameter name to its entry with [`parameter_base_name`].
//!
//! ```
//! let delay = oce_vocab::class_vocab("CDL.Logical.TrueDelay").expect("an authorable class");
//! let delay_time = &delay.parameters[0];
//! assert_eq!(delay_time.name, "delayTime");
//! assert_eq!(delay_time.unit, "s");
//! assert_eq!(delay_time.quantity, "Time");
//! assert!(delay.notes_about("delayOnInit").next().is_some());
//! ```
//!
//! The crate sits above the facade: it depends on `oce-api` only, loads no engine and evaluates
//! nothing.

mod data;
mod error;
mod json;
mod naming;
mod view;
mod vocabulary;

use std::sync::OnceLock;

pub use data::{
    Connector, CuratedClass, CuratedDescription, CuratedVocabulary, GeneratedClass,
    GeneratedVocabulary, MAX_VOCAB_BYTES, MAX_VOCAB_STRING_BYTES, Note, NoteKind, Parameter, Side,
    Source, VOCAB_FORMAT,
};
pub use error::VocabError;
pub use naming::{PortElement, parameter_base_name, port_element};
pub use view::{CLASS_BASE, ClassLinks, ClassVocab, ConnectorVocab, ParameterVocab, TextOrigin};
pub use vocabulary::Vocabulary;

/// The checked-in `data/generated.json`, byte for byte.
pub const GENERATED_JSON: &str = include_str!("../data/generated.json");

/// The checked-in `data/curated.json`, byte for byte.
pub const CURATED_JSON: &str = include_str!("../data/curated.json");

/// The embedded vocabulary, merged and checked against the facade's catalog.
///
/// The first call parses [`GENERATED_JSON`] and [`CURATED_JSON`] and allocates once; later calls
/// return the same value. It mutates nothing and loads no engine.
///
/// # Panics
///
/// If the embedded files are malformed, disagree with each other or disagree with the catalog
/// this crate was built against ([`Vocabulary::from_json`] refuses them). The crate's tests
/// prove they do not, the same posture as `oce_api::catalog()`.
#[must_use]
pub fn vocabulary() -> &'static Vocabulary {
    static VOCABULARY: OnceLock<Vocabulary> = OnceLock::new();
    VOCABULARY.get_or_init(|| {
        Vocabulary::from_json(GENERATED_JSON, CURATED_JSON)
            .unwrap_or_else(|error| panic!("the embedded vocabulary is refused: {error}"))
    })
}

/// The vocabulary of one class joined with the catalog: its description, links, connectors,
/// parameters and notes.
///
/// `class_path` is relative to `Buildings.Controls.OBC.` (`CDL.Reals.PID`), exactly as the
/// catalog spells it. `None` for an unknown path and for the engine-reserved lowering
/// identities, which no author can write.
///
/// # Panics
///
/// As [`vocabulary()`].
#[must_use]
pub fn class_vocab(class_path: &str) -> Option<ClassVocab<'static>> {
    vocabulary().class(class_path)
}
