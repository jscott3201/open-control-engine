//! Unit tests of the join: the name comparison against the catalog, and what the curated join
//! costs on the largest documents the readers accept.
//!
//! Cost is asserted in name comparisons ([`JoinCost`]), a measure that does not depend on the
//! machine, never in wall-clock time (`TESTING.md`; GitHub #381 is the pattern).

use super::{Vocabulary, same_names};
use crate::data::{
    Connector, CuratedClass, CuratedDescription, CuratedVocabulary, GeneratedClass,
    GeneratedVocabulary, MAX_VOCAB_BYTES, Note, NoteKind, Side, Source,
};
use crate::{CURATED_JSON, GENERATED_JSON, TextOrigin, VocabError};

fn missing(subject: &str) -> VocabError {
    VocabError::MissingFromVocabulary {
        class: "C".to_owned(),
        subject: subject.to_owned(),
    }
}

fn extra(subject: &str) -> VocabError {
    VocabError::NotInCatalog {
        class: "C".to_owned(),
        subject: subject.to_owned(),
    }
}

#[test]
fn equal_lists_pass_and_two_empty_lists_pass() {
    assert_eq!(same_names("C", &["a", "b"], &["a", "b"]), Ok(()));
    assert_eq!(same_names("C", &[], &[]), Ok(()));
}

#[test]
fn a_name_on_one_side_only_is_reported_from_that_side() {
    assert_eq!(same_names("C", &["a"], &["a", "b"]), Err(missing("b")));
    assert_eq!(same_names("C", &[], &["a"]), Err(missing("a")));
    assert_eq!(same_names("C", &["a", "x"], &["a"]), Err(extra("x")));
    assert_eq!(same_names("C", &["x"], &[]), Err(extra("x")));
    // A rename is a catalog name the vocabulary lacks.
    assert_eq!(
        same_names("C", &["a", "old"], &["a", "new"]),
        Err(missing("new"))
    );
}

#[test]
fn the_same_names_in_another_order_are_refused() {
    assert_eq!(same_names("C", &["b", "a"], &["a", "b"]), Err(extra("b")));
}

fn connector(name: String, description: &str, side: Side) -> Connector {
    Connector {
        description: description.to_owned(),
        dims: 0,
        name,
        quantity: String::new(),
        side,
        unit: String::new(),
    }
}

fn note(subject: &str) -> Note {
    Note {
        evidence: "e".to_owned(),
        kind: NoteKind::Loop,
        subject: subject.to_owned(),
        text: "t".to_owned(),
    }
}

/// `ceil(log2(n))`, and 0 for `n <= 1`.
fn log2_ceil(n: u64) -> u64 {
    u64::from(u64::BITS - n.saturating_sub(1).leading_zeros())
}

/// The most name comparisons an indexed join may make for one curated class: sorting the
/// class's `subjects` names (four times `n log n` covers any comparison sort the standard
/// library may choose) and one binary search per curated entry.
fn indexed_bound(subjects: usize, lookups: usize) -> u64 {
    let (subjects, lookups) = (subjects as u64, lookups as u64);
    let depth = log2_ceil(subjects);
    4 * subjects * depth + lookups * (depth + 1)
}

/// Both documents as the readers accept them: canonical and inside the byte bound.
fn read_back(
    generated: &GeneratedVocabulary,
    curated: &CuratedVocabulary,
) -> (GeneratedVocabulary, CuratedVocabulary) {
    let (generated, curated) = (generated.to_json(), curated.to_json());
    assert!(generated.len() <= MAX_VOCAB_BYTES, "{}", generated.len());
    assert!(curated.len() <= MAX_VOCAB_BYTES, "{}", curated.len());
    (
        GeneratedVocabulary::from_json(&generated).expect("a canonical, in-bound document"),
        CuratedVocabulary::from_json(&curated).expect("a canonical, in-bound document"),
    )
}

#[test]
fn the_bound_is_n_log_n_and_far_below_the_product() {
    assert_eq!(
        [0, 1, 2, 3, 4, 5, 1024, 1025].map(log2_ceil),
        [0, 0, 1, 2, 2, 3, 10, 11]
    );
    assert_eq!(indexed_bound(0, 0), 0);
    assert_eq!(indexed_bound(1, 5), 5);
    assert_eq!(indexed_bound(8, 2), 4 * 8 * 3 + 2 * 4);
}

/// The pair from the review of GitHub #511: one class with 24,000 connectors, and 32,000 notes
/// that each name the last of them. Both documents are canonical and inside the 4 MiB bound; a
/// scan of the class per note is 768 million name comparisons before the pair is refused.
#[test]
fn a_wide_class_with_many_notes_is_refused_after_n_log_n_name_comparisons() {
    const CONNECTORS: usize = 24_000;
    const NOTES: usize = 32_000;
    let last = format!("c{}", CONNECTORS - 1);
    let generated = GeneratedVocabulary {
        classes: vec![GeneratedClass {
            class_path: "CDL.Hostile.Wide".to_owned(),
            connectors: (0..CONNECTORS)
                .map(|index| connector(format!("c{index}"), "d", Side::Input))
                .collect(),
            parameters: Vec::new(),
            upstream: true,
        }],
        format: 1,
        source: Source {
            buildings_commit: "a".repeat(40),
            document: "d".to_owned(),
            modelica_json_commit: "b".repeat(40),
            sha256: "c".repeat(64),
        },
    };
    let curated = CuratedVocabulary {
        classes: vec![CuratedClass {
            class_path: "CDL.Hostile.Wide".to_owned(),
            descriptions: Vec::new(),
            notes: vec![note(&last); NOTES],
        }],
        format: 1,
    };
    let (generated, curated) = read_back(&generated, &curated);
    let (joined, cost) = Vocabulary::from_parts_costed(generated, curated);
    // The curated join passes (every note names a connector); the catalog then has no such
    // class, and its first authorable class is what the vocabulary is reported to lack.
    let first = oce_api::catalog()
        .iter()
        .find(|entry| !entry.reserved)
        .expect("an authorable class");
    assert_eq!(
        joined.err(),
        Some(VocabError::MissingFromVocabulary {
            class: first.class_path.to_owned(),
            subject: String::new(),
        })
    );
    let bound = indexed_bound(CONNECTORS, NOTES);
    assert!(
        cost.name_comparisons <= bound,
        "{cost:?} is over the indexed bound {bound}"
    );
    // The bound is what separates an index from a scan: hundreds of times below the product.
    assert!(bound * 300 < (CONNECTORS * NOTES) as u64);
    // Every note was looked up: the cost is not zero because the work was skipped.
    assert!(cost.name_comparisons >= NOTES as u64);
}

/// An accepted pair at scale: the embedded files, with a width-driven class (whose connectors
/// the catalog does not name) widened by thousands of connectors, half of them described only
/// by `curated.json`, and thousands of notes on the last one. The join stays inside the indexed
/// bound, and every view reads its curated text from the positions the join resolved.
#[test]
fn an_accepted_pair_with_a_wide_curated_class_joins_inside_the_indexed_bound() {
    const CLASS: &str = "CDL.Reals.MatrixGain";
    const EXTRA: usize = 8_000;
    const NOTES: usize = 12_000;
    let mut generated = GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json");
    let mut curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    let class = generated
        .classes
        .iter_mut()
        .find(|class| class.class_path == CLASS)
        .expect("a generated class");
    let outputs = class
        .connectors
        .iter()
        .position(|connector| connector.side == Side::Output)
        .expect("an output");
    // Inputs stay before outputs. Every other added connector has no generated text.
    let added: Vec<Connector> = (0..EXTRA)
        .map(|index| {
            let description = if index % 2 == 0 { "" } else { "generated" };
            connector(format!("extra{index}"), description, Side::Input)
        })
        .collect();
    class.connectors.splice(outputs..outputs, added);
    let subjects = class.connectors.len() + class.parameters.len();

    let curated_class = curated
        .classes
        .iter_mut()
        .find(|class| class.class_path == CLASS)
        .expect("a curated class");
    for index in (0..EXTRA).step_by(2) {
        curated_class.descriptions.push(CuratedDescription {
            evidence: "e".to_owned(),
            subject: format!("extra{index}"),
            text: format!("curated {index}"),
        });
    }
    curated_class.notes = vec![note(&format!("extra{}", EXTRA - 1)); NOTES];
    let lookups = curated_class.descriptions.len() + NOTES;
    // The other curated classes are the embedded ones: a few subjects and a few entries each.
    let others: u64 = curated
        .classes
        .iter()
        .filter(|class| class.class_path != CLASS)
        .map(|class| indexed_bound(64, class.descriptions.len() + class.notes.len()))
        .sum();

    let (generated, curated) = read_back(&generated, &curated);
    let (joined, cost) = Vocabulary::from_parts_costed(generated, curated);
    let vocabulary = joined.expect("the widened pair is accepted");
    let bound = indexed_bound(subjects, lookups) + others;
    assert!(
        cost.name_comparisons <= bound,
        "{cost:?} is over the indexed bound {bound}"
    );
    assert!(bound * 30 < (subjects * lookups) as u64);

    let view = vocabulary.class(CLASS).expect("the class");
    assert_eq!(view.connectors.len() + view.parameters.len(), subjects);
    assert_eq!(view.notes.len(), NOTES);
    let mut added = view
        .connectors
        .iter()
        .filter(|connector| connector.name.starts_with("extra"));
    for index in 0..EXTRA {
        let found = added.next().expect("the added connector");
        assert_eq!(found.name, format!("extra{index}"));
        let expected = if index % 2 == 0 {
            (format!("curated {index}"), TextOrigin::Curated)
        } else {
            ("generated".to_owned(), TextOrigin::Generated)
        };
        assert_eq!((found.description.to_owned(), found.origin), expected);
    }
    assert!(added.next().is_none());
    // The class's own curated descriptions are still the ones served.
    for parameter in ["nout", "nin"] {
        let found = view
            .parameters
            .iter()
            .find(|found| found.name == parameter)
            .expect("the parameter");
        assert_eq!(found.origin, TextOrigin::Curated, "{parameter}");
    }
}

#[test]
fn the_embedded_pair_costs_fewer_than_a_thousand_name_comparisons() {
    let generated = GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json");
    let curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    let entries: usize = curated
        .classes
        .iter()
        .map(|class| class.descriptions.len() + class.notes.len())
        .sum();
    let (joined, cost) = Vocabulary::from_parts_costed(generated, curated);
    assert!(joined.is_ok());
    assert!(cost.name_comparisons >= entries as u64, "{cost:?}");
    assert!(cost.name_comparisons < 1_000, "{cost:?}");
}
