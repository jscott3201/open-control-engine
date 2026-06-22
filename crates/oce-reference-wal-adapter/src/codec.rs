//! Private JSON codec for the reference WAL adapter.

use oce_store::{DomainKey, Durability, OcValue, PointSample, PointStatus, ResolvedModel};
use serde::{Deserialize, Serialize};

/// Current private on-disk format version.
pub(crate) const FORMAT_VERSION: u8 = 1;

/// Snapshot image persisted by `commit()`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SnapshotFile {
    pub(crate) format_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ordering_high_water: Option<u64>,
    pub(crate) models: Vec<ResolvedModel>,
    pub(crate) point_keys: Vec<DomainKey>,
    pub(crate) point_samples: Vec<Option<StoredPointSample>>,
}

impl SnapshotFile {
    pub(crate) fn new(
        models: Vec<ResolvedModel>,
        point_keys: Vec<DomainKey>,
        point_samples: Vec<Option<StoredPointSample>>,
        ordering_high_water: u64,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            ordering_high_water: Some(ordering_high_water),
            models,
            point_keys,
            point_samples,
        }
    }
}

/// One append-only WAL record.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WalRecord {
    pub(crate) format_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ordering_high_water: Option<u64>,
    pub(crate) record: WalRecordKind,
}

impl WalRecord {
    pub(crate) fn point_write(
        durability: Durability,
        key: DomainKey,
        sample: &PointSample,
        ordering_high_water: Option<u64>,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            ordering_high_water,
            record: WalRecordKind::PointWrite {
                durability: StoredDurability::from(durability),
                key,
                sample: StoredPointSample::from(sample),
            },
        }
    }

    pub(crate) fn encode_line(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut line = serde_json::to_vec(self)?;
        line.push(b'\n');
        Ok(line)
    }
}

/// Private WAL record variants.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WalRecordKind {
    PointWrite {
        durability: StoredDurability,
        key: DomainKey,
        sample: StoredPointSample,
    },
}

/// Store-facing durability encoded in the WAL.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) enum StoredDurability {
    Critical,
    Telemetry,
}

impl From<Durability> for StoredDurability {
    fn from(value: Durability) -> Self {
        match value {
            Durability::Critical => Self::Critical,
            Durability::Telemetry => Self::Telemetry,
        }
    }
}

/// A sample representation that preserves every `f64::to_bits()` pattern through JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredPointSample {
    pub(crate) value: StoredValue,
    pub(crate) status: PointStatus,
    pub(crate) at_unix_nanos: u64,
}

impl StoredPointSample {
    pub(crate) fn to_sample(&self) -> PointSample {
        PointSample {
            value: self.value.to_value(),
            status: self.status,
            at_unix_nanos: self.at_unix_nanos,
        }
    }
}

impl From<&PointSample> for StoredPointSample {
    fn from(sample: &PointSample) -> Self {
        Self {
            value: StoredValue::from_value(&sample.value),
            status: sample.status,
            at_unix_nanos: sample.at_unix_nanos,
        }
    }
}

/// JSON-safe store value encoding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(crate) enum StoredValue {
    Bool(bool),
    Int(i64),
    RealBits(u64),
    Decimal(String),
    Enum { type_iri: String, literal: String },
    String(String),
}

impl StoredValue {
    fn from_value(value: &OcValue) -> Self {
        match value {
            OcValue::Bool(v) => Self::Bool(*v),
            OcValue::Int(v) => Self::Int(*v),
            OcValue::Real(v) => Self::RealBits(v.to_bits()),
            OcValue::Decimal(v) => Self::Decimal(v.clone()),
            OcValue::Enum { type_iri, literal } => Self::Enum {
                type_iri: type_iri.clone(),
                literal: literal.clone(),
            },
            OcValue::String(v) => Self::String(v.clone()),
        }
    }

    fn to_value(&self) -> OcValue {
        match self {
            Self::Bool(v) => OcValue::Bool(*v),
            Self::Int(v) => OcValue::Int(*v),
            Self::RealBits(bits) => OcValue::Real(f64::from_bits(*bits)),
            Self::Decimal(v) => OcValue::Decimal(v.clone()),
            Self::Enum { type_iri, literal } => OcValue::Enum {
                type_iri: type_iri.clone(),
                literal: literal.clone(),
            },
            Self::String(v) => OcValue::String(v.clone()),
        }
    }
}
