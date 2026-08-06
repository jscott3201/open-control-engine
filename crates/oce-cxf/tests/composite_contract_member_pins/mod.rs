//! Pin-table data for the `hasInstance` member-interface slice of the contract corpus
//! (`_spec/19`), consumed by `composite_contract_corpus.rs`'s assembled tables — the guards
//! there always compare the one assembled table against the disk, never a partial view.

use oce_diag::{DiagCode, Diagnostic};

fn error_with_subject(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(code, message).with_subject(subject.to_owned())
}

fn warning_with_subject(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::warning(code, message).with_subject(subject.to_owned())
}

/// The slice's accepted fixtures paired with their goldens: the mixed member interface, the
/// padded-output tie exemplar, the permutation control (same `@id`s as the mixed fixture,
/// arrays reordered — its golden is byte-identical by construction), the refusal controls,
/// and the thirteen inert-discard documents whose clean loads pin that a `hasInstance` list
/// outside the three ruled domains is read by nothing.
pub(crate) fn accepted() -> Vec<(String, String)> {
    [
        "agreeing_parameter_values",
        "carveout_member_array_unreferenced",
        "inert_bothcarrying_array_member",
        "inert_bothcarrying_depth2_member",
        "inert_bothcarrying_instance_member",
        "inert_nested_composite_array_member",
        "inert_nested_composite_depth2_member",
        "inert_nested_composite_instance_member",
        "inert_orphan_array_member",
        "inert_pruned_bothcarrying_list",
        "inert_pruned_shape_self_member",
        "inert_root_conditional_member",
        "inert_root_dangling_member",
        "inert_root_empty_list",
        "member_array_permutation",
        "mixed_member_interface",
        "padded_output_tie",
        "two_level_nesting_inner_list",
        "unclassifiable_member_control",
    ]
    .into_iter()
    .map(|stem| {
        (
            format!("accepted/{stem}.jsonld"),
            format!("tests/fixtures/golden/composite_contract_{stem}.modelgraph.txt"),
        )
    })
    .collect()
}

/// The slice's rejected fixtures with their complete pinned diagnostic vectors. Every fixture
/// carries exactly one offending instance, member, or collision — the count-of-one holds only
/// because the derivation skip REPLACES the arity diagnostic — except the
/// partial-port-declaration document, whose complete three-entry vector (arity, dangling port,
/// exactly one Warning naming the input the list declares and the node's own routes do not) is
/// the all-or-nothing fallback's pin. The two dialect-agreement pairs (carve-out array member,
/// inactive classified member) pin byte-equal vectors across spellings.
pub(crate) fn rejections() -> Vec<(&'static str, Vec<Diagnostic>)> {
    vec![
        (
            "rejected/vector_port_instance.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.rep",
                "composite/vector-port-instance: instance of class \
                 `CDL.Routing.RealScalarReplicator` derives its port count from a parameter; \
                 this subset derives scalar interfaces only",
            )],
        ),
        (
            "rejected/unsupported_instance_member.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.ramp.zzz",
                "composite/unsupported-instance-member: `zzz` is neither a declared port nor a \
                 declared parameter of `CDL.Reals.Sources.Ramp`",
            )],
        ),
        (
            "rejected/member_outside_owner.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.other.q",
                "composite/unsupported-instance-member: member is not a direct member of \
                 `http://example.org#M.c`",
            )],
        ),
        (
            "rejected/member_is_instance.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.a.sub",
                "composite/unsupported-instance-member: member `http://example.org#M.a.sub` is \
                 itself a block instance",
            )],
        ),
        (
            // The skipped instance also carries a node-bearing port member and the document
            // later connectors, so keeping withdrawn members in block 1 (the R19-10 mutation)
            // both renumbers them and fires "connector owned by no instance" — either reds
            // this count-of-one pin.
            "rejected/minted_identity_shadows_node.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.del.y",
                "composite/colliding-member-identity: synthesized connector identity \
                 `http://example.org#M.del.y` is already an `@graph` node",
            )],
        ),
        (
            "rejected/minted_identity_twice.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c.u",
                "composite/colliding-member-identity: connector identity \
                 `http://example.org#M.c.u` is minted twice by `http://example.org#M.c`",
            )],
        ),
        (
            "rejected/param_member_duplicates_hasparameter.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.del.samplePeriod",
                "composite/colliding-member-identity: parameter `samplePeriod` is declared by \
                 both `hasParameter` and `hasInstance` on `http://example.org#M.del`",
            )],
        ),
        (
            "rejected/param_member_declared_twice.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.del.samplePeriod",
                "composite/colliding-member-identity: parameter `samplePeriod` is declared \
                 twice by `http://example.org#M.del`",
            )],
        ),
        (
            "rejected/array_marked_member.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c.u",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/array_marked_nested_composite_input.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.sub.arr",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/array_marked_orphan_input.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.orphan.arr",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/carveout_member_array_hasinstance.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.con.protected.arr",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/carveout_member_array_hasinput.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.con.protected.arr",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/nodeless_parameter_member.jsonld",
            vec![error_with_subject(
                DiagCode::GroundingFailed,
                "http://example.org#M.del.samplePeriod",
                "parameter has no value (Ground mode)",
            )],
        ),
        (
            "rejected/member_isofdatatype_unresolved.jsonld",
            vec![error_with_subject(
                DiagCode::UnresolvedReference,
                "http://data.ashrae.org/S231P#Bogus",
                "unresolved isOfDataType",
            )],
        ),
        (
            "rejected/member_string_type.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.c.u",
                "String connector not permitted (§7.8)",
            )],
        ),
        (
            "rejected/member_unrecognized_type.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.c.u",
                "connector lacks a recognized data type",
            )],
        ),
        (
            "rejected/missing_input_member.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.and3",
                "block interface mismatch for `CDL.Logical.And`: declared 1 input(s)/1 \
                 output(s), class requires 2/1",
            )],
        ),
        (
            "rejected/inactive_member_hasinstance.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.gain",
                "block interface mismatch for `CDL.Reals.MultiplyByParameter`: declared 0 \
                 input(s)/1 output(s), class requires 1/1",
            )],
        ),
        (
            "rejected/inactive_member_hasinput.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.gain",
                "block interface mismatch for `CDL.Reals.MultiplyByParameter`: declared 0 \
                 input(s)/1 output(s), class requires 1/1",
            )],
        ),
        (
            "rejected/partial_port_declaration.jsonld",
            vec![
                warning_with_subject(
                    DiagCode::ConflictingInterfaceDeclaration,
                    "http://example.org#M.c",
                    "hasInstance carries class-declared name(s) `u` declared on none of the \
                     node's own hasInput/hasOutput/hasParameter/hasConstant routes",
                ),
                error_with_subject(
                    DiagCode::MalformedDocument,
                    "http://example.org#M.c",
                    "block interface mismatch for `CDL.Logical.Not`: declared 0 input(s)/0 \
                     output(s), class requires 1/1",
                ),
                error_with_subject(
                    DiagCode::UnresolvedReference,
                    "http://example.org#M.c.y",
                    "instance port not found",
                ),
            ],
        ),
        (
            "rejected/conflicting_parameter_values.jsonld",
            vec![error_with_subject(
                DiagCode::ConflictingInterfaceDeclaration,
                "http://example.org#M.del",
                "parameter `y_start` is declared with different values by hasParameter and \
                 hasInstance",
            )],
        ),
    ]
}

/// The slice's warned fixtures with their complete pinned warning vectors.
pub(crate) fn warnings() -> Vec<(&'static str, Vec<Diagnostic>)> {
    vec![
        (
            "warned/analog_coerced_member.jsonld",
            vec![warning_with_subject(
                DiagCode::AnalogCoercedToReal,
                "http://example.org#cc.analog_coerced_member.gain.u",
                "Analog connector coerced to Real",
            )],
        ),
        (
            "warned/conflicting_interface_name.jsonld",
            vec![warning_with_subject(
                DiagCode::ConflictingInterfaceDeclaration,
                "http://example.org#cc.conflicting_interface_name.del",
                "hasInstance carries class-declared name(s) `y_start` declared on none of the \
                 node's own hasInput/hasOutput/hasParameter/hasConstant routes",
            )],
        ),
    ]
}
