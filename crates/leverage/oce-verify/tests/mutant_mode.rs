//! Mutant mode (vectors note, "Mutant mode"; decisions 27 to 30): the AHU-0016 set with its three
//! hand-written mutants (`tests/fixtures/AHU-0016/`), the score, and the receipt's `mutants`
//! section. The golden receipt of that run is in `tests/receipt_goldens.rs`.

mod support;

use oce_replay::EngineRefusal;
use oce_verify::{
    Failure, MutantSection, MutantStatus, ReceiptValue, ScoreStatus, VectorSet, VerifyLimits,
};

use support::{
    AHU_0016, ahu_mutants, body, edited, library_subject, library_vectors, mutant, mutant_set,
    run_mutants,
};

fn section(receipt: &oce_verify::Receipt) -> &MutantSection {
    body(receipt)
        .mutants
        .as_ref()
        .expect("the set lists mutants")
}

fn scored() -> oce_verify::Receipt {
    run_mutants(&mutant_set(), &library_subject("AHU-0016"), &ahu_mutants())
        .unwrap_or_else(|error| panic!("{}: {error}", error.code()))
        .receipt
}

/// The status, catching scenario and first failure's code and time of each entry.
type Outcome<'a> = (&'a str, MutantStatus, Option<(&'a str, &'static str, u64)>);

fn outcomes(section: &MutantSection) -> Vec<Outcome<'_>> {
    section
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.status,
                entry.caught_by.as_ref().map(|caught| {
                    (
                        caught.scenario.as_str(),
                        caught.failure.code(),
                        caught.failure.t().to_bits(),
                    )
                }),
            )
        })
        .collect()
}

#[test]
fn the_three_mutants_score_as_the_note_reads_them() {
    let receipt = scored();
    let section = section(&receipt);
    assert_eq!(section.status, ScoreStatus::Scored);
    assert_eq!((section.total, section.caught, section.survived), (3, 2, 1));
    assert_eq!(
        outcomes(section),
        [
            (
                "mutants/threshold_inclusive.cxf.json",
                MutantStatus::Caught,
                Some(("threshold_edge", "value-mismatch", 900.0_f64.to_bits()))
            ),
            (
                "mutants/no_persistence.cxf.json",
                MutantStatus::Caught,
                Some(("both_open", "value-mismatch", 0.0_f64.to_bits()))
            ),
            ("mutants/late_alarm.cxf.json", MutantStatus::Survived, None),
        ]
    );
    // The first failure on `threshold_inclusive` is the note's worked record.
    let Some(caught) = &section.entries[0].caught_by else {
        unreachable!("caught above")
    };
    let Failure::ValueMismatch {
        expectation,
        output,
        expected,
        actual,
        ..
    } = &caught.failure
    else {
        panic!("{:?}", caught.failure);
    };
    assert_eq!(*expectation, 0);
    assert_eq!(output, &format!("{AHU_0016}yFault"));
    assert!(expected.bit_eq(&ReceiptValue::Boolean(false)));
    assert!(actual.bit_eq(&ReceiptValue::Boolean(true)));
    // The score never changes `passed`: the subject passed, and a mutant survived.
    assert!(receipt.header.passed);
}

#[test]
fn each_entry_records_the_mutants_digest_and_content_id() {
    let receipt = scored();
    for ((path, bytes), entry) in ahu_mutants().iter().zip(&section(&receipt).entries) {
        assert_eq!(&entry.path, path);
        assert_eq!(
            entry.digest,
            format!("fnv1a128:{:032x}", reference_hash(bytes))
        );
        let mut engine = oce_api::Engine::new();
        engine.load_cxf(bytes).expect("the mutant loads");
        assert_eq!(
            entry.content_id,
            engine.content_id().expect("an id").to_string()
        );
        assert_ne!(entry.content_id, receipt.header.content_id);
    }
}

/// FNV-1a-128, written again here so the digest is checked against something other than the
/// crate's own hash.
fn reference_hash(bytes: &[u8]) -> u128 {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

#[test]
fn against_a_subject_that_does_not_pass_no_mutant_runs() {
    // `both_open`'s first window ending at 900 s fails the subject itself.
    let failing = mutant_set().replacen(r#""to_s": 840"#, r#""to_s": 900"#, 1);
    assert_ne!(failing, mutant_set());
    let receipt = run_mutants(&failing, &library_subject("AHU-0016"), &ahu_mutants())
        .expect("a receipt")
        .receipt;
    assert!(!receipt.header.passed);
    let section = section(&receipt);
    assert_eq!(section.status, ScoreStatus::NotRun);
    assert_eq!((section.total, section.caught, section.survived), (3, 0, 0));
    for entry in &section.entries {
        assert_eq!(entry.status, MutantStatus::NotRun, "{}", entry.path);
        assert!(entry.caught_by.is_none());
        assert!(entry.content_id.starts_with("oce:canon1:"));
    }
    // A subject that checks nothing (`no_data`) is not a pass either.
    let nothing = r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800},
        "mutants": [{"path": "mutants/late_alarm.cxf.json"}],
        "scenarios": [{"name": "s", "inputs": {"htg_vlv_cmd": 0.0, "clg_vlv_cmd": 0.0}, "expect": []}]}"#;
    let late = vec![(
        "mutants/late_alarm.cxf.json".to_owned(),
        mutant("late_alarm"),
    )];
    let receipt = run_mutants(nothing, &library_subject("AHU-0016"), &late)
        .expect("a receipt")
        .receipt;
    let section = body(&receipt).mutants.as_ref().expect("listed");
    assert_eq!(section.status, ScoreStatus::NotRun, "no_data is not a pass");
    assert_eq!(section.entries[0].status, MutantStatus::NotRun);
}

#[test]
fn a_mutant_that_narrows_an_input_bound_so_a_frame_is_refused_is_caught() {
    // The heating valve may not pass 25 in the mutant; `normal_heating` holds it at 45.
    let narrowed = edited(
        &library_subject("AHU-0016"),
        &[(
            r#""@id": "urn:cxf-library:ahu-fc-050#ahu_fc_050.htg_vlv_cmd",
      "@type": "S231:RealInput","#,
            r#""@id": "urn:cxf-library:ahu-fc-050#ahu_fc_050.htg_vlv_cmd",
      "@type": "S231:RealInput",
      "S231:max": 25.0,"#,
        )],
    );
    let mut mutants = ahu_mutants();
    mutants[2].1 = narrowed;
    let receipt = run_mutants(&mutant_set(), &library_subject("AHU-0016"), &mutants)
        .expect("a frame refusal is a catch, not an error")
        .receipt;
    let entry = &section(&receipt).entries[2];
    assert_eq!(entry.status, MutantStatus::Caught);
    let caught = entry.caught_by.as_ref().expect("caught");
    assert_eq!(caught.scenario, "normal_heating");
    assert!(matches!(
        caught.failure,
        Failure::FrameRefused { t, refusal: EngineRefusal::InputDomain } if t.to_bits() == 0
    ));
}

#[test]
fn a_mutants_run_stops_at_its_first_failing_scenario_in_file_order() {
    // `no_persistence` alarms at once whenever both valves are open: `both_open` is the first
    // such scenario, and `transient_overlap`, later in the file, fails on it too.
    let receipt = scored();
    let caught = section(&receipt).entries[1]
        .caught_by
        .as_ref()
        .expect("caught");
    assert_eq!(caught.scenario, "both_open");
    // Moving `transient_overlap` first moves the catch with it.
    let set = VectorSet::from_json_bytes(mutant_set().as_bytes(), &VerifyLimits::default())
        .expect("loads");
    let mut document: serde_json::Value =
        serde_json::from_str(&set.to_canonical_json()).expect("JSON");
    let scenarios = document["scenarios"].as_array_mut().expect("a list");
    let last = scenarios.pop().expect("a scenario");
    scenarios.insert(0, last);
    let reordered = serde_json::to_string(&document).expect("JSON");
    let receipt = run_mutants(&reordered, &library_subject("AHU-0016"), &ahu_mutants())
        .expect("a receipt")
        .receipt;
    let caught = section(&receipt).entries[1]
        .caught_by
        .as_ref()
        .expect("caught");
    assert_eq!(caught.scenario, "transient_overlap");
}

#[test]
fn the_spec_id_covers_the_mutant_paths_and_the_receipt_covers_their_bytes() {
    let load = |text: &str| {
        VectorSet::from_json_bytes(text.as_bytes(), &VerifyLimits::default()).expect("loads")
    };
    let listed = load(&mutant_set());
    let in_place = load(&String::from_utf8(library_vectors("AHU-0016")).expect("UTF-8"));
    assert_ne!(
        listed.spec_id(),
        in_place.spec_id(),
        "the listing is in the set"
    );
    let another = mutant_set().replacen(
        r#""mutants": ["#,
        r#""mutants": [
    { "path": "mutants/fourth.cxf.json" },"#,
        1,
    );
    assert_ne!(
        load(&another).spec_id(),
        listed.spec_id(),
        "a path added moves it"
    );
    // Other bytes under the same paths: the same `spec_id`, another digest and receipt id.
    let mut changed = ahu_mutants();
    changed[2].1 = edited(&changed[2].1, &[(r#""930.0""#, r#""960.0""#)]);
    let first = scored();
    let second = run_mutants(&mutant_set(), &library_subject("AHU-0016"), &changed)
        .expect("a receipt")
        .receipt;
    assert_eq!(first.header.spec_id, second.header.spec_id);
    assert_ne!(
        section(&first).entries[2].digest,
        section(&second).entries[2].digest
    );
    assert_ne!(first.header.receipt_id, second.header.receipt_id);
}

#[test]
fn the_fixture_set_is_the_in_place_set_with_the_mutants_listed() {
    // Its clock and scenarios must stay those of the facade's copy: change that copy and this
    // test fails until the fixture follows. The facade's copy lists no mutant.
    let load =
        |bytes: &[u8]| VectorSet::from_json_bytes(bytes, &VerifyLimits::default()).expect("loads");
    let fixture = load(mutant_set().as_bytes());
    let in_place = load(&library_vectors("AHU-0016"));
    assert_eq!(fixture.clock(), in_place.clock());
    assert_eq!(fixture.scenarios(), in_place.scenarios());
    assert_eq!(fixture.params(), in_place.params());
    assert_eq!(in_place.mutants(), None);
    let listed = fixture.mutants().expect("listed");
    let paths: Vec<&str> = listed.iter().map(|mutant| mutant.path()).collect();
    assert_eq!(
        paths,
        [
            "mutants/threshold_inclusive.cxf.json",
            "mutants/no_persistence.cxf.json",
            "mutants/late_alarm.cxf.json"
        ]
    );
    for mutant in listed {
        assert!(
            mutant
                .description()
                .is_some_and(|text| text.starts_with("Hand edit of AHU-0016 rule.cxf.jsonld")),
            "{} carries its provenance",
            mutant.path()
        );
    }
}

#[test]
fn an_empty_mutant_list_writes_no_section() {
    let set = mutant_set();
    let start = set.find(r#""mutants": ["#).expect("the list");
    let end = start + set[start..].find("\n  ],\n").expect("its end") + "\n  ],".len();
    let empty = format!("{}\"mutants\": [],{}", &set[..start], &set[end..]);
    let receipt = run_mutants(&empty, &library_subject("AHU-0016"), &[])
        .expect("a receipt")
        .receipt;
    assert!(body(&receipt).mutants.is_none());
    assert!(!receipt.to_canonical_json().contains("\"mutants\""));
}
