//! JSON verification configuration for conformance runs.
//!
//! The configuration mirrors the CDL verification artifact shape: references, base tolerances,
//! ordered per-output tolerance overrides, indicator masks, sampling hints, and device/CDL point
//! mappings.

use std::error::Error;
use std::fmt;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde::{
    Deserializer, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};

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
    #[serde(
        default,
        deserialize_with = "deserialize_output_patterns",
        serialize_with = "serialize_output_patterns"
    )]
    pub outputs: Vec<OutputPattern>,
    /// Per-output regex string to indicator signal names.
    #[serde(
        default,
        deserialize_with = "deserialize_indicator_patterns",
        serialize_with = "serialize_indicator_patterns"
    )]
    pub indicators: Vec<IndicatorPattern>,
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

    /// Return the base tolerance with matching per-output regex overrides applied in declaration
    /// order. Later matching records override only the fields they name, so duplicate patterns are
    /// preserved instead of being collapsed by JSON-object key semantics.
    ///
    /// # Errors
    /// Returns [`ConfigError::Invalid`] if a pattern is not a valid regex.
    pub fn tolerance_for_output(&self, output: &str) -> Result<Tolerances, ConfigError> {
        let mut tolerance = self.tolerances;
        for entry in &self.outputs {
            if Regex::new(&entry.pattern)
                .map_err(|err| invalid_error(format!("outputs {:?}: {err}", entry.pattern)))?
                .is_match(output)
            {
                tolerance = entry.tolerances.apply_to(tolerance);
            }
        }
        Ok(tolerance)
    }

    /// Return all indicator signal names whose regex pattern matches `output`, preserving
    /// declaration order. Multiple matching records are concatenated; [`crate::Mask`] ANDs the
    /// resulting indicators.
    ///
    /// # Errors
    /// Returns [`ConfigError::Invalid`] if a pattern is not a valid regex.
    pub fn indicator_signals_for_output(&self, output: &str) -> Result<Vec<String>, ConfigError> {
        let mut signals = Vec::new();
        for entry in &self.indicators {
            if Regex::new(&entry.pattern)
                .map_err(|err| invalid_error(format!("indicators {:?}: {err}", entry.pattern)))?
                .is_match(output)
            {
                signals.extend(entry.signals.iter().cloned());
            }
        }
        Ok(signals)
    }

    /// Return the base tolerance with an exact-key per-output override applied.
    ///
    /// Retained for B2-era tests and callers that already resolved a literal output key; new driver
    /// code should use [`VerifyConfig::tolerance_for_output`] so regex patterns are honored.
    #[must_use]
    pub fn tolerance_for_exact_output(&self, output: &str) -> Tolerances {
        self.outputs
            .iter()
            .find(|entry| entry.pattern == output)
            .map_or(self.tolerances, |entry| {
                entry.tolerances.apply_to(self.tolerances)
            })
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

/// One ordered per-output tolerance pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputPattern {
    /// Regex matched against an output point name.
    pub pattern: String,
    /// Partial tolerance override applied when the regex matches.
    pub tolerances: PartialTolerances,
}

/// One ordered per-output indicator pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndicatorPattern {
    /// Regex matched against an output point name.
    pub pattern: String,
    /// Indicator signal names ANDed when the regex matches.
    pub signals: Vec<String>,
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
    for entry in &config.outputs {
        if entry.pattern.trim().is_empty() {
            return invalid("output tolerance pattern must not be empty");
        }
        validate_regex("outputs", &entry.pattern)?;
        validate_partial_tolerances(&entry.pattern, entry.tolerances)?;
    }
    for entry in &config.indicators {
        if entry.pattern.trim().is_empty() {
            return invalid("indicator output pattern must not be empty");
        }
        validate_regex("indicators", &entry.pattern)?;
        if entry.signals.is_empty() {
            return invalid(format!(
                "indicator mask for {:?} has no signals",
                entry.pattern
            ));
        }
        if entry.signals.iter().any(|signal| signal.trim().is_empty()) {
            return invalid(format!(
                "indicator mask for {:?} contains an empty signal",
                entry.pattern
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

fn validate_regex(section: &str, pattern: &str) -> Result<(), ConfigError> {
    Regex::new(pattern)
        .map(|_| ())
        .map_err(|err| invalid_error(format!("{section} pattern {pattern:?} is not regex: {err}")))
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

fn invalid_error(message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(message.into())
}

fn default_run_controller() -> bool {
    true
}

fn serialize_output_patterns<S>(entries: &[OutputPattern], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(entries.len()))?;
    for entry in entries {
        map.serialize_entry(&entry.pattern, &entry.tolerances)?;
    }
    map.end()
}

fn serialize_indicator_patterns<S>(
    entries: &[IndicatorPattern],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(entries.len()))?;
    for entry in entries {
        map.serialize_entry(&entry.pattern, &entry.signals)?;
    }
    map.end()
}

fn deserialize_output_patterns<'de, D>(deserializer: D) -> Result<Vec<OutputPattern>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OutputPatternsVisitor;

    impl<'de> Visitor<'de> for OutputPatternsVisitor {
        type Value = Vec<OutputPattern>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object mapping output regexes to tolerance overrides")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut entries = Vec::with_capacity(access.size_hint().unwrap_or(0));
            while let Some((pattern, tolerances)) =
                access.next_entry::<String, PartialTolerances>()?
            {
                entries.push(OutputPattern {
                    pattern,
                    tolerances,
                });
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(OutputPatternsVisitor)
}

fn deserialize_indicator_patterns<'de, D>(
    deserializer: D,
) -> Result<Vec<IndicatorPattern>, D::Error>
where
    D: Deserializer<'de>,
{
    struct IndicatorPatternsVisitor;

    impl<'de> Visitor<'de> for IndicatorPatternsVisitor {
        type Value = Vec<IndicatorPattern>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object mapping output regexes to indicator signal lists")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut entries = Vec::with_capacity(access.size_hint().unwrap_or(0));
            while let Some((pattern, signals)) = access.next_entry::<String, Vec<String>>()? {
                entries.push(IndicatorPattern { pattern, signals });
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(IndicatorPatternsVisitor)
}
