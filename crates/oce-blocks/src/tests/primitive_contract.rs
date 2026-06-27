use super::common::*;

#[test]
fn ctx_warn_uses_scheduler_time_not_block_fabricated_time() {
    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(3.0, &diag);
    cx.warn("test.assert", "tripped");
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "test.assert");
    assert_eq!(events[0].1, "tripped");
    assert_eq!(events[0].2.to_bits(), 3.0f64.to_bits());
}

#[test]
fn port_shape_expands_to_resolved_scalar_port_kinds() {
    let shape = PortShape::new(PortKind::Boolean, 3);
    assert_eq!(
        shape.to_kinds(),
        vec![PortKind::Boolean, PortKind::Boolean, PortKind::Boolean]
    );
    let scalar = PortShape::scalar(PortKind::Real);
    assert_eq!(scalar.to_kinds(), vec![PortKind::Real]);
    let empty = PortShape::new(PortKind::Integer, 0);
    assert!(empty.to_kinds().is_empty());
}

#[test]
fn resolved_signature_uses_instance_width_for_vector_ports() {
    let fixed = Add.resolved_signature();
    assert_eq!(fixed.inputs.as_ref(), &[PortKind::Real, PortKind::Real]);
    assert_eq!(fixed.outputs.as_ref(), &[PortKind::Real]);

    let multi_and = MultiAnd::new(3);
    let multi = multi_and.resolved_signature();
    assert_eq!(multi.class_path, "CDL.Logical.MultiAnd");
    assert_eq!(
        multi.inputs.as_ref(),
        &[PortKind::Boolean, PortKind::Boolean, PortKind::Boolean]
    );
    assert_eq!(multi.outputs.as_ref(), &[PortKind::Boolean]);

    let multi_sum = MultiSum::new(vec![0.5, 1.0]);
    let real_multi = multi_sum.resolved_signature();
    assert_eq!(real_multi.class_path, "CDL.Reals.MultiSum");
    assert_eq!(
        real_multi.inputs.as_ref(),
        &[PortKind::Real, PortKind::Real]
    );
    assert_eq!(real_multi.outputs.as_ref(), &[PortKind::Real]);

    let routing_block = RealVectorReplicator::new(2, 3);
    let routing = routing_block.resolved_signature();
    assert_eq!(routing.class_path, "CDL.Routing.RealVectorReplicator");
    assert_eq!(routing.inputs.as_ref(), &[PortKind::Real, PortKind::Real]);
    assert_eq!(routing.outputs.as_ref(), &[PortKind::Real; 6]);

    let extractor_block = RealExtractor::new(2);
    let extractor = extractor_block.resolved_signature();
    assert_eq!(
        extractor.inputs.as_ref(),
        &[PortKind::Integer, PortKind::Real, PortKind::Real]
    );
    assert_eq!(extractor.outputs.as_ref(), &[PortKind::Real]);
}

#[test]
fn read_int_reads_integer_and_release_degrades_to_zero() {
    assert_eq!(read_int(&[Value::Integer(42)], 0), 42);
    assert_eq!(read_int(&[Value::Integer(-7)], 0), -7);
    if cfg!(debug_assertions) {
        assert!(
            std::panic::catch_unwind(|| read_int(&[Value::Real(1.0)], 0)).is_err(),
            "debug builds must trip the validation-bug assertion"
        );
    } else {
        assert_eq!(read_int(&[Value::Real(1.0)], 0), 0);
    }
}
