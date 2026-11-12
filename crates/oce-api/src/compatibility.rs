//! Closed host compatibility facts, independent of executable and state identity.

use std::fmt;
use std::sync::OnceLock;

use crate::{ContentId, ContractDomain};

/// Catalog metadata content tag, never an exported-document or execution identity.
///
/// Obtained from [`CompatibilityDescriptor::catalog_content_id`]. The private field prevents
/// substituting a string, authored model ID, or [`ContentId`]. Its text is the
/// unchanged [`crate::catalog_content_id`] result: FNV-1a-128 over `oce:catalog:1\0` followed by
/// every canonical catalog JSON byte (including the trailing LF). Non-cryptographic; not authority.
///
/// ```compile_fail,E0308
/// use oce_api::{CatalogContentId, ContentId};
/// fn confuse_content_with_catalog(content: ContentId) -> CatalogContentId {
///     content
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

/// Closed, owned revision-1 host compatibility facts, independent of any engine lifetime.
///
/// [`Self::current`] captures only public catalog and contract revisions, fixed HostTick v1,
/// `oce-api`'s Cargo package version, and optionally a canonical content id. No executable
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
/// let receipt = oce_api::CompatibilityDescriptor::current(None);
/// let bytes = receipt.to_string();
/// assert!(bytes.starts_with("oce-compatibility:1\ncatalog-schema:1\n"));
/// assert!(bytes.ends_with("oce-api-version:0.5.0\ncontent:none\n"));
/// receipt.check_compatible(&receipt)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// The fields are `oce-compatibility`, `catalog-schema`, `catalog`, `io-schema`, `value-schema`,
/// `parameter-schema`, `execution-profile`, `execution-profile-schema`, `oce-api-version`, `content`.
/// `content` is `none` or the `oce:canon1:` id ([`ContentId`]).
/// `execution-profile` is exactly `HostTick-v1`; its separate descriptor revision is 1.
/// Every revision here stays 1 until 1.0.0 and a field changes in place, so `oce-api-version` is
/// the line that tells a descriptor of one layout from another.
/// `oce-api-version` includes the exact Cargo version, including any pre-release/build suffix.
/// `oce-api` declares no cargo features, so there is no feature selection to distinguish.
/// Package version is NOT a source/dependency-lock/compiler/target/codegen/binary identity. Equal
/// facts, especially on unpublished same-version builds, do not establish execution equivalence,
/// cross-platform numerics, restore eligibility, release compatibility or authority to actuate.
///
/// The catalog tag is FNV-1a-128: offset `0x6c62272e07bb014262b821756295c58d`, XOR each byte then
/// multiply by `0x0000000001000000000000000000013b` modulo 2^128; non-cryptographic and
/// collision-prone in adversarial use. The content id is SHA-256 ([`ContentId`]). This descriptor
/// adds no hash/signature of its own. Hosts may hash/sign canonical bytes
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
    content: Option<ContentId>,
}

impl CompatibilityDescriptor {
    /// Capture current public facts and optionally a canonical content id.
    ///
    /// `None` records no content id, not an empty one. Pass the id [`crate::Engine::content_id`]
    /// returned to bind the receipt to that loaded model; the descriptor holds its own copy, so a
    /// later reload or parameter edit does not change it: capture again for new content. No Engine
    /// access or state capture occurs. Allocates bounded tags; the first call also initializes
    /// the cached catalog tag.
    ///
    /// # Panics
    /// As with [`crate::catalog()`], an internal registry-constructor defect can panic. Missing a
    /// required entry in the repository-owned closed contract table is an internal defect.
    #[must_use]
    pub fn current(content: Option<&ContentId>) -> Self {
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
        Self {
            revision: 1,
            catalog_schema: revision(ContractDomain::Catalog),
            catalog,
            io_schema: revision(ContractDomain::Io),
            value_schema: revision(ContractDomain::Values),
            parameter_schema: revision(ContractDomain::Parameters),
            execution_profile: "HostTick-v1",
            execution_profile_schema: revision(ContractDomain::ExecutionProfile),
            oce_api_version: env!("CARGO_PKG_VERSION"),
            content: content.cloned(),
        }
    }

    /// Canonical descriptor revision, 1 until 1.0.0; independent of every domain/state revision.
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

    /// Captured canonical content id, or absence when none was supplied.
    #[must_use]
    pub fn content_id(&self) -> Option<&ContentId> {
        self.content.as_ref()
    }

    /// Require exact agreement of these bounded facts, not executable or restore compatibility.
    ///
    /// No wildcard/version-range matching: two absent content ids agree only about absence;
    /// absence never matches presence. No allocation, mutation or panic. Hosts requiring content
    /// identity separately require `content_id().is_some()` even when this check succeeds.
    ///
    /// # Errors
    /// Returns the first mismatch in canonical field order. Both descriptor revisions must be 1.
    /// Profile label/revision share `ExecutionProfile`; content presence precedes content id.
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
                self.content.is_some() == other.content.is_some(),
                M::ContentIdPresence,
            ),
            (self.content == other.content, M::ContentIdMismatch),
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
             execution-profile-schema:{}\noce-api-version:{}\ncontent:{}\n",
            self.revision,
            self.catalog_schema,
            self.catalog,
            self.io_schema,
            self.value_schema,
            self.parameter_schema,
            self.execution_profile,
            self.execution_profile_schema,
            self.oce_api_version,
            self.content.as_ref().map_or("none", ContentId::as_str),
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
    /// Only one descriptor includes a canonical content id.
    ContentIdPresence,
    /// Both descriptors include canonical content ids, but they differ.
    ContentIdMismatch,
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
            Self::ContentIdPresence => "canonical content id presence mismatch",
            Self::ContentIdMismatch => "canonical content id mismatch",
        })
    }
}

impl std::error::Error for CompatibilityMismatch {}

#[cfg(test)]
#[path = "compatibility_tests.rs"]
mod tests;
