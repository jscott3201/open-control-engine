use super::*;

#[test]
fn a_point_type_matches_its_port_type_exactly() {
    for (kind, value_type, same) in [
        ("Real", ValueType::Real, true),
        ("Integer", ValueType::Integer, true),
        ("Boolean", ValueType::Boolean, true),
        ("Real", ValueType::Integer, false),
        ("Integer", ValueType::Real, false),
        ("Boolean", ValueType::Integer, false),
        ("Real", ValueType::Boolean, false),
        ("Enum", ValueType::Integer, false),
        ("Real", ValueType::String, false),
    ] {
        assert_eq!(same_type(kind, value_type), same, "{kind} {value_type:?}");
    }
    assert_eq!(column_type(&ColumnKind::Real), "Real");
    assert_eq!(column_type(&ColumnKind::Enum("x".to_owned())), "Enum");
    assert_eq!(series_type(&SeriesValues::Integer(Vec::new())), "Integer");
}

fn check(
    units: Option<(&str, &str)>,
    port_unit: Option<&str>,
    quantity: Option<&str>,
) -> Option<&'static str> {
    check_point(
        "s",
        "urn:x#top.p",
        ValueType::Real,
        "Real",
        units,
        Labels {
            unit: port_unit,
            quantity,
        },
    )
    .err()
    .map(|error| {
        assert_eq!(error.code(), "unit-refused");
        assert_eq!(error.path(), Some("urn:x#top.p"));
        error.cause().expect("a unit code")
    })
}

#[test]
fn a_real_port_checks_its_unit_then_its_quantity() {
    // Agreeing units, a listed row that fits the quantity.
    assert_eq!(
        check(
            Some(("degF", "K")),
            Some("K"),
            Some("ThermodynamicTemperature")
        ),
        None
    );
    assert_eq!(
        check(
            Some(("degF_delta", "K")),
            Some("K"),
            Some("q:TemperatureDifference")
        ),
        None
    );
    assert_eq!(check(None, None, None), None);
    assert_eq!(
        check(Some(("K", "K")), Some("K"), Some("TemperatureDifference")),
        None
    );
    // The unit first, whatever the quantity says.
    assert_eq!(
        check(
            Some(("degF", "degC")),
            Some("K"),
            Some("TemperatureDifference")
        ),
        Some("port-unit-mismatch")
    );
    assert_eq!(check(None, Some("K"), None), Some("port-unit-mismatch"));
    assert_eq!(
        check(Some(("percent", "1")), None, None),
        Some("port-unit-mismatch")
    );
    // Then the quantity.
    assert_eq!(
        check(
            Some(("degF", "K")),
            Some("K"),
            Some("TemperatureDifference")
        ),
        Some("absolute-for-difference")
    );
    assert_eq!(
        check(
            Some(("degC_delta", "K")),
            Some("K"),
            Some("ThermodynamicTemperature")
        ),
        Some("difference-for-absolute")
    );
    // A port with no quantity takes any row.
    assert_eq!(check(Some(("degF", "K")), Some("K"), None), None);
    assert_eq!(check(Some(("degF_delta", "K")), Some("K"), None), None);
}

#[test]
fn a_discrete_port_is_checked_by_type_only() {
    let checked = check_point(
        "s",
        "urn:x#top.b",
        ValueType::Boolean,
        "Boolean",
        Some(("1", "1")),
        Labels::default(),
    );
    assert!(checked.is_ok());
    let refused = check_point(
        "s",
        "urn:x#top.b",
        ValueType::Boolean,
        "Real",
        None,
        Labels::default(),
    )
    .expect_err("a float onto a Boolean");
    assert_eq!(refused.code(), "value-type-mismatch");
}
