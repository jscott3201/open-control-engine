//! Compiled runtime catalog observations, separate from reference catalog owners.

#[path = "../../../scripts/authority_claims/native.rs"]
mod native;

#[test]
fn authority_claims_match_compiled_catalog_and_reject_mutations() {
    let catalog = crate::catalog();
    native::verify_with_controls(&[
        ("catalog-entries", u32::try_from(catalog.len()).unwrap()),
        (
            "catalog-reserved",
            u32::try_from(catalog.iter().filter(|entry| entry.reserved).count()).unwrap(),
        ),
    ]);
}
