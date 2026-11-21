//! A blob summary is not acceptance (issue #448): what `oce_api::inspect_blob` and
//! `oce_graph::blob::inspect` refuse after the decoder's structural pass (a class this build does
//! not register, a class index outside the class table, another engine version), and what they
//! report for a blob the decoder refuses at a later step, which is the blob as written.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_corpus;
use crate::blob_image;
use crate::blob_models;
use crate::blob_summary_facts;

use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, VType, Val};
use blob_models::{adder, chain, class, delay, input, inst, moving_mean, out};
use blob_summary_facts::{Facts, assert_invariants, summarise};
use oce_api::{BlobError, ENGINE_VERSION, EngineVersion, InstanceField, Value};
use oce_graph::blob::{ConnectorFault, OutputFault, ScheduleFault, decode};

/// The blob an earlier release wrote under the same format number (`blob_old_layout.rs`).
const OLD_LAYOUT: &[u8] = include_bytes!("fixtures/blob_legacy.oceb");

#[test]
fn a_target_bound_class_in_the_table_makes_the_summary_target_bound() {
    let mut sin = Image::new();
    sin.classes = vec![class("CDL.Reals.Sin")];
    sin.connectors = 2;
    sin.instances = vec![inst(0, "CDL.Reals.Sin", &[], &[0], &[1])];
    sin.schedule = vec![0];
    sin.inputs = vec![input("urn:test:u", VType::Real, &[0])];
    sin.outputs = vec![out("urn:test:y", 1)];
    let bytes = sin.to_bytes();
    assert!(decode(&bytes).unwrap().target_bound);
    assert!(summarise(&bytes).unwrap().target_bound);

    for image in [chain(), adder(), delay(), moving_mean(4)] {
        let bytes = image.to_bytes();
        assert!(!decode(&bytes).unwrap().target_bound);
        assert!(!summarise(&bytes).unwrap().target_bound);
    }

    // The flag is a statement about the class table. A target-bound class no instance uses makes
    // the decoder refuse the blob, and the summary still says the table holds one.
    let mut unused = chain();
    unused.classes.insert(1, class("CDL.Reals.Sin"));
    unused.instances[0].class = 2;
    let bytes = unused.to_bytes();
    assert_eq!(
        decode(&bytes).err(),
        Some(BlobError::UnusedClass { class: 1 })
    );
    let facts = summarise(&bytes).unwrap();
    assert!(facts.target_bound);
    assert_eq!(facts, Facts::of_image(&unused));
}

#[test]
fn a_class_this_build_does_not_register_is_refused_by_its_table_index() {
    // One unregistered class, in either position: the decoder's own refusal.
    for (index, path) in [(0u32, "CDL.Reals.Abz"), (1, "CDL.Reals.Sources.Constanz")] {
        let mut unknown = chain();
        unknown.classes[index as usize].0 = path.as_bytes().to_vec();
        let bytes = unknown.to_bytes();
        let refusal = BlobError::UnknownClass { class: index };
        assert_eq!(decode(&bytes).err(), Some(refusal), "{path}");
        assert_eq!(summarise(&bytes), Err(refusal), "{path}");
    }

    // Two unregistered classes: the first in table order.
    let mut both = chain();
    both.classes[0].0 = b"CDL.Reals.Abz".to_vec();
    both.classes[1].0 = b"CDL.Reals.Sources.Constanz".to_vec();
    assert_eq!(
        summarise(&both.to_bytes()),
        Err(BlobError::UnknownClass { class: 0 })
    );

    // A class path is matched exactly: a registered path in another case, with a trailing space or
    // empty is not registered.
    for path in [
        "cdl.discrete.triggeredmovingmean",
        "CDL.Discrete.TriggeredMovingMean ",
        "",
    ] {
        let mut image = moving_mean(4);
        image.classes[0].0 = path.as_bytes().to_vec();
        assert_eq!(
            summarise(&image.to_bytes()),
            Err(BlobError::UnknownClass { class: 0 }),
            "{path:?}"
        );
    }

    // After the structural pass the summary's first refusal need not be the decoder's: the
    // decoder stops at class 0's hash, the summary does not compare hashes and stops at class 1.
    let mut mixed = chain();
    let expected = mixed.classes[0].1;
    mixed.classes[0].1 ^= 1;
    mixed.classes[1].0 = b"CDL.Reals.Sources.Constanz".to_vec();
    let bytes = mixed.to_bytes();
    assert_eq!(
        decode(&bytes).err(),
        Some(BlobError::ClassSignatureMismatch {
            class: 0,
            expected,
            found: expected ^ 1
        })
    );
    assert_eq!(summarise(&bytes), Err(BlobError::UnknownClass { class: 1 }));
}

#[test]
fn a_class_index_outside_the_class_table_is_refused_by_its_block() {
    // The table of `chain` has two classes: index 1 is the last one in it, 2 is one past.
    for (block, index) in [(0usize, 2u32), (1, 2), (1, 3), (0, u32::MAX)] {
        let mut image = chain();
        image.instances[block].class = index;
        let bytes = image.to_bytes();
        let refusal = BlobError::InstanceMismatch {
            block: u32::try_from(block).unwrap(),
            field: InstanceField::ClassIndex,
        };
        assert_eq!(decode(&bytes).err(), Some(refusal), "block {block}");
        assert_eq!(summarise(&bytes), Err(refusal), "block {block} = {index}");
    }
    // Both blocks out of range: the first block.
    let mut both = chain();
    both.instances[0].class = 9;
    both.instances[1].class = 9;
    assert_eq!(
        summarise(&both.to_bytes()),
        Err(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::ClassIndex
        })
    );
    // An instance in a blob with no class table at all.
    let mut no_table = chain();
    no_table.classes.clear();
    assert_eq!(
        summarise(&no_table.to_bytes()),
        Err(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::ClassIndex
        })
    );
    // The last index in the table is in range: both blocks on class 1 summarise, though the
    // decoder refuses the blob (block 1 is not a Constant, and class 0 is now unused).
    let mut last = chain();
    last.instances[1].class = 1;
    let bytes = last.to_bytes();
    assert!(decode(&bytes).is_err());
    assert_eq!(summarise(&bytes), Ok(Facts::of_image(&last)));
}

#[test]
fn a_blob_the_decoder_refuses_after_its_structural_pass_summarises_as_written() {
    let cases: Vec<(&str, Image, BlobError)> = vec![
        (
            "a schedule that runs the consumer first",
            {
                let mut image = chain();
                image.schedule = vec![1, 0];
                image
            },
            BlobError::InvalidSchedule {
                position: 0,
                fault: ScheduleFault::EmitBefore,
            },
        ),
        (
            "a missing schedule",
            {
                let mut image = chain();
                image.schedule = vec![];
                image
            },
            BlobError::InvalidSchedule {
                position: 0,
                fault: ScheduleFault::CountMismatch,
            },
        ),
        (
            "an input no connection drives",
            {
                let mut image = chain();
                image.connections.clear();
                image
            },
            BlobError::UndrivenInput { connector: 1 },
        ),
        (
            "two boundary outputs with one path",
            {
                let mut image = chain();
                image.outputs.push(out("urn:test:y", 0));
                image
            },
            BlobError::InvalidOutput {
                output: 1,
                fault: OutputFault::DuplicatePath,
            },
        ),
        (
            "a non-finite parameter",
            {
                let mut image = chain();
                image.instances[0].params[0].1 = Val::real(f64::NAN);
                image
            },
            BlobError::NonFiniteParameter {
                block: 0,
                parameter: 0,
            },
        ),
        (
            "a parameter named twice",
            {
                let mut image = chain();
                let first = image.instances[0].params[0].clone();
                image.instances[0].params.push(first);
                image
            },
            BlobError::DuplicateParameter {
                block: 0,
                parameter: 1,
            },
        ),
        (
            "a connector id out of range",
            {
                let mut image = chain();
                image.instances[1].outputs = vec![9];
                image
            },
            BlobError::InvalidConnector {
                connector: 9,
                fault: ConnectorFault::OutOfRange,
            },
        ),
        (
            "a class no instance uses",
            {
                let mut image = chain();
                image.classes.insert(1, class("CDL.Reals.Add"));
                image.instances[0].class = 2;
                image
            },
            BlobError::UnusedClass { class: 1 },
        ),
        (
            "a state region whose parameters allocate other words",
            {
                let mut image = delay();
                image.instances[1].params[1].1 = Val::from_value(&Value::Real(8.0));
                image
            },
            BlobError::InitialStateMismatch {
                block: 1,
                expected: delay().instances[1].state_crc,
                found: 0,
            },
        ),
    ];
    for (what, image, refusal) in cases {
        let bytes = image.to_bytes();
        match (decode(&bytes).err(), refusal) {
            // The words the decoder allocated are its own business; the block and the carried
            // CRC are what the case pins.
            (
                Some(BlobError::InitialStateMismatch {
                    block, expected, ..
                }),
                BlobError::InitialStateMismatch {
                    block: b,
                    expected: e,
                    ..
                },
            ) => assert_eq!((block, expected), (b, e), "{what}"),
            (found, refusal) => assert_eq!(found, Some(refusal), "{what}"),
        }
        let facts = summarise(&bytes).unwrap_or_else(|err| panic!("{what}: {err:?}"));
        assert_eq!(facts, Facts::of_image(&image), "{what}");
        assert_invariants(&facts, what);
    }
}

#[test]
fn a_blob_another_engine_version_wrote_is_refused_by_its_version() {
    let refusal = BlobError::EngineVersionMismatch {
        found: EngineVersion {
            major: 0,
            minor: 4,
            patch: 0,
        },
        expected: ENGINE_VERSION,
    };
    assert_eq!(decode(OLD_LAYOUT).err(), Some(refusal));
    assert_eq!(summarise(OLD_LAYOUT), Err(refusal));

    // Each version field, one step away from this build's in either direction.
    let bytes = golden(FIXTURES[0]);
    for at in [8usize, 10, 12] {
        for step in [1u16, u16::MAX] {
            let mut other = bytes.clone();
            let field = u16::from_le_bytes([other[at], other[at + 1]]).wrapping_add(step);
            other[at..at + 2].copy_from_slice(&field.to_le_bytes());
            blob_image::rechecksum(&mut other);
            let found = summarise(&other).unwrap_err();
            assert!(
                matches!(
                    found,
                    BlobError::EngineVersionMismatch { expected, found }
                        if expected == ENGINE_VERSION && found != ENGINE_VERSION
                ),
                "offset {at}: {found:?}"
            );
            assert_eq!(decode(&other).err(), Some(found), "offset {at}");
        }
    }
}
