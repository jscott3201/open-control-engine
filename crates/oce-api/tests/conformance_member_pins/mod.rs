//! End-to-end pin-table data for the `hasInstance` member-interface slice of the contract
//! corpus (`_spec/19`), consumed by `conformance.rs`'s assembled tables — the drivers always
//! compare one assembled table against the disk, never a partial view.

use oce_diag::DiagCode;

use super::{CompositeRejection, CompositeWarning};

/// The slice's rejected fixtures: contract rule id (`None` for untagged shared machinery) and
/// the exact ordered (code, subject, message) triples `Engine::load_cxf` must surface. The
/// partial-declaration row pins its complete three-entry vector, Warning included.
pub(crate) const MEMBER_REJECTIONS: [CompositeRejection; 22] = [
    (
        "vector_port_instance.jsonld",
        Some("vector-port-instance"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.rep"),
            "composite/vector-port-instance: instance of class \
             `CDL.Routing.RealScalarReplicator` derives its port count from a parameter; this \
             subset derives scalar interfaces only",
        )],
    ),
    (
        "unsupported_instance_member.jsonld",
        Some("unsupported-instance-member"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.sin.zzz"),
            "composite/unsupported-instance-member: `zzz` is neither a declared port nor a \
             declared parameter of `CDL.Reals.Sources.Sin`",
        )],
    ),
    (
        "member_outside_owner.jsonld",
        Some("unsupported-instance-member"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.other.q"),
            "composite/unsupported-instance-member: member is not a direct member of \
             `http://example.org#M.c`",
        )],
    ),
    (
        "member_is_instance.jsonld",
        Some("unsupported-instance-member"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.a.sub"),
            "composite/unsupported-instance-member: member `http://example.org#M.a.sub` is \
             itself a block instance",
        )],
    ),
    (
        "minted_identity_shadows_node.jsonld",
        Some("colliding-member-identity"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.del.y"),
            "composite/colliding-member-identity: synthesized connector identity \
             `http://example.org#M.del.y` is already an `@graph` node",
        )],
    ),
    (
        "minted_identity_twice.jsonld",
        Some("colliding-member-identity"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c.u"),
            "composite/colliding-member-identity: connector identity \
             `http://example.org#M.c.u` is minted twice by `http://example.org#M.c`",
        )],
    ),
    (
        "param_member_duplicates_hasparameter.jsonld",
        Some("colliding-member-identity"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.del.samplePeriod"),
            "composite/colliding-member-identity: parameter `samplePeriod` is declared by both \
             `hasParameter` and `hasInstance` on `http://example.org#M.del`",
        )],
    ),
    (
        "param_member_declared_twice.jsonld",
        Some("colliding-member-identity"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.del.samplePeriod"),
            "composite/colliding-member-identity: parameter `samplePeriod` is declared twice \
             by `http://example.org#M.del`",
        )],
    ),
    (
        "array_marked_member.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c.u"),
            "composite/array-connector: array-valued connector nodes are not supported; \
             flatten the array to one connector per element",
        )],
    ),
    (
        "array_marked_nested_composite_input.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.sub.arr"),
            "composite/array-connector: array-valued connector nodes are not supported; \
             flatten the array to one connector per element",
        )],
    ),
    (
        "array_marked_orphan_input.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.orphan.arr"),
            "composite/array-connector: array-valued connector nodes are not supported; \
             flatten the array to one connector per element",
        )],
    ),
    (
        "carveout_member_array_hasinstance.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.con.protected.arr"),
            "composite/array-connector: array-valued connector nodes are not supported; \
             flatten the array to one connector per element",
        )],
    ),
    (
        "carveout_member_array_hasinput.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.con.protected.arr"),
            "composite/array-connector: array-valued connector nodes are not supported; \
             flatten the array to one connector per element",
        )],
    ),
    (
        "nodeless_parameter_member.jsonld",
        None,
        &[(
            DiagCode::GroundingFailed,
            Some("http://example.org#M.del.samplePeriod"),
            "parameter has no value (Ground mode)",
        )],
    ),
    (
        "member_isofdatatype_unresolved.jsonld",
        None,
        &[(
            DiagCode::UnresolvedReference,
            Some("http://data.ashrae.org/S231P#Bogus"),
            "unresolved isOfDataType",
        )],
    ),
    (
        "member_string_type.jsonld",
        None,
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.c.u"),
            "String connector not permitted (§7.8)",
        )],
    ),
    (
        "member_unrecognized_type.jsonld",
        None,
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.c.u"),
            "connector lacks a recognized data type",
        )],
    ),
    (
        "missing_input_member.jsonld",
        None,
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.and3"),
            "block interface mismatch for `CDL.Logical.And`: declared 1 input(s)/1 output(s), \
             class requires 2/1",
        )],
    ),
    (
        "inactive_member_hasinstance.jsonld",
        None,
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.gain"),
            "block interface mismatch for `CDL.Reals.MultiplyByParameter`: declared 0 \
             input(s)/1 output(s), class requires 1/1",
        )],
    ),
    (
        "inactive_member_hasinput.jsonld",
        None,
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.gain"),
            "block interface mismatch for `CDL.Reals.MultiplyByParameter`: declared 0 \
             input(s)/1 output(s), class requires 1/1",
        )],
    ),
    (
        // The end-to-end signature is errors-only by the driver's contract; the complete
        // vector including the exactly-one `conflicting-interface-declaration` Warning is
        // pinned at the resolver layer (`composite_contract_member_pins`).
        "partial_port_declaration.jsonld",
        None,
        &[
            (
                DiagCode::MalformedDocument,
                Some("http://example.org#M.c"),
                "block interface mismatch for `CDL.Logical.Not`: declared 0 input(s)/0 \
                 output(s), class requires 1/1",
            ),
            (
                DiagCode::UnresolvedReference,
                Some("http://example.org#M.c.y"),
                "instance port not found",
            ),
        ],
    ),
    (
        "conflicting_parameter_values.jsonld",
        None,
        &[(
            DiagCode::ConflictingInterfaceDeclaration,
            Some("http://example.org#M.del"),
            "parameter `y_start` is declared with different values by hasParameter and \
             hasInstance",
        )],
    ),
];

/// The slice's warned fixtures: the exact ordered warning triples a successful
/// `Engine::load_cxf` must surface.
pub(crate) const MEMBER_WARNINGS: [CompositeWarning; 2] = [
    (
        "analog_coerced_member.jsonld",
        &[(
            DiagCode::AnalogCoercedToReal,
            Some("http://example.org#cc.analog_coerced_member.gain.u"),
            "Analog connector coerced to Real",
        )],
    ),
    (
        "conflicting_interface_name.jsonld",
        &[(
            DiagCode::ConflictingInterfaceDeclaration,
            Some("http://example.org#cc.conflicting_interface_name.del"),
            "hasInstance carries class-declared name(s) `y_start` declared on none of the \
             node's own hasInput/hasOutput/hasParameter/hasConstant routes",
        )],
    ),
];
