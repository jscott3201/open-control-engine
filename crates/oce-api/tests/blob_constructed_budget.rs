//! Where the constructed-budget refusal stands among the decoder's refusals, and that every
//! reader of a blob reports it alike (issue #700).
//!
//! `oce_graph::blob::decode` charges each block against `oce_graph::MAX_CONSTRUCTED_WORDS` the
//! moment it is constructed, in step 7, before any field of that block is compared. So the
//! refusal loses to everything checked before the crossing block is constructed (the header and
//! structure, the class table, every earlier block, and the crossing block's own class index
//! and parameters) and wins over everything checked after (that block's kind, state length,
//! arity and connectors, every later block, and steps 8 to 10). `inspect`, `inspect_blob` and
//! `Executable::from_blob` run the same building pass and report the same error.
//!
//! The memory side of the budget is pinned in `blob_peak_allocation.rs`.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_image;
use crate::blob_models;
use crate::blob_summary_facts;

use blob_image::{Image, Val};
use blob_models::{class, moving_mean, time_table_words, time_tables};
use blob_summary_facts::{Facts, summarise_with_refusal};
use oce_api::{BlobError, InstanceField, OcError, inspect_blob};
use oce_graph::blob::{ConnectorFault, Executable, MalformedFault, decode};
use oce_graph::{MAX_CONSTRUCTED_WORDS, MAX_STATE_WORDS};

/// A table of 524,287 rows reports 1,048,576 words: four of them are exactly the budget.
const QUARTER_BUDGET: &str = "table_524287_2";
/// The smallest table: four words.
const SMALLEST: &str = "table_1_2";

/// Four quarter-budget tables and a fifth, smallest one that crosses the budget at block 4.
fn one_block_past_the_budget() -> Image {
    time_tables(&[
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        SMALLEST,
    ])
}

/// The refusal of [`one_block_past_the_budget`].
const CROSSED_AT_BLOCK_4: BlobError = BlobError::ConstructedTooLarge {
    block: 4,
    requested: MAX_CONSTRUCTED_WORDS as u64 + 4,
};

fn refusal(image: &Image) -> Option<BlobError> {
    decode(&image.to_bytes()).err()
}

/// One defect applied to an image.
type Fault<'a> = &'a dyn Fn(&mut Image);

#[test]
fn the_fixture_is_one_block_past_a_blob_that_decodes() {
    assert_eq!(4 * time_table_words(524_287), MAX_CONSTRUCTED_WORDS as u64);
    assert_eq!(time_table_words(1), 4);
    let mut at_budget = one_block_past_the_budget();
    at_budget.instances.pop();
    at_budget.schedule.pop();
    at_budget.connectors = 4;
    assert_eq!(refusal(&at_budget), None);
    assert_eq!(
        refusal(&one_block_past_the_budget()),
        Some(CROSSED_AT_BLOCK_4)
    );
}

#[test]
fn the_constructed_refusal_wins_over_every_fault_of_the_crossing_block_checked_after_it() {
    let faults: [(&str, Fault<'_>); 7] = [
        ("recorded as stateful", &|image| image.instances[4].kind = 1),
        ("a state length it does not have", &|image| {
            image.instances[4].state_len = 9;
        }),
        ("a CRC without state", &|image| {
            image.instances[4].state_crc = 1;
        }),
        ("an input it does not have", &|image| {
            image.instances[4].inputs = vec![4];
            image.instances[4].outputs = vec![5];
            image.connectors = 6;
        }),
        ("no outputs", &|image| image.instances[4].outputs.clear()),
        ("a connector out of range", &|image| {
            image.instances[4].outputs = vec![99];
        }),
        ("a connector another block claimed", &|image| {
            image.instances[4].outputs = vec![0];
        }),
    ];
    for (what, fault) in faults {
        let mut image = one_block_past_the_budget();
        fault(&mut image);
        assert_eq!(refusal(&image), Some(CROSSED_AT_BLOCK_4), "{what}");

        // The same fault in a blob within the budget is the refusal that fault names.
        let mut within = time_tables(&[SMALLEST; 5]);
        fault(&mut within);
        let named = refusal(&within).unwrap_or_else(|| panic!("{what}: decodes"));
        assert!(
            matches!(
                named,
                BlobError::InstanceMismatch { block: 4, .. } | BlobError::InvalidConnector { .. }
            ),
            "{what}: {named:?}"
        );
    }
}

#[test]
fn the_constructed_refusal_wins_over_every_later_block_and_step() {
    let faults: [(&str, Fault<'_>, BlobError); 7] = [
        (
            "a later block with the wrong arity",
            &|image| {
                image.instances.push(image.instances[4].clone());
                image.instances[5].outputs.clear();
            },
            BlobError::InstanceMismatch {
                block: 5,
                field: InstanceField::OutputArity,
            },
        ),
        (
            "a class no instance uses",
            &|image| {
                image.classes.insert(0, class("CDL.Reals.Abs"));
                for instance in &mut image.instances {
                    instance.class = 1;
                }
            },
            BlobError::UnusedClass { class: 0 },
        ),
        (
            "a connector no block claims",
            &|image| image.connectors += 1,
            BlobError::InvalidConnector {
                connector: 5,
                fault: ConnectorFault::Missing,
            },
        ),
        (
            "a boundary output read from a connector that does not exist",
            &|image| image.outputs = vec![blob_models::out("urn:test:y", 77)],
            BlobError::InvalidOutput {
                output: 0,
                fault: oce_graph::blob::OutputFault::SourceOutOfRange,
            },
        ),
        (
            "a connection between connectors that do not exist",
            &|image| image.connections = vec![(70, 71)],
            BlobError::InvalidConnection {
                connection: 0,
                fault: oce_graph::blob::ConnectionFault::OutOfRange,
            },
        ),
        (
            "an empty schedule",
            &|image| image.schedule.clear(),
            BlobError::InvalidSchedule {
                position: 0,
                fault: oce_graph::blob::ScheduleFault::CountMismatch,
            },
        ),
        (
            "a schedule naming one block twice",
            &|image| image.schedule[1] = 0,
            BlobError::InvalidSchedule {
                position: 1,
                fault: oce_graph::blob::ScheduleFault::Duplicate,
            },
        ),
    ];
    for (what, fault, named) in faults {
        let mut image = one_block_past_the_budget();
        fault(&mut image);
        assert_eq!(refusal(&image), Some(CROSSED_AT_BLOCK_4), "{what}");

        // The same fault in a blob within the budget is the refusal that fault names.
        let mut within = time_tables(&[SMALLEST; 5]);
        fault(&mut within);
        assert_eq!(refusal(&within), Some(named), "{what}");
    }
}

/// `tables` beside one moving mean (block 0) whose window is one word past the state budget.
fn tables_after_a_state_region_past_its_budget(tables: &[&str]) -> Image {
    let window = i64::try_from(MAX_STATE_WORDS).unwrap() - 3;
    let mean = moving_mean(window);
    let mut image = time_tables(tables);
    image.classes = vec![mean.classes[0].clone(), image.classes[0].clone()];
    for instance in &mut image.instances {
        instance.class = 1;
        instance.outputs[0] += 3;
    }
    image.instances.insert(0, mean.instances[0].clone());
    image.connectors += 3;
    image.schedule = (0..u32::try_from(image.instances.len()).unwrap()).collect();
    image.inputs = mean.inputs;
    image
}

#[test]
fn the_constructed_refusal_wins_over_the_state_budget_even_for_an_earlier_block() {
    // The state budget is the last step: a stateful block declared first, one word past the
    // state budget, is reported only when the constructed blocks fit.
    let state_refusal = BlobError::StateTooLarge {
        block: 0,
        requested: MAX_STATE_WORDS as u64 + 1,
    };
    let within = tables_after_a_state_region_past_its_budget(&[SMALLEST; 5]);
    assert_eq!(refusal(&within), Some(state_refusal));

    let past = tables_after_a_state_region_past_its_budget(&[
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        SMALLEST,
    ]);
    assert_eq!(
        refusal(&past),
        Some(BlobError::ConstructedTooLarge {
            block: 5,
            requested: MAX_CONSTRUCTED_WORDS as u64 + 4,
        })
    );
}

#[test]
fn a_fault_met_before_the_crossing_block_is_constructed_wins_over_the_constructed_refusal() {
    let faults: [(&str, Fault<'_>, BlobError); 8] = [
        (
            "reserved flags",
            &|image| image.flags = 1,
            BlobError::NonzeroFlags { flags: 1 },
        ),
        (
            "a class hash that is not this build's",
            &|image| image.classes[0].1 ^= 1,
            BlobError::ClassSignatureMismatch {
                class: 0,
                expected: class(blob_models::REAL_TIME_TABLE).1,
                found: class(blob_models::REAL_TIME_TABLE).1 ^ 1,
            },
        ),
        (
            "an earlier block with the wrong arity",
            &|image| {
                image.instances[1].outputs.clear();
            },
            BlobError::InstanceMismatch {
                block: 1,
                field: InstanceField::OutputArity,
            },
        ),
        (
            "an earlier block claiming a connector twice",
            &|image| image.instances[2].outputs = vec![0],
            BlobError::InvalidConnector {
                connector: 0,
                fault: ConnectorFault::Duplicated,
            },
        ),
        (
            "a repeated parameter name in the crossing block",
            &|image| {
                let again = image.instances[4].params[0].clone();
                image.instances[4].params.push(again);
            },
            BlobError::DuplicateParameter {
                block: 4,
                parameter: 1,
            },
        ),
        (
            "a non-finite parameter in the crossing block",
            &|image| image.instances[4].params[0].1 = Val::real(f64::NAN),
            BlobError::NonFiniteParameter {
                block: 4,
                parameter: 0,
            },
        ),
        (
            "an enum ordinal of zero in the crossing block",
            &|image| {
                image.instances[4].params[0].1 =
                    Val::Enum(b"CDL.Types.SimpleController".to_vec(), 0);
            },
            BlobError::EnumOrdinalOutOfRange {
                offset: 0,
                ordinal: 0,
            },
        ),
        (
            "a tag outside its table in the crossing block",
            &|image| image.instances[4].params[0].1 = Val::Tag(9),
            BlobError::Malformed {
                offset: 0,
                fault: MalformedFault::InvalidTag,
            },
        ),
    ];
    for (what, fault, named) in faults {
        let mut image = one_block_past_the_budget();
        fault(&mut image);
        let found = refusal(&image).unwrap_or_else(|| panic!("{what}: decodes"));
        // Offsets depend on the layout; the variant and its other fields are what is pinned.
        let same = match (found, named) {
            (
                BlobError::EnumOrdinalOutOfRange { ordinal: a, .. },
                BlobError::EnumOrdinalOutOfRange { ordinal: b, .. },
            ) => a == b,
            (BlobError::Malformed { fault: a, .. }, BlobError::Malformed { fault: b, .. }) => {
                a == b
            }
            (found, named) => found == named,
        };
        assert!(same, "{what}: {found:?}");
    }

    // A class index outside the table is found before the block is constructed, too.
    let mut image = one_block_past_the_budget();
    image.instances[4].class = 3;
    assert_eq!(
        refusal(&image),
        Some(BlobError::InstanceMismatch {
            block: 4,
            field: InstanceField::ClassIndex
        })
    );
}

#[test]
fn every_reader_of_a_blob_reports_the_decoders_constructed_refusal() {
    let image = one_block_past_the_budget();
    let bytes = image.to_bytes();
    assert_eq!(decode(&bytes).err(), Some(CROSSED_AT_BLOCK_4));

    // The core and the facade summary: the blob as written, with the decoder's refusal beside
    // it (`summarise_with_refusal` holds both readers to `decode` and to each other).
    let (facts, verdict) = summarise_with_refusal(&bytes).unwrap();
    assert_eq!(verdict, Some(CROSSED_AT_BLOCK_4));
    assert_eq!(facts, Facts::of_image(&image));
    let summary = inspect_blob(&bytes).expect("the blob has a summary");
    assert_eq!(summary.decode_refusal, Some(CROSSED_AT_BLOCK_4));
    assert_eq!(summary.instances.len(), 5, "every block as written");

    // The execution entry refuses with the same error and no executable.
    assert_eq!(
        Executable::from_blob(&bytes).err(),
        Some(CROSSED_AT_BLOCK_4)
    );

    // At the budget every reader accepts.
    let at_budget = time_tables(&[QUARTER_BUDGET; 4]).to_bytes();
    assert_eq!(summarise_with_refusal(&at_budget).unwrap().1, None);
    assert_eq!(inspect_blob(&at_budget).unwrap().decode_refusal, None);
    let executable = Executable::from_blob(&at_budget).expect("decodes at the budget");
    assert_eq!(executable.outputs().len(), 0);
}

#[test]
fn the_refusal_is_a_typed_value_with_a_message_naming_the_block_and_the_words() {
    assert_eq!(
        CROSSED_AT_BLOCK_4.to_string(),
        "constructed blocks hold 4194308 words through block 4, over the supported maximum"
    );
    let wrapped = OcError::from(CROSSED_AT_BLOCK_4);
    assert!(matches!(
        wrapped,
        OcError::Blob(BlobError::ConstructedTooLarge { block: 4, .. })
    ));
    assert!(wrapped.to_string().contains("4194308 words"), "{wrapped}");
}
