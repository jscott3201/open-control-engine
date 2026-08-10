//! Strict, bounded reader for the test-policy known-divergence register.

use std::collections::BTreeSet;

use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::date::valid_date;

#[path = "reader/collection.rs"]
mod collection;

use collection::validate_collection;

pub(crate) const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_PRODUCER_CASES: usize = 16;
pub(crate) const MAX_PARTIES: usize = 6;
pub(crate) const MAX_EVIDENCE: usize = 32;
pub(crate) const MAX_HUMAN_TEXT_BYTES: usize = 2048;
pub(crate) const MAX_SUMMARY_BYTES: usize = 512;
pub(crate) const MAX_IDENTITY_BYTES: usize = 512;
pub(crate) const MAX_PATH_BYTES: usize = 512;
pub(crate) const MAX_URL_BYTES: usize = 2048;

const FORMAT: &str = "oce-known-divergence-register-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValidationCode {
    InputTooLarge,
    JsonSyntax,
    Schema,
    Format,
    EntryCount,
    InvalidId,
    InvalidString,
    InvalidPath,
    InvalidDigest,
    InvalidDate,
    ReviewBeforeOpen,
    InvalidCommit,
    ProducerCaseCount,
    ProducerCaseDuplicate,
    ProducerCaseOrder,
    PartyCount,
    PartyDuplicate,
    PartyOrder,
    EvidenceCount,
    EvidenceDuplicate,
    EvidenceOrder,
    PartyEvidence,
    UpstreamIssue,
    Lifecycle,
    IdDuplicate,
    EntryOrder,
    SubjectDuplicate,
    ProducerCaseGlobalDuplicate,
    SupersessionTarget,
    SupersessionSelf,
    SupersessionCycle,
    ThreeWayIssue,
    EvidenceMissing,
    EvidenceNotFile,
    EvidenceDigestMismatch,
    ComparisonReferenceMissing,
    ComparisonReferenceNotFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidationError {
    pub(crate) code: ValidationCode,
    pub(crate) entry: Option<usize>,
    pub(crate) detail: String,
}

impl ValidationError {
    pub(crate) fn new(
        code: ValidationCode,
        entry: Option<usize>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            entry,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Register {
    format: String,
    #[serde(deserialize_with = "deserialize_entries")]
    pub(crate) entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub(crate) id: String,
    summary: String,
    subject: Subject,
    #[serde(deserialize_with = "deserialize_producer_cases")]
    producer_cases: Vec<ProducerCase>,
    #[serde(deserialize_with = "deserialize_parties")]
    parties: Vec<Party>,
    #[serde(deserialize_with = "deserialize_evidence")]
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) comparison_reference: String,
    opened_on: String,
    reviewed_on: String,
    reviewed_commit: String,
    state: State,
    disposition: String,
    upstream_issue: UpstreamIssue,
    superseded_by: Option<String>,
    conformance_effect: ConformanceEffect,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Subject {
    class: String,
    scenario: String,
    signal: String,
    observation: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct ProducerCase {
    producer: Producer,
    case_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Producer {
    CleanRoom,
    TierA,
    Engine,
    Dymola,
    OpenModelica,
    Spawn,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Party {
    Analytical,
    TierA,
    Engine,
    Dymola,
    OpenModelica,
    Spawn,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    party: Party,
    kind: EvidenceKind,
    pub(crate) path: String,
    pub(crate) sha256: String,
    summary: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum EvidenceKind {
    Derivation,
    TierAReference,
    EngineRun,
    ExternalRun,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum State {
    Open,
    Resolved,
    Superseded,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct UpstreamIssue {
    status: UpstreamStatus,
    url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum UpstreamStatus {
    NotApplicable,
    NotFiled,
    Filed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ConformanceEffect {
    EvidenceOnly,
}

fn deserialize_entries<'de, D>(deserializer: D) -> Result<Vec<Entry>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded(deserializer, MAX_ENTRIES, "entry_count", true)
}

fn deserialize_producer_cases<'de, D>(deserializer: D) -> Result<Vec<ProducerCase>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded(
        deserializer,
        MAX_PRODUCER_CASES,
        "producer_case_count",
        false,
    )
}

fn deserialize_parties<'de, D>(deserializer: D) -> Result<Vec<Party>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded(deserializer, MAX_PARTIES, "party_count", false)
}

fn deserialize_evidence<'de, D>(deserializer: D) -> Result<Vec<Evidence>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded(deserializer, MAX_EVIDENCE, "evidence_count", false)
}

fn deserialize_bounded<'de, D, T>(
    deserializer: D,
    limit: usize,
    marker: &'static str,
    annotate_entries: bool,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedVisitor<T> {
        limit: usize,
        marker: &'static str,
        annotate_entries: bool,
        item: std::marker::PhantomData<T>,
    }

    impl<'de, T: Deserialize<'de>> Visitor<'de> for BoundedVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "an array with at most {} items", self.limit)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.limit));
            while items.len() < self.limit {
                let index = items.len();
                let next = sequence.next_element::<T>();
                let next = if self.annotate_entries {
                    next.map_err(|error| A::Error::custom(format!("oce_entry[{index}]: {error}")))?
                } else {
                    next?
                };
                match next {
                    Some(item) => items.push(item),
                    None => return Ok(items),
                }
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom(format!(
                    "oce_code={}; limit={}",
                    self.marker, self.limit
                )));
            }
            Ok(items)
        }
    }

    deserializer.deserialize_seq(BoundedVisitor {
        limit,
        marker,
        annotate_entries,
        item: std::marker::PhantomData,
    })
}

pub(crate) fn read_register(input: &[u8]) -> Result<Register, ValidationError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(ValidationError::new(
            ValidationCode::InputTooLarge,
            None,
            format!(
                "register is {} bytes; limit is {MAX_INPUT_BYTES}",
                input.len()
            ),
        ));
    }
    let register = parse_register(input)?;
    validate_register(&register)?;
    Ok(register)
}

pub(crate) fn parse_register(input: &[u8]) -> Result<Register, ValidationError> {
    serde_json::from_slice(input).map_err(classify_json_error)
}

fn classify_json_error(error: serde_json::Error) -> ValidationError {
    let detail = error.to_string();
    let mut entry = detail
        .find("oce_entry[")
        .and_then(|start| detail[start + 10..].split(']').next())
        .and_then(|index| index.parse().ok());
    let code = if detail.contains("oce_code=entry_count") {
        entry = Some(MAX_ENTRIES);
        ValidationCode::EntryCount
    } else if detail.contains("oce_code=producer_case_count") {
        ValidationCode::ProducerCaseCount
    } else if detail.contains("oce_code=party_count") {
        ValidationCode::PartyCount
    } else if detail.contains("oce_code=evidence_count") {
        ValidationCode::EvidenceCount
    } else {
        match error.classify() {
            serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                ValidationCode::JsonSyntax
            }
            serde_json::error::Category::Data | serde_json::error::Category::Io => {
                ValidationCode::Schema
            }
        }
    };
    ValidationError::new(code, entry, detail)
}

fn validate_register(register: &Register) -> Result<(), ValidationError> {
    if register.format != FORMAT {
        return Err(ValidationError::new(
            ValidationCode::Format,
            None,
            "unsupported register format",
        ));
    }
    for (index, entry) in register.entries.iter().enumerate() {
        validate_entry(entry, index)?;
    }
    validate_collection(register)
}

fn validate_entry(entry: &Entry, index: usize) -> Result<(), ValidationError> {
    let at = Some(index);
    if !valid_id(&entry.id) {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            at,
            "invalid id",
        ));
    }
    valid_text(&entry.summary, MAX_SUMMARY_BYTES, false, at)?;
    for value in [
        &entry.subject.class,
        &entry.subject.scenario,
        &entry.subject.signal,
        &entry.subject.observation,
    ] {
        valid_identity(value, at)?;
    }
    if entry.producer_cases.is_empty() {
        return Err(ValidationError::new(
            ValidationCode::ProducerCaseCount,
            at,
            "at least one producer case is required",
        ));
    }
    for producer_case in &entry.producer_cases {
        valid_identity(&producer_case.case_id, at)?;
    }
    check_order(
        &entry.producer_cases,
        ValidationCode::ProducerCaseDuplicate,
        ValidationCode::ProducerCaseOrder,
        index,
    )?;
    if entry.parties.len() < 2 {
        return Err(ValidationError::new(
            ValidationCode::PartyCount,
            at,
            "at least two parties are required",
        ));
    }
    check_order(
        &entry.parties,
        ValidationCode::PartyDuplicate,
        ValidationCode::PartyOrder,
        index,
    )?;
    if entry.evidence.len() < entry.parties.len() {
        return Err(ValidationError::new(
            ValidationCode::EvidenceCount,
            at,
            "each party requires evidence",
        ));
    }
    validate_evidence(entry, index)?;
    valid_repository_path(&entry.comparison_reference, at)?;
    valid_date(&entry.opened_on, at)?;
    valid_date(&entry.reviewed_on, at)?;
    if entry.reviewed_on < entry.opened_on {
        return Err(ValidationError::new(
            ValidationCode::ReviewBeforeOpen,
            at,
            "reviewed_on precedes opened_on",
        ));
    }
    if !exact_lower_hex(&entry.reviewed_commit, 40) {
        return Err(ValidationError::new(
            ValidationCode::InvalidCommit,
            at,
            "reviewed_commit must be 40 lowercase hexadecimal characters",
        ));
    }
    valid_text(&entry.disposition, MAX_HUMAN_TEXT_BYTES, false, at)?;
    validate_upstream(entry, index)?;
    match (entry.state, entry.superseded_by.as_deref()) {
        (State::Open | State::Resolved, None) | (State::Superseded, Some(_)) => {}
        _ => {
            return Err(ValidationError::new(
                ValidationCode::Lifecycle,
                at,
                "state and superseded_by disagree",
            ));
        }
    }
    if let Some(target) = &entry.superseded_by
        && !valid_id(target)
    {
        return Err(ValidationError::new(
            ValidationCode::InvalidId,
            at,
            "invalid superseded_by id",
        ));
    }
    Ok(())
}

fn validate_evidence(entry: &Entry, index: usize) -> Result<(), ValidationError> {
    let at = Some(index);
    let mut keys = BTreeSet::new();
    for evidence in &entry.evidence {
        let key = (evidence.party, evidence.kind, evidence.path.as_str());
        if !keys.insert(key) {
            return Err(ValidationError::new(
                ValidationCode::EvidenceDuplicate,
                at,
                "duplicate evidence key",
            ));
        }
    }
    let mut previous = None;
    let parties = entry.parties.iter().copied().collect::<BTreeSet<_>>();
    let mut evidenced = BTreeSet::new();
    for evidence in &entry.evidence {
        let key = (evidence.party, evidence.kind, evidence.path.as_str());
        if let Some(prior) = previous
            && key < prior
        {
            return Err(ValidationError::new(
                ValidationCode::EvidenceOrder,
                at,
                "evidence is not canonically ordered",
            ));
        }
        previous = Some(key);
        if !parties.contains(&evidence.party) {
            return Err(ValidationError::new(
                ValidationCode::PartyEvidence,
                at,
                "evidence names an unlisted party",
            ));
        }
        evidenced.insert(evidence.party);
        valid_repository_path(&evidence.path, at)?;
        if !exact_lower_hex(&evidence.sha256, 64) {
            return Err(ValidationError::new(
                ValidationCode::InvalidDigest,
                at,
                "sha256 must be 64 lowercase hexadecimal characters",
            ));
        }
        valid_text(&evidence.summary, MAX_SUMMARY_BYTES, false, at)?;
    }
    if evidenced != parties {
        return Err(ValidationError::new(
            ValidationCode::PartyEvidence,
            at,
            "every listed party requires evidence",
        ));
    }
    Ok(())
}

fn validate_upstream(entry: &Entry, index: usize) -> Result<(), ValidationError> {
    let at = Some(index);
    match (&entry.upstream_issue.status, &entry.upstream_issue.url) {
        (UpstreamStatus::Filed, Some(url)) if valid_https_url(url) => Ok(()),
        (UpstreamStatus::NotApplicable | UpstreamStatus::NotFiled, None) => Ok(()),
        _ => Err(ValidationError::new(
            ValidationCode::UpstreamIssue,
            at,
            "upstream status and URL disagree",
        )),
    }
}

fn check_order<T: Ord>(
    values: &[T],
    duplicate: ValidationCode,
    out_of_order: ValidationCode,
    index: usize,
) -> Result<(), ValidationError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ValidationError::new(
                duplicate,
                Some(index),
                "duplicate item",
            ));
        }
    }
    for pair in values.windows(2) {
        if pair[0] > pair[1] {
            return Err(ValidationError::new(
                out_of_order,
                Some(index),
                "items are not canonically ordered",
            ));
        }
    }
    Ok(())
}

fn valid_text(
    value: &str,
    max_bytes: usize,
    ascii: bool,
    entry: Option<usize>,
) -> Result<(), ValidationError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
        || (ascii && !value.is_ascii())
    {
        return Err(ValidationError::new(
            ValidationCode::InvalidString,
            entry,
            format!("invalid or over-limit string; limit is {max_bytes} bytes"),
        ));
    }
    Ok(())
}

fn valid_repository_path(value: &str, entry: Option<usize>) -> Result<(), ValidationError> {
    valid_text(value, MAX_PATH_BYTES, true, entry)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains('*')
        || value.contains('?')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ValidationError::new(
            ValidationCode::InvalidPath,
            entry,
            "path is not a canonical repository-relative path",
        ));
    }
    Ok(())
}

fn valid_identity(value: &str, entry: Option<usize>) -> Result<(), ValidationError> {
    valid_text(value, MAX_IDENTITY_BYTES, true, entry)?;
    if value.contains('*') || value.contains('?') {
        return Err(ValidationError::new(
            ValidationCode::InvalidString,
            entry,
            "identity must not be a regex or glob",
        ));
    }
    Ok(())
}

fn valid_https_url(value: &str) -> bool {
    if valid_text(value, MAX_URL_BYTES, true, None).is_err()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    !authority.is_empty() && !authority.starts_with('.') && !authority.ends_with('.')
}

fn valid_id(value: &str) -> bool {
    value.len() == 10
        && value.starts_with("DVG-")
        && value[4..].bytes().all(|byte| byte.is_ascii_digit())
}

fn exact_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
