//! Closed-field refusal and canonical coverage controls; future values are injected privately.

use super::*;

#[test]
fn every_field_changes_canonical_bytes_and_has_an_exact_symmetric_refusal() {
    use CompatibilityMismatch as M;
    type Mutation = (fn(&mut CompatibilityDescriptor), M);
    let baseline = CompatibilityDescriptor::current(None).unwrap();
    let mutations: &[Mutation] = &[
        (|d| d.revision = 0, M::DescriptorRevision),
        (|d| d.revision = u32::MAX, M::DescriptorRevision),
        (|d| d.catalog_schema += 1, M::CatalogSchema),
        (|d| d.catalog.0.push('0'), M::CatalogContent),
        (|d| d.io_schema += 1, M::IoSchema),
        (|d| d.value_schema += 1, M::ValueSchema),
        (|d| d.parameter_schema += 1, M::ParameterSchema),
        (|d| d.execution_profile = "HostTick-v2", M::ExecutionProfile),
        (|d| d.execution_profile_schema += 1, M::ExecutionProfile),
        (|d| d.oce_api_version = "0.1.1", M::Build),
        (|d| d.oce_api_version = "0.1.0+other", M::Build),
        (
            |d| d.export = Some(CompleteExportContentId("cxf:fnv1a128:0".into())),
            M::ExportPresence,
        ),
    ];
    assert_eq!(baseline.check_compatible(&baseline), Ok(()));
    for (mutation, cause) in mutations {
        let mut changed = baseline.clone();
        mutation(&mut changed);
        assert_ne!(baseline.to_string(), changed.to_string(), "{cause:?}");
        assert_eq!(baseline.check_compatible(&changed), Err(*cause));
        assert_eq!(changed.check_compatible(&baseline), Err(*cause));
    }
}

#[test]
fn refusal_precedence_is_canonical_and_unknown_equal_revisions_still_refuse() {
    use CompatibilityMismatch as M;
    let baseline = CompatibilityDescriptor::current(None).unwrap();
    let mut changed = baseline.clone();
    changed.revision = 2;
    assert_eq!(
        changed.check_compatible(&changed),
        Err(M::DescriptorRevision)
    );
    changed.catalog_schema += 1;
    changed.catalog.0.push('0');
    changed.io_schema += 1;
    changed.value_schema += 1;
    changed.parameter_schema += 1;
    changed.execution_profile_schema += 1;
    changed.oce_api_version = "0.2.0";
    changed.export = Some(CompleteExportContentId("different".into()));
    for (cause, field) in [
        (M::DescriptorRevision, 0),
        (M::CatalogSchema, 1),
        (M::CatalogContent, 2),
        (M::IoSchema, 3),
        (M::ValueSchema, 4),
        (M::ParameterSchema, 5),
        (M::ExecutionProfile, 6),
        (M::Build, 7),
        (M::ExportPresence, 8),
    ] {
        assert_eq!(baseline.check_compatible(&changed), Err(cause));
        match field {
            0 => changed.revision = baseline.revision,
            1 => changed.catalog_schema = baseline.catalog_schema,
            2 => changed.catalog = baseline.catalog.clone(),
            3 => changed.io_schema = baseline.io_schema,
            4 => changed.value_schema = baseline.value_schema,
            5 => changed.parameter_schema = baseline.parameter_schema,
            6 => changed.execution_profile_schema = baseline.execution_profile_schema,
            7 => changed.oce_api_version = baseline.oce_api_version,
            8 => changed.export = None,
            _ => unreachable!(),
        }
    }
    assert_eq!(baseline.check_compatible(&changed), Ok(()));
}
