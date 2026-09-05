//! Source-owned absence invariants, independent of ledger ranges and hashes.

const RETIRED: [&str; 6] = [
    "oce_api::Engine<S>::load_modelica",
    "oce_api::Engine<S>::load_from_semantic",
    "oce_api::TemplateRef",
    "pub use oce_api::SemanticQuery",
    "pub oce_api::InputSource::Csv",
    "pub oce_api::AssertLevel::Error",
];

pub(super) fn validate(contents: &str) -> Result<(), &'static str> {
    for forbidden in RETIRED {
        if contents.lines().any(|line| line.contains(forbidden)) {
            return Err(forbidden);
        }
    }
    Ok(())
}

#[test]
fn reintroducing_each_retired_surface_is_rejected_even_with_a_new_baseline() {
    let current = include_str!("../public-api.txt");
    for forbidden in RETIRED {
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
