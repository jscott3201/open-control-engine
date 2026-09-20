//! Independent wire corpus and deterministic allocation-free rejection of hostile records.

mod replay_oracle;

use allocation_counter::measure;
use oce_api::{
    CompatibilityDescriptor, CompatibilityMismatch, MAX_REPLAY_BYTES, ReplayError, ReplayRecord,
    Value,
};
use replay_oracle::{DESCRIPTOR, assemble, body_start, hex, offset, seal};

fn refuses(bytes: &[u8], expected: ReplayError) {
    for _ in 0..3 {
        let census = measure(|| {
            assert_eq!(ReplayRecord::from_bytes(bytes).unwrap_err(), expected);
        });
        assert_eq!(
            census.count_total, 0,
            "admission refuses before any untrusted allocation"
        );
    }
}

#[test]
fn hand_authored_values_decode_every_domain_without_normalization() {
    let bytes = replay_oracle::values();
    for _ in 0..3 {
        let record = ReplayRecord::from_bytes(&bytes).unwrap();
        assert_eq!(record.time().to_bits(), (-0.0_f64).to_bits());
        let values = record.inputs();
        assert_eq!(
            values.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c", "d", "e", "f", "g"]
        );
        for (i, expected) in [
            Value::Real(-0.0),
            Value::Real(f64::from_bits(0x7ff8_0000_0000_0042)),
            Value::Integer(i64::MIN),
            Value::Integer(i64::MAX),
            Value::Boolean(true),
            Value::String("λ\n\0".into()),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(values[i].1.bit_eq(&expected));
        }
        assert!(matches!(values[6].1, Value::Enum { ordinal: 4, .. }));
        let events = record.diagnostics();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].block, "α");
        assert_eq!(events[0].message, "x\0\n");
        assert_eq!(events[0].t.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(events[1].t.to_bits(), 0x7ff8_0000_0000_0042);
        assert_eq!(events[0].level, oce_api::AssertLevel::Warning);
        assert_eq!(record.as_bytes(), bytes);
    }
}

#[test]
fn header_version_lengths_integrity_and_every_truncation_refuse_repeatedly() {
    let bytes = replay_oracle::values();
    for len in 0..bytes.len() {
        refuses(
            &bytes[..len],
            if len < 20 {
                ReplayError::Header
            } else {
                ReplayError::Length { offset: 12 }
            },
        );
    }
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    refuses(&changed, ReplayError::Header);
    changed = bytes.clone();
    changed[8..12].copy_from_slice(&2_u32.to_le_bytes());
    refuses(&changed, ReplayError::UnsupportedFormat { revision: 2 });
    changed = bytes.clone();
    changed[12..20].fill(255);
    refuses(&changed, ReplayError::Length { offset: 12 });
    for i in 20..bytes.len() {
        changed.clone_from(&bytes);
        changed[i] ^= 1;
        refuses(&changed, ReplayError::Integrity);
    }
    // Repair outer length/checksum on every partial body. The field parser still rejects it,
    // without a panic or an allocation; exact first cause repeats for each particular prefix.
    for end in 20..bytes.len() - 16 {
        let mut prefix = bytes[..end].to_vec();
        prefix[12..20].copy_from_slice(&((end - 20) as u64).to_le_bytes());
        prefix.extend([0; 16]);
        seal(&mut prefix);
        let error = ReplayRecord::from_bytes(&prefix).unwrap_err();
        assert!(matches!(error, ReplayError::Length { .. }));
        refuses(&prefix, error);
    }
}

#[test]
fn rechecksummed_tags_boolean_enum_utf8_time_counts_and_suffix_are_not_canonical() {
    let bytes = replay_oracle::values();
    let value = |key: u8| offset(&bytes, &[1, 0, 0, 0, key]) + 5;
    let mut cases = vec![
        (20, 1, ReplayError::UnsupportedExactness { tag: 1 }),
        (21, 2, ReplayError::UnknownPlacement { tag: 2 }),
        (value(b'a'), 9, ReplayError::UnknownValue { tag: 9 }),
        (
            value(b'e') + 1,
            2,
            ReplayError::Noncanonical {
                offset: value(b'e') + 1,
            },
        ),
        (
            value(b'f') + 5,
            255,
            ReplayError::Utf8 {
                offset: value(b'f') + 5,
            },
        ),
        (
            value(b'g') + 5,
            b'X',
            ReplayError::EnumDomain {
                offset: value(b'g') + 1,
            },
        ),
        (
            value(b'g') + 5 + 26,
            0,
            ReplayError::EnumDomain {
                offset: value(b'g') + 1,
            },
        ),
        (
            value(b'g') + 5 + 26,
            5,
            ReplayError::EnumDomain {
                offset: value(b'g') + 1,
            },
        ),
        (bytes.len() - 17, 1, ReplayError::UnknownSeverity { tag: 1 }),
    ];
    for tag in 5..=255 {
        cases.push((value(b'a'), tag, ReplayError::UnknownValue { tag }));
    }
    for (at, byte, expected) in cases {
        let mut changed = bytes.clone();
        changed[at] = byte;
        seal(&mut changed);
        refuses(&changed, expected);
    }
    for bits in [
        f64::NAN.to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ] {
        let mut changed = bytes.clone();
        changed[body_start()..body_start() + 8].copy_from_slice(&bits.to_le_bytes());
        seal(&mut changed);
        refuses(&changed, ReplayError::NonFiniteTime);
    }
    for at in [22, body_start() + 8, value(b'f') + 1] {
        let mut changed = bytes.clone();
        changed[at..at + 4].fill(255);
        seal(&mut changed);
        // Strings report the start of missing bytes; count reports the count prefix.
        refuses(
            &changed,
            ReplayError::Length {
                offset: if at == body_start() + 8 { at } else { at + 4 },
            },
        );
    }
    let mut changed = bytes.clone();
    let end = changed.len() - 16;
    changed.insert(end, 0);
    changed[12..20].copy_from_slice(&((end + 1 - 20) as u64).to_le_bytes());
    seal(&mut changed);
    refuses(&changed, ReplayError::Noncanonical { offset: end });
}

#[test]
fn duplicate_and_unordered_keys_refuse_for_both_input_and_output_sections() {
    for output in [false, true] {
        for keys in [["a", "a"], ["z", "a"], ["", "a"]] {
            let mut payload = vec![0; 8];
            if output {
                payload.extend(0_u32.to_le_bytes());
            }
            payload.extend(2_u32.to_le_bytes());
            let at = body_start() + payload.len();
            for key in keys {
                replay_oracle::string(&mut payload, key);
                payload.extend([2, 0]);
            }
            if !output {
                payload.extend(0_u32.to_le_bytes());
            }
            payload.extend(0_u32.to_le_bytes());
            let bytes = assemble(DESCRIPTOR, None, &payload);
            refuses(
                &bytes,
                ReplayError::Noncanonical {
                    offset: at + if keys[0].is_empty() { 0 } else { 7 },
                },
            );
        }
    }
}

#[test]
fn malformed_descriptor_fields_refuse_instead_of_becoming_ignored_extensions() {
    let payload = hex(include_str!("fixtures/replay_add.hex"));
    for text in [
        DESCRIPTOR.replace("compatibility:1", "compatibility:01"),
        DESCRIPTOR.replace("catalog-schema:1", "catalog-schema:0"),
        DESCRIPTOR.replace("io-schema:1", "io-schema:4294967296"),
        DESCRIPTOR.replace("96724061", "9672406A"),
        DESCRIPTOR.replace("HostTick-v1", "arbitrary"),
        DESCRIPTOR.replace("0.1.0", "01.1.0"),
        DESCRIPTOR.replace("0.1.0", "0.1.0-01"),
        DESCRIPTOR.replace("0.1.0", "0.1.0+"),
        DESCRIPTOR.replace("export:none", "export:"),
        DESCRIPTOR.replace("value-schema:1\n", ""),
        DESCRIPTOR.replace("\n", "\r\n"),
        format!("{DESCRIPTOR}extra:1\n"),
        DESCRIPTOR.trim_end().into(),
        "".into(),
        "x".repeat(1025),
    ] {
        refuses(
            &assemble(&text, None, &payload),
            ReplayError::MalformedDescriptor,
        );
    }
}

#[test]
fn every_public_fact_has_an_ordered_typed_eligibility_refusal() {
    let expected = CompatibilityDescriptor::current(None).unwrap();
    let payload = hex(include_str!("fixtures/replay_add.hex"));
    for (from, to, cause) in [
        (
            "compatibility:1",
            "compatibility:2",
            CompatibilityMismatch::DescriptorRevision,
        ),
        (
            "catalog-schema:1",
            "catalog-schema:2",
            CompatibilityMismatch::CatalogSchema,
        ),
        (
            "96724061",
            "06724061",
            CompatibilityMismatch::CatalogContent,
        ),
        (
            "io-schema:1",
            "io-schema:2",
            CompatibilityMismatch::IoSchema,
        ),
        (
            "value-schema:1",
            "value-schema:2",
            CompatibilityMismatch::ValueSchema,
        ),
        (
            "parameter-schema:1",
            "parameter-schema:2",
            CompatibilityMismatch::ParameterSchema,
        ),
        (
            "HostTick-v1",
            "HostTick-v2",
            CompatibilityMismatch::ExecutionProfile,
        ),
        (
            "execution-profile-schema:2",
            "execution-profile-schema:3",
            CompatibilityMismatch::ExecutionProfile,
        ),
        (
            "0.1.0",
            "0.1.0-next.1+build.01",
            CompatibilityMismatch::Build,
        ),
        (
            "export:none",
            "export:cxf:fnv1a128:00000000000000000000000000000000",
            CompatibilityMismatch::ExportPresence,
        ),
    ] {
        // Simultaneous foreign target proves descriptor precedence over placement.
        let bytes = assemble(
            &DESCRIPTOR.replace(from, to),
            Some(("foreign", "foreign")),
            &payload,
        );
        for _ in 0..3 {
            let record = ReplayRecord::from_bytes(&bytes).unwrap();
            assert_eq!(
                record.check_compatible(&expected),
                Err(ReplayError::DescriptorMismatch(cause))
            );
        }
    }
    // A present expected export is compared as content, never treated as a wildcard.
    let mut engine = oce_api::Engine::in_memory();
    engine
        .load_cxf(include_bytes!("fixtures/legacy_frame_add.jsonld"))
        .unwrap();
    let export = engine.export_cxf().unwrap();
    let descriptor = CompatibilityDescriptor::current(Some(&export)).unwrap();
    let text = descriptor.to_string();
    let at = text.find("export:cxf:fnv1a128:").unwrap() + "export:cxf:fnv1a128:".len();
    let mut changed = text.into_bytes();
    changed[at] = if changed[at] == b'0' { b'1' } else { b'0' };
    let record = ReplayRecord::from_bytes(&assemble(
        std::str::from_utf8(&changed).unwrap(),
        None,
        &payload,
    ))
    .unwrap();
    assert_eq!(
        record.check_compatible(&descriptor),
        Err(ReplayError::DescriptorMismatch(
            CompatibilityMismatch::ExportContent
        ))
    );
}

#[test]
fn target_labels_are_canonical_and_foreign_components_refuse_independently() {
    let payload = hex(include_str!("fixtures/replay_add.hex"));
    let expected = CompatibilityDescriptor::current(None).unwrap();
    for target in [
        ("foreign", std::env::consts::OS),
        (std::env::consts::ARCH, "foreign"),
    ] {
        let bytes = assemble(DESCRIPTOR, Some(target), &payload);
        for _ in 0..3 {
            assert_eq!(
                ReplayRecord::from_bytes(&bytes)
                    .unwrap()
                    .check_compatible(&expected),
                Err(ReplayError::TargetMismatch)
            );
        }
    }
    for label in ["", "a-b", "MACOS", "☃"] {
        refuses(
            &assemble(DESCRIPTOR, Some((label, "linux")), &payload),
            ReplayError::Noncanonical { offset: 22 },
        );
    }
}

#[test]
fn inclusive_record_cap_accepts_a_full_string_and_one_past_allocates_nothing() {
    let mut payload = vec![0; 8];
    payload.extend(0_u32.to_le_bytes()); // no inputs
    payload.extend(1_u32.to_le_bytes()); // one output
    replay_oracle::string(&mut payload, "s");
    payload.push(3);
    let overhead = assemble(DESCRIPTOR, None, &[payload.clone(), vec![0; 8]].concat()).len();
    let text_len = MAX_REPLAY_BYTES - overhead;
    payload.extend((text_len as u32).to_le_bytes());
    payload.resize(payload.len() + text_len, b'x');
    payload.extend(0_u32.to_le_bytes());
    let mut bytes = assemble(DESCRIPTOR, None, &payload);
    drop(payload);
    assert_eq!(bytes.len(), MAX_REPLAY_BYTES);
    let census = measure(|| {
        let record = ReplayRecord::from_bytes(&bytes).unwrap();
        assert!(matches!(&record.outputs()[0].1, Value::String(s) if s.len() == text_len));
        assert_eq!(record.as_bytes(), bytes);
    });
    assert_eq!(census.bytes_current, 0);
    assert!(census.bytes_total <= (3 * MAX_REPLAY_BYTES) as u64);
    bytes.push(0);
    refuses(
        &bytes,
        ReplayError::TooLarge {
            actual_bytes: MAX_REPLAY_BYTES + 1,
            limit_bytes: MAX_REPLAY_BYTES,
        },
    );
}

#[test]
fn small_wire_entries_cannot_amplify_decode_workspace_past_the_charged_budget() {
    let count = 2 * MAX_REPLAY_BYTES / 64 + 1;
    let mut payload = vec![0; 8];
    payload.extend((count as u32).to_le_bytes());
    payload.resize(payload.len() + 6 * count, 0);
    let bytes = assemble(DESCRIPTOR, None, &payload);
    assert!(bytes.len() < MAX_REPLAY_BYTES);
    // Count is possible from raw length alone but impossible under the decoded allocation budget.
    refuses(&bytes, ReplayError::WorkspaceLimit);
    let positive = measure(|| {
        std::hint::black_box(vec![0; 1024]);
    });
    assert!(positive.count_total > 0);
}
