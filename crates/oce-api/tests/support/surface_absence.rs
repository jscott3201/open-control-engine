//! Source-owned absence invariants, independent of ledger ranges and hashes.

const RETIRED: &[&str] = &[
    "oce_api::Engine<S>::load_modelica",
    "oce_api::Engine<S>::load_from_semantic",
    "oce_api::TemplateRef",
    "pub use oce_api::SemanticQuery",
    "oce_api::InputSource",
    "pub oce_api::AssertLevel::Error",
    "oce_api::Engine<S>::tick(",
    "oce_api::Engine<S>::tick_with(",
    "oce_api::Engine<S>::set_input(",
    "oce_api::Engine<S>::simulate(",
    "oce_api::Engine<S>::step_realtime(",
    "oce_api::Engine<S>::outputs(",
    "oce_api::Engine<S>::set_realtime_epoch_unix_nanos(",
    "oce_api::Engine<S>::realtime_epoch_unix_nanos(",
    "oce_api::Outputs",
    "oce_api::OutputTrace",
    "oce_api::SimSpec",
    "oce_api::SimMetrics",
    "oce_api::StepReport",
    "oce_api::CollectSpec",
    "oce_api::OcError::RealtimeEpochUnset",
    "oce_api::OcError::RealtimeInstantUnrepresentable",
];

pub(super) fn validate(contents: &str) -> Result<(), &'static str> {
    for &forbidden in RETIRED {
        if contents.lines().any(|line| line.contains(forbidden)) {
            return Err(forbidden);
        }
    }
    Ok(())
}

#[test]
fn reintroducing_each_retired_surface_is_rejected_even_with_a_new_baseline() {
    let current = include_str!("../public-api.txt");
    for &forbidden in RETIRED {
        let injected = format!("{current}\n{forbidden}\n");
        assert_ne!(injected, current);
        assert_eq!(validate(&injected), Err(forbidden));
    }
}

#[test]
fn associated_error_types_and_the_conditional_query_namespace_are_not_enum_variants() {
    assert_eq!(
        validate(concat!(
            "pub type oce_api::AssertLevel::Error = core::convert::Infallible\n",
            "pub type oce_api::AssertLevel::Error = <U as core::convert::TryFrom<T>>::Error\n",
            "pub use oce_api::oce_store\n",
            "pub enum oce_store::SemanticQuery\n",
        )),
        Ok(())
    );
}
