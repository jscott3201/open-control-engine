use super::*;

fn series(times: &[f64], values: &[bool]) -> IndicatorSeries {
    IndicatorSeries {
        times: times.to_vec(),
        values: values.to_vec(),
    }
}

#[test]
fn an_indicator_is_false_before_its_first_sample_and_held_after_each() {
    let fan = series(&[10.0, 20.0], &[true, false]);
    assert_eq!(
        active_at(&[&fan], &[0.0, 9.99, 10.0, 15.0, 20.0, 99.0]),
        [false, false, true, true, false, false]
    );
}

#[test]
fn several_indicators_are_anded() {
    let a = series(&[0.0, 10.0], &[true, false]);
    let b = series(&[0.0, 5.0], &[false, true]);
    assert_eq!(
        active_at(&[&a, &b], &[0.0, 5.0, 9.0, 10.0]),
        [false, true, true, false]
    );
}

#[test]
fn no_indicator_gates_nothing() {
    assert_eq!(active_at(&[], &[0.0, 1.0]), [true, true]);
    assert!(active_at(&[], &[]).is_empty());
}

#[test]
fn an_equal_time_pair_holds_its_later_value_from_that_instant() {
    let switch = series(&[0.0, 5.0, 5.0], &[false, false, true]);
    assert_eq!(active_at(&[&switch], &[4.0, 5.0]), [false, true]);
}

#[test]
fn only_exactly_zero_and_one_are_boolean_values() {
    assert_eq!(real_flag(0.0), Some(false));
    assert_eq!(real_flag(-0.0), Some(false));
    assert_eq!(real_flag(1.0), Some(true));
    for x in [0.5, 1.0 + f64::EPSILON, -1.0, f64::NAN, f64::INFINITY, 2.0] {
        assert_eq!(real_flag(x), None, "{x:?}");
    }
    assert_eq!(integer_flag(0), Some(false));
    assert_eq!(integer_flag(1), Some(true));
    assert_eq!(integer_flag(2), None);
    assert_eq!(integer_flag(-1), None);
}

#[test]
fn a_file_indicator_at_one_half_is_refused_naming_its_time_and_value() {
    let indicator = BoundIndicator {
        name: "fanSta.y".to_owned(),
        signal: IndicatorSignal::File(0),
    };
    let files = [TrendSeries {
        name: "fanSta.y".to_owned(),
        device: "Fan".to_owned(),
        times: vec![0.0, 30.0],
        values: SeriesValues::Real(vec![1.0, 0.5]),
    }];
    // A trace is not needed for a file indicator: an empty replay's would do, but none is read.
    let trace = empty_trace();
    let refused = indicator_series(&indicator, &trace, &[], &files, "references[0].indicators")
        .expect_err("0.5 is no Boolean");
    assert_eq!(refused.code(), "indicator-not-boolean");
    assert_eq!(refused.subject(), "references[0].indicators");
    assert!(
        refused.detail().contains("fanSta.y"),
        "{}",
        refused.detail()
    );
    assert!(refused.detail().contains("0.5"), "{}", refused.detail());
    assert!(refused.detail().contains("30.0"), "{}", refused.detail());

    let files = [TrendSeries {
        values: SeriesValues::Integer(vec![1, 0]),
        ..files[0].clone()
    }];
    let read = indicator_series(&indicator, &trace, &[], &files, "x").expect("0 and 1");
    assert_eq!(read.values, [true, false]);
    assert_eq!(read.times, [0.0, 30.0]);
}

/// A trace of no frame: the replay of one `CDL.Logical.Not` over no row.
fn empty_trace() -> Trace {
    use oce_replay::{
        Column, ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, InputMap, Replay, ReplayLimits,
        TimeColumn,
    };
    let document = br#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
      {"@id": "urn:doc#top", "@type": "S231:Block",
       "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
       "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
      {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
       "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
      {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
       "S231:isOfDataType": {"@id": "S231:Boolean"}},
      {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
       "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
      {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
       "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
      {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
       "S231:isOfDataType": {"@id": "S231:Boolean"}}
    ]}"#;
    let mut engine = oce_api::Engine::new();
    engine.load_cxf(document).expect("loads");
    let definitions = engine.input_definitions().expect("inputs");
    let columns = [ColumnSpec::new("u", ColumnKind::Boolean)];
    let map = InputMap::resolve(
        &definitions,
        &columns,
        &ColumnMap::by_name(&columns),
        &oce_replay::MissingPolicy::Refuse,
        &[],
    )
    .expect("maps");
    let source = ColumnarSource::new(
        &map,
        TimeColumn::Seconds(Vec::new()),
        vec![(
            "u".to_owned(),
            Column::Boolean {
                values: Vec::new(),
                valid: None,
            },
        )],
        &ReplayLimits::default(),
    )
    .expect("no row");
    let options = oce_replay::ReplayOptions::new(oce_replay::TimeBase::Recorded);
    let (_, trace) = Replay::new(engine, map, source, options)
        .expect("starts")
        .run()
        .expect("runs");
    trace
}
