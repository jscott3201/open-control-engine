//! Closed-field refusal and canonical coverage controls; future values are injected privately.

use super::*;

#[test]
fn every_field_changes_canonical_bytes_and_has_an_exact_symmetric_refusal() {
    use CompatibilityMismatch as M;
    type Mutation = (fn(&mut CompatibilityDescriptor), M);
    let baseline = CompatibilityDescriptor::current(None);
    let mutations: &[Mutation] = &[
        (|d| d.revision = 0, M::DescriptorRevision),
        (|d| d.revision = 2, M::DescriptorRevision),
        (|d| d.revision = u32::MAX, M::DescriptorRevision),
        (|d| d.catalog_schema += 1, M::CatalogSchema),
        (|d| d.catalog.0.push('0'), M::CatalogContent),
        (|d| d.io_schema += 1, M::IoSchema),
        (|d| d.value_schema += 1, M::ValueSchema),
        (|d| d.parameter_schema += 1, M::ParameterSchema),
        (|d| d.execution_profile = "HostTick-v2", M::ExecutionProfile),
        (|d| d.execution_profile_schema += 1, M::ExecutionProfile),
        (|d| d.oce_api_version = "0.5.1", M::Build),
        (|d| d.oce_api_version = "0.5.0+other", M::Build),
        (
            |d| d.content = Some(ContentId::from_digest([0; 32])),
            M::ContentIdPresence,
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
    let baseline = CompatibilityDescriptor::current(None);
    let mut changed = baseline.clone();
    changed.revision = 3;
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
    changed.oce_api_version = "0.6.0";
    changed.content = Some(ContentId::from_digest([1; 32]));
    for (cause, field) in [
        (M::DescriptorRevision, 0),
        (M::CatalogSchema, 1),
        (M::CatalogContent, 2),
        (M::IoSchema, 3),
        (M::ValueSchema, 4),
        (M::ParameterSchema, 5),
        (M::ExecutionProfile, 6),
        (M::Build, 7),
        (M::ContentIdPresence, 8),
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
            8 => changed.content = None,
            _ => unreachable!(),
        }
    }
    assert_eq!(baseline.check_compatible(&changed), Ok(()));
}

#[test]
fn two_present_content_ids_agree_only_when_equal() {
    use CompatibilityMismatch as M;
    let mut left = CompatibilityDescriptor::current(Some(&ContentId::from_digest([7; 32])));
    let right = CompatibilityDescriptor::current(Some(&ContentId::from_digest([7; 32])));
    assert_eq!(left.check_compatible(&right), Ok(()));
    assert!(
        left.to_string()
            .ends_with(&format!("content:oce:canon1:{}\n", "07".repeat(32)))
    );
    left.content = Some(ContentId::from_digest([8; 32]));
    assert_eq!(left.check_compatible(&right), Err(M::ContentIdMismatch));
    assert_eq!(right.check_compatible(&left), Err(M::ContentIdMismatch));
}
