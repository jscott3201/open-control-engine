#![forbid(unsafe_code)]
//! `oce-semantics` — effective-metadata resolution for the Open Control Engine
//! (`05-semantics-and-point-graph.md`).
//!
//! The active M3 resolver derives the *effective* per-point metadata the store projects from the
//! data currently carried by `oce-model`: connector attributes, direction, value type, and
//! defaults. It records point type AI/AO/DI/DO/Mode, hardwired flag, trend interval with the
//! `interval=0` on-change sentinel, quantity/unit, and description. Raw `__cdl(...)`/`__CDL(...)`
//! vendor annotation parsing and higher-overrides-lower propagation are deferred until those
//! annotation blobs are carried through ingest. The resolved metadata is **non-computational** data
//! (CDL §7.17): it is never read on the tick. The crate is **Group A** (no store, no database).
//!
//! The metadata enums and error seam are reserved; the annotation resolver is deferred to the
//! semantic projection layer.

use std::sync::Arc;

use oce_diag::Diagnostic;
use oce_model::{Attrs, ConnectorId, Dir, ModelGraph, ValueType};

/// The point type inferred for a connector (`05` §5; `06` §2.1). `Mode` is an open-control
/// extension to the §7.7.5 Table 7.3 column (collapsed to AI/AO by direction in a conformant
/// export).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PointType {
    /// Analog input (`RealInput`).
    Ai,
    /// Analog output (`RealOutput`).
    Ao,
    /// Digital input (`BooleanInput`).
    Di,
    /// Digital output (`BooleanOutput`).
    Do,
    /// Operating-mode signal (an Integer connector whose quantity/enum indicates a mode).
    Mode,
}

/// A stable resolver key for one connector's effective metadata.
///
/// CXF-loaded connectors use their source IRI; hand-built/no-IRI connectors use their dense
/// [`ConnectorId`] as a deterministic fallback inside this side table. The store projection maps
/// the same rule onto its `DomainKey` string space.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PointKey {
    /// Source connector IRI.
    Iri(Arc<str>),
    /// In-memory connector id fallback for models with no source IRI.
    Connector(ConnectorId),
}

impl PointKey {
    /// Build the resolver key for a connector identity pair.
    #[must_use]
    pub fn from_connector(iri: Option<&Arc<str>>, id: ConnectorId) -> Self {
        iri.cloned().map(Self::Iri).unwrap_or(Self::Connector(id))
    }
}

/// Trend interval resolved from CDL semantic metadata. `OnChange` is the `interval=0` COV
/// sentinel and is kept distinct from periodic intervals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrendInterval {
    /// Record on every value change (`interval=0`).
    OnChange,
    /// Record every non-zero N seconds. Use [`TrendSpec::from_interval_seconds`] to map raw
    /// `0` values to [`TrendInterval::OnChange`].
    EverySeconds(u32),
}

/// Effective trend configuration for one point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TrendSpec {
    /// The resolved interval.
    pub interval: TrendInterval,
    /// Whether trending is enabled.
    pub enabled: bool,
}

impl TrendSpec {
    /// Construct a trend spec from the raw CDL `interval` seconds value.
    #[must_use]
    pub fn from_interval_seconds(interval: u32, enabled: bool) -> Self {
        let interval = if interval == 0 {
            TrendInterval::OnChange
        } else {
            TrendInterval::EverySeconds(interval)
        };
        Self { interval, enabled }
    }
}

impl Default for TrendSpec {
    fn default() -> Self {
        Self::from_interval_seconds(0, false)
    }
}

/// Effective non-computational metadata for one store-visible point.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EffectivePoint {
    /// Stable metadata key.
    pub key: PointKey,
    /// The connector this metadata describes.
    pub connector_id: ConnectorId,
    /// AI/AO/DI/DO/Mode classification.
    pub point_type: PointType,
    /// Effective §7.11 hardwired flag.
    pub hardwired: bool,
    /// Effective §7.11 trend configuration.
    pub trend: TrendSpec,
    /// Physical quantity, if declared.
    pub quantity: Option<Arc<str>>,
    /// Computation unit, if declared.
    pub unit: Option<Arc<str>>,
    /// Presentation-only display unit, if declared.
    pub display_unit: Option<Arc<str>>,
    /// Effective §7.7.5 point-list membership.
    pub in_pointlist: bool,
    /// Effective §7.7.5 controlled-device label.
    pub controlled_device: Option<Arc<str>>,
    /// Human-readable description.
    pub description: Option<Arc<str>>,
}

/// The thin, data-only result of semantic resolution.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ResolvedSemantics {
    /// Effective metadata in connector arena order.
    pub points: Vec<EffectivePoint>,
    /// Should-level diagnostics emitted during resolution. Empty until raw CDL annotation parsing is
    /// wired into `oce-model`.
    pub diagnostics: Vec<Diagnostic>,
}

/// A semantics-resolution error (typed; never a panic).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SemanticsError {
    /// A vendor annotation was malformed.
    #[error("annotation parse error: {0}")]
    Annotation(String),
}

/// Resolve a model's vendor annotations into effective per-point metadata (resolved once at
/// ingest; never resolved on read — Integration Brief §1.3).
///
/// This M3 resolver is intentionally thin and data-only: `oce-model` currently carries typed
/// connector attributes but not raw `__cdl(...)` annotation blobs, so it derives only the effective
/// metadata available from connector attrs, direction, signal type, and defaults. String connectors
/// are metadata-only and are not returned as points.
///
/// # Errors
/// Returns [`SemanticsError`] if an annotation is malformed.
pub fn resolve(model: &ModelGraph) -> Result<ResolvedSemantics, SemanticsError> {
    let mut points = Vec::with_capacity(model.connectors.len());
    for connector in &model.connectors {
        if connector.value_type == ValueType::String {
            continue;
        }
        let (quantity, unit, display_unit) = match &connector.attrs {
            Attrs::Real(attrs) => (
                attrs.quantity.clone(),
                attrs.unit.clone(),
                attrs.display_unit.clone(),
            ),
            Attrs::Integer(_) | Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => {
                (None, None, None)
            }
        };
        points.push(EffectivePoint {
            key: PointKey::from_connector(connector.iri.as_ref(), connector.id),
            connector_id: connector.id,
            point_type: infer_point_type(connector.dir, connector.value_type),
            hardwired: false,
            trend: TrendSpec::default(),
            quantity,
            unit,
            display_unit,
            in_pointlist: true,
            controlled_device: None,
            description: None,
        });
    }
    Ok(ResolvedSemantics {
        points,
        diagnostics: Vec::new(),
    })
}

fn infer_point_type(dir: Dir, value_type: ValueType) -> PointType {
    match (dir, value_type) {
        (Dir::In, ValueType::Boolean) => PointType::Di,
        (Dir::Out, ValueType::Boolean) => PointType::Do,
        (Dir::In, _) => PointType::Ai,
        (Dir::Out, _) => PointType::Ao,
    }
}

#[cfg(test)]
mod tests {
    //! Resolver side-table tests.

    use super::*;
    use oce_model::{
        BlockId, BlockInstance, Connector, EnumClassId, NoAttrs, ParamTable, RealAttrs, Value,
    };

    const EFFECTIVE_METADATA_GOLDEN: &str =
        include_str!("tests/fixtures/golden/effective_metadata.txt");

    #[test]
    fn resolve_derives_effective_metadata_without_mutating_model() {
        let model = representative_model();
        let before = render_model(&model);

        let resolved = resolve(&model).expect("metadata resolves");

        assert_eq!(render_model(&model), before, "resolve must be data-only");
        assert_eq!(
            render_effective_metadata(&resolved),
            EFFECTIVE_METADATA_GOLDEN
        );
        assert!(resolved.diagnostics.is_empty());
    }

    #[test]
    fn zero_trend_interval_is_on_change_sentinel() {
        assert_eq!(
            TrendSpec::from_interval_seconds(0, true),
            TrendSpec {
                interval: TrendInterval::OnChange,
                enabled: true
            }
        );
        assert_eq!(
            TrendSpec::from_interval_seconds(15, false),
            TrendSpec {
                interval: TrendInterval::EverySeconds(15),
                enabled: false
            }
        );
    }

    fn representative_model() -> ModelGraph {
        let block = BlockId(0);
        let real_attrs = RealAttrs {
            quantity: Some(Arc::from("ThermodynamicTemperature")),
            unit: Some(Arc::from("K")),
            display_unit: Some(Arc::from("degC")),
            min: Some(250.0),
            max: Some(330.0),
            nominal: Some(293.15),
            unbounded: Some(false),
        };
        let connectors = vec![
            Connector::new(ConnectorId(0), block, Dir::In, ValueType::Real, 0)
                .with_attrs(Attrs::Real(real_attrs))
                .expect("real attrs match")
                .with_iri("iri:zone_temp"),
            Connector::new(ConnectorId(1), block, Dir::In, ValueType::Integer, 1)
                .with_attrs(Attrs::Integer(Default::default()))
                .expect("integer attrs match"),
            Connector::new(ConnectorId(2), block, Dir::Out, ValueType::Boolean, 2)
                .with_attrs(Attrs::Boolean(NoAttrs))
                .expect("boolean attrs match")
                .with_iri("iri:enabled"),
            Connector::new(ConnectorId(3), block, Dir::Out, ValueType::String, 3)
                .with_attrs(Attrs::String(NoAttrs))
                .expect("string attrs match")
                .with_iri("iri:label"),
            Connector::new(
                ConnectorId(4),
                block,
                Dir::Out,
                ValueType::Enum(EnumClassId(7)),
                4,
            )
            .with_attrs(Attrs::Enum(NoAttrs))
            .expect("enum attrs match")
            .with_iri("iri:mode_ordinal"),
        ];
        ModelGraph {
            blocks: vec![BlockInstance {
                id: block,
                class_iri: Arc::from("CDL.Test.Semantics"),
                inputs: vec![ConnectorId(0), ConnectorId(1)],
                outputs: vec![ConnectorId(2), ConnectorId(3), ConnectorId(4)],
                params: ParamTable {
                    values: vec![(Arc::from("k"), Value::Real(1.0))],
                },
                decl_order: 0,
                instance_iri: Some(Arc::from("iri:block")),
            }],
            connectors,
            connections: Vec::new(),
            external_inputs: vec![ConnectorId(0), ConnectorId(1)],
        }
    }

    fn render_effective_metadata(resolved: &ResolvedSemantics) -> String {
        let mut out = String::new();
        out.push_str("diagnostics=");
        out.push_str(&resolved.diagnostics.len().to_string());
        out.push('\n');
        for point in &resolved.points {
            out.push_str(&format!(
                "connector={} key={} type={:?} hardwired={} trend={:?}/{} pointlist={} quantity={:?} unit={:?} display_unit={:?} controlled_device={:?} description={:?}\n",
                point.connector_id.0,
                render_key(&point.key),
                point.point_type,
                point.hardwired,
                point.trend.interval,
                point.trend.enabled,
                point.in_pointlist,
                point.quantity.as_deref(),
                point.unit.as_deref(),
                point.display_unit.as_deref(),
                point.controlled_device.as_deref(),
                point.description.as_deref()
            ));
        }
        out
    }

    fn render_key(key: &PointKey) -> String {
        match key {
            PointKey::Iri(iri) => iri.to_string(),
            PointKey::Connector(id) => format!("conn#{}", id.0),
        }
    }

    fn render_model(model: &ModelGraph) -> String {
        let mut out = String::new();
        for block in &model.blocks {
            out.push_str(&format!(
                "block={} class={} iri={:?}\n",
                block.id.0,
                block.class_iri,
                block.instance_iri.as_deref()
            ));
        }
        for connector in &model.connectors {
            out.push_str(&format!(
                "connector={} block={} dir={:?} type={:?} decl={} iri={:?} attrs={:?}\n",
                connector.id.0,
                connector.block.0,
                connector.dir,
                connector.value_type,
                connector.decl_order,
                connector.iri.as_deref(),
                connector.attrs
            ));
        }
        out
    }
}
