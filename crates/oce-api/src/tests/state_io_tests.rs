//! Independent domain encoding and adversarial input-definition byte tests.

use super::*;
use crate::state_wire::DecodeBudget;

fn decode(bytes: &[u8]) -> Result<Vec<InputManifestEntry>, EngineStateError> {
    read_inputs(&mut Reader::new(
        bytes,
        0,
        DecodeBudget::for_input(bytes.len()),
    ))
}

#[test]
fn input_domain_bytes_match_the_hand_assembled_signed_zero_infinity_vector() {
    let entries = vec![InputManifestEntry {
        path: "u".into(),
        value_type: WireValueType::Real,
        min: Some(WireValue::Real((-0.0f64).to_bits())),
        max: Some(WireValue::Real(f64::INFINITY.to_bits())),
    }];
    // LE count=1, byte-length=1, 'u', Real tag=0; each bound: Some=1, Real=0, LE bits.
    let expected = [
        1, 0, 0, 0, 1, 0, 0, 0, b'u', 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 128, 1, 0, 0, 0, 0, 0, 0, 0,
        240, 127,
    ];
    for _ in 0..2 {
        let mut writer = Writer::new();
        write_inputs(&mut writer, &entries).unwrap();
        assert_eq!(writer.finish().unwrap(), expected);
        assert_eq!(decode(&expected).unwrap(), entries);
    }
    for end in 0..expected.len() {
        assert!(
            matches!(
                decode(&expected[..end]),
                Err(EngineStateError::MalformedSnapshot { .. })
            ),
            "{end}"
        );
    }
}

#[test]
fn native_bound_kinds_preserve_bits_and_never_round_integer_limits() {
    let entries = vec![
        InputManifestEntry {
            path: "enum".into(),
            value_type: WireValueType::Enum("C".into()),
            min: Some(WireValue::Enum {
                class_path: "C".into(),
                ordinal: 1,
            }),
            max: None,
        },
        InputManifestEntry {
            path: "int".into(),
            value_type: WireValueType::Integer,
            min: Some(WireValue::Integer(i64::MIN)),
            max: Some(WireValue::Integer(i64::MAX)),
        },
        InputManifestEntry {
            path: "real".into(),
            value_type: WireValueType::Real,
            min: Some(WireValue::Real(0x7ff8_0000_0000_0042)),
            max: Some(WireValue::Real(1)),
        },
    ];
    let mut writer = Writer::new();
    write_inputs(&mut writer, &entries).unwrap();
    assert_eq!(decode(&writer.finish().unwrap()).unwrap(), entries);
}

#[test]
fn optional_tags_and_noncanonical_definition_order_refuse_exactly() {
    for tag in [2, 255] {
        assert!(
            matches!(read_optional_text(&mut Reader::new(&[tag], 0, DecodeBudget::for_input(1))),
            Err(EngineStateError::MalformedSnapshot { offset: 1, detail })
                if detail == format!("invalid optional text tag {tag}"))
        );
        let bytes = [1, 0, 0, 0, 1, 0, 0, 0, b'u', 0, tag, 0];
        assert!(
            matches!(decode(&bytes), Err(EngineStateError::MalformedSnapshot { offset: 11, detail })
            if detail == format!("invalid input bound tag {tag}"))
        );
    }
    let entry = |path: &str| InputManifestEntry {
        path: path.into(),
        value_type: WireValueType::Boolean,
        min: None,
        max: None,
    };
    for entries in [vec![entry("u"), entry("u")], vec![entry("z"), entry("a")]] {
        let mut writer = Writer::new();
        write_inputs(&mut writer, &entries).unwrap();
        assert!(
            matches!(decode(&writer.finish().unwrap()), Err(EngineStateError::MalformedSnapshot { detail, .. })
            if detail == "input definitions are duplicate or out of order")
        );
    }
}
