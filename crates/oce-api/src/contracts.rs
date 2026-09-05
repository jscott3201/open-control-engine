//! Packaged, versioned shape and semantic descriptors for the supported host facade.

/// Independently versioned facade contract domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractDomain {
    /// Class catalog DTOs and canonical JSON.
    Catalog,
    /// Immutable producer-stage load/export receipts and machine ordering.
    Diagnostics,
    /// Typed IO inventory and current enum/string projections.
    Io,
    /// Scalar value kinds, bit semantics, and model-local connector identity.
    Values,
    /// Tune-at-rest parameter rows, attributes, and metadata limits.
    Parameters,
    /// Warning-only block evidence and collection boundaries.
    Assertions,
    /// Fixed HostTick v1 semantics; not a profile selector.
    ExecutionProfile,
}

/// One versioned, packaged contract description.
///
/// Catalog uses JSON Schema 2020-12 for its canonical JSON. Other domains describe actual Rust
/// shapes and semantics in JSON; they do not introduce JSON codecs. Schema revisions are separate
/// from state wire, execution ABI, registry fingerprint, or host build-compatibility identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractDescriptor {
    /// Domain whose shapes and semantics are described.
    pub domain: ContractDomain,
    /// Revision within this domain.
    pub revision: u32,
    /// Complete UTF-8 JSON artifact, packaged inside oce-api.
    pub schema: &'static str,
}

/// Read every domain descriptor in catalog, diagnostics, IO, values, parameters, assertions,
/// execution-profile order. Static storage; no allocation, engine/store access, or panic.
#[must_use]
pub fn contract_descriptors() -> &'static [ContractDescriptor] {
    const DESCRIPTORS: &[ContractDescriptor] = &[
        ContractDescriptor {
            domain: ContractDomain::Catalog,
            revision: crate::CATALOG_SCHEMA_REVISION,
            schema: include_str!("../contracts/catalog.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::Diagnostics,
            revision: crate::DIAGNOSTIC_SCHEMA_REVISION,
            schema: include_str!("../contracts/diagnostics.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::Io,
            revision: 1,
            schema: include_str!("../contracts/io.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::Values,
            revision: 1,
            schema: include_str!("../contracts/values.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::Parameters,
            revision: 1,
            schema: include_str!("../contracts/parameters.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::Assertions,
            revision: 1,
            schema: include_str!("../contracts/assertions.schema.json"),
        },
        ContractDescriptor {
            domain: ContractDomain::ExecutionProfile,
            revision: 1,
            schema: include_str!("../contracts/execution-profile.schema.json"),
        },
    ];
    DESCRIPTORS
}
