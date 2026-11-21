//! The published listings load as written (commissioning note, decision 3; "Run readings",
//! "OBC §12.7, read on 2026-11-20"): OBC §12.7 Listing 12.1 and Listing 12.2 verbatim, the copy
//! of Listing 12.1 with comments added, and the note's Building 33 worked spec.
//!
//! The two verbatim fixtures are the text of the listings as published (the fixture
//! directory's own notes record the page and the day it was read); their bytes are pinned here,
//! so an edit to either is a failure, not a quiet change of what "loads unchanged" means.

use oce_verify::{
    Envelope, FunnelReference, LBL_FIELD_IGNORED, Literal, OverrideKey, PointNameMapping,
    PointType, SPEC_FINDING_CODES, SourceTimeColumn, TrendFormat, glob_matches,
};

use crate::{fixture_text, load_mapping, loaded};

/// FNV-1a-128, written again here so the pins are not the crate's own hash.
fn reference_hash(bytes: &[u8]) -> u128 {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

/// One side of a mapped point: its name, its unit and its type.
type Side<'a> = (&'a str, Option<&'a str>, PointType);

const SUPPLY_U: &str =
    "Buildings.Controls.OBC.ASHRAE.G36.AHUs.SingleZone.VAV.SetPoints.Validation.Supply_u";

#[test]
fn the_verbatim_listings_are_the_published_bytes() {
    for (name, length, hash) in [
        (
            "listing_12_1.json",
            1_079,
            0x916a_423f_e920_05e2_0f69_4cc2_ed92_e7cd_u128,
        ),
        (
            "listing_12_2.json",
            1_953,
            0x1c98_712b_2d5f_17c8_7de9_2a24_04cd_e508_u128,
        ),
    ] {
        let text = fixture_text(name);
        assert_eq!(text.len(), length, "{name}");
        assert_eq!(reference_hash(text.as_bytes()), hash, "{name}");
        // As published: no comment, no schema tag, and nothing of this project's.
        assert!(!text.contains("//") && !text.contains("/*"), "{name}");
        assert!(!text.contains("schema") && !text.contains("oce/"), "{name}");
    }
}

#[test]
fn listing_12_1_loads_as_published() {
    let spec = loaded(&fixture_text("listing_12_1.json"));
    assert_eq!(spec.references().len(), 2);
    assert_eq!(spec.sampling_s(), Some(120.0));
    assert_eq!(spec.time_origin(), None);
    let tolerances = spec.tolerances().expect("the global tolerances");
    assert_eq!(
        [
            tolerances.rtolx(),
            tolerances.rtoly(),
            tolerances.atolx(),
            tolerances.atoly()
        ],
        [Some(0.002), Some(0.002), Some(10.0), Some(0.0)]
    );
    assert_eq!((tolerances.ltolx(), tolerances.ltoly()), (None, None));

    let first = &spec.references()[0];
    assert_eq!(first.model(), SUPPLY_U);
    assert_eq!(first.sequence(), "setPoiVAV");
    assert_eq!(
        first.point_name_mapping(),
        Some(&PointNameMapping::Path(
            "realControllerPointMapping.json".to_owned()
        ))
    );
    assert_eq!(first.controller_output(), Some("test/real_outputs.csv"));
    assert!(first.outputs().is_empty() && first.indicators().is_empty());
    assert_eq!(first.sampling_s(), None);
    assert_eq!(spec.sampling_for(first), Some(120.0), "the global sampling");

    let second = &spec.references()[1];
    assert_eq!(second.model(), SUPPLY_U);
    assert_eq!(second.sequence(), "setPoiVAV1");
    assert_eq!(second.sampling_s(), Some(60.0));
    assert_eq!(spec.sampling_for(second), Some(60.0), "its own sampling");
    // One mapping file serves both references.
    assert_eq!(second.point_name_mapping(), first.point_name_mapping());

    // What an LBL spec does not have is absent, and it still loads.
    for reference in spec.references() {
        assert_eq!(reference.cxf(), None);
        assert_eq!(reference.window(), None);
        assert_eq!(reference.parameters(), None);
        assert_eq!(reference.inputs(), None);
        assert_eq!(reference.expected(), None);
        assert_eq!(reference.indicator_sources(), None);
        assert_eq!(reference.envelope(), None);
        assert_eq!(reference.funnel_reference(), None);
    }
}

#[test]
fn listing_12_1s_glob_key_and_outside_indicator_load_unchanged() {
    let spec = loaded(&fixture_text("listing_12_1.json"));
    let second = &spec.references()[1];
    assert_eq!(second.outputs().len(), 1);
    let output = &second.outputs()[0];
    assert_eq!(output.key(), "setPoiVAV1.TSup*");
    assert_eq!(output.tolerances().atoly(), Some(0.5));
    assert_eq!(
        output.tolerances().atolx(),
        None,
        "left to the global value"
    );
    let key = OverrideKey::parse(output.key(), second.sequence()).expect("the prefix");
    assert_eq!(key.rest(), "TSup*");
    assert!(key.is_glob());
    assert!(glob_matches(key.rest(), "TSupHeaEco") && glob_matches(key.rest(), "TSup"));
    assert!(!glob_matches(key.rest(), "yHea"));
    // The indicator is a signal of another instance of the enclosing model: it carries no
    // `setPoiVAV1.` prefix, and is kept as written for an `indicatorSources` column to supply.
    assert_eq!(second.indicators().len(), 1);
    assert_eq!(second.indicators()[0].key(), "setPoiVAV1.TSup*");
    assert_eq!(second.indicators()[0].names(), ["fanSta.y"]);
    assert_eq!(OverrideKey::parse("fanSta.y", second.sequence()), None);
}

#[test]
fn the_scenario_one_fields_are_findings_in_the_stated_order() {
    let spec = loaded(&fixture_text("listing_12_1.json"));
    let findings: Vec<(&str, String)> = spec
        .findings()
        .iter()
        .map(|finding| (finding.code(), finding.at().to_owned()))
        .collect();
    // The top-level fields sorted by name, then each reference in index order with its fields
    // sorted by name. The listing writes `modelJsonDirectory` before `controller`.
    let expected = [
        "controller",
        "modelJsonDirectory",
        "references[0].generateJson",
        "references[0].runController",
        "references[1].generateJson",
        "references[1].runController",
    ];
    assert_eq!(
        findings,
        expected.map(|at| (LBL_FIELD_IGNORED, at.to_owned()))
    );
    assert_eq!(LBL_FIELD_IGNORED, "lbl-field-ignored");
    assert_eq!(SPEC_FINDING_CODES, ["lbl-field-ignored"]);
    // `controllerOutput` is honoured, not ignored: it is no finding.
    assert!(
        findings
            .iter()
            .all(|(_, at)| !at.contains("controllerOutput"))
    );

    // What one reference's receipt carries: the top level, then that reference.
    let ats = |index: usize| -> Vec<String> {
        spec.findings_for(index)
            .iter()
            .map(|finding| finding.at().to_owned())
            .collect()
    };
    assert_eq!(
        ats(0),
        [
            "controller",
            "modelJsonDirectory",
            "references[0].generateJson",
            "references[0].runController"
        ]
    );
    assert_eq!(
        ats(1),
        [
            "controller",
            "modelJsonDirectory",
            "references[1].generateJson",
            "references[1].runController"
        ]
    );
    assert_eq!(ats(2), ["controller", "modelJsonDirectory"], "past the end");
    assert_eq!(ats(usize::MAX), ["controller", "modelJsonDirectory"]);
}

#[test]
fn the_order_of_the_findings_does_not_depend_on_the_files_key_order() {
    let forward = r#"{"controller": {}, "modelJsonDirectory": "d", "references": [
        {"generateJson": true, "runController": false, "model": "M", "sequence": "a"},
        {"model": "M", "sequence": "b"},
        {"runController": true, "model": "M", "sequence": "c"}]}"#;
    let backward = r#"{"references": [
        {"sequence": "a", "model": "M", "runController": false, "generateJson": true},
        {"sequence": "b", "model": "M"},
        {"sequence": "c", "model": "M", "runController": true}],
        "modelJsonDirectory": "d", "controller": {}}"#;
    let expected = [
        "controller",
        "modelJsonDirectory",
        "references[0].generateJson",
        "references[0].runController",
        "references[2].runController",
    ];
    for text in [forward, backward] {
        let ats: Vec<String> = loaded(text)
            .findings()
            .iter()
            .map(|finding| finding.at().to_owned())
            .collect();
        assert_eq!(ats, expected, "{text}");
    }
    assert_eq!(loaded(forward).spec_id(), loaded(backward).spec_id());
    // A spec with none of the four fields has no finding; one field is one finding.
    assert!(
        loaded(r#"{"references": [{"model": "M", "sequence": "s"}]}"#)
            .findings()
            .is_empty()
    );
    let only = loaded(r#"{"references": [], "modelJsonDirectory": null}"#).findings();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].at(), "modelJsonDirectory");
    let only = loaded(r#"{"references": [], "controller": 1}"#).findings();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].at(), "controller");
}

#[test]
fn the_commented_copy_loads_as_the_listing_does() {
    let commented = fixture_text("listing_12_1_commented.json");
    assert!(commented.contains("// ") && commented.contains("/* "));
    assert!(
        !commented.ends_with('\n'),
        "its last line is a comment with no line feed"
    );
    let spec = loaded(&commented);
    assert_eq!(spec, loaded(&fixture_text("listing_12_1.json")));
    assert_eq!(spec.references()[1].outputs()[0].key(), "setPoiVAV1.TSup*");
    assert_eq!(spec.findings().len(), 6);
}

#[test]
fn listing_12_2_loads_with_its_bare_names() {
    let mapping = load_mapping(&fixture_text("listing_12_2.json")).expect("Listing 12.2 loads");
    let rows: Vec<(Side<'_>, Side<'_>)> = mapping
        .points()
        .iter()
        .map(|pair| {
            let (cdl, device) = (pair.cdl(), pair.device());
            (
                (cdl.name(), cdl.unit(), cdl.point_type()),
                (device.name(), device.unit(), device.point_type()),
            )
        })
        .collect();
    // The six temperatures: kelvin in CDL, degrees Fahrenheit on the device, floats both.
    let temperature = |cdl: &'static str, device: &'static str| {
        (
            (cdl, Some("K"), PointType::Float),
            (device, Some("degF"), PointType::Float),
        )
    };
    // "Some points may not have a unit, but only have a type."
    let switch = |cdl: &'static str, device: &'static str| {
        ((cdl, None, PointType::Int), (device, None, PointType::Bool))
    };
    assert_eq!(
        rows,
        [
            temperature("TZonCooSetOcc", "Occupied Cooling Setpoint_1"),
            temperature("TZonHeaSetOcc", "Occupied Heating Setpoint_1"),
            temperature("TZonCooSetUno", "Unoccupied Cooling Setpoint_1"),
            temperature("TZonHeaSetUno", "Unoccupied Heating Setpoint_1"),
            temperature("setAdj", "setpt_adj_1"),
            temperature("heaSetAdj", "Heating Adjustment_1"),
            switch("uOccSen", "occ_sensor_bni_1"),
            switch("uWinSta", "window_sw_1"),
            temperature("TZonCooSet", "Effective Cooling Setpoint_1"),
            temperature("TZonHeaSet", "Effective Heating Setpoint_1"),
        ]
    );
    // No CDL name carries a sequence prefix: a name is kept exactly as the listing writes it.
    for pair in mapping.points() {
        assert!(!pair.cdl().name().contains('.'), "{}", pair.cdl().name());
    }
}

#[test]
fn the_building_33_worked_spec_loads_whole() {
    let spec = loaded(&fixture_text("building_33.json"));
    assert_eq!(
        spec.time_origin().map(oce_replay::TimeOrigin::as_str),
        Some("2018-06-07T00:00:00-07:00")
    );
    // Midnight PDT is 07:00 UTC.
    assert_eq!(
        spec.time_origin().map(oce_replay::TimeOrigin::unix_nanos),
        Some(1_528_354_800_i128 * 1_000_000_000)
    );
    assert_eq!(spec.sampling_s(), Some(5.0));
    let global = spec.tolerances().expect("the global tolerances");
    assert_eq!(global.or_zero(), [1.0, 0.0, 0.0, 0.0, 0.0, 0.01]);
    assert!(spec.findings().is_empty(), "no Scenario 1 field");

    assert_eq!(spec.references().len(), 1);
    let reference = &spec.references()[0];
    assert_eq!(
        reference.model(),
        "Buildings.Utilities.Plotters.Examples.ControlsVerification_CoolingCoilValve"
    );
    assert_eq!(reference.sequence(), "cooValSta");
    assert_eq!(
        reference.cxf(),
        Some("cxf/Buildings/Utilities/Plotters/Examples/BaseClasses/CoolingCoilValve.json")
    );
    assert_eq!(
        reference.point_name_mapping(),
        Some(&PointNameMapping::Path("b33-points.json".to_owned()))
    );
    assert_eq!(
        reference.parameters().map(<[_]>::len),
        Some(0),
        "present and empty"
    );
    assert_eq!(reference.envelope(), Some(Envelope::Funnel));
    assert_eq!(reference.funnel_reference(), Some(FunnelReference::Engine));
    let window = reference.window().expect("the window");
    assert_eq!((window.start_s(), window.stop_s()), (0.0, 86_400.0));
    assert_eq!(spec.sampling_for(reference), Some(5.0));

    let inputs = reference.inputs().expect("the inputs");
    let files: Vec<(&str, &str, Option<f64>)> = inputs
        .iter()
        .map(|source| {
            assert_eq!(source.format(), TrendFormat::CombiTimeTable);
            assert_eq!(source.time_column(), &SourceTimeColumn::Index(1));
            assert_eq!(source.table(), None);
            assert_eq!(source.columns().len(), 1);
            assert_eq!(source.columns()[0].column(), "3");
            (
                source.path(),
                source.columns()[0].point(),
                source.hold_max_age_s(),
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("data/Supply_Air_Temp.mos", "Supply Air Temp", Some(10.0)),
            ("data/SA_Clg_Stpt.mos", "SA Clg Stpt", Some(10.0)),
            ("data/OA_Temp.mos", "OA Temp", Some(10.0)),
            ("data/VFD_Fan_Feedback.mos", "VFD Fan Feedback", Some(10.0)),
            // The fan-enable trend logs one sample a day, hence its 25-hour hold.
            ("data/VFD_Fan_Enable.mos", "VFD Fan Enable", Some(90_000.0)),
        ]
    );
    let expected = reference.expected().expect("the expected sources");
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].path(), "data/Clg_Coil_Valve.mos");
    assert_eq!(expected[0].columns()[0].point(), "Clg Coil Valve");
    assert_eq!(
        expected[0].hold_max_age_s(),
        None,
        "a missing sample refuses its frame"
    );
    assert_eq!(reference.indicator_sources(), None);
    assert_eq!(reference.controller_output(), None);

    // The exact key and the glob over the same output, as the note writes them.
    assert_eq!(reference.outputs()[0].key(), "cooValSta.yCooVal");
    assert_eq!(reference.outputs()[0].tolerances().atolx(), Some(1.0));
    assert_eq!(reference.outputs()[0].tolerances().rtoly(), Some(0.01));
    assert_eq!(reference.indicators()[0].key(), "cooValSta.yCoo*");
    assert_eq!(reference.indicators()[0].names(), ["cooValSta.uFanSta"]);
}

#[test]
fn every_field_of_the_hand_written_spec_is_read() {
    let spec = loaded(&fixture_text("every_field.json"));
    assert_eq!(spec.sampling_s(), Some(120.0));
    assert_eq!(
        spec.time_origin().map(oce_replay::TimeOrigin::as_str),
        Some("2018-06-07T00:00:00.250-07:00")
    );
    let global = spec.tolerances().expect("the global tolerances");
    // atolx, atoly, ltolx, ltoly, rtolx, rtoly; `-0` and `-0.0` read as zero.
    assert_eq!(
        global.or_zero().map(f64::to_bits),
        [10.0, 0.0, 0.000_01, 0.000_1, 0.0, 0.002].map(f64::to_bits)
    );
    let reference = &spec.references()[0];
    assert_eq!(reference.sequence(), "ahu.1", "the escaped dot is a dot");
    assert_eq!(
        reference.model(),
        "Buildings./*not a comment*/ \u{c9}cole // nor this \u{1f321}"
    );
    assert_eq!(reference.cxf(), Some("cxf/a b/s\u{e9}quence.json"));
    assert_eq!(reference.envelope(), Some(Envelope::AlignedGrid));
    assert_eq!(reference.funnel_reference(), Some(FunnelReference::Trend));
    assert_eq!(reference.sampling_s(), Some(0.5));
    let window = reference.window().expect("the window");
    assert_eq!(
        window.start_s().to_bits(),
        0.0_f64.to_bits(),
        "a time has no sign"
    );
    assert_eq!(window.stop_s(), 86_400.0);

    // Parameters are sorted by path bytes and keep the kind each value was written as.
    let parameters: Vec<(&str, Literal)> = reference
        .parameters()
        .expect("the parameters")
        .iter()
        .map(|each| (each.path(), each.value()))
        .collect();
    assert_eq!(
        parameters,
        [
            ("alpha.Ti", Literal::Real(30.0)),
            ("alpha.k", Literal::Real(-0.25)),
            ("mid.enabled", Literal::Boolean(true)),
            ("zeta.k", Literal::Integer(1)),
            ("\u{e9}t\u{e9}.n", Literal::Integer(-7)),
        ]
    );
    // Override keys keep the file's order, which is not sorted order.
    let keys: Vec<&str> = reference.outputs().iter().map(|each| each.key()).collect();
    assert_eq!(keys, ["ahu.1.y*", "ahu.1.yFan", "ahu.1.?Dam", "ahu.1.TSup"]);
    assert!(reference.outputs()[2].tolerances().is_empty());
    let rules: Vec<(&str, &[String])> = reference
        .indicators()
        .iter()
        .map(|rule| (rule.key(), rule.names()))
        .collect();
    assert_eq!(rules[0], ("ahu.1.z", &[][..]));
    assert_eq!(rules[1].1, ["ahu.1.uFan", "fanSta.y", "ahu.1.uFan"]);
    assert_eq!(rules[2], ("ahu.1.TSup", &["occupied".to_owned()][..]));

    let Some(PointNameMapping::Inline(mapping)) = reference.point_name_mapping() else {
        panic!("the mapping is inline");
    };
    assert_eq!(mapping.points().len(), 3);
    assert_eq!(mapping.points()[2].device().name(), "VFD \u{b0} Cmd");
    assert_eq!(mapping.points()[2].device().unit(), Some("%"));

    let inputs = reference.inputs().expect("the inputs");
    assert_eq!(inputs[0].format(), TrendFormat::Csv);
    assert_eq!(
        inputs[0].time_column(),
        &SourceTimeColumn::Name("Time Stamp".to_owned())
    );
    assert_eq!(inputs[0].path(), "trends/in puts.csv");
    assert_eq!(inputs[0].hold_max_age_s(), Some(15.0));
    let columns: Vec<(&str, &str)> = inputs[0]
        .columns()
        .iter()
        .map(|each| (each.column(), each.point()))
        .collect();
    assert_eq!(
        columns,
        [
            ("Occ", "occ_sensor_bni_1"),
            ("Supply Air Temp (\u{b0}F)", "Supply Air Temp")
        ]
    );
    assert_eq!(inputs[1].table(), Some("tab1"));
    assert_eq!(inputs[1].time_column(), &SourceTimeColumn::Index(1));
    // Sorted by the key's bytes: "10" is before "2".
    let positions: Vec<&str> = inputs[1]
        .columns()
        .iter()
        .map(|each| each.column())
        .collect();
    assert_eq!(positions, ["10", "2", "3"]);
    assert_eq!(
        inputs[1].hold_max_age_s().map(f64::to_bits),
        Some(0.0_f64.to_bits())
    );
    assert_eq!(reference.expected().map(<[_]>::len), Some(0));
    let indicator_sources = reference
        .indicator_sources()
        .expect("the indicator sources");
    assert!(indicator_sources[0].columns().is_empty());
    assert_eq!(reference.controller_output(), Some("trends/out.csv"));

    // The smallest reference, and one whose every list is present and empty.
    assert_eq!(
        spec.references()[1].model(),
        "",
        "an empty label is a label"
    );
    assert_eq!(spec.sampling_for(&spec.references()[1]), Some(120.0));
    let third = &spec.references()[2];
    assert_eq!(third.inputs().map(<[_]>::len), Some(0));
    assert_eq!(third.parameters().map(<[_]>::len), Some(0));
    let instant = third.window().expect("the window");
    assert_eq!(
        instant.start_s(),
        instant.stop_s(),
        "a window of one instant"
    );
    let ats: Vec<String> = spec.findings().iter().map(|f| f.at().to_owned()).collect();
    assert_eq!(
        ats,
        [
            "controller",
            "modelJsonDirectory",
            "references[0].generateJson",
            "references[0].runController",
            "references[2].generateJson"
        ]
    );
}
