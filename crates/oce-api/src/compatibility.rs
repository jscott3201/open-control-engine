//! Closed host compatibility facts, independent of executable and state identity.

use std::fmt;
use std::sync::OnceLock;

use crate::{ContentIdError, ContractDomain, ExportReport};

/// Catalog metadata content tag, never an exported-document or execution identity.
///
/// Obtained from [`CompatibilityDescriptor::catalog_content_id`]. The private field prevents
/// substituting a string, authored model ID, or [`CompleteExportContentId`]. Its text is the
/// unchanged [`crate::catalog_content_id`] result: FNV-1a-128 over `oce:catalog:1\0` followed by
/// every canonical catalog JSON byte (including the trailing LF). Non-cryptographic; not authority.
///
/// ```compile_fail,E0308
/// use oce_api::{CatalogContentId, CompleteExportContentId};
/// fn confuse_export_with_catalog(export: CompleteExportContentId) -> CatalogContentId {
///     export
/// }
/// ```
///
/// ```compile_fail,E0277
/// let _: oce_api::CatalogContentId = String::from("catalog:1:fnv1a128:unverified").into();
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogContentId(String);

impl CatalogContentId {
    /// Borrow the unchanged `catalog:1:fnv1a128:<32 lowercase hex digits>` tag. No allocation/panic.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CatalogContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Complete exported-document content tag captured through [`ExportReport::content_id_complete`].
///
/// FNV-1a-128 covers exactly the supplied report's bytes, with no domain prefix or length. The
/// report is mutable host-owned data: completeness means its warning list was empty at capture,
/// not authenticated producer provenance or revalidated CXF. The captured tag is independently
/// owned and never changes with that report. It is not source, executable, state or deployment ID.
/// No public string constructor or conversion from [`CatalogContentId`] exists.
///
/// ```compile_fail,E0308
/// use oce_api::{CatalogContentId, CompleteExportContentId};
/// fn confuse_catalog_with_export(catalog: CatalogContentId) -> CompleteExportContentId {
///     catalog
/// }
/// ```
///
/// ```compile_fail,E0308
/// let model_id = oce_api::oce_store::DomainKey::new("authored-model");
/// let _: oce_api::CompleteExportContentId = model_id;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteExportContentId(String);

impl CompleteExportContentId {
    /// Borrow the unchanged `cxf:fnv1a128:<32 lowercase hex digits>` tag. No allocation/panic.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CompleteExportContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Closed, owned revision-1 host compatibility facts, independent of any engine lifetime.
///
/// [`Self::current`] captures only public catalog and contract revisions, fixed HostTick v1,
/// `oce-api`'s Cargo package version, and optionally a checked complete export tag. No executable
/// fingerprint, input-definition digest, generation, clock, state-wire or host policy is included.
/// IO revision describes the inventory contract, NOT a loaded model's executable input schema.
///
/// `Display` (and thus `to_string()`) is the canonical artifact: UTF-8, the ten labeled lines in
/// the field list below, that exact order, unsigned decimal revisions without leading zeros, no
/// spaces/CR/BOM, and one LF after EACH line, including the last. Tags are copied verbatim; absence
/// is exactly `none`, never an empty tag. No escaping is needed for these closed producer fields.
/// Debug output is not canonical. There is no parser, extension map, or mutable field API.
///
/// ```
/// let receipt = oce_api::CompatibilityDescriptor::current(None)?;
/// let bytes = receipt.to_string();
/// assert!(bytes.starts_with("oce-compatibility:1\ncatalog-schema:1\n"));
/// assert!(bytes.ends_with("oce-api-version:0.1.0\nexport:none\n"));
/// receipt.check_compatible(&receipt)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// The fields are `oce-compatibility`, `catalog-schema`, `catalog`, `io-schema`, `value-schema`,
/// `parameter-schema`, `execution-profile`, `execution-profile-schema`, `oce-api-version`, `export`.
/// `execution-profile` is exactly `HostTick-v1`; its separate descriptor revision is currently 2.
/// `oce-api-version` includes the exact Cargo version, including any pre-release/build suffix.
/// The three supported feature selections are equivalent and are intentionally not distinguished.
/// Package version is NOT a source/dependency-lock/compiler/target/codegen/binary identity. Equal
/// facts, especially on unpublished same-version builds, do not establish execution equivalence,
/// cross-platform numerics, restore eligibility, release compatibility or authority to actuate.
///
/// FNV tags use offset `0x6c62272e07bb014262b821756295c58d`, XOR each byte then multiply by
/// `0x0000000001000000000000000000013b` modulo 2^128. They are non-cryptographic and collision-prone
/// in adversarial use. This descriptor adds no hash/signature. Hosts may hash/sign canonical bytes
/// themselves and own authenticity, freshness and build qualification; OCE is not a signing/PKI
/// authority. Persisted receipts require exact byte comparison against a newly captured descriptor;
/// refuse unknown revisions, missing/extra fields, or any byte difference rather than ignoring it.
#[derive(Clone, Debug)]
pub struct CompatibilityDescriptor {
    revision: u32,
    catalog_schema: u32,
    catalog: CatalogContentId,
    io_schema: u32,
    value_schema: u32,
    parameter_schema: u32,
    execution_profile: &'static str,
    execution_profile_schema: u32,
    oce_api_version: &'static str,
    export: Option<CompleteExportContentId>,
}

impl CompatibilityDescriptor {
    /// Capture current public facts and optionally a complete exported-document identity.
    ///
    /// `None` records no export, not an empty or failed export. A supplied report is checked before
    /// catalog work. Retain this owned value across engine/report mutation; capture again after
    /// re-export if new content is wanted. No Engine/Store access or state capture occurs. Allocates
    /// bounded tags; the first success also initializes the cached catalog tag.
    ///
    /// # Errors
    /// Refuses a warning-bearing export with the existing completeness error.
    ///
    /// # Panics
    /// As with [`crate::catalog()`], an internal registry-constructor defect can panic. Missing a
    /// required entry in the repository-owned closed contract table is an internal defect.
    pub fn current(export: Option<&ExportReport>) -> Result<Self, ContentIdError> {
        let export = export
            .map(|report| report.content_id_complete().map(CompleteExportContentId))
            .transpose()?;
        static CATALOG: OnceLock<CatalogContentId> = OnceLock::new();
        let catalog = CATALOG
            .get_or_init(|| CatalogContentId(crate::catalog_content_id(crate::catalog())))
            .clone();
        let revision = |domain| {
            crate::contract_descriptors()
                .iter()
                .find(|descriptor| descriptor.domain == domain)
                .expect("closed contract table contains every required domain")
                .revision
        };
        Ok(Self {
            revision: 1,
            catalog_schema: revision(ContractDomain::Catalog),
            catalog,
            io_schema: revision(ContractDomain::Io),
            value_schema: revision(ContractDomain::Values),
            parameter_schema: revision(ContractDomain::Parameters),
            execution_profile: "HostTick-v1",
            execution_profile_schema: revision(ContractDomain::ExecutionProfile),
            oce_api_version: env!("CARGO_PKG_VERSION"),
            export,
        })
    }

    /// Canonical descriptor revision, currently 1; independent of every domain/state revision.
    #[must_use]
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Revision of canonical catalog shape, not its content or executable semantics.
    #[must_use]
    pub fn catalog_schema_revision(&self) -> u32 {
        self.catalog_schema
    }

    /// Captured catalog metadata identity. Borrowing never allocates or panics.
    #[must_use]
    pub fn catalog_content_id(&self) -> &CatalogContentId {
        &self.catalog
    }

    /// IO inventory contract revision, not the model-specific `input_definitions()` schema.
    #[must_use]
    pub fn io_schema_revision(&self) -> u32 {
        self.io_schema
    }

    /// Public value shape/semantic contract revision; not a new value codec.
    #[must_use]
    pub fn value_schema_revision(&self) -> u32 {
        self.value_schema
    }

    /// Public parameter metadata contract revision, not loaded parameter values.
    #[must_use]
    pub fn parameter_schema_revision(&self) -> u32 {
        self.parameter_schema
    }

    /// Fixed compatibility label `HostTick-v1`, not a runtime profile selector.
    #[must_use]
    pub fn execution_profile(&self) -> &'static str {
        self.execution_profile
    }

    /// Public execution-profile descriptor revision, separate from the HostTick version.
    #[must_use]
    pub fn execution_profile_schema_revision(&self) -> u32 {
        self.execution_profile_schema
    }

    /// Exact public `oce-api` Cargo version; NOT a unique build or source fingerprint.
    #[must_use]
    pub fn oce_api_version(&self) -> &'static str {
        self.oce_api_version
    }

    /// Captured complete export tag, or absence when no report was supplied. Never a partial tag.
    #[must_use]
    pub fn export_content_id(&self) -> Option<&CompleteExportContentId> {
        self.export.as_ref()
    }

    /// Require exact agreement of these bounded facts, not executable or restore compatibility.
    ///
    /// No wildcard/version-range matching: two absent exports agree only about absence; absence
    /// never matches presence. No allocation, mutation or panic. Hosts requiring content identity
    /// separately require `export_content_id().is_some()` even when this check succeeds.
    ///
    /// # Errors
    /// Returns the first mismatch in canonical field order. Both descriptor revisions must be 1.
    /// Profile label/revision share `ExecutionProfile`; export presence precedes export content.
    /// No public parsing of persisted/untrusted receipts is implied by this in-memory comparison.
    pub fn check_compatible(&self, other: &Self) -> Result<(), CompatibilityMismatch> {
        use CompatibilityMismatch as M;
        let checks = [
            (
                self.revision == 1 && other.revision == 1,
                M::DescriptorRevision,
            ),
            (
                self.catalog_schema == other.catalog_schema,
                M::CatalogSchema,
            ),
            (self.catalog == other.catalog, M::CatalogContent),
            (self.io_schema == other.io_schema, M::IoSchema),
            (self.value_schema == other.value_schema, M::ValueSchema),
            (
                self.parameter_schema == other.parameter_schema,
                M::ParameterSchema,
            ),
            (
                self.execution_profile == other.execution_profile
                    && self.execution_profile_schema == other.execution_profile_schema,
                M::ExecutionProfile,
            ),
            (self.oce_api_version == other.oce_api_version, M::Build),
            (
                self.export.is_some() == other.export.is_some(),
                M::ExportPresence,
            ),
            (self.export == other.export, M::ExportContent),
        ];
        checks
            .into_iter()
            .find_map(|(equal, cause)| (!equal).then_some(cause))
            .map_or(Ok(()), Err)
    }
}

impl fmt::Display for CompatibilityDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "oce-compatibility:{}\ncatalog-schema:{}\ncatalog:{}\nio-schema:{}\n\
             value-schema:{}\nparameter-schema:{}\nexecution-profile:{}\n\
             execution-profile-schema:{}\noce-api-version:{}\nexport:{}\n",
            self.revision,
            self.catalog_schema,
            self.catalog,
            self.io_schema,
            self.value_schema,
            self.parameter_schema,
            self.execution_profile,
            self.execution_profile_schema,
            self.oce_api_version,
            self.export
                .as_ref()
                .map_or("none", CompleteExportContentId::as_str),
        )
    }
}

/// Closed first-cause outcome of [`CompatibilityDescriptor::check_compatible`].
///
/// Equality is required, never inferred from SemVer ranges. These causes describe public facts
/// only; they are not state restore errors or security decisions. Display text is explanatory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityMismatch {
    /// Either descriptor revision is unsupported (not 1).
    DescriptorRevision,
    /// Canonical catalog schema revisions differ.
    CatalogSchema,
    /// Catalog metadata content tags differ.
    CatalogContent,
    /// IO inventory contract revisions differ.
    IoSchema,
    /// Value contract revisions differ.
    ValueSchema,
    /// Parameter contract revisions differ.
    ParameterSchema,
    /// Fixed HostTick label or its public descriptor revision differs.
    ExecutionProfile,
    /// Public OCE package versions differ; equality is not unique-build qualification.
    Build,
    /// Only one descriptor includes complete exported content.
    ExportPresence,
    /// Both descriptors include complete exports, but their tags differ.
    ExportContent,
}

impl fmt::Display for CompatibilityMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DescriptorRevision => "unsupported compatibility descriptor revision",
            Self::CatalogSchema => "catalog schema mismatch",
            Self::CatalogContent => "catalog content mismatch",
            Self::IoSchema => "IO schema mismatch",
            Self::ValueSchema => "value schema mismatch",
            Self::ParameterSchema => "parameter schema mismatch",
            Self::ExecutionProfile => "execution profile mismatch",
            Self::Build => "OCE package version mismatch",
            Self::ExportPresence => "complete export presence mismatch",
            Self::ExportContent => "complete export content mismatch",
        })
    }
}

impl std::error::Error for CompatibilityMismatch {}

#[cfg(test)]
#[path = "compatibility_tests.rs"]
mod tests;
