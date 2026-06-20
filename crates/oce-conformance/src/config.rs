//! JSON verification configuration for conformance runs.
//!
//! The configuration mirrors the CDL verification artifact shape: references, base tolerances,
//! per-output tolerance overrides, indicator masks, sampling hints, and device/CDL point mappings.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Tolerances;

/// Top-level verification configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyConfig {
    /// Reference models/sequences and their point-name mappings.
    #[serde(default)]
    pub references: Vec<ReferenceSpec>,
    /// Base tolerance applied when no per-output override matches.
    #[serde(default)]
    pub tolerances: Tolerances,
    /// Per-output regex string to partial tolerance override.
    ///
    /// This B2 DTO keeps the JSON object shape deterministic with a map. The B3 driver will own the
    /// ordered duplicate-preserving pattern list described in the conformance spec.
    #[serde(default)]
    pub outputs: BTreeMap<String, PartialTolerances>,
    /// Per-output regex string to indicator signal names.
    ///
    /// This B2 DTO keeps the JSON object shape deterministic with a map. The B3 driver will own the
    /// ordered duplicate-preserving pattern list described in the conformance spec.
    #[serde(default)]
    pub indicators: BTreeMap<String, Vec<String>>,
    /// Optional comparison sampling-rate hint in seconds.
    #[serde(default)]
    pub sampling: Option<f64>,
    /// CDL Scenario-1 compatibility flag; open-control always runs the controller itself.
    #[serde(default = "default_run_controller")]
    pub run_controller: bool,
}

impl VerifyConfig {
    /// Parse and validate a JSON verification config.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when JSON parsing fails or semantic validation rejects the config.
    pub fn from_json_str(text: &str) -> Result<Self, ConfigError> {
        let config = serde_json::from_str(text).map_err(ConfigError::Json)?;
        validate_config(&config)?;
        Ok(config)
    }

    /// Serialize the config to stable pretty JSON.
    ///
    /// # Errors
    /// Returns [`ConfigError::Json`] if serialization fails.
    pub fn to_json_string_pretty(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self).map_err(ConfigError::Json)
    }

    /// Return the base tolerance with an exact-key per-output override applied.
    ///
    /// Regex compilation and matching belongs to the B3 driver; this helper intentionally applies
    /// only a literal key so B2 does not introduce a second pattern engine.
    #[must_use]
    pub fn tolerance_for_exact_output(&self, output: &str) -> Tolerances {
        self.outputs
            .get(output)
            .map_or(self.tolerances, |partial| partial.apply_to(self.tolerances))
    }

    /// Validate semantic invariants that serde alone cannot express.
    ///
    /// # Errors
    /// Returns [`ConfigError::Invalid`] for empty required names, negative/non-finite tolerances,
    /// non-positive sampling, or empty indicator references.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_config(self)
    }
}

/// One reference sequence and its device-to-CDL point mapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSpec {
    /// Path or logical name of the source model.
    pub model: String,
    /// CDL sequence identifier within the model/reference set.
    pub sequence: String,
    /// Device/CDL point mapping entries used at the I/O boundary.
    #[serde(default)]
    pub point_name_mapping: Vec<PointMapEntry>,
}

/// One point-name mapping row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointMapEntry {
    /// CDL-side point descriptor.
    pub cdl: PointEnd,
    /// Device/oracle-side point descriptor.
    pub device: PointEnd,
}

/// One side of a point-name mapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointEnd {
    /// Point name on this side of the mapping.
    pub name: String,
    /// Engineering unit label, when supplied by the artifact.
    #[serde(default)]
    pub unit: Option<String>,
    /// Declared point type, serialized as JSON field `type`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

/// Optional tolerance override fields for one output pattern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartialTolerances {
    /// Absolute x/time tolerance override.
    #[serde(default)]
    pub atolx: Option<f64>,
    /// Absolute y/value tolerance override.
    #[serde(default)]
    pub atoly: Option<f64>,
    /// Relative-to-range x/time tolerance override.
    #[serde(default)]
    pub rtolx: Option<f64>,
    /// Relative-to-range y/value tolerance override.
    #[serde(default)]
    pub rtoly: Option<f64>,
    /// Local x/time tolerance override.
    #[serde(default)]
    pub ltolx: Option<f64>,
    /// Local y/value tolerance override.
    #[serde(default)]
    pub ltoly: Option<f64>,
}

impl PartialTolerances {
    /// Apply this partial override to a base tolerance record.
    #[must_use]
    pub fn apply_to(self, mut base: Tolerances) -> Tolerances {
        if let Some(value) = self.atolx {
            base.atolx = value;
        }
        if let Some(value) = self.atoly {
            base.atoly = value;
        }
        if let Some(value) = self.rtolx {
            base.rtolx = value;
        }
        if let Some(value) = self.rtoly {
            base.rtoly = value;
        }
        if let Some(value) = self.ltolx {
            base.ltolx = value;
        }
        if let Some(value) = self.ltoly {
            base.ltoly = value;
        }
        base
    }
}

/// Verification configuration parse/validation error.
#[non_exhaustive]
#[derive(Debug)]
pub enum ConfigError {
    /// JSON syntax or type-shape error.
    Json(serde_json::Error),
    /// Semantic validation error.
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Json(err) => write!(f, "invalid verification JSON: {err}"),
            ConfigError::Invalid(message) => write!(f, "invalid verification config: {message}"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Json(err) => Some(err),
            ConfigError::Invalid(_) => None,
        }
    }
}

fn validate_config(config: &VerifyConfig) -> Result<(), ConfigError> {
    if config.references.is_empty() {
        return invalid("at least one reference is required");
    }
    validate_tolerances("tolerances", config.tolerances)?;
    for (pattern, partial) in &config.outputs {
        if pattern.trim().is_empty() {
            return invalid("output tolerance pattern must not be empty");
        }
        validate_partial_tolerances(pattern, *partial)?;
    }
    for (pattern, signals) in &config.indicators {
        if pattern.trim().is_empty() {
            return invalid("indicator output pattern must not be empty");
        }
        if signals.is_empty() {
            return invalid(format!("indicator mask for {pattern:?} has no signals"));
        }
        if signals.iter().any(|signal| signal.trim().is_empty()) {
            return invalid(format!(
                "indicator mask for {pattern:?} contains an empty signal"
            ));
        }
    }
    if let Some(sampling) = config.sampling
        && (!sampling.is_finite() || sampling <= 0.0)
    {
        return invalid("sampling must be finite and positive");
    }
    for (idx, reference) in config.references.iter().enumerate() {
        if reference.model.trim().is_empty() {
            return invalid(format!("reference {idx} model must not be empty"));
        }
        if reference.sequence.trim().is_empty() {
            return invalid(format!("reference {idx} sequence must not be empty"));
        }
        for (map_idx, mapping) in reference.point_name_mapping.iter().enumerate() {
            validate_point_end(idx, map_idx, "cdl", &mapping.cdl)?;
            validate_point_end(idx, map_idx, "device", &mapping.device)?;
        }
    }
    Ok(())
}

fn validate_point_end(
    reference_idx: usize,
    map_idx: usize,
    side: &str,
    end: &PointEnd,
) -> Result<(), ConfigError> {
    if end.name.trim().is_empty() {
        return invalid(format!(
            "reference {reference_idx} mapping {map_idx} {side} name must not be empty"
        ));
    }
    Ok(())
}

fn validate_tolerances(label: &str, tol: Tolerances) -> Result<(), ConfigError> {
    for (field, value) in [
        ("atolx", tol.atolx),
        ("atoly", tol.atoly),
        ("rtolx", tol.rtolx),
        ("rtoly", tol.rtoly),
        ("ltolx", tol.ltolx),
        ("ltoly", tol.ltoly),
    ] {
        validate_nonnegative_finite(&format!("{label}.{field}"), value)?;
    }
    Ok(())
}

fn validate_partial_tolerances(label: &str, partial: PartialTolerances) -> Result<(), ConfigError> {
    for (field, value) in [
        ("atolx", partial.atolx),
        ("atoly", partial.atoly),
        ("rtolx", partial.rtolx),
        ("rtoly", partial.rtoly),
        ("ltolx", partial.ltolx),
        ("ltoly", partial.ltoly),
    ] {
        if let Some(value) = value {
            validate_nonnegative_finite(&format!("outputs[{label:?}].{field}"), value)?;
        }
    }
    Ok(())
}

fn validate_nonnegative_finite(label: &str, value: f64) -> Result<(), ConfigError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        invalid(format!("{label} must be finite and non-negative"))
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

fn default_run_controller() -> bool {
    true
}
