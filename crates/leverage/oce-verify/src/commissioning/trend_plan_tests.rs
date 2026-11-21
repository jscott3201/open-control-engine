//! How a source addresses its file, per format, and the one-column-per-point rule.

use super::*;
use crate::commissioning::limits::SpecLimits;

fn column(key: &str, point: &str) -> SourceColumn {
    SourceColumn {
        column: key.to_owned(),
        point: point.to_owned(),
    }
}

fn code(result: Result<(TimeAddress, Vec<Address>), SourceError>) -> &'static str {
    result.map_or_else(|error| error.code(), |_| "planned")
}

#[test]
fn a_combi_time_table_takes_positions_written_as_canonical_decimals() {
    let combi = TrendFormat::CombiTimeTable;
    let (time, places) = addresses(
        "a.mos",
        combi,
        None,
        &SourceTimeColumn::Index(1),
        &[column("10", "x"), column("3", "y")],
    )
    .expect("planned");
    assert_eq!(time, TimeAddress::Position(1));
    assert_eq!(places, [Address::Position(10), Address::Position(3)]);
    for key in [
        "03",
        "+3",
        "-3",
        "0",
        "3.0",
        " 3",
        "three",
        "",
        "4294967296",
    ] {
        let result = addresses(
            "a.mos",
            combi,
            None,
            &SourceTimeColumn::Index(1),
            &[column(key, "x")],
        );
        assert_eq!(code(result), "column-reference-invalid", "{key:?}");
    }
    let named_time = addresses(
        "a.mos",
        combi,
        None,
        &SourceTimeColumn::Name("time".to_owned()),
        &[],
    );
    assert_eq!(code(named_time), "column-reference-invalid");
    let as_time = addresses(
        "a.mos",
        combi,
        None,
        &SourceTimeColumn::Index(2),
        &[column("2", "x")],
    );
    assert_eq!(code(as_time), "column-reference-invalid");
}

#[test]
fn a_csv_takes_header_names_byte_for_byte_and_no_table() {
    let csv = TrendFormat::Csv;
    let time = SourceTimeColumn::Name("time".to_owned());
    let (_, places) = addresses(
        "a.csv",
        csv,
        None,
        &time,
        &[column("3", "x"), column(" T", "y")],
    )
    .expect("planned");
    // A header may be named `3`; on a csv source it is a name.
    assert_eq!(
        places,
        [
            Address::Name("3".to_owned()),
            Address::Name(" T".to_owned())
        ]
    );
    assert_eq!(
        code(addresses(
            "a.csv",
            csv,
            None,
            &SourceTimeColumn::Index(1),
            &[]
        )),
        "column-reference-invalid"
    );
    assert_eq!(
        code(addresses("a.csv", csv, Some("t"), &time, &[])),
        "column-reference-invalid"
    );
    assert_eq!(
        code(addresses("a.csv", csv, None, &time, &[column("time", "x")])),
        "column-reference-invalid"
    );
}

fn mapping(text: &str) -> PointMapping {
    PointMapping::from_json_str(text, &SpecLimits::default()).expect("the mapping loads")
}

#[test]
fn a_column_is_planned_from_its_mapped_point_or_refused_before_any_row() {
    let points = mapping(
        r#"[
        {"cdl": {"name": "T", "unit": "K", "type": "float"}, "device": {"name": "temp", "unit": "degF", "type": "float"}},
        {"cdl": {"name": "on", "type": "float"}, "device": {"name": "enable", "type": "bool"}},
        {"cdl": {"name": "n", "unit": "1", "type": "int"}, "device": {"name": "count", "unit": "percent", "type": "float"}},
        {"cdl": {"name": "k", "unit": "1", "type": "int"}, "device": {"name": "same", "unit": "1", "type": "int"}},
        {"cdl": {"name": "u", "unit": "K", "type": "float"}, "device": {"name": "odd", "unit": "degR", "type": "float"}},
        {"cdl": {"name": "v", "unit": "K", "type": "float"}, "device": {"name": "half", "type": "float"}}
    ]"#,
    );
    let by_device = DevicePoints::new(&points);
    let plan = |device: &str| by_device.plan(Address::Name("c".to_owned()), device, "f.csv");
    let planned = plan("temp").expect("planned");
    assert_eq!((planned.cdl.as_str(), planned.rule), ("T", CellRule::Real));
    assert_eq!(planned.units, Some(("degF".to_owned(), "K".to_owned())));
    assert!(
        plan("same").is_ok(),
        "the identity on an Integer point changes nothing"
    );
    let refusal = |device: &str| {
        let error = plan(device).expect_err("refused");
        (
            error.code(),
            error.cause(),
            error.file().map(str::to_owned),
            error.column().map(str::to_owned),
        )
    };
    assert_eq!(
        refusal("missing"),
        (
            "unmapped-point",
            None,
            Some("f.csv".to_owned()),
            Some("c".to_owned())
        )
    );
    assert_eq!(refusal("enable").0, "type-pair-refused");
    assert_eq!(refusal("enable").1, Some("type-pair-refused"));
    assert_eq!(refusal("count").0, "unit-on-discrete-point");
    assert_eq!(refusal("odd").1, Some("unknown-pair"));
    assert_eq!(refusal("half").1, Some("one-sided"));
    // Device names compare byte for byte.
    assert_eq!(refusal("Temp").0, "unmapped-point");
}

#[test]
fn two_columns_naming_one_device_point_in_one_role_are_refused() {
    let points = mapping(
        r#"[{"cdl": {"name": "T", "type": "float"}, "device": {"name": "temp", "type": "float"}}]"#,
    );
    let by_device = DevicePoints::new(&points);
    let source = |role, path: &str, keys: &[&str]| PlannedSource {
        role,
        path: path.to_owned(),
        format: TrendFormat::Csv,
        table: None,
        time: TimeAddress::Name("t".to_owned()),
        columns: keys
            .iter()
            .map(|key| {
                by_device
                    .plan(Address::Name((*key).to_owned()), "temp", path)
                    .expect("planned")
            })
            .collect(),
        hold_max_age_s: None,
        seconds_only: false,
    };
    // Two columns of one source: the owner's comment on #660, `"1": "A", "2": "A"`.
    let error = one_column_per_point(&[source(SourceRole::Inputs, "a.csv", &["x", "y"])])
        .expect_err("refused");
    assert_eq!(
        (error.code(), error.file(), error.column()),
        ("point-read-twice", Some("a.csv"), Some("y"))
    );
    // Two sources of one role.
    let error = one_column_per_point(&[
        source(SourceRole::Expected, "a.csv", &["x"]),
        source(SourceRole::Expected, "b.csv", &["x"]),
    ])
    .expect_err("refused");
    assert_eq!(
        (error.code(), error.file()),
        ("point-read-twice", Some("b.csv"))
    );
    // Two roles may read one point.
    assert!(
        one_column_per_point(&[
            source(SourceRole::Inputs, "a.csv", &["x"]),
            source(SourceRole::Indicators, "a.csv", &["x"]),
        ])
        .is_ok()
    );
}
