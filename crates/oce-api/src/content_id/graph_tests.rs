//! Unit tests for naming, ordering and hand-built-model refusals of the canonical graph.

use std::sync::Arc;

use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, ValueType,
};

use super::*;
use crate::Engine;

#[test]
fn the_namespace_ends_at_the_last_hash_slash_or_colon() {
    for (iri, name) in [
        ("http://example.org#g36.vav.TZon", Some("g36.vav.TZon")),
        ("urn:oce:author:g36.vav.TZon", Some("g36.vav.TZon")),
        ("http://example.org/lib/g36.a", Some("g36.a")),
        ("no-delimiter.at.all", Some("no-delimiter.at.all")),
        ("http://example.org#", None),
        ("urn:x:", None),
        ("", None),
    ] {
        assert_eq!(relative_name(iri), name, "{iri}");
    }
}

#[test]
fn colliding_and_empty_remainders_keep_the_whole_iri_for_every_member() {
    let iris = [
        "http://a.example#seq.x",
        "urn:b:seq.x",
        "http://a.example#seq.y",
        "http://a.example/",
    ];
    let names = name_all(iris.iter().copied());
    let form = |iri: &str| (names[iri].form, names[iri].text.clone());
    assert_eq!(form(iris[0]), (1, iris[0].to_owned()));
    assert_eq!(form(iris[1]), (1, iris[1].to_owned()));
    assert_eq!(form(iris[2]), (0, "seq.y".to_owned()));
    assert_eq!(form(iris[3]), (1, iris[3].to_owned()));
}

#[test]
fn names_order_by_form_then_bytes() {
    let relative = Name {
        form: 0,
        text: "z".to_owned(),
    };
    let absolute = Name {
        form: 1,
        text: "a".to_owned(),
    };
    assert!(relative < absolute);
    let upper = Name {
        form: 0,
        text: "delayOnInit".to_owned(),
    };
    let lower = Name {
        form: 0,
        text: "delayTime".to_owned(),
    };
    assert!(upper < lower, "`O` (0x4f) precedes `T` (0x54)");
}

fn block(id: u32, class: &str, iri: Option<&str>) -> BlockInstance {
    BlockInstance {
        id: BlockId(id),
        class_iri: Arc::from(class),
        inputs: Vec::new(),
        outputs: Vec::new(),
        params: ParamTable::default(),
        decl_order: id,
        instance_iri: iri.map(Arc::from),
    }
}

fn graph_of(model: &ModelGraph) -> Result<usize, ContentIdError> {
    let io = IoInventory::build_at_load(model);
    canonical_graph(model, &io).map(|graph| graph.blocks.len())
}

#[test]
fn an_unnamed_non_reserved_block_is_a_typed_refusal_not_a_panic() {
    let mut model = ModelGraph::new();
    model
        .blocks
        .push(block(0, "CDL.Reals.Sources.Constant", Some("urn:t:a")));
    model
        .blocks
        .push(block(1, "CDL.Reals.Sources.Constant", None));
    assert_eq!(
        graph_of(&model),
        Err(ContentIdError::UnnamedBlock { block: 1 })
    );
}

#[test]
fn an_unnamed_reserved_block_is_not_a_block_of_the_id() {
    let mut model = ModelGraph::new();
    model
        .blocks
        .push(block(0, "CDL.Reals.Sources.Constant", Some("urn:t:a")));
    model
        .blocks
        .push(block(1, "urn:oce:lowering#PassThrough.Real", None));
    assert_eq!(graph_of(&model), Ok(1));
}

#[test]
fn an_unregistered_class_and_a_dangling_connection_are_typed_refusals() {
    let mut model = ModelGraph::new();
    model
        .blocks
        .push(block(0, "CDL.Not.A.Class", Some("urn:t:a")));
    assert_eq!(
        graph_of(&model),
        Err(ContentIdError::UnregisteredClass {
            class_path: "CDL.Not.A.Class".into()
        })
    );
    let mut model = ModelGraph::new();
    model
        .blocks
        .push(block(0, "CDL.Reals.Sources.Constant", Some("urn:t:a")));
    model.connections.push(oce_model::Connection {
        from: ConnectorId(7),
        to: ConnectorId(8),
    });
    assert_eq!(graph_of(&model), Err(ContentIdError::MalformedModel));
    // A connector that names a port its block does not list.
    let mut model = ModelGraph::new();
    model
        .blocks
        .push(block(0, "CDL.Reals.Sources.Constant", Some("urn:t:a")));
    model
        .blocks
        .push(block(1, "CDL.Reals.Abs", Some("urn:t:b")));
    model.connectors.push(Connector::new(
        ConnectorId(0),
        BlockId(0),
        Dir::Out,
        ValueType::Real,
        0,
    ));
    model.connectors.push(Connector::new(
        ConnectorId(1),
        BlockId(1),
        Dir::In,
        ValueType::Real,
        1,
    ));
    model.connections.push(oce_model::Connection {
        from: ConnectorId(0),
        to: ConnectorId(1),
    });
    assert_eq!(graph_of(&model), Err(ContentIdError::MalformedModel));
}

#[test]
fn the_engine_surfaces_the_refusal_as_a_typed_error() {
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!(
            "../../tests/fixtures/content_id/offset_gain.jsonld"
        ))
        .unwrap();
    std::sync::Arc::make_mut(&mut engine.model).blocks[0].instance_iri = None;
    assert!(matches!(
        engine.content_id(),
        Err(crate::OcError::ContentId(
            ContentIdError::UnnamedBlock { .. }
        ))
    ));
    // The blob then records no id rather than refusing to compile.
    assert_eq!(engine.compile_blob().unwrap().content_id, None);
}

#[test]
fn the_encoder_allocates_its_bytes_once_at_their_exact_length() {
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!(
            "../../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld"
        ))
        .unwrap();
    let graph = canonical_graph(&engine.model, &engine.io).unwrap();
    let mut bytes = Vec::new();
    let census = allocation_counter::measure(|| {
        bytes = super::super::encode::encode(&graph).unwrap();
    });
    assert_eq!(
        census.count_total, 1,
        "one allocation for the whole encoding"
    );
    assert_eq!(
        bytes.len(),
        bytes.capacity(),
        "sized exactly by the counting pass"
    );
    assert_eq!(census.bytes_total, bytes.len() as u64);
}
