//! Identity at the CXF level (issue #666): the root prefix, what is refused and why, the unnamed
//! block, and topologies no load writes.
//!
//! The adapter takes any `Topology`, and its fields are public: a test edits a loaded document's
//! topology to reach what no public load path produces (a block with no instance IRI, a path
//! naming no port, a block whose ports cannot be named). That is the only way to the unnamed
//! block, which the facade spells `b<id>` (issue #701, owner-accepted).

mod support;

use oce_api::{Engine, Topology};
use oce_diff::{
    DiffError, DiffOptions, Note, Side, diff_topology, diff_topology_with_ids, render_text,
};
use serde_json::json;

use support::loaded::{
    Loaded, compare_loaded, compare_topologies, emitted, emitted_under, load, loaded, refusal,
    reordered,
};

/// The worked example (c)'s reference: five blocks, two inputs, one output.
fn plant() -> oce_author::Sequence {
    support::example("plant_reset").0
}

fn json(base: &Topology, head: &Topology) -> serde_json::Value {
    support::json(&compare_topologies(base, head))
}

#[test]
fn two_instance_namespaces_with_one_graph_are_graph_equal() {
    let first = load(&emitted(&plant()));
    let second = load(&emitted_under(&plant(), "http://example.com/plants#"));
    assert_ne!(first.topology.model_id, second.topology.model_id);
    assert_eq!(first.id, second.id);
    let diff = compare_loaded(&first, &second);
    assert!(
        diff.summary.equal && diff.summary.graph_equal,
        "{}",
        diff.to_json()
    );
    assert_eq!(diff.summary.blocks.by_name, 5);
    assert_eq!(diff.base, diff.head);
}

#[test]
fn a_permuted_graph_diffs_empty_with_no_reordered_entry() {
    let bytes = emitted(&plant());
    let (original, permuted) = (load(&bytes), load(&reordered(&bytes)));
    let order = |loaded: &Loaded| -> Vec<String> {
        let blocks = loaded.topology.blocks.iter();
        blocks.map(|block| block.instance_path.clone()).collect()
    };
    assert_ne!(order(&original), order(&permuted), "the loader order moved");
    let diff = compare_loaded(&original, &permuted);
    assert!(diff.summary.equal, "{}", diff.to_json());
    assert!(!diff.to_json().contains("reordered"));
}

/// The emitted plant with the IRIs of `name` (a block and its ports, or a boundary) moved into
/// another namespace. It still loads: the loader does not check the convention.
fn moved(name: &str) -> Loaded {
    let text = String::from_utf8(emitted(&plant())).expect("UTF-8");
    let from = format!("urn:oce:author:chw_plant_reset.{name}");
    assert!(text.contains(&format!("\"{from}\"")));
    load(
        text.replace(&from, &format!("http://other.org#chw_plant_reset.{name}"))
            .as_bytes(),
    )
}

#[test]
fn a_block_outside_the_root_is_refused_on_the_side_it_is_on() {
    let good = load(&emitted(&plant()));
    let bad = moved("sel");
    let outside = |side| DiffError::OutsideRoot {
        side,
        root: "urn:oce:author:chw_plant_reset".to_owned(),
        path: "http://other.org#chw_plant_reset.sel".to_owned(),
    };
    assert_eq!(refusal(&bad.topology, &good.topology), outside(Side::Base));
    assert_eq!(refusal(&good.topology, &bad.topology), outside(Side::Head));
    // Base is checked first.
    assert_eq!(refusal(&bad.topology, &bad.topology), outside(Side::Base));
    let error = refusal(&good.topology, &bad.topology);
    assert_eq!(error.code(), "outside-root");
    assert_eq!(
        error.to_string(),
        "head: `http://other.org#chw_plant_reset.sel` is not named under the root `urn:oce:author:chw_plant_reset`"
    );
}

#[test]
fn a_boundary_outside_the_root_is_refused() {
    let good = load(&emitted(&plant()));
    for name in ["uReq", "TChiWatSupSet"] {
        let bad = moved(name);
        match refusal(&good.topology, &bad.topology) {
            DiffError::OutsideRoot { side, path, .. } => {
                assert_eq!(side, Side::Head);
                assert_eq!(path, format!("http://other.org#chw_plant_reset.{name}"));
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn an_exported_document_keeps_its_id_and_is_refused_as_outside_its_root() {
    let original = load(&emitted(&plant()));
    let export = original.engine.export_cxf().expect("the model exports");
    export.complete().expect("the export is complete");
    let exported = load(&export.bytes);
    assert_eq!(exported.id, original.id);
    assert_eq!(
        exported.topology.model_id.as_str(),
        "urn:open-control:cxf-export:root"
    );
    match refusal(&original.topology, &exported.topology) {
        DiffError::OutsideRoot { side, root, path } => {
            assert_eq!(side, Side::Head);
            assert_eq!(root, "urn:open-control:cxf-export:root");
            assert!(
                path.starts_with("urn:oce:author:chw_plant_reset."),
                "{path}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_name_is_what_follows_the_root_and_one_dot_and_is_never_empty() {
    let good = load(&emitted(&plant())).topology;
    let root = good.model_id.as_str().to_owned();
    for path in [
        root.clone(),
        format!("{root}."),
        format!("{root}x.sel"),
        format!("{root}/sel"),
        format!("x{root}.sel"),
    ] {
        let mut bad = good.clone();
        bad.blocks[4].instance_path.clone_from(&path);
        let error = refusal(&good, &bad);
        assert!(
            matches!(&error, DiffError::OutsideRoot { path: refused, .. } if *refused == path),
            "{path}: {error:?}"
        );
    }
    // A name may hold dots of its own: a flattened nested block is `<root>.<sub>.<block>`.
    let mut nested = good.clone();
    nested.blocks[4].instance_path = format!("{root}.sub.sel");
    let diff = json(&good, &nested);
    assert_eq!(diff["graph"][0]["subject"], json!("sub.sel"));
    assert_eq!(diff["graph"][0]["from"], json!("sel"));
}

/// The plant's topology with the block `sel` spelled as an unnamed block.
fn unnamed(spelling: &str) -> Topology {
    let mut topology = load(&emitted(&plant())).topology;
    let sel = topology
        .blocks
        .iter_mut()
        .find(|block| block.instance_path.ends_with(".sel"))
        .expect("the switch");
    sel.instance_path = spelling.to_owned();
    topology
}

#[test]
fn an_unnamed_block_is_matched_only_by_the_rename_pass() {
    let named = load(&emitted(&plant())).topology;
    let unnamed_head = unnamed("b4");
    let diff = json(&named, &unnamed_head);
    assert_eq!(
        diff["graph"],
        json!([{ "kind": "renamed", "subject": "b4", "from": "sel",
                 "class": "CDL.Reals.Switch", "params": [], "confidence": "exact" }])
    );
    assert_eq!(diff["notes"], json!(["unnamed-blocks"]));
    assert_eq!(diff["summary"]["graph_equal"], false);
    assert_eq!(diff["summary"]["blocks"]["by_name"], 4);
    // The same spelling on both sides is still no identity: loader order names no block.
    let both = json(&unnamed_head, &unnamed_head);
    assert_eq!(
        both["summary"]["blocks"],
        json!({ "by_name": 4, "renamed": 1, "base_only": 0, "head_only": 0 })
    );
    assert_eq!(both["notes"], json!(["unnamed-blocks"]));
    // On the base side too.
    let back = json(&unnamed_head, &named);
    assert_eq!(back["graph"][0]["subject"], json!("sel"));
    assert_eq!(back["graph"][0]["from"], json!("b4"));
    // With the pass off it is removed and added.
    let mut options = DiffOptions::default();
    options.renames = false;
    let off = diff_topology(&named, &unnamed_head, &options).expect("diffs");
    assert_eq!(off.notes, [Note::RenamePassOff, Note::UnnamedBlocks]);
    assert_eq!(
        (off.summary.blocks.base_only, off.summary.blocks.head_only),
        (1, 1)
    );
}

#[test]
fn an_unnamed_block_the_rename_pass_cannot_match_is_removed_and_added() {
    let named = load(&emitted(&plant())).topology;
    // `hold` has parameters: change one and the pass's gate refuses the pair.
    let mut head = named.clone();
    let hold = head
        .blocks
        .iter_mut()
        .find(|block| block.instance_path.ends_with(".hold"))
        .expect("the delay");
    hold.instance_path = "b1".to_owned();
    hold.params[0].1 = oce_api::Value::Real(301.0);
    let diff = json(&named, &head);
    let kinds: Vec<(String, String)> = support::kinds(&diff["graph"]);
    assert_eq!(
        kinds,
        [
            ("added".to_owned(), "b1".to_owned()),
            ("removed".to_owned(), "hold".to_owned()),
            ("rewired".to_owned(), "sel.u2".to_owned())
        ]
    );
    assert_eq!(diff["notes"], json!(["unnamed-blocks"]));
}

#[test]
fn only_b_and_decimal_digits_spell_an_unnamed_block() {
    let named = load(&emitted(&plant())).topology;
    for spelling in ["b0", "b12", "b4294967295"] {
        assert_eq!(
            json(&named, &unnamed(spelling))["notes"],
            json!(["unnamed-blocks"])
        );
    }
    for spelling in ["b", "B4", "c4", "b4x", "bx4", " b4", "b-4"] {
        let error = refusal(&named, &unnamed(spelling));
        assert_eq!(error.code(), "outside-root", "{spelling}");
    }
}

#[test]
fn a_block_whose_ports_cannot_be_named_is_refused() {
    let good = load(&emitted(&plant())).topology;
    type Break = fn(&mut oce_api::TopologyBlock);
    let breaks: [(&str, Break); 4] = [
        ("an unknown class", |block| {
            block.class_iri = "CDL.Reals.Nothing".to_owned()
        }),
        ("an input too many", |block| {
            block.inputs.push("urn:x".to_owned())
        }),
        ("an output too many", |block| {
            block.outputs.push("urn:x".to_owned())
        }),
        ("an input too few", |block| {
            block.inputs.pop();
        }),
    ];
    for (what, break_it) in breaks {
        let mut bad = good.clone();
        break_it(&mut bad.blocks[4]);
        for (side, base, head) in [(Side::Base, &bad, &good), (Side::Head, &good, &bad)] {
            assert_eq!(
                refusal(base, head),
                DiffError::UnresolvedPorts {
                    side,
                    block: "urn:oce:author:chw_plant_reset.sel".to_owned(),
                    class: bad.blocks[4].class_iri.clone(),
                },
                "{what}"
            );
        }
    }
    let mut bad = good.clone();
    bad.blocks[4].class_iri = "CDL.Reals.Nothing".to_owned();
    let error = refusal(&good, &bad);
    assert_eq!(error.code(), "unresolved-ports");
    assert_eq!(
        error.to_string(),
        "head: the ports of block `urn:oce:author:chw_plant_reset.sel` (CDL.Reals.Nothing) cannot be named"
    );
}

#[test]
fn the_refusal_codes_are_pinned() {
    assert_eq!(DiffError::CODES, ["outside-root", "unresolved-ports"]);
    let errors = [
        DiffError::OutsideRoot {
            side: Side::Base,
            root: String::new(),
            path: String::new(),
        },
        DiffError::UnresolvedPorts {
            side: Side::Base,
            block: String::new(),
            class: String::new(),
        },
    ];
    let codes: Vec<&str> = errors.iter().map(DiffError::code).collect();
    assert_eq!(codes, DiffError::CODES);
    assert!(errors[0].to_string().starts_with("base: "));
    let boxed: Box<dyn std::error::Error> = Box::new(errors[1].clone());
    assert!(boxed.to_string().starts_with("base: the ports"));
}

#[test]
fn a_topology_no_load_writes_is_diffed_without_a_panic() {
    let good = load(&emitted(&plant())).topology;
    // A connection from a path no port carries, and one into a path two ports carry.
    let mut strange = good.clone();
    strange.connections[0].from = "urn:nowhere".to_owned();
    let shared = strange.blocks[4].inputs[0].clone();
    strange.blocks[4].inputs[2].clone_from(&shared);
    let diff = json(&good, &strange);
    assert_eq!(diff["summary"]["graph_equal"], false);
    assert!(diff.to_string().contains("urn:nowhere"), "{diff}");
    // The connection into the shared path names no one port: it is kept as spelled, never given
    // to the first port that carries the path.
    let graph = diff["graph"].as_array().expect("a list");
    assert!(
        graph
            .iter()
            .any(|entry| entry["kind"] == "rewired" && entry["subject"] == json!(shared)),
        "{diff}"
    );
    // A declared output whose driver no port carries, and a pass-through of unknown paths.
    let mut outputs = good.clone();
    outputs.boundary_outputs[0].driver_path = "urn:nowhere".to_owned();
    let diff = json(&good, &outputs);
    assert_eq!(
        diff["graph"],
        json!([{ "kind": "rewired", "subject": "TChiWatSupSet", "old": "sel.y", "new": "urn:nowhere" }])
    );
    let pass = load(&support::loaded::corpus_bytes(
        "crates/oce-cxf/tests/fixtures/pass_through_miniature.jsonld",
    ))
    .topology;
    let mut unknown = pass.clone();
    unknown.pass_through[0].input = "urn:in".to_owned();
    unknown.pass_through[0].output = "urn:out".to_owned();
    let diff = json(&pass, &unknown);
    assert!(diff.to_string().contains("urn:in"), "{diff}");
    // The empty topology of an engine with nothing loaded.
    let empty = Engine::new().topology();
    let nothing = json(&empty, &empty);
    assert_eq!(nothing["summary"]["equal"], true);
    assert_eq!(nothing["base"]["sequence"], json!(""));
    let everything = json(&empty, &good);
    assert_eq!(everything["summary"]["blocks"]["head_only"], 5);
    assert_eq!(everything["head"]["sequence"], json!("chw_plant_reset"));
}

#[test]
fn the_order_of_a_topology_s_lists_changes_nothing() {
    let base = load(&emitted(&plant())).topology;
    let mut head = support::example("plant_reset").1;
    head.name = oce_author::Identifier::unchecked("chw_plant_reset");
    let head = loaded(&head).topology;
    let mut reversed = head.clone();
    reversed.blocks.reverse();
    reversed.connections.reverse();
    reversed.boundary_inputs.reverse();
    reversed.boundary_outputs.reverse();
    for block in &mut reversed.blocks {
        block.params.reverse();
    }
    let expected = compare_topologies(&base, &head);
    assert!(!expected.summary.equal);
    assert_eq!(
        compare_topologies(&base, &reversed).to_json(),
        expected.to_json()
    );
    assert_eq!(
        compare_topologies(&reversed, &base).to_json(),
        compare_topologies(&head, &base).to_json()
    );
}

#[test]
fn the_ids_a_caller_states_never_change_the_result() {
    let base = load(&emitted(&plant()));
    let head = loaded(&support::example("plant_reset").1);
    let options = DiffOptions::default();
    for (b, h) in [(&base, &head), (&base, &base), (&head, &base)] {
        let plain = diff_topology(&b.topology, &h.topology, &options).expect("diffs");
        for (left, right) in [(&b.id, &h.id), (&b.id, &b.id), (&h.id, &b.id)] {
            let stated = diff_topology_with_ids(&b.topology, left, &h.topology, right, &options)
                .expect("diffs");
            assert_eq!(stated, plain);
        }
    }
    // The refusal is the same too.
    let bad = moved("sel");
    let stated =
        diff_topology_with_ids(&base.topology, &base.id, &bad.topology, &base.id, &options);
    assert_eq!(stated, Err(refusal(&base.topology, &bad.topology)));
}

#[test]
fn the_unnamed_blocks_note_has_its_own_header_line() {
    let named = load(&emitted(&plant())).topology;
    let text = render_text(&compare_topologies(&named, &unnamed("b4")));
    assert_eq!(
        text,
        "# oce diff chw_plant_reset -> chw_plant_reset\n\
         # graph differs; parameters not separable; interface equal; metadata not compared\n\
         # blocks: 4 by name, 1 renamed, 0 removed, 0 added\n\
         # unnamed blocks: a block with no instance IRI is spelled b<id> and matched only by the rename pass\n\
         # graph\n\
         > block b4 = CDL.Reals.Switch   # was sel; renamed, exact\n"
    );
}

#[test]
fn a_side_s_sequence_is_its_root_id_after_the_namespace() {
    for (root, sequence) in [
        ("urn:oce:author:chw_plant_reset", "chw_plant_reset"),
        ("http://example.org#g36.source.reheat", "g36.source.reheat"),
        ("http://example.org/plants/reset", "reset"),
        ("synthetic", "synthetic"),
        ("http://example.org/", "http://example.org/"),
        ("urn:x:", "urn:x:"),
    ] {
        let mut topology = Engine::new().topology();
        topology.model_id = oce_api::DomainKey::new(root);
        let diff = compare_topologies(&topology, &Engine::new().topology());
        assert_eq!(diff.base.sequence, sequence, "{root}");
        assert_eq!(diff.head.sequence, "", "{root}");
    }
}
