//! The bytes the export writes one node at a time are those of the whole document built first
//! and serialized with [`crate::write_document`], the way the exporter wrote them before (issue
//! #803): for every fixture the exporter accepts and for hand-built graphs whose identities and
//! values need escaping.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use oce_diag::{DiagCode, Severity};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, EnumClassId, ModelGraph, ParamTable,
    Value, ValueType,
};

use super::super::{S231_CONTEXT_IRI, param_binding, plan};
use super::{ByteCount, Plan, emit, write};
use crate::dto::{Context, CxfDocument, IriRef, Node, OneOrMany};
use crate::export::EXPORT_ROOT_IRI;
use crate::export_attrs::emit_port_attrs;
use crate::{ResolveOptions, import_cxf, write_document};

/// A [`Node`] with only its `@id` set — every other field absent/empty.
fn blank_node(id: String) -> Node {
    Node {
        id,
        r#type: None,
        has_input: OneOrMany::None,
        has_output: OneOrMany::None,
        has_parameter: OneOrMany::None,
        has_constant: OneOrMany::None,
        contains_block: OneOrMany::None,
        has_instance: OneOrMany::None,
        is_connected_to: OneOrMany::None,
        is_of_data_type: None,
        label: None,
        access: None,
        value: None,
        min: None,
        max: None,
        nominal: None,
        unit: None,
        quantity: None,
        display_unit: None,
        is_final: None,
        is_array: None,
        n_dims: None,
        size_dims: None,
        fmu_path: None,
        is_conditional: None,
        cond_expr: None,
        is_replaceable: None,
        other: BTreeMap::new(),
    }
}

/// A bare `{"@id": …}` cross-reference.
fn iri_ref(id: String) -> IriRef {
    IriRef {
        id,
        other: BTreeMap::new(),
    }
}

/// `@id`s as a [`OneOrMany`] edge: absent for zero, a bare object for one, an array otherwise.
fn refs(ids: Vec<String>) -> OneOrMany<IriRef> {
    let mut ids = ids;
    match ids.len() {
        0 => OneOrMany::None,
        1 => OneOrMany::One(iri_ref(ids.remove(0))),
        _ => OneOrMany::Many(ids.into_iter().map(iri_ref).collect()),
    }
}

/// Each of `ids` spelled as a `String`.
fn spelled<T: ToString>(ids: &[T]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

/// The document for `plan` built in full, every node at once, as the exporter built it before
/// it wrote one node at a time: [`Node`]s, `emit_port_attrs`, `param_binding` and
/// `crate::translation_software::stamp`.
fn document_built_in_full(plan: &Plan<'_>) -> CxfDocument {
    let mut graph = Vec::new();
    let mut root = blank_node(EXPORT_ROOT_IRI.to_owned());
    root.r#type = Some(OneOrMany::One("S231:Block".to_owned()));
    root.has_input = refs(plan.boundaries.iter().map(|b| b.iri.to_owned()).collect());
    root.has_output = refs(
        plan.boundary_outputs
            .iter()
            .map(|o| o.iri.to_owned())
            .collect(),
    );
    root.contains_block = refs(plan.blocks.iter().map(|b| b.iri.to_string()).collect());
    crate::translation_software::stamp(&mut root);
    graph.push(root);
    for b in &plan.blocks {
        let ids: Vec<String> = b
            .params
            .iter()
            .map(|(name, _)| format!("{}.{name}", b.iri))
            .collect();
        let mut node = blank_node(b.iri.to_string());
        node.r#type = Some(OneOrMany::One(b.type_iri.clone()));
        node.has_input = refs(spelled(&b.input_ports));
        node.has_output = refs(spelled(&b.output_ports));
        node.has_parameter = refs(ids.clone());
        graph.push(node);
        for (id, (_, value)) in ids.into_iter().zip(b.params) {
            let mut pnode = blank_node(id);
            pnode.value = Some(param_binding(value).expect("a planned binding"));
            graph.push(pnode);
        }
    }
    for port in &plan.ports {
        let mut node = blank_node(port.iri.to_string());
        node.is_of_data_type = Some(iri_ref(port.datatype.to_owned()));
        node.is_connected_to = refs(spelled(&port.targets));
        emit_port_attrs(&mut node, &port.attrs);
        graph.push(node);
    }
    for boundary in &plan.boundaries {
        let mut node = blank_node(boundary.iri.to_owned());
        node.is_of_data_type = Some(iri_ref(boundary.datatype.to_owned()));
        node.is_connected_to = refs(spelled(&boundary.targets));
        emit_port_attrs(&mut node, &boundary.attrs);
        graph.push(node);
    }
    for output in &plan.boundary_outputs {
        let mut node = blank_node(output.iri.to_owned());
        node.is_of_data_type = Some(iri_ref(output.datatype.to_owned()));
        emit_port_attrs(&mut node, &output.attrs);
        graph.push(node);
    }
    let mut context = BTreeMap::new();
    context.insert(
        "S231".to_owned(),
        serde_json::Value::String(S231_CONTEXT_IRI.to_owned()),
    );
    CxfDocument {
        context: Context::Map(context),
        graph,
        other: BTreeMap::new(),
    }
}

/// Assert that the export of `g` writes the bytes of its document built in full, that the
/// counting pass counts exactly those bytes, and return how many parameters it wrote.
pub(super) fn assert_written_as_built_in_full(g: &ModelGraph, what: &str) -> usize {
    let (plan, _) = plan(g).unwrap_or_else(|d| panic!("{what}: the plan is refused: {d:?}"));
    let written = write(&plan).expect("the plan writes");
    let built = write_document(&document_built_in_full(&plan)).expect("the document serializes");
    assert!(written == built, "{what}: the bytes differ");
    // Sized by the counting pass: a count short of the bytes would have grown the buffer, and one
    // past them would leave capacity unused.
    assert_eq!(written.capacity(), written.len(), "{what}");
    let mut counted = ByteCount(0);
    emit(&plan, &mut counted).expect("the plan counts");
    assert_eq!(counted.0, written.len(), "{what}");
    plan.blocks.iter().map(|b| b.params.len()).sum()
}

/// Every `.jsonld` file under `dir`, in name order.
fn documents(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("the fixtures directory reads")
        .map(|e| e.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            documents(&path, found);
        } else if path.extension().is_some_and(|e| e == "jsonld") {
            found.push(path);
        }
    }
}

#[test]
fn every_fixture_the_exporter_accepts_is_written_as_built_in_full() {
    let mut found = Vec::new();
    documents(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        &mut found,
    );
    let (mut exported, mut params) = (0, 0);
    for path in &found {
        let bytes = std::fs::read(path).expect("the fixture reads");
        let Ok((g, _)) = import_cxf(&bytes, &ResolveOptions::default()) else {
            continue;
        };
        if plan(&g).is_err() {
            continue;
        }
        params += assert_written_as_built_in_full(&g, &path.display().to_string());
        exported += 1;
    }
    // A floor, so a fixture tree that stops reading cannot pass by comparing nothing.
    assert!(exported >= 40, "{exported} fixtures exported");
    assert!(params >= 100, "{params} parameters written");
}

/// One `Constant` block named `iri` with the parameters `params` and one Real output.
fn constant(id: u32, iri: &str, params: Params) -> (BlockInstance, Connector) {
    (
        BlockInstance {
            id: BlockId(id),
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: vec![ConnectorId(id)],
            params: ParamTable { values: params },
            decl_order: id,
            instance_iri: Some(Arc::from(iri)),
        },
        Connector::new(ConnectorId(id), BlockId(id), Dir::Out, ValueType::Real, 0),
    )
}

/// A parameter table's entries.
type Params = Vec<(Arc<str>, Value)>;

/// A graph of `blocks`, each a `Constant` built by [`constant`].
fn constants(blocks: Vec<(&str, Params)>) -> ModelGraph {
    let mut g = ModelGraph::new();
    for (i, (iri, params)) in blocks.into_iter().enumerate() {
        let (block, connector) = constant(i as u32, iri, params);
        g.blocks.push(block);
        g.connectors.push(connector);
    }
    g
}

/// The parameter `name` bound to `value`.
pub(super) fn param(name: &str, value: Value) -> (Arc<str>, Value) {
    (Arc::from(name), value)
}

#[test]
fn identities_and_values_that_need_escaping_are_written_as_built_in_full() {
    let awkward = "urn:\"q\"\\b\u{1}\u{1f}\u{7f}é\u{2028}\u{1F600}/";
    let values = vec![
        param("r0", Value::Real(0.0)),
        param("r1", Value::Real(-0.0)),
        param("r2", Value::Real(1.0)),
        param("r3", Value::Real(0.1)),
        param("r4", Value::Real(1e300)),
        param("r5", Value::Real(-2.5e-308)),
        param("r6", Value::Real(f64::MIN_POSITIVE / 4.0)),
        param("r7", Value::Real(f64::MAX)),
        param("i0", Value::Integer(i64::MIN)),
        param("i1", Value::Integer(i64::MAX)),
        param("i2", Value::Integer(0)),
        param("b0", Value::Boolean(true)),
        param("b1", Value::Boolean(false)),
        param("s0", Value::String(Arc::from(""))),
        param("s1", Value::String(Arc::from("a \"quoted\" \\ text"))),
        param(
            "s2",
            Value::String(Arc::from("\u{0}\n\t\u{1b}\u{2029}é\u{1F600}")),
        ),
        param("n\"\\\u{2}é", Value::Integer(3)),
        // Names the plan admits (it refuses only an empty name and one with a `.`) whose JSON
        // spelling escapes or keeps a character a lossy writer could change.
        param("n\u{2028}", Value::Integer(4)),
        param("n\u{2029}", Value::Integer(5)),
        param("n\u{7f}", Value::Integer(6)),
        param("n\u{1}\u{1f}", Value::Integer(7)),
        param("n\u{1F600}", Value::Integer(8)),
        param("n\u{10FFFF}", Value::Integer(9)),
    ];
    let shapes = [
        (
            "one block with every kind of value",
            constants(vec![(awkward, values.clone())]),
        ),
        (
            "a block with no parameter",
            constants(vec![("urn:none", vec![])]),
        ),
        (
            "a block with one parameter",
            constants(vec![("urn:one", vec![param("k", Value::Real(2.0))])]),
        ),
        (
            "blocks with none, one and many",
            constants(vec![
                ("urn:a", vec![]),
                ("urn:b", vec![param("k", Value::Real(2.0))]),
                (awkward, values),
                ("urn:d", vec![]),
            ]),
        ),
    ];
    for (what, g) in &shapes {
        assert_written_as_built_in_full(g, what);
    }
}

#[test]
fn arrays_of_every_rank_and_long_identities_are_written_as_built_in_full() {
    // A block's array elements are scalar parameters named `k_1` … or `k_1_1` …: rank 1, 2 and
    // 3, an empty table, a string array, and a 4 KiB instance IRI.
    let long = format!("urn:{}", "x".repeat(4092));
    let mut rank1 = Vec::new();
    let mut rank2 = Vec::new();
    let mut rank3 = Vec::new();
    let mut strings = Vec::new();
    for i in 1..=10 {
        rank1.push(param(&format!("k_{i}"), Value::Real(f64::from(i) / 3.0)));
        strings.push(param(&format!("s_{i}"), Value::String(Arc::from("\"x\""))));
        for j in 1..=3 {
            rank2.push(param(
                &format!("K_{i}_{j}"),
                Value::Integer(i64::from(i * j)),
            ));
            rank3.push(param(&format!("T_{i}_{j}_2"), Value::Boolean(i % 2 == 0)));
        }
    }
    let g = constants(vec![
        ("urn:r1", rank1),
        (&long, rank2),
        ("urn:empty", vec![]),
        ("urn:r3", rank3),
        ("urn:s", strings),
    ]);
    let params = assert_written_as_built_in_full(&g, "arrays");
    assert_eq!(params, 10 + 30 + 30 + 10);
}

#[test]
fn the_document_has_only_its_context_and_graph() {
    let g = constants(vec![("urn:one", vec![param("k", Value::Real(2.0))])]);
    let (plan, _) = plan(&g).expect("plans");
    let written = write(&plan).expect("writes");
    let parsed: serde_json::Value = serde_json::from_slice(&written).expect("JSON");
    let keys: Vec<&String> = parsed.as_object().expect("an object").keys().collect();
    assert_eq!(keys, ["@context", "@graph"]);
    assert!(matches!(
        serde_json::from_slice::<CxfDocument>(&written)
            .expect("a document")
            .context,
        Context::Map(_)
    ));
}

#[test]
fn a_partial_export_beside_deferred_blocks_is_written_as_built_in_full() {
    // Blocks with an enumeration parameter are deferred with a warning each and left out; the
    // rest is written. 16 deferred blocks under 4 KiB IRIs: their warnings fit the list.
    let enumeration = || {
        vec![param(
            "e",
            Value::Enum {
                class: EnumClassId(0),
                ordinal: 1,
            },
        )]
    };
    let long: Vec<String> = (0..16)
        .map(|i| format!("urn:d{i:02}{}", "x".repeat(4_090)))
        .collect();
    let mut blocks = vec![("urn:kept", vec![param("k_1", Value::Real(1.0))])];
    blocks.extend(long.iter().map(|iri| (iri.as_str(), enumeration())));
    blocks.push((
        "urn:kept2",
        vec![param("s", Value::String(Arc::from("\"")))],
    ));
    let g = constants(blocks);
    let (_, warnings) = plan(&g).expect("a partial export");
    assert_eq!(warnings.len(), 16);
    assert!(
        warnings
            .iter()
            .all(|w| w.code == DiagCode::ExportDeferred && w.severity == Severity::Warning)
    );
    assert_eq!(assert_written_as_built_in_full(&g, "partial"), 2);
}

#[test]
fn an_export_whose_list_ends_partway_writes_nothing() {
    // 128 deferred blocks under 64 KiB IRIs: their warnings pass the list's 8 MiB, the list ends
    // partway with its closing error, and the export is refused before a byte is written.
    let iri = format!("urn:d{}", "x".repeat(64 * 1024));
    let enumeration = vec![param(
        "e",
        Value::Enum {
            class: EnumClassId(0),
            ordinal: 1,
        },
    )];
    let mut blocks = vec![("urn:kept", vec![param("k_1", Value::Real(1.0))])];
    blocks.extend((0..128).map(|_| (iri.as_str(), enumeration.clone())));
    let g = constants(blocks);
    let Err(diagnostics) = plan(&g) else {
        panic!("the export is refused");
    };
    let closing = diagnostics.last().expect("a list");
    assert_eq!(closing.code, DiagCode::ResourceLimitExceeded);
    assert!(diagnostics.len() < 129);
}

/// The duplicate-`@id` refusal naming `subject`.
fn duplicate(subject: &str) -> oce_diag::Diagnostic {
    oce_diag::Diagnostic::error(
        DiagCode::ExportUnsupported,
        "export subset: emitted node @id collides with another emitted node @id",
    )
    .with_subject(subject.to_owned())
}

#[test]
fn a_parameter_id_held_in_parts_collides_with_the_same_id_spelled_whole_in_either_order() {
    // `urn:x` with the parameter `k` spells `urn:x.k`, the IRI of another block: the later of the
    // two is refused, whichever comes first.
    let k = || vec![param("k", Value::Real(1.0))];
    let member_first = constants(vec![("urn:x", k()), ("urn:x.k", vec![])]);
    assert_eq!(plan(&member_first).err(), Some(vec![duplicate("urn:x.k")]));
    let whole_first = constants(vec![("urn:x.k", vec![]), ("urn:x", k())]);
    assert_eq!(plan(&whole_first).err(), Some(vec![duplicate("urn:x")]));
    // Two blocks of one IRI: the second block's own `@id`, its parameter's and its minted port's
    // (`urn:x.out0`) all collide.
    let twice = constants(vec![("urn:x", k()), ("urn:x", k())]);
    assert_eq!(plan(&twice).err(), Some(vec![duplicate("urn:x"); 3]));
    // A parameter named like the block's minted output port: one `@id`, refused once.
    let port = constants(vec![("urn:x", vec![param("out0", Value::Real(1.0))])]);
    assert_eq!(plan(&port).err(), Some(vec![duplicate("urn:x")]));
}

#[test]
fn reals_of_every_written_form_are_counted_as_written() {
    // The counting pass and the writing pass spell each Real the same way, so the buffer is the
    // bytes' size: whole numbers with a `.0`, exponents both ways, subnormals, the extremes and
    // both zeros, as bindings and as connector bounds and nominals.
    let reals = [
        0.0,
        -0.0,
        1.0,
        -2.0,
        0.1,
        1.0 / 3.0,
        1e15,
        1e16,
        1e17,
        1e21,
        -7.25e22,
        1e-6,
        1e-7,
        1e-300,
        5e-324,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        123_456_789.012_345_68,
    ];
    let params = reals
        .iter()
        .enumerate()
        .map(|(i, r)| param(&format!("r_{i}"), Value::Real(*r)))
        .collect();
    let mut g = constants(vec![("urn:reals", params)]);
    g.connectors[0].attrs = oce_model::Attrs::Real(oce_model::RealAttrs {
        min: Some(1e-300),
        max: Some(-7.25e22),
        nominal: Some(5e-324),
        unit: Some(Arc::from("K")),
        ..Default::default()
    });
    assert_eq!(assert_written_as_built_in_full(&g, "reals"), reals.len());
}

#[test]
fn minted_port_ids_of_two_digit_positions_are_spelled_in_full() {
    // One block, `urn:m`, with 11 inputs and 12 outputs, none with an IRI of its own, its output
    // at position 11 wired to its input at position 10: every port's `@id` is minted, and the
    // bytes are stated by hand, not spelled by the writer's own id type.
    let mut g = constants(vec![("urn:m", vec![])]);
    g.connectors.clear();
    g.blocks[0].outputs.clear();
    for (dir, count) in [(Dir::In, 11), (Dir::Out, 12)] {
        for k in 0..count {
            let cid = ConnectorId(g.connectors.len() as u32);
            g.connectors
                .push(Connector::new(cid, BlockId(0), dir, ValueType::Real, k));
            match dir {
                Dir::In => g.blocks[0].inputs.push(cid),
                Dir::Out => g.blocks[0].outputs.push(cid),
            }
        }
    }
    g.connections.push(oce_model::Connection {
        from: ConnectorId(11 + 11),
        to: ConnectorId(10),
    });
    let (plan, _) = plan(&g).expect("plans");
    let written = String::from_utf8(write(&plan).expect("writes")).expect("UTF-8");
    let ids = |prefix: &str, count: usize| {
        (0..count)
            .map(|k| format!("{{\"@id\":\"urn:m.{prefix}{k}\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let block = format!(
        "{{\"@id\":\"urn:m\",\"@type\":\"http://example.org#Buildings.Controls.OBC.CDL.Reals.\
         Sources.Constant\",\"S231:hasInput\":[{}],\"S231:hasOutput\":[{}]}}",
        ids("in", 11),
        ids("out", 12)
    );
    for expected in [
        block.as_str(),
        ",{\"@id\":\"urn:m.in10\",\"S231:isOfDataType\":{\"@id\":\"S231:Real\"}},",
        ",{\"@id\":\"urn:m.out11\",\"S231:isConnectedTo\":{\"@id\":\"urn:m.in10\"},\
         \"S231:isOfDataType\":{\"@id\":\"S231:Real\"}}]}",
    ] {
        assert!(written.contains(expected), "{expected}\nnot in\n{written}");
    }
    assert!(block.contains("{\"@id\":\"urn:m.in10\"}]"));
    assert!(block.contains("{\"@id\":\"urn:m.out11\"}]"));
}
