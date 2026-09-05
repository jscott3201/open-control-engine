//! Compiled private state revision observations; no production or public surface.

#[path = "../../../../scripts/authority_claims/native.rs"]
mod native;

#[test]
fn authority_claims_match_compiled_state_revisions_and_reject_mutations() {
    native::verify_with_controls(&[
        ("state-format", crate::state::FORMAT_REVISION),
        ("execution-abi", crate::state::EXECUTION_ABI_REVISION),
    ]);
}
