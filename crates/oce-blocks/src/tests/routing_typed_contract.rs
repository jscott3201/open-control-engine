use super::common::*;

#[test]
fn typed_routing_resolved_signatures_use_instance_width() {
    let boolean_routing_block = BooleanVectorReplicator::new(2, 3);
    let boolean_routing = boolean_routing_block.resolved_signature();
    assert_eq!(
        boolean_routing.class_path,
        "CDL.Routing.BooleanVectorReplicator"
    );
    assert_eq!(
        boolean_routing.inputs.as_ref(),
        &[PortKind::Boolean, PortKind::Boolean]
    );
    assert_eq!(boolean_routing.outputs.as_ref(), &[PortKind::Boolean; 6]);

    let integer_routing_block = IntegerVectorReplicator::new(2, 3);
    let integer_routing = integer_routing_block.resolved_signature();
    assert_eq!(
        integer_routing.class_path,
        "CDL.Routing.IntegerVectorReplicator"
    );
    assert_eq!(
        integer_routing.inputs.as_ref(),
        &[PortKind::Integer, PortKind::Integer]
    );
    assert_eq!(integer_routing.outputs.as_ref(), &[PortKind::Integer; 6]);

    let boolean_extractor_block = BooleanExtractor::new(2);
    let boolean_extractor = boolean_extractor_block.resolved_signature();
    assert_eq!(
        boolean_extractor.inputs.as_ref(),
        &[PortKind::Integer, PortKind::Boolean, PortKind::Boolean]
    );
    assert_eq!(boolean_extractor.outputs.as_ref(), &[PortKind::Boolean]);

    let integer_extractor_block = IntegerExtractor::new(2);
    let integer_extractor = integer_extractor_block.resolved_signature();
    assert_eq!(
        integer_extractor.inputs.as_ref(),
        &[PortKind::Integer, PortKind::Integer, PortKind::Integer]
    );
    assert_eq!(integer_extractor.outputs.as_ref(), &[PortKind::Integer]);
}

#[test]
fn typed_routing_feedthrough_classification_matches_spec() {
    assert!(BooleanExtractSignal::new(3, 2, vec![3, 1]).feeds_through(2, 0));
    assert!(BooleanExtractor::new(3).feeds_through(3, 0));
    assert!(BooleanScalarReplicator::new(2).feeds_through(0, 1));
    assert!(BooleanVectorFilter::new(3, 2, vec![true, false, true]).feeds_through(2, 1));
    assert!(BooleanVectorReplicator::new(2, 3).feeds_through(1, 5));
    assert!(IntegerExtractSignal::new(3, 2, vec![3, 1]).feeds_through(2, 0));
    assert!(IntegerExtractor::new(3).feeds_through(3, 0));
    assert!(IntegerScalarReplicator::new(2).feeds_through(0, 1));
    assert!(IntegerVectorFilter::new(3, 2, vec![true, false, true]).feeds_through(2, 1));
    assert!(IntegerVectorReplicator::new(2, 3).feeds_through(1, 5));
}

#[test]
fn typed_routing_registry_make_resolves_parameters() {
    let boolean_extract_signal =
        (lookup("CDL.Routing.BooleanExtractSignal").unwrap().make)(&ParamTable {
            values: vec![
                (Arc::from("nin"), Value::Integer(3)),
                (Arc::from("nout"), Value::Integer(2)),
                (Arc::from("extract_1"), Value::Integer(3)),
                (Arc::from("extract_2"), Value::Integer(1)),
            ],
        });
    assert_eq!(
        boolean_extract_signal.resolved_signature().inputs.as_ref(),
        &[PortKind::Boolean; 3]
    );
    let boolean_extract_out = outs(
        boolean_extract_signal.as_ref(),
        &[
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true),
        ],
    );
    assert!(boolean_extract_out[0].bit_eq(&Value::Boolean(true)));
    assert!(boolean_extract_out[1].bit_eq(&Value::Boolean(false)));

    let integer_vector_filter =
        (lookup("CDL.Routing.IntegerVectorFilter").unwrap().make)(&ParamTable {
            values: vec![
                (Arc::from("nin"), Value::Integer(3)),
                (Arc::from("nout"), Value::Integer(2)),
                (Arc::from("msk_1"), Value::Boolean(true)),
                (Arc::from("msk_2"), Value::Boolean(false)),
                (Arc::from("msk_3"), Value::Boolean(true)),
            ],
        });
    assert_eq!(
        integer_vector_filter.resolved_signature().outputs.as_ref(),
        &[PortKind::Integer; 2]
    );
    assert!(
        outs(
            integer_vector_filter.as_ref(),
            &[Value::Integer(4), Value::Integer(5), Value::Integer(6)]
        )[1]
        .bit_eq(&Value::Integer(6))
    );
}
