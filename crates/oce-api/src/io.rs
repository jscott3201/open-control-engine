//! The typed IO / point inventory (`08` §6). Built once at load from the flat model connectors;
//! **the tick never reads it** (CDL §7.17). It is the host's metadata surface for point-schedule
//! export (use case 4.2.1) and the `set_input`/`get_output` name→connector resolver (R-IO-4).
//!
//! The inventory derives every field that is structurally available from `oce-model` connectors
//! (path, direction, value type, min/max/unit/quantity). Semantic classifications that require
//! `oce-semantics` (physical sensor/actuator, network-vs-field classification, point-list
//! membership, trend config) use documented defaults until semantic projection is wired.

use std::collections::HashMap;

use oce_model::{Attrs, ConnectorId, Dir, ModelGraph, ValueType};
use oce_store::Store;

use crate::engine::Engine;
use crate::error::OcError;

// Re-export the store-seam point DTOs (R-PUB-1) so a host names one type space. These are the
// canonical direction/value-type/trend-interval enums; `oce-api` owns only the IO-class +
// physical-kind classifications that have no store equivalent.
pub use oce_store::{PointDirection, PointValueType, TrendInterval};

/// AI/AO/DI/DO/Network (use case 4.2.1). `oce-api`-owned (distinct from `oce_store::PointType`,
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
    /// Dotted instance path == `DomainKey` identity (06 D5); `conn#<id>` for an unnamed connector.
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
    /// SI computation unit; `None` if unset (CDL §7.17.1).
    pub unit: Option<String>,
    /// UI-only display unit; **never** affects computation (CDL §7.4.1 / §7.17).
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
/// store-free, owned by `Engine`. A private parallel `conn_id` lets the engine stage `set_input` /
/// read `get_output` by [`ConnectorId`] **without leaking it** (R-API-8); `by_path` is the O(1) name
/// resolver (R-IO-4).
#[derive(Clone, Debug, Default)]
pub struct IoInventory {
    points: Vec<PointInfo>,
    conn_id: Vec<ConnectorId>,
    by_path: HashMap<String, usize>,
}

/// Load-time binding from a store point key to an input connector arena slot.
#[derive(Clone, Debug)]
pub(crate) struct InputPointBinding {
    pub(crate) path: String,
    pub(crate) connector_id: ConnectorId,
}

#[derive(Clone, Debug)]
pub(crate) struct PointInventoryRow {
    pub(crate) info: PointInfo,
    pub(crate) connector_id: ConnectorId,
}

/// The canonical point path of a connector: its `iri`, or a synthetic `conn#<id>` for an unnamed
/// (hand-built) connector. The **single** path-derivation rule, shared by the inventory and by the
/// `Outputs` snapshot, so the two never disagree (the `to_map` keying contract).
#[must_use]
pub(crate) fn connector_path(iri: Option<&str>, id: ConnectorId) -> String {
    iri.map(str::to_owned)
        .unwrap_or_else(|| format!("conn#{}", id.0))
}

/// The shared load-time point walk. It is the single source of truth for both host-visible
/// [`IoInventory`] rows and the durable [`oce_store::PointDto`] projection.
pub(crate) fn point_rows_at_load(model: &ModelGraph) -> Vec<PointInventoryRow> {
    let mut rows = Vec::with_capacity(model.connectors.len());
    for c in &model.connectors {
        let value_type = match c.value_type {
            ValueType::Real => PointValueType::Real,
            ValueType::Integer | ValueType::Enum(_) => PointValueType::Int,
            ValueType::Boolean => PointValueType::Bool,
            // A String connector is metadata-only (CDL §7.8) — not a control point.
            ValueType::String => continue,
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
        rows.push(PointInventoryRow {
            info: PointInfo {
                path: connector_path(c.iri.as_deref(), c.id),
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
        });
    }
    rows
}

impl IoInventory {
    /// Build the inventory from a flattened, structurally-valid [`ModelGraph`]. `String`-typed
    /// connectors are skipped (metadata-only, CDL §7.8). Pure function of the model — no map order
    /// feeds a position (connectors are walked in arena order).
    pub(crate) fn build_at_load(model: &ModelGraph) -> IoInventory {
        let mut points = Vec::with_capacity(model.connectors.len());
        let mut conn_id = Vec::with_capacity(model.connectors.len());
        let mut by_path = HashMap::with_capacity(model.connectors.len());
        for row in point_rows_at_load(model) {
            let path = row.info.path.clone();
            let idx = points.len();
            // First-wins on a duplicate path: a well-formed model has unique connector IRIs (the
            // resolver/validate gate rejects duplicate @id), so this is only a defensive fallback.
            by_path.entry(path.clone()).or_insert(idx);
            points.push(row.info);
            conn_id.push(row.connector_id);
        }
        IoInventory {
            points,
            conn_id,
            by_path,
        }
    }

    /// Resolve a point path to its [`ConnectorId`] **only if it is an input** (the `set_input`
    /// staging target).
    pub(crate) fn resolve_input(&self, path: &str) -> Option<ConnectorId> {
        self.by_path.get(path).and_then(|&i| {
            (self.points[i].direction == PointDirection::In).then_some(self.conn_id[i])
        })
    }

    /// Resolve a point path to its [`ConnectorId`] **only if it is an output** (the `get_output`
    /// and `CollectSpec::Named` recording target).
    pub(crate) fn resolve_output(&self, path: &str) -> Option<ConnectorId> {
        self.by_path.get(path).and_then(|&i| {
            (self.points[i].direction == PointDirection::Out).then_some(self.conn_id[i])
        })
    }

    /// The `(path, ConnectorId)` columns for every **output** point, in inventory order — the
    /// `CollectSpec::All` recording set for `simulate`.
    pub(crate) fn out_columns(&self) -> Vec<(String, ConnectorId)> {
        self.points
            .iter()
            .zip(&self.conn_id)
            .filter(|(p, _)| p.direction == PointDirection::Out)
            .map(|(p, &cid)| (p.path.clone(), cid))
            .collect()
    }

    /// The store point keys for every input connector, in inventory order. This is resolved once at
    /// load; the tick keeps only opaque handles and connector slots.
    pub(crate) fn input_bindings(&self) -> Vec<InputPointBinding> {
        self.points
            .iter()
            .zip(&self.conn_id)
            .filter(|(p, _)| p.direction == PointDirection::In)
            .map(|(p, &cid)| InputPointBinding {
                path: p.path.clone(),
                connector_id: cid,
            })
            .collect()
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

impl<S: Store> Engine<S> {
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
    /// (R-IO-3); a `controlled_device` filter requires the §7.7.5 equipment traversal
    /// (`oce-semantics` + `SemanticStore::point_list`), which is deferred.
    ///
    /// # Errors
    /// [`OcError::Load`] for a `controlled_device` query (deferred); `point_list(None)` is
    /// infallible. Never panics (R-ERR-1).
    pub fn point_list(&self, controlled_device: Option<&str>) -> Result<Vec<PointInfo>, OcError> {
        match controlled_device {
            None => Ok(self.io.point_list_mirror()),
            Some(dev) => Err(OcError::Load {
                detail: format!(
                    "point_list(controlled_device={dev:?}) requires the §7.7.5 equipment traversal \
                     (oce-semantics + SemanticStore::point_list), deferred to M2; call \
                     point_list(None) for the full in-memory inventory mirror"
                ),
            }),
        }
    }
}
