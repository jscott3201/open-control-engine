//! Having a blob summary is not acceptance, and the summary says which it is (issues #448 and
//! #680): what `oce_api::inspect_blob` and `oce_graph::blob::inspect` refuse after the decoder's
//! structural pass (a class this build does not register, a class index outside the class table,
//! another engine version), and what they report for a blob the decoder refuses at a later step,
//! which is the blob as written with the decoder's own refusal in `decode_refusal`.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_corpus;
use crate::blob_image;
use crate::blob_models;
use crate::blob_summary_facts;

use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, VType, Val};
use blob_models::{adder, chain, class, delay, input, inst, moving_mean, out};
use blob_summary_facts::{Facts, assert_invariants, summarise, summarise_with_refusal};
use oce_api::{BlobError, ENGINE_VERSION, EngineVersion, InstanceField, OcError, Value};
use oce_graph::MAX_STATE_WORDS;
use oce_graph::blob::{
    ConnectionFault, ConnectorFault, InputFault, OutputFault, ScheduleFault, decode,
};

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
    let (facts, refusal) = summarise_with_refusal(&bytes).unwrap();
    assert!(facts.target_bound);
    // Target-bound is not a refusal: this build accepts the blob.
    assert_eq!(refusal, None);

    for image in [chain(), adder(), delay(), moving_mean(4)] {
        let bytes = image.to_bytes();
        assert!(!decode(&bytes).unwrap().target_bound);
        let (facts, refusal) = summarise_with_refusal(&bytes).unwrap();
        assert!(!facts.target_bound);
        assert_eq!(refusal, None);
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
    let (facts, refusal) = summarise_with_refusal(&bytes).unwrap();
    assert!(facts.target_bound);
    assert_eq!(facts, Facts::of_image(&unused));
    assert_eq!(refusal, Some(BlobError::UnusedClass { class: 1 }));
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
    // decoder refuses the blob (block 1 lists an input a Constant does not have, and class 0 is
    // now unused), and the summary carries the first of those refusals.
    let mut last = chain();
    last.instances[1].class = 1;
    let bytes = last.to_bytes();
    let refusal = Some(BlobError::InstanceMismatch {
        block: 1,
        field: InstanceField::InputArity,
    });
    assert_eq!(decode(&bytes).err(), refusal);
    assert_eq!(
        summarise_with_refusal(&bytes),
        Ok((Facts::of_image(&last), refusal))
    );
}

#[test]
fn a_blob_the_decoder_refuses_after_its_structural_pass_summarises_as_written_with_that_refusal() {
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
        let decoder = decode(&bytes).err();
        match (decoder, refusal) {
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
        let (facts, verdict) =
            summarise_with_refusal(&bytes).unwrap_or_else(|err| panic!("{what}: {err:?}"));
        assert_eq!(facts, Facts::of_image(&image), "{what}");
        assert_invariants(&facts, what);
        // The summary carries exactly the error `decode` returns: the same variant with the
        // same indices and, for the state case, the same computed CRC.
        assert!(decoder.is_some(), "{what}");
        assert_eq!(verdict, decoder, "{what}");

        // Through the public surface alone, with no test helper between: a host reads the
        // refusal off the summary it was given.
        let summary = oce_api::inspect_blob(&bytes).unwrap_or_else(|err| panic!("{what}: {err}"));
        assert_eq!(summary.decode_refusal, decoder, "{what}");
        assert_eq!(
            oce_graph::blob::inspect(&bytes).unwrap().decode_refusal,
            decoder,
            "{what}"
        );
    }
}

/// The acceptance check a host writes with the facade alone.
fn accepted_by_this_build(bytes: &[u8]) -> bool {
    matches!(oce_api::inspect_blob(bytes), Ok(summary) if summary.decode_refusal.is_none())
}

#[test]
fn a_summary_with_no_refusal_is_exactly_a_blob_the_decoder_accepts() {
    // Accepted: the hand-built models, and every golden (`blob_summary.rs`, `blob_goldens.rs`).
    for image in [chain(), adder(), delay(), moving_mean(4), Image::new()] {
        let bytes = image.to_bytes();
        decode(&bytes).unwrap();
        assert!(accepted_by_this_build(&bytes));
    }
    for name in FIXTURES {
        assert!(accepted_by_this_build(&golden(name)), "{name}");
    }

    // `is_ok()` is not that check: this blob has a summary, and no build of this engine version
    // runs it, because its class signature hash is not the one this build computes.
    let mut wrong_hash = chain();
    let expected = wrong_hash.classes[1].1;
    wrong_hash.classes[1].1 = !expected;
    let bytes = wrong_hash.to_bytes();
    let summary = oce_api::inspect_blob(&bytes).unwrap();
    assert!(!accepted_by_this_build(&bytes));
    assert_eq!(
        summary.decode_refusal,
        Some(BlobError::ClassSignatureMismatch {
            class: 1,
            expected,
            found: !expected
        })
    );
    assert_eq!(decode(&bytes).err(), summary.decode_refusal);

    // Bytes with no summary are not accepted either, and the refusal is then the `Err`.
    let mut unknown = chain();
    unknown.classes[0].0 = b"CDL.Reals.Abz".to_vec();
    let bytes = unknown.to_bytes();
    assert!(!accepted_by_this_build(&bytes));
    assert!(matches!(
        oce_api::inspect_blob(&bytes),
        Err(OcError::Blob(BlobError::UnknownClass { class: 0 }))
    ));
}

#[test]
fn the_refusal_is_the_first_one_the_decoder_reports_when_a_blob_has_several() {
    // A wrong hash in class 1 (step 7, the class table), a non-finite parameter in block 0
    // (step 7, the blocks), an undriven input (step 8) and a reversed schedule (step 9): the
    // decoder reports them in that order as each earlier one is repaired, and so does the
    // summary.
    let mut image = chain();
    let hash = image.classes[1].1;
    image.classes[1].1 ^= 1 << 63;
    image.instances[0].params[0].1 = Val::real(f64::INFINITY);
    image.connections.clear();
    image.schedule = vec![1, 0];
    let expected = [
        BlobError::ClassSignatureMismatch {
            class: 1,
            expected: hash,
            found: hash ^ (1 << 63),
        },
        BlobError::NonFiniteParameter {
            block: 0,
            parameter: 0,
        },
        BlobError::UndrivenInput { connector: 1 },
        BlobError::InvalidSchedule {
            position: 0,
            fault: ScheduleFault::EmitBefore,
        },
    ];
    let good = chain();
    let repairs: [&dyn Fn(&mut Image); 4] = [
        &|image| image.classes[1].1 = good.classes[1].1,
        &|image| image.instances[0].params = good.instances[0].params.clone(),
        &|image| image.connections = good.connections.clone(),
        &|image| image.schedule = good.schedule.clone(),
    ];
    for (refusal, repair) in expected.into_iter().zip(repairs) {
        let bytes = image.to_bytes();
        assert_eq!(decode(&bytes).err(), Some(refusal));
        let (facts, verdict) = summarise_with_refusal(&bytes).unwrap();
        assert_eq!(verdict, Some(refusal));
        assert_eq!(facts, Facts::of_image(&image));
        repair(&mut image);
    }
    // Every defect repaired: the same summary reader now reports acceptance.
    assert_eq!(
        summarise_with_refusal(&image.to_bytes()),
        Ok((Facts::of_image(&good), None))
    );
}

#[test]
fn a_refusal_of_every_later_decoder_step_reaches_the_summary_unchanged() {
    // One blob per refusal the nine cases above do not reach: the remaining instance fields of
    // step 7, an enum parameter fault, a boundary input and a connection fault of step 8, the
    // other schedule faults of step 9 and the state budget of step 10.
    let enum_holder = || {
        let mut image = chain();
        image.instances[0].params[0].1 = Val::Enum(b"CDL.Types.SimpleController".to_vec(), 1);
        image
    };
    let cases: Vec<(&str, Image)> = vec![
        ("a stateless block recorded as stateful", {
            let mut image = chain();
            image.instances[1].kind = 1;
            image
        }),
        ("a state length the constructed block does not have", {
            let mut image = chain();
            image.instances[1].state_len = 1;
            image
        }),
        ("a CRC on a block without state", {
            let mut image = chain();
            image.instances[1].state_crc = 1;
            image
        }),
        ("one input too many", {
            let mut image = chain();
            image.instances[1].inputs.push(3);
            image.connectors = 4;
            image
        }),
        ("no outputs", {
            let mut image = chain();
            image.instances[1].outputs.clear();
            image
        }),
        ("an enum class this build does not have", {
            let mut image = enum_holder();
            image.instances[0].params[0].1 = Val::Enum(b"CDL.Types.NoSuchEnum".to_vec(), 1);
            image
        }),
        ("an enum ordinal of zero", {
            let mut image = enum_holder();
            image.instances[0].params[0].1 = Val::Enum(b"CDL.Types.SimpleController".to_vec(), 0);
            image
        }),
        ("a connector claimed twice", {
            let mut image = chain();
            image.instances[1].outputs = vec![0];
            image
        }),
        ("a boundary input with no targets", {
            let mut image = adder();
            image.inputs[0].targets.clear();
            image
        }),
        ("a boundary input of the wrong type", {
            let mut image = adder();
            image.inputs[0].vtype = VType::Integer;
            image
        }),
        ("a connection from an input", {
            let mut image = chain();
            image.connections = vec![(1, 1)];
            image
        }),
        ("a boundary output read from an input", {
            let mut image = chain();
            image.outputs = vec![out("urn:test:y", 1)];
            image
        }),
        ("a schedule naming a block that does not exist", {
            let mut image = chain();
            image.schedule = vec![0, 7];
            image
        }),
        ("a schedule naming one block twice", {
            let mut image = chain();
            image.schedule = vec![0, 0];
            image
        }),
        ("one word past the state budget", {
            let overhead = moving_mean(1).instances[0].state_len as usize - 1;
            moving_mean(i64::try_from(MAX_STATE_WORDS - overhead).unwrap() + 1)
        }),
    ];
    let mut seen = Vec::new();
    for (what, image) in &cases {
        let bytes = image.to_bytes();
        let decoder = decode(&bytes)
            .err()
            .unwrap_or_else(|| panic!("{what}: the decoder accepts the blob"));
        let (facts, verdict) =
            summarise_with_refusal(&bytes).unwrap_or_else(|err| panic!("{what}: {err:?}"));
        assert_eq!(verdict, Some(decoder), "{what}");
        assert_eq!(facts, Facts::of_image(image), "{what}");
        seen.push(decoder);
    }
    // The cases reach the refusals they are named for, so the list above is not fifteen ways to
    // one error.
    let count = |pattern: &dyn Fn(&BlobError) -> bool| seen.iter().filter(|e| pattern(e)).count();
    assert_eq!(
        count(&|e| matches!(e, BlobError::InstanceMismatch { .. })),
        5
    );
    for field in [
        InstanceField::Kind,
        InstanceField::StateLen,
        InstanceField::StateCrc,
        InstanceField::InputArity,
        InstanceField::OutputArity,
    ] {
        assert!(seen.contains(&BlobError::InstanceMismatch { block: 1, field }));
    }
    assert_eq!(
        count(&|e| matches!(e, BlobError::UnknownEnumClass { .. })),
        1
    );
    assert_eq!(
        count(&|e| matches!(e, BlobError::EnumOrdinalOutOfRange { ordinal: 0, .. })),
        1
    );
    assert!(seen.contains(&BlobError::InvalidConnector {
        connector: 0,
        fault: ConnectorFault::Duplicated
    }));
    assert!(seen.contains(&BlobError::InvalidInput {
        input: 0,
        fault: InputFault::NoTargets
    }));
    assert!(seen.contains(&BlobError::InvalidInput {
        input: 0,
        fault: InputFault::TargetTypeMismatch
    }));
    assert!(seen.contains(&BlobError::InvalidConnection {
        connection: 0,
        fault: ConnectionFault::ProducerNotOutput
    }));
    assert!(seen.contains(&BlobError::InvalidOutput {
        output: 0,
        fault: OutputFault::SourceNotOutput
    }));
    assert!(seen.contains(&BlobError::InvalidSchedule {
        position: 1,
        fault: ScheduleFault::BlockOutOfRange
    }));
    assert!(seen.contains(&BlobError::InvalidSchedule {
        position: 1,
        fault: ScheduleFault::Duplicate
    }));
    assert!(seen.contains(&BlobError::StateTooLarge {
        block: 0,
        requested: MAX_STATE_WORDS as u64 + 1
    }));
}

#[test]
fn a_summary_refusal_is_never_reported_as_a_decode_refusal() {
    // The two checks the summary makes itself, and every refusal of the framing and structural
    // pass, are `Err`s of the summary: `decode_refusal` never carries one, because bytes refused
    // that way have no summary to carry it. Each of these blobs has such a defect together with
    // one a later decoder step would report.
    let mut unknown = chain();
    unknown.classes[0].0 = b"CDL.Reals.Abz".to_vec();
    unknown.schedule = vec![1, 0];
    assert_eq!(
        summarise_with_refusal(&unknown.to_bytes()),
        Err(BlobError::UnknownClass { class: 0 })
    );
    let mut index = chain();
    index.instances[1].class = 2;
    index.schedule = vec![];
    assert_eq!(
        summarise_with_refusal(&index.to_bytes()),
        Err(BlobError::InstanceMismatch {
            block: 1,
            field: InstanceField::ClassIndex
        })
    );
    let mut flagged = chain();
    flagged.flags = 1;
    flagged.connections.clear();
    assert_eq!(
        summarise_with_refusal(&flagged.to_bytes()),
        Err(BlobError::NonzeroFlags { flags: 1 })
    );
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
