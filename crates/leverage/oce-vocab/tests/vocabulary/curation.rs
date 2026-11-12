//! The rules between the two files: curation is additive, names only what exists, and leaves
//! nothing undescribed. Each refusal is a typed [`VocabError`]; the seed itself is pinned.

use std::collections::{BTreeMap, BTreeSet};

use oce_vocab::{
    CURATED_JSON, CuratedClass, CuratedDescription, CuratedVocabulary, GENERATED_JSON,
    GeneratedVocabulary, Note, NoteKind, TextOrigin, VocabError, Vocabulary, class_vocab,
    vocabulary,
};

fn generated() -> GeneratedVocabulary {
    GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json")
}

fn curated() -> CuratedVocabulary {
    CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json")
}

/// The join's refusal of the embedded generated file with an edited curated file.
fn refused(edit: impl FnOnce(&mut CuratedVocabulary)) -> VocabError {
    let mut curated = curated();
    edit(&mut curated);
    // The edit must survive the reader, so the refusal is the join's.
    let curated = CuratedVocabulary::from_json(&curated.to_json()).expect("still readable");
    Vocabulary::from_parts(generated(), curated).expect_err("refused")
}

fn class<'a>(curated: &'a mut CuratedVocabulary, class_path: &str) -> &'a mut CuratedClass {
    curated
        .classes
        .iter_mut()
        .find(|class| class.class_path == class_path)
        .unwrap_or_else(|| panic!("{class_path} is curated"))
}

fn description(subject: &str) -> CuratedDescription {
    CuratedDescription {
        evidence: "a test".to_owned(),
        subject: subject.to_owned(),
        text: "A better description".to_owned(),
    }
}

fn note(subject: &str) -> Note {
    Note {
        evidence: "a test".to_owned(),
        kind: NoteKind::Pitfall,
        subject: subject.to_owned(),
        text: "A note".to_owned(),
    }
}

fn about(class: &str, subject: &str) -> (String, String) {
    (class.to_owned(), subject.to_owned())
}

#[test]
fn the_embedded_files_join() {
    Vocabulary::from_json(GENERATED_JSON, CURATED_JSON).expect("the embedded pair joins");
    assert_eq!(vocabulary().generated(), &generated());
    assert_eq!(vocabulary().curated(), &curated());
}

#[test]
fn a_curated_description_never_replaces_a_generated_one() {
    for (class_path, subject) in [
        ("CDL.Reals.Subtract", "u1"),
        ("CDL.Reals.MatrixGain", "K"),
        ("CDL.Logical.TrueDelay", "delayTime"),
    ] {
        let error = refused(|curated| {
            class(curated, class_path)
                .descriptions
                .push(description(subject));
        });
        let (class, subject) = about(class_path, subject);
        assert_eq!(
            error,
            VocabError::CuratedReplacesGenerated { class, subject }
        );
    }
    // The same for a class that was not curated before, as the design note's own example of a
    // poor upstream description.
    let error = refused(|curated| {
        curated.classes.insert(
            0,
            CuratedClass {
                class_path: "CDL.Reals.Sources.Constant".to_owned(),
                descriptions: vec![description("k")],
                notes: Vec::new(),
            },
        );
    });
    let (class, subject) = about("CDL.Reals.Sources.Constant", "k");
    assert_eq!(
        error,
        VocabError::CuratedReplacesGenerated { class, subject }
    );
}

#[test]
fn a_curated_entry_names_only_an_existing_class_and_subject() {
    let error = refused(|curated| {
        curated.classes.push(CuratedClass {
            class_path: "CDL.Reals.Subtracts".to_owned(),
            descriptions: Vec::new(),
            notes: vec![note("")],
        });
    });
    assert_eq!(
        error,
        VocabError::UnknownClass {
            class: "CDL.Reals.Subtracts".to_owned()
        }
    );
    // A reserved lowering identity is not a vocabulary class.
    let error = refused(|curated| {
        curated.classes.push(CuratedClass {
            class_path: "urn:oce:lowering#PassThrough.Real".to_owned(),
            descriptions: Vec::new(),
            notes: vec![note("")],
        });
    });
    assert!(
        matches!(error, VocabError::UnknownClass { .. }),
        "{error:?}"
    );

    for subject in ["u3", "U1", "u", "Y", "k_<i>"] {
        let (class_path, subject_owned) = about("CDL.Reals.Subtract", subject);
        let expected = VocabError::UnknownSubject {
            class: class_path,
            subject: subject_owned,
        };
        let by_note = refused(|curated| {
            class(curated, "CDL.Reals.Subtract")
                .notes
                .push(note(subject));
        });
        assert_eq!(by_note, expected);
        let by_description = refused(|curated| {
            class(curated, "CDL.Reals.Subtract")
                .descriptions
                .push(description(subject));
        });
        assert_eq!(by_description, expected);
    }
    // A note may name the class (the empty subject), a connector or a parameter.
    for subject in ["", "u", "y", "delayTime", "delayOnInit"] {
        let mut curated = curated();
        class(&mut curated, "CDL.Logical.TrueDelay")
            .notes
            .push(note(subject));
        assert!(
            Vocabulary::from_parts(generated(), curated).is_ok(),
            "{subject}"
        );
    }
}

#[test]
fn a_class_or_a_described_subject_is_curated_once_and_in_catalog_order() {
    let error = refused(|curated| {
        let again = curated.classes[0].clone();
        curated.classes.insert(1, again);
    });
    assert_eq!(
        error,
        VocabError::DuplicateClass {
            class: "CDL.Reals.Subtract".to_owned()
        }
    );
    let error = refused(|curated| curated.classes.swap(0, 1));
    assert_eq!(
        error,
        VocabError::CuratedOutOfOrder {
            class: "CDL.Reals.Subtract".to_owned()
        }
    );
    let error = refused(|curated| {
        let hold = class(curated, "CDL.Logical.TrueHoldWithReset");
        let again = hold.descriptions[0].clone();
        hold.descriptions.push(again);
    });
    let (class_path, subject) = about("CDL.Logical.TrueHoldWithReset", "u");
    assert_eq!(
        error,
        VocabError::DuplicateSubject {
            class: class_path,
            subject
        }
    );
    let error = refused(|curated| {
        let subtract = class(curated, "CDL.Reals.Subtract");
        subtract.notes.clear();
    });
    assert_eq!(
        error,
        VocabError::EmptyCuratedClass {
            class: "CDL.Reals.Subtract".to_owned()
        }
    );
}

#[test]
fn every_connector_and_parameter_is_described_after_the_merge() {
    for (class_path, subject) in [
        ("CDL.Logical.TrueHoldWithReset", "u"),
        ("CDL.Logical.TrueHoldWithReset", "clr"),
        ("CDL.Logical.TrueHoldWithReset", "y"),
        ("CDL.Logical.TrueHoldWithReset", "duration"),
        ("CDL.Reals.MatrixGain", "nout"),
        ("CDL.Reals.MatrixGain", "nin"),
    ] {
        let error = refused(|curated| {
            let curated_class = class(curated, class_path);
            curated_class.descriptions.retain(|d| d.subject != subject);
            // Keep the class non-empty so the refusal is the missing description.
            curated_class.notes.push(note(""));
        });
        let (class, subject) = about(class_path, subject);
        assert_eq!(error, VocabError::Undescribed { class, subject });
    }
    // The six are exactly the generated entries with no text.
    let undescribed: Vec<(String, String)> = generated()
        .classes
        .iter()
        .flat_map(|class| {
            let connectors = class.connectors.iter().map(|c| (&c.name, &c.description));
            let parameters = class.parameters.iter().map(|p| (&p.name, &p.description));
            connectors
                .chain(parameters)
                .filter(|(_, description)| description.is_empty())
                .map(|(name, _)| (class.class_path.clone(), name.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let described: Vec<(String, String)> = curated()
        .classes
        .iter()
        .flat_map(|class| {
            class
                .descriptions
                .iter()
                .map(|d| (class.class_path.clone(), d.subject.clone()))
        })
        .collect();
    assert_eq!(undescribed.len(), 6);
    assert_eq!(
        undescribed.iter().collect::<BTreeSet<_>>(),
        described.iter().collect::<BTreeSet<_>>()
    );
}

#[test]
fn a_repeated_generated_class_or_name_is_refused() {
    let mut twice = generated();
    let again = twice.classes[0].clone();
    twice.classes.push(again);
    assert_eq!(
        Vocabulary::from_parts(twice, curated()).expect_err("refused"),
        VocabError::DuplicateClass {
            class: "CDL.Reals.Sources.Constant".to_owned()
        }
    );
    // A parameter named like a connector: a curated subject could not tell them apart.
    let mut clash = generated();
    clash.classes[0].parameters[0].name = "y".to_owned();
    let (class, subject) = about("CDL.Reals.Sources.Constant", "y");
    assert_eq!(
        Vocabulary::from_parts(clash, curated()).expect_err("refused"),
        VocabError::DuplicateSubject { class, subject }
    );
}

#[test]
fn note_kinds_are_the_closed_set_of_five() {
    let spelled: Vec<&str> = NoteKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(
        spelled,
        ["convention", "pitfall", "port-order", "loop", "units"]
    );
    // Each spelling reads back as its kind through the file reader.
    for kind in NoteKind::ALL {
        let mut curated = curated();
        curated.classes[0].notes[0].kind = kind;
        let read = CuratedVocabulary::from_json(&curated.to_json()).expect("readable");
        assert_eq!(read.classes[0].notes[0].kind, kind);
    }
}

/// Class path (without `CDL.`) to the kinds of its notes, in file order.
fn seeded_notes() -> BTreeMap<String, Vec<(NoteKind, String)>> {
    curated()
        .classes
        .iter()
        .filter(|class| !class.notes.is_empty())
        .map(|class| {
            let notes = class
                .notes
                .iter()
                .map(|note| (note.kind, note.subject.clone()))
                .collect();
            (class.class_path.clone(), notes)
        })
        .collect()
}

#[test]
fn the_seed_is_the_prototypes_three_rules_over_eleven_classes_and_the_sampler_pitfall() {
    let class_note = |kind: NoteKind| (kind, String::new());
    let expected: BTreeMap<String, Vec<(NoteKind, String)>> = [
        // Rule 5, port order.
        ("CDL.Reals.Subtract", vec![class_note(NoteKind::PortOrder)]),
        ("CDL.Reals.Divide", vec![class_note(NoteKind::PortOrder)]),
        ("CDL.Reals.Greater", vec![class_note(NoteKind::PortOrder)]),
        ("CDL.Reals.Less", vec![class_note(NoteKind::PortOrder)]),
        // Rule 7, the library convention, and rule 9, which classes break a loop.
        (
            "CDL.Logical.TrueDelay",
            vec![
                (NoteKind::Convention, "delayOnInit".to_owned()),
                class_note(NoteKind::Loop),
            ],
        ),
        ("CDL.Logical.Pre", vec![class_note(NoteKind::Loop)]),
        ("CDL.Logical.Latch", vec![class_note(NoteKind::Loop)]),
        (
            "CDL.Logical.TrueFalseHold",
            vec![class_note(NoteKind::Loop)],
        ),
        ("CDL.Discrete.UnitDelay", vec![class_note(NoteKind::Loop)]),
        (
            "CDL.Discrete.ZeroOrderHold",
            vec![class_note(NoteKind::Loop)],
        ),
        // The pitfall written from the engine's tests.
        (
            "CDL.Discrete.Sampler",
            vec![class_note(NoteKind::Pitfall), class_note(NoteKind::Loop)],
        ),
    ]
    .into_iter()
    .map(|(class, notes)| (class.to_owned(), notes))
    .collect();
    assert_eq!(expected.len(), 11);
    assert_eq!(seeded_notes(), expected);
}

#[test]
fn the_view_serves_curated_text_only_where_the_generated_text_is_empty() {
    let mut curated_subjects = Vec::new();
    for class in vocabulary().classes() {
        let connectors = class
            .connectors
            .iter()
            .map(|c| (c.name, c.description, c.origin));
        let parameters = class
            .parameters
            .iter()
            .map(|p| (p.name, p.description, p.origin));
        for (name, description, origin) in connectors.chain(parameters) {
            assert!(!description.is_empty(), "{} {name}", class.class_path);
            if origin == TextOrigin::Curated {
                curated_subjects.push(format!("{}.{name}", class.class_path));
            }
        }
    }
    assert_eq!(
        curated_subjects,
        [
            "CDL.Reals.MatrixGain.nout",
            "CDL.Reals.MatrixGain.nin",
            "CDL.Logical.TrueHoldWithReset.u",
            "CDL.Logical.TrueHoldWithReset.clr",
            "CDL.Logical.TrueHoldWithReset.y",
            "CDL.Logical.TrueHoldWithReset.duration",
        ]
    );
    let hold = class_vocab("CDL.Logical.TrueHoldWithReset").expect("a class");
    assert!(!hold.upstream && hold.links.is_none());
    assert_eq!(
        hold.connectors[1].description,
        "Clear input: while true the output is false and the hold is cancelled"
    );
    let delay = class_vocab("CDL.Logical.TrueDelay").expect("a class");
    assert_eq!(delay.notes.len(), 2);
    assert_eq!(delay.notes_about("delayOnInit").count(), 1);
    assert_eq!(delay.notes_about("").count(), 1);
    assert_eq!(delay.notes_about("delayTime").count(), 0);
    assert!(
        class_vocab("CDL.Reals.Add")
            .expect("a class")
            .notes
            .is_empty()
    );
}
