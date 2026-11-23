//! The typed IO / point inventory (`08` §6). Built once at load from the flat model connectors;
//! **the tick never reads it** (OBC §7.17). It is the host's metadata surface for point-schedule
//! export (use case 4.2.1), complete-frame bindings and latest-state output name resolution.
//!
//! The inventory derives every field that is structurally available from `oce-model` connectors
//! (path, direction, value type, min/max/unit/quantity). Semantic classifications that require
//! `oce-semantics` (physical sensor/actuator, network-vs-field classification, point-list
//! membership, trend config) use documented defaults until semantic projection is wired.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_model::{Attrs, Connector, ConnectorId, Dir, ModelGraph, ValueType};

use crate::engine::Engine;
use crate::error::OcError;
use crate::frame_inputs::{InputBinding, InputDefinition, build_inputs};

// Re-export the resolved-model point enums (R-PUB-1) so a host names one type space. These are
// the canonical direction/value-type/trend-interval enums of the plain-data load projection
// (`oce_model::resolved_model`); `oce-api` owns only the IO-class + physical-kind
// classifications that have no projection equivalent.
pub use oce_model::resolved_model::{PointDirection, PointValueType, TrendInterval};

/// AI/AO/DI/DO/Network (use case 4.2.1). `oce-api`-owned (distinct from the projection's
/// [`oce_model::resolved_model::PointType`],
/// whose variants are `Ai/Ao/Di/Do/Mode`). Derive set matches `08` §6 (no `Hash` — counted by
/// `match`, never used as a map key).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IoClass {
    /// A physical analog input (sensor).
    AnalogInput,
    /// A physical analog output (actuator).
    AnalogOutput,
    /// A physical digital input.
    DigitalInput,
    /// A physical digital output.
    DigitalOutput,
    /// A logical / inter-controller (network) point.
    Network,
}

#[cfg(test)]
mod frame_alias_tests {
    use super::*;

    #[test]
    fn two_resolver_spellings_cannot_satisfy_one_logical_input_twice() {
        // Public ingest has no input aliases. Inject an alias at the shared identity seam to
        // ensure uniqueness is by logical input, never submitted spelling, if one is added later.
        let mut engine = Engine::new();
        engine
            .load_cxf(include_bytes!("../tests/fixtures/legacy_frame_add.jsonld"))
            .unwrap();
        let canonical = "urn:legacy-frame:a";
        let binding = engine.io.input_by_path[canonical].clone();
        engine.io.input_by_path.insert("alias".to_owned(), binding);
        let before = engine.state_snapshot().unwrap();
        for entries in [
            [
                (canonical, oce_model::Value::Real(0.0)),
                ("alias", oce_model::Value::Real(0.0)),
            ],
            [
                ("alias", oce_model::Value::Real(0.0)),
                (canonical, oce_model::Value::Real(0.0)),
            ],
        ] {
            assert!(
                matches!(engine.prepare_frame(0.0, &entries), Err(OcError::FrameDuplicateInput(path)) if path == canonical)
            );
            assert_eq!(
                engine.state_snapshot().unwrap().as_bytes(),
                before.as_bytes()
            );
        }
    }
}

/// Sensor | Actuator | SoftwarePoint (CDL req 5.2.7/5.2.8). `oce-api`-owned. The current default is
/// `SoftwarePoint` everywhere; sensor/actuator classification belongs to the semantic projection.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PhysicalKind {
    /// A physical sensor input.
    Sensor,
    /// A physical actuator output.
    Actuator,
    /// A logical software point (the current default).
    SoftwarePoint,
}

/// §7.11 trend metadata. Reuses [`TrendInterval`] (the `OnChange` == interval-0 COV sentinel).
/// [`PointInfo::trend`] is currently `None` until trend config is projected from semantic metadata.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TrendCfg {
    /// The trend recording interval.
    pub interval: TrendInterval,
    /// Whether trending is enabled for this point.
    pub enabled: bool,
}

/// One typed point in the inventory (`08` §6). `Clone` so a PyO3 binder can own it (R-PUB-6). `path`
/// is an owned `String` (no `Arc<str>` / `DomainKey` leak, R-API-8). `String`-typed connectors are
/// excluded (CDL §7.8: a String signal is metadata, not a control point).
#[derive(Clone, Debug, PartialEq)]
pub struct PointInfo {
    /// The point identity == `DomainKey` identity (06 D5): the document-derived `@id` of the
    /// connector's host-visible identity — the declared boundary input's node for a
    /// composite-boundary-driven connector (one host point fans out to every internal
    /// consumer); otherwise the connector's own authored node, or a `hasInstance`-derived
    /// member identity (`<owner @id>.<declared port name>`) minted from the owner's authored
    /// identity, never from a position — expanded against the document `@context` to canonical
    /// absolute form at ingest, so compact and expanded spellings of the same subject IRI name
    /// the same point. The positional `conn#<id>` fallback appears only for hand-built,
    /// IRI-less models, which no public API can construct.
    pub path: String,
    /// `In` | `Out` (CDL §7.8).
    pub direction: PointDirection,
    /// `Real` | `Int` | `Bool` (CDL §7.4; `String` is metadata-only and excluded).
    pub value_type: PointValueType,
    /// `AI` | `AO` | `DI` | `DO` | `Network` (use case 4.2.1).
    pub io_class: IoClass,
    /// `Sensor` | `Actuator` | `SoftwarePoint` (currently always `SoftwarePoint`).
    pub physical: PhysicalKind,
    /// Physical quantity (e.g. `"ThermodynamicTemperature"`); `None` if unset (CDL §7.4.1).
    pub quantity: Option<String>,
    /// SI computation unit; `None` if unset (OBC §7.17.1).
    pub unit: Option<String>,
    /// UI-only display unit; **never** affects computation (OBC §7.4.1 / §7.17).
    pub display_unit: Option<String>,
    /// Lower validation bound, if declared.
    pub min: Option<f64>,
    /// Upper validation bound, if declared.
    pub max: Option<f64>,
    /// Effective §7.7.5 point-list membership (currently always `true`).
    pub in_pointlist: bool,
    /// §7.11 hardwired-connection flag (currently always `false`).
    pub hardwired: bool,
    /// §7.11 trend metadata (currently always `None`).
    pub trend: Option<TrendCfg>,
}

/// Counts by [`IoClass`] + total (`08` §3/§6). `Copy + Clone + Default`; lands in `LoadReport.io`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct IoSummary {
    /// Number of analog-input points.
    pub analog_inputs: usize,
    /// Number of analog-output points.
    pub analog_outputs: usize,
    /// Number of digital-input points.
    pub digital_inputs: usize,
    /// Number of digital-output points.
    pub digital_outputs: usize,
    /// Number of logical/network points.
    pub network: usize,
    /// Total points in the inventory.
    pub total: usize,
}

/// The typed IO surface, built at load from the model connectors. Dense arena keyed by point path;
/// storage-free, owned by `Engine` (a [`crate::LoadedModel`] carries an owned copy). Private bindings resolve frames and latest-state reads by
/// [`ConnectorId`] without leaking the mutable arena. Composite boundary inputs
/// may fan out to multiple internal input connectors; the host still sees one logical point path,
/// while `input_by_path` preserves every staging target at load time.
#[derive(Clone, Debug, Default)]
pub struct IoInventory {
    points: Vec<PointInfo>,
    input_by_path: HashMap<String, InputBinding>,
    pub(crate) frame_definitions: Vec<InputDefinition>,
    /// The executable root boundary, lexical by authored identity; not the point projection.
    pub(crate) frame_outputs: Vec<(String, ConnectorId)>,
    output_by_path: HashMap<String, ConnectorId>,
    /// Declared boundary-output alias keys (`_spec/18` R18-2): the authored root `hasOutput`
    /// IRI of each elided boundary output, resolving to its driving connector's slot. Read-only
    /// aliases — they never enter `points`, so inventory rows and summaries are untouched.
    /// A pass-through declared output needs no entry here: its
    /// lowered output connector already carries the declared IRI in `output_by_path`.
    output_alias_by_path: HashMap<String, ConnectorId>,
}

#[derive(Clone, Debug)]
pub(crate) struct PointInventoryRow {
    pub(crate) info: PointInfo,
    pub(crate) connector_id: ConnectorId,
}

/// The canonical point path of a connector: the document-derived `@id` of its host-visible
/// identity — the declared boundary input's node for a composite-boundary-driven connector;
/// otherwise the connector's own authored node, or a `hasInstance`-derived member identity
/// minted from the owner's authored `@id` and the class signature (the resolver stamps the
/// choice into `Connector::iri` at ingest).
///
/// The stored `@id` is the canonical absolute subject IRI: the resolver expands the authored
/// spelling against the document `@context` at ingest, so a document re-serialized between
/// compact and expanded spellings keeps its point paths, and a relative `@id` no context can
/// canonicalize is refused at load with a typed diagnostic rather than admitted under a
/// spelling-dependent key. CXF ingest rejects any connector node without an `@id` and mints a
/// document-derived identity for every node-less derived connector, so every document-loaded
/// connector has a document-derived identity and the positional `conn#<id>` spelling is
/// unreachable from the public API. It survives only as the fallback for hand-built, IRI-less
/// models (`Connector::new` without `with_iri`), which are constructible from in-crate tests
/// alone. The distinction matters: a positional index renumbers under JSON-LD `@graph`
/// reordering, an authored `@id` does not.
#[must_use]
pub(crate) fn connector_path(iri: Option<&str>, id: ConnectorId) -> String {
    iri.map(str::to_owned)
        .unwrap_or_else(|| format!("conn#{}", id.0))
}

/// The load-time point walk, every connector's row in arena order. The inventory and the
/// projection walk the connectors themselves and ask [`point_row`] only for the rows they keep,
/// so a connector either of them skips first costs no copy of its path.
#[cfg(test)]
pub(crate) fn point_rows_at_load(model: &ModelGraph) -> impl Iterator<Item = PointInventoryRow> {
    model.connectors.iter().filter_map(point_row)
}

/// The point row of one connector, or `None` for a `String` connector, which is metadata-only
/// (CDL §7.8) and not a control point. It is the single source of truth for both host-visible
/// [`IoInventory`] rows and the plain-data [`oce_model::resolved_model::PointDto`] projection.
pub(crate) fn point_row(c: &Connector) -> Option<PointInventoryRow> {
    point_row_with_path(c, || connector_path(c.iri.as_deref(), c.id))
}

/// [`point_row`] with the path `path` makes, called only for a connector that is a point: a
/// caller that already holds the path passes an empty one and copies nothing.
pub(crate) fn point_row_with_path(
    c: &Connector,
    path: impl FnOnce() -> String,
) -> Option<PointInventoryRow> {
    let value_type = match c.value_type {
        ValueType::Real => PointValueType::Real,
        ValueType::Integer | ValueType::Enum(_) => PointValueType::Int,
        ValueType::Boolean => PointValueType::Bool,
        ValueType::String => return None,
    };
    let direction = match c.dir {
        Dir::In => PointDirection::In,
        Dir::Out => PointDirection::Out,
    };
    // Current IO-class mapping from direction + signal type. Bool ⇒ digital; else analog.
    // Network/physical classification belongs to the semantic projection (R-IO-1).
    let io_class = match (direction, value_type) {
        (PointDirection::In, PointValueType::Bool) => IoClass::DigitalInput,
        (PointDirection::In, _) => IoClass::AnalogInput,
        (PointDirection::Out, PointValueType::Bool) => IoClass::DigitalOutput,
        (PointDirection::Out, _) => IoClass::AnalogOutput,
    };
    let (quantity, unit, display_unit, min, max) = match &c.attrs {
        Attrs::Real(a) => (
            a.quantity.as_deref().map(str::to_owned),
            a.unit.as_deref().map(str::to_owned),
            a.display_unit.as_deref().map(str::to_owned),
            a.min,
            a.max,
        ),
        Attrs::Integer(a) => (
            None,
            None,
            None,
            a.min.map(|v| v as f64),
            a.max.map(|v| v as f64),
        ),
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => (None, None, None, None, None),
    };
    Some(PointInventoryRow {
        info: PointInfo {
            path: path(),
            direction,
            value_type,
            io_class,
            physical: PhysicalKind::SoftwarePoint,
            quantity,
            unit,
            display_unit,
            min,
            max,
            in_pointlist: true,
            hardwired: false,
            trend: None,
        },
        connector_id: c.id,
    })
}

/// The boundary-driven input connectors of `model`, as a set: one pass over `external_inputs`, so
/// a walk over the connectors asks membership at a fixed cost rather than searching the list for
/// each connector, which was quadratic in a boundary's fan-out (issue #811).
pub(crate) fn external_input_set(model: &ModelGraph) -> HashSet<ConnectorId> {
    model.external_inputs.iter().copied().collect()
}

/// The address of a connector's shared path allocation, if it has one.
///
/// Every input a boundary input drives holds that boundary's IRI as one shared allocation, so two
/// connectors with the same address spell the same path. A walk that has handled one of them can
/// recognise the others by address alone, without copying or hashing the path again: a boundary
/// fanned out to `n` inputs then costs its path once, not `n` times (issue #811). Equal addresses
/// imply equal paths; unequal addresses imply nothing, and such a walk compares the text.
pub(crate) fn shared_path_address(c: &Connector) -> Option<*const u8> {
    c.iri.as_ref().map(|iri| Arc::as_ptr(iri).cast::<u8>())
}

impl IoInventory {
    /// Every staging target of the boundary input `path`, in load order, or `None` for a path that
    /// is not an input binding.
    pub(crate) fn input_targets(&self, path: &str) -> Option<&[ConnectorId]> {
        self.input_by_path
            .get(path)
            .map(|binding| binding.targets.as_slice())
    }

    /// Build the inventory from a flattened, structurally-valid [`ModelGraph`]. `String`-typed
    /// connectors are skipped (metadata-only, CDL §7.8). Pure function of the model — no map order
    /// feeds a position (connectors are walked in arena order).
    pub(crate) fn build_at_load(model: &ModelGraph) -> IoInventory {
        let mut points = Vec::with_capacity(model.connectors.len());
        let (input_by_path, frame_definitions) = build_inputs(model);
        let external_inputs = external_input_set(model);
        let mut seen_inputs = HashSet::new();
        // The shared path allocations of inputs already walked: a boundary-driven input whose
        // allocation is here repeats a path `seen_inputs` holds, and is skipped as the text
        // comparison below would skip it, without building its row.
        let mut seen_shared_inputs = HashSet::new();
        let mut output_by_path = HashMap::with_capacity(model.connectors.len());
        for connector in &model.connectors {
            let is_external = external_inputs.contains(&connector.id);
            if connector.dir == Dir::In
                && is_external
                && shared_path_address(connector)
                    .is_some_and(|address| seen_shared_inputs.contains(&address))
            {
                continue;
            }
            let Some(row) = point_row(connector) else {
                continue;
            };
            let path = row.info.path.clone();
            if row.info.direction == PointDirection::In {
                let first = seen_inputs.insert(path.clone());
                if let Some(address) = shared_path_address(connector) {
                    seen_shared_inputs.insert(address);
                }
                if !first && is_external {
                    continue;
                }
            } else {
                output_by_path
                    .entry(path.clone())
                    .or_insert(row.connector_id);
            }
            points.push(row.info);
        }
        // Declared boundary-output aliases (R18-2). Keyed only when the driving connector is an
        // inventory output point (a non-String `Out`), so an alias can never resolve to a slot
        // the point surfaces exclude. CXF ingest refuses a declared IRI that collides with a
        // connector path (`BoundaryOutputShadowsConnector`), so for document-loaded models the
        // alias key space is disjoint from `output_by_path`; `resolve_output` still consults
        // connector paths first so the lookup stays defined against a malformed hand-built
        // store.
        let mut output_alias_by_path = HashMap::with_capacity(model.boundary_outputs.len());
        // Whether each driving connector's path is an inventory output, asked once a driver: one
        // driver may serve many declared outputs, and its path is looked up once, not once each.
        let mut driver_is_output: HashMap<ConnectorId, bool> = HashMap::new();
        for boundary in &model.boundary_outputs {
            // `get` (not indexing) keeps this total for a malformed hand-built graph (R-ERR-1);
            // an out-of-range source simply mints no alias.
            let Some(source) = model.connectors.get(boundary.source.0 as usize) else {
                continue;
            };
            let is_output = *driver_is_output.entry(boundary.source).or_insert_with(|| {
                output_by_path.contains_key(&connector_path(source.iri.as_deref(), source.id))
            });
            if is_output {
                output_alias_by_path
                    .entry(boundary.iri.to_string())
                    .or_insert(boundary.source);
            }
        }
        IoInventory {
            points,
            input_by_path,
            frame_definitions,
            frame_outputs: executable_boundary_outputs(model),
            output_by_path,
            output_alias_by_path,
        }
    }

    /// Identity resolver for complete-frame preparation.
    pub(crate) fn input_binding(&self, path: &str) -> Option<&InputBinding> {
        self.input_by_path.get(path)
    }

    /// Resolve a point path to its [`ConnectorId`] **only if it is an output** — the shared
    /// resolver behind latest-state `get_output` and `watch` inspection.
    ///
    /// Two key spaces resolve here, connector paths first, then declared boundary-output
    /// aliases (`_spec/18` R18-2). The order is defensive: ingest refuses a declared IRI that
    /// shadows a connector path, so the two spaces are disjoint for every document-loaded
    /// model, and the ordering only keeps the lookup defined against a malformed hand-built
    /// one. An alias resolves to the driving connector's slot, so a declared name and its
    /// driver path return bit-equal values.
    pub(crate) fn resolve_output(&self, path: &str) -> Option<ConnectorId> {
        self.output_by_path
            .get(path)
            .or_else(|| self.output_alias_by_path.get(path))
            .copied()
    }

    /// Every input point as `(canonical path, value type)`, in inventory order. This is the
    /// plain-data view a host hands its own persistence or point-binding layer; the engine never
    /// resolves handles for it. Borrows only; allocates nothing; never panics.
    pub fn input_points(&self) -> impl Iterator<Item = (&str, PointValueType)> + '_ {
        self.points
            .iter()
            .filter(|p| p.direction == PointDirection::In)
            .map(|p| (p.path.as_str(), p.value_type))
    }

    /// Owned iterator over every [`PointInfo`] (R-PUB-6; a PyO3 binder copies without the borrow).
    pub fn iter(&self) -> impl Iterator<Item = PointInfo> + '_ {
        self.points.iter().cloned()
    }

    /// Eager owned snapshot of the inventory (R-PUB-6).
    #[must_use]
    pub fn to_vec(&self) -> Vec<PointInfo> {
        self.points.clone()
    }

    /// Number of points in the inventory.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether the inventory has no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The point at index `idx` (declaration/inventory order), if in range.
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<&PointInfo> {
        self.points.get(idx)
    }

    /// Count points by [`IoClass`] into an [`IoSummary`] (the `LoadReport.io` summary).
    #[must_use]
    pub(crate) fn summary(&self) -> IoSummary {
        let mut s = IoSummary::default();
        for p in &self.points {
            match p.io_class {
                IoClass::AnalogInput => s.analog_inputs += 1,
                IoClass::AnalogOutput => s.analog_outputs += 1,
                IoClass::DigitalInput => s.digital_inputs += 1,
                IoClass::DigitalOutput => s.digital_outputs += 1,
                IoClass::Network => s.network += 1,
            }
        }
        s.total = self.points.len();
        s
    }

    /// The in-memory point-list mirror: every point flagged `in_pointlist` (CDL §7.7.5).
    pub(crate) fn point_list_mirror(&self) -> Vec<PointInfo> {
        self.points
            .iter()
            .filter(|p| p.in_pointlist)
            .cloned()
            .collect()
    }
}

/// Same disjoint union as `Topology::boundary_outputs`: elided root declarations and lowered
/// pass-through outputs. One driver may serve multiple declarations; never deduplicate by driver
/// or append internal connector aliases. Ingest guarantees distinct declared identities.
fn executable_boundary_outputs(model: &ModelGraph) -> Vec<(String, ConnectorId)> {
    let mut columns: Vec<_> = model
        .boundary_outputs
        .iter()
        .map(|output| (output.iri.to_string(), output.source))
        .chain(
            model
                .blocks
                .iter()
                .filter(|block| block.class_iri.starts_with("urn:oce:lowering#"))
                .filter_map(|block| block.outputs.first())
                .map(|id| {
                    (
                        connector_path(model.connectors[id.0 as usize].iri.as_deref(), *id),
                        *id,
                    )
                }),
        )
        .collect();
    columns.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    columns
}

impl Engine {
    /// The full typed IO inventory (host GUI / point-schedule export, use case 4.2.1).
    #[must_use]
    pub fn io(&self) -> &IoInventory {
        &self.io
    }

    /// Counts by [`IoClass`] + total — the `LoadReport.io` summary.
    #[must_use]
    pub fn io_summary(&self) -> IoSummary {
        self.io.summary()
    }

    /// The effective point list (CDL §7.7.5). `None` returns the full in-memory inventory mirror
    /// (R-IO-3). Device filtering (`Some`) is outside the supported profile and is refused
    /// directly; the engine has no equipment index to delegate to.
    ///
    /// # Errors
    /// [`OcError::Load`] for any unsupported `controlled_device` query; `point_list(None)` is
    /// infallible. Never panics (R-ERR-1).
    pub fn point_list(&self, controlled_device: Option<&str>) -> Result<Vec<PointInfo>, OcError> {
        match controlled_device {
            None => Ok(self.io.point_list_mirror()),
            Some(dev) => Err(OcError::Load {
                detail: format!(
                    "point_list(controlled_device={dev:?}) requires the §7.7.5 equipment traversal \
                     (oce-semantics equipment index), deferred to M2; call \
                     point_list(None) for the full in-memory inventory mirror"
                ),
            }),
        }
    }
}
