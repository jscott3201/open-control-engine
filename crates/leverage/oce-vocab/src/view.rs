//! The served view: one class's vocabulary joined with what the facade's catalog says about it.
//!
//! Nothing the engine owns is copied into the data files. A [`ClassVocab`] reads the class's
//! one-line description and each parameter's type, default and settable flag from
//! `oce_api::class_metadata()` when it is built, and takes only meanings, declared units and
//! quantities, links and notes from the vocabulary (design note decisions 8 and 10).

use oce_api::ClassParameter;

use crate::data::{Note, Side};
use crate::naming::{PortElement, port_element};

/// The prefix every catalog class path is relative to.
pub const CLASS_BASE: &str = "Buildings.Controls.OBC.";

/// Which file a description came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOrigin {
    /// `generated.json`: upstream's own description string.
    Generated,
    /// `curated.json`: written by a person where upstream has no text.
    Curated,
}

/// Where to read more about an upstream class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassLinks {
    /// The class's Modelica source on GitHub at the pinned Buildings commit. Stable: it is the
    /// source the vocabulary was generated from.
    pub pinned_source: String,
    /// The class's page in the latest Buildings release documentation. It follows upstream
    /// releases, so **it may drift from the pin**: a description there can differ from the
    /// vocabulary's, and a class can be renamed or removed.
    pub latest_help: String,
}

impl ClassLinks {
    /// The links of the upstream class `class_path` (relative to [`CLASS_BASE`]) at
    /// `buildings_commit`. Pure string construction; it does not check that either page exists.
    #[must_use]
    pub fn new(class_path: &str, buildings_commit: &str) -> Self {
        let qualified = format!("{CLASS_BASE}{class_path}");
        let file = qualified.replace('.', "/");
        let package = qualified
            .rsplit_once('.')
            .map_or(qualified.as_str(), |(package, _)| package)
            .replace('.', "_");
        Self {
            pinned_source: format!(
                "https://github.com/lbl-srg/modelica-buildings/blob/{buildings_commit}/{file}.mo"
            ),
            latest_help: format!(
                "https://simulationresearch.lbl.gov/modelica/releases/latest/help/{package}.html#{qualified}"
            ),
        }
    }
}

/// One connector of a class, with its merged description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorVocab<'a> {
    /// The CDL connector name.
    pub name: &'a str,
    /// Input or output.
    pub side: Side,
    /// 0 for a scalar, 1 for a vector, 2 for a matrix connector.
    pub dims: u8,
    /// What the connector carries. Never empty.
    pub description: &'a str,
    /// Which file the description came from.
    pub origin: TextOrigin,
    /// The declared Modelica unit (`s`, `K`, `1`), or `""` when the source declares none.
    pub unit: &'a str,
    /// The declared Modelica quantity, or `""`.
    pub quantity: &'a str,
}

/// One parameter of a class: the catalog's facts and the vocabulary's text.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterVocab<'a> {
    /// The CDL parameter name (`k` for the catalog's template `k_<i>`).
    pub name: &'a str,
    /// The catalog's entry: its name or array template, value type, default and settable flag.
    /// Defaults and ranges are read from the catalog, never from the vocabulary.
    pub catalog: &'static ClassParameter,
    /// What the parameter means. Never empty.
    pub description: &'a str,
    /// Which file the description came from.
    pub origin: TextOrigin,
    /// The declared Modelica unit, or `""` when the source declares none.
    pub unit: &'a str,
    /// The declared Modelica quantity, or `""`.
    pub quantity: &'a str,
}

/// The vocabulary of one class, joined with the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassVocab<'a> {
    /// The catalog class path, relative to [`CLASS_BASE`].
    pub class_path: &'a str,
    /// The class's one-line description, from `oce_api::class_metadata()`.
    pub description: &'static str,
    /// Whether the vendored upstream document has the class.
    pub upstream: bool,
    /// Upstream links; `None` for a class upstream does not have.
    pub links: Option<ClassLinks>,
    /// Connectors, inputs then outputs, each side in declaration order.
    pub connectors: Vec<ConnectorVocab<'a>>,
    /// Parameters in the catalog's order.
    pub parameters: Vec<ParameterVocab<'a>>,
    /// Curated notes about the class and its connectors and parameters, in file order.
    pub notes: &'a [Note],
}

impl<'a> ClassVocab<'a> {
    /// The connector on `side` that the resolved port `port_name` belongs to, and which element
    /// of it the port is: `u3` of a `MultiSum` is element 3 of the vector connector `u`.
    ///
    /// `port_name` is a name `oce_api::class_ports` gives for this class. `None` when no
    /// connector on that side has such an element, or when more than one does (which the
    /// crate's tests rule out for every class at every width they resolve). Pure.
    #[must_use]
    pub fn connector_of_port(
        &self,
        side: Side,
        port_name: &str,
    ) -> Option<(&ConnectorVocab<'a>, PortElement)> {
        let mut matches = self.connectors.iter().filter_map(|connector| {
            if connector.side != side {
                return None;
            }
            port_element(connector.name, connector.dims, port_name)
                .map(|element| (connector, element))
        });
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// The notes whose subject is `subject`: a connector or parameter name, or `""` for the
    /// notes about the class as a whole. In file order.
    pub fn notes_about<'s>(&'s self, subject: &'s str) -> impl Iterator<Item = &'a Note> + 's {
        self.notes
            .iter()
            .filter(move |note| note.subject == subject)
    }
}
