//! A CDL extension block (OBC §7.13 and §8.7) is refused at load with its own code,
//! `extension-block-unsupported`, in every position a document can hold one (GitHub issue #537).
//!
//! Before the code existed, a leaf extension block was `class-not-found`, an extension-block root
//! with no children was `composite/root-count`, and four shapes loaded: an extension block that
//! lists `containsBlock` children (as the root or nested) ran those children in place of its FMU,
//! an extension block no `containsBlock` edge reaches was ignored, and a node whose `@type` list
//! named a registered class before `S231:ExtensionBlock`, or that carried `S231:hasFmuPath` beside
//! a registered class, ran natively. Each test below names the shape it replaces.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand. Each document is the
//! smallest that loads warning-free once its extension block is replaced by a registered class,
//! and the controls prove it, so the refusal is the extension block's alone.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Map, Value as Json, json};

const WITH_FMU_PATH: &str = include_str!("fixtures/invalid/extension_block_with_fmu_path.jsonld");
const WITHOUT_FMU_PATH: &str =
    include_str!("fixtures/invalid/extension_block_without_fmu_path.jsonld");
const S231_NS: &str = "http://data.ashrae.org/S231#";
const EX: &str = "http://example.org#";
const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";
const ABS: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Abs";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
/// The extension block of both fixtures.
const LEAF: &str = "http://example.org#Fdd.ext";
/// The class an instance names when the document carries the class's definition.
const CLASS: &str = "http://example.org#Pkg.FaultDetector";

const TYPED_WITH: &str = "extension block (typed S231:ExtensionBlock, with S231:hasFmuPath): its \
                          behaviour is a Functional Mockup Unit, and the engine does not execute \
                          FMUs (FMI is out of scope before 1.0)";
const TYPED_WITHOUT: &str = "extension block (typed S231:ExtensionBlock, without \
                             S231:hasFmuPath): its behaviour is a Functional Mockup Unit, and the \
                             engine does not execute FMUs (FMI is out of scope before 1.0)";
const FMU_PATH_ONLY: &str = "extension block (it carries S231:hasFmuPath, whose domain is \
                             S231:ExtensionBlock): its behaviour is a Functional Mockup Unit, and \
                             the engine does not execute FMUs (FMI is out of scope before 1.0)";
const INSTANCE: &str = "instance of an extension block (its @type names a node of this document \
                        that is one): its behaviour is a Functional Mockup Unit, and the engine \
                        does not execute FMUs (FMI is out of scope before 1.0)";

fn parse(text: &str) -> Json {
    serde_json::from_str(text).expect("fixture is JSON")
}

fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == id)
        .and_then(Json::as_object_mut)
        .expect("node present")
}

fn push(doc: &mut Json, node: Json) {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(node);
}

fn import(doc: &Json, opts: &ResolveOptions) -> Result<usize, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, opts).map(|(graph, report)| {
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        graph.blocks.len()
    })
}

/// The complete outcome, rendered: two documents agree iff these strings agree.
fn outcome(doc: &Json) -> String {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => format!("ok {graph:?}\n{:?}", report.diagnostics),
        Err(error) => format!("err {error:?}"),
    }
}

/// The block count of a warning-free load.
fn loads(doc: &Json) -> usize {
    import(doc, &ResolveOptions::default()).unwrap_or_else(|e| panic!("expected a load: {e:?}"))
}

/// Every diagnostic of a resolver refusal, in the order returned.
fn refusal(doc: &Json) -> Vec<Diagnostic> {
    match import(doc, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a resolver refusal, got {other:?}"),
    }
}

fn refused(subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(DiagCode::ExtensionBlockUnsupported, message).with_subject(subject)
}

/// A source-shaped block `id` of `class` (one parameter `k`, one output `y`), with `extra` keys.
fn source(id: &str, class: &str, extra: &[(&str, Json)]) -> Vec<Json> {
    let mut block = json!({
        "@id": id,
        "@type": class,
        "S231:hasParameter": { "@id": format!("{id}.k") },
        "S231:hasOutput": { "@id": format!("{id}.y") }
    });
    for (key, value) in extra {
        block[*key] = value.clone();
    }
    vec![
        block,
        json!({ "@id": format!("{id}.k"), "S231:value": { "@value": "1.0", "@type": DOUBLE } }),
        json!({
            "@id": format!("{id}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]
}

/// A block `id` of `class` containing `children`, with `extra` keys.
fn container(id: &str, class: &str, children: &[&str], extra: &[(&str, Json)]) -> Json {
    let mut node = json!({
        "@id": id,
        "@type": class,
        "S231:containsBlock": children.iter().map(|c| json!({ "@id": c })).collect::<Vec<_>>()
    });
    for (key, value) in extra {
        node[*key] = value.clone();
    }
    node
}

fn document(graph: Vec<Json>) -> Json {
    json!({ "@context": { "S231": S231_NS }, "@graph": graph })
}

fn fmu() -> (&'static str, Json) {
    ("S231:hasFmuPath", json!("fmu/FaultDetector.fmu"))
}

/// The fixture without an FMU path, its extension block retyped as `CDL.Reals.Abs`.
fn registered_leaf() -> Json {
    let mut doc = parse(WITHOUT_FMU_PATH);
    node_mut(&mut doc, LEAF).insert("@type".into(), json!(ABS));
    doc
}

#[test]
fn an_extension_block_child_of_the_root_is_refused_with_and_without_an_fmu_path() {
    // Before: `class-not-found`, "no registered block class for `ExtensionBlock`".
    assert_eq!(
        refusal(&parse(WITH_FMU_PATH)),
        vec![refused(LEAF, TYPED_WITH)]
    );
    assert_eq!(
        refusal(&parse(WITHOUT_FMU_PATH)),
        vec![refused(LEAF, TYPED_WITHOUT)]
    );
}

#[test]
fn both_fixtures_load_once_the_extension_block_is_a_registered_class() {
    for text in [WITH_FMU_PATH, WITHOUT_FMU_PATH] {
        let mut doc = parse(text);
        let leaf = node_mut(&mut doc, LEAF);
        leaf.insert("@type".into(), json!(ABS));
        leaf.remove("S231:hasFmuPath");
        assert_eq!(loads(&doc), 2);
    }
}

#[test]
fn an_extension_block_inside_a_nested_composite_is_refused() {
    // Before: `class-not-found` at the nested leaf.
    let (root, sub, ext) = (
        format!("{EX}R"),
        format!("{EX}R.sub"),
        format!("{EX}R.sub.ext"),
    );
    let nested = |class: &str, extra: &[(&str, Json)]| {
        let mut graph = vec![
            container(&root, "S231:Block", &[&sub], &[]),
            container(&sub, &format!("{EX}Pkg.Sub"), &[&ext], &[]),
        ];
        graph.extend(source(&ext, class, extra));
        document(graph)
    };
    assert_eq!(loads(&nested(CONSTANT, &[])), 1);
    assert_eq!(
        refusal(&nested("S231:ExtensionBlock", &[fmu()])),
        vec![refused(&ext, TYPED_WITH)]
    );
    assert_eq!(
        refusal(&nested("S231:ExtensionBlock", &[])),
        vec![refused(&ext, TYPED_WITHOUT)]
    );
}

#[test]
fn an_extension_block_that_lists_children_is_refused_not_flattened() {
    // Before: the block was read as a composite and its child `con` loaded and ran in place of
    // the FMU. Refused nested and as the root; the child itself is not an extension block.
    let (root, ext, con) = (
        format!("{EX}R"),
        format!("{EX}R.ext"),
        format!("{EX}R.ext.con"),
    );
    let mut nested = vec![
        container(&root, "S231:Block", &[&ext], &[]),
        container(&ext, "S231:ExtensionBlock", &[&con], &[fmu()]),
    ];
    nested.extend(source(&con, CONSTANT, &[]));
    assert_eq!(refusal(&document(nested)), vec![refused(&ext, TYPED_WITH)]);

    let con = format!("{EX}R.con");
    let mut rooted = vec![container(&root, "S231:ExtensionBlock", &[&con], &[])];
    rooted.extend(source(&con, CONSTANT, &[]));
    assert_eq!(
        refusal(&document(rooted.clone())),
        vec![refused(&root, TYPED_WITHOUT)]
    );
    rooted[0]["@type"] = json!("S231:Block");
    assert_eq!(loads(&document(rooted)), 1);
}

#[test]
fn a_document_that_is_an_extension_block_is_refused_not_reported_as_rootless() {
    // An extension block's own translation, no children. Before: `composite/root-count`, zero
    // candidate roots.
    let root = format!("{EX}FaultDetector");
    let doc = document(vec![
        json!({
            "@id": root,
            "@type": "S231:ExtensionBlock",
            "S231:hasFmuPath": "fmu/FaultDetector.fmu",
            "S231:hasOutput": { "@id": format!("{root}.y") }
        }),
        json!({
            "@id": format!("{root}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    assert_eq!(refusal(&doc), vec![refused(&root, TYPED_WITH)]);
}

#[test]
fn an_extension_block_no_containment_edge_reaches_is_refused() {
    // Before: ignored, and the document loaded. Unreferenced (a class definition carried
    // along), and listed through the root's `S231:hasInstance` (the spec form's member list).
    assert_eq!(loads(&registered_leaf()), 2);
    let mut orphan = registered_leaf();
    push(
        &mut orphan,
        json!({ "@id": CLASS, "@type": "S231:ExtensionBlock", "S231:hasFmuPath": "a.fmu" }),
    );
    assert_eq!(refusal(&orphan), vec![refused(CLASS, TYPED_WITH)]);

    let member = format!("{EX}Fdd.fmuMember");
    let mut listed = registered_leaf();
    node_mut(&mut listed, "http://example.org#Fdd")
        .insert("S231:hasInstance".into(), json!({ "@id": member }));
    push(
        &mut listed,
        json!({ "@id": member, "@type": "S231:ExtensionBlock" }),
    );
    assert_eq!(refusal(&listed), vec![refused(&member, TYPED_WITHOUT)]);
}

#[test]
fn an_instance_of_an_extension_block_the_document_declares_is_refused_with_its_class() {
    // Before: `class-not-found` at the instance, the class node ignored.
    let (root, det) = (format!("{EX}R"), format!("{EX}R.det"));
    let mut graph = vec![
        container(&root, "S231:Block", &[&det], &[]),
        json!({ "@id": CLASS, "@type": "S231:ExtensionBlock", "S231:hasFmuPath": "a.fmu" }),
    ];
    graph.extend(source(&det, CLASS, &[]));
    assert_eq!(
        refusal(&document(graph.clone())),
        vec![refused(CLASS, TYPED_WITH), refused(&det, INSTANCE)]
    );

    // Without the class's definition the class is simply unknown: `class-not-found`, unchanged.
    graph.remove(1);
    let diags = refusal(&document(graph));
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(
                DiagCode::ClassNotFound,
                "no registered block class for `Pkg.FaultDetector`"
            )
            .with_subject(det.as_str())
        ]
    );
}

#[test]
fn a_conditionally_removed_extension_block_is_pruned_and_an_active_one_is_refused() {
    let (root, con, ext) = (format!("{EX}R"), format!("{EX}R.con"), format!("{EX}R.ext"));
    let doc = |has_fdd: bool| {
        let mut graph = vec![
            container(
                &root,
                "S231:Block",
                &[&con, &ext],
                &[(
                    "S231:hasParameter",
                    json!({ "@id": format!("{root}.hasFdd") }),
                )],
            ),
            json!({
                "@id": format!("{root}.hasFdd"),
                "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:value": has_fdd
            }),
        ];
        graph.extend(source(&con, CONSTANT, &[]));
        graph.extend(source(
            &ext,
            "S231:ExtensionBlock",
            &[
                fmu(),
                ("S231:isConditionalComponent", json!(true)),
                ("S231:conditionalExpression", json!("hasFdd")),
            ],
        ));
        document(graph)
    };
    assert_eq!(loads(&doc(false)), 1);
    assert_eq!(refusal(&doc(true)), vec![refused(&ext, TYPED_WITH)]);
}

#[test]
fn an_extension_block_type_anywhere_in_the_type_list_is_refused() {
    // Before: `[Abs, ExtensionBlock]` loaded and ran `Abs`; `[ExtensionBlock, Abs]` was
    // `class-not-found`.
    for types in [
        json!([ABS, "S231:ExtensionBlock"]),
        json!(["S231:ExtensionBlock", ABS]),
    ] {
        let mut doc = registered_leaf();
        node_mut(&mut doc, LEAF).insert("@type".into(), types);
        assert_eq!(refusal(&doc), vec![refused(LEAF, TYPED_WITHOUT)]);
    }
}

#[test]
fn an_fmu_path_on_any_other_class_is_refused() {
    // Before: loaded and ran `Abs`, the FMU path unread. The empty string is a value too.
    for path in ["fmu/FaultDetector.fmu", ""] {
        let mut doc = registered_leaf();
        node_mut(&mut doc, LEAF).insert("S231:hasFmuPath".into(), json!(path));
        assert_eq!(refusal(&doc), vec![refused(LEAF, FMU_PATH_ONLY)]);
    }
}

#[test]
fn every_spelling_of_the_type_and_the_fmu_path_is_refused_identically() {
    let compact = outcome(&parse(WITH_FMU_PATH));
    assert!(compact.starts_with("err Validation"), "{compact}");
    let respelled = |key: &str, ty: &str, prefix: bool| {
        let mut doc = parse(WITH_FMU_PATH);
        if prefix {
            doc["@context"]["s231"] = json!(S231_NS);
        }
        let leaf = node_mut(&mut doc, LEAF);
        let path = leaf.remove("S231:hasFmuPath").expect("fixture carries it");
        leaf.insert(key.to_owned(), path);
        leaf.insert("@type".into(), json!(ty));
        outcome(&doc)
    };
    let full_key = format!("{S231_NS}hasFmuPath");
    let full_type = format!("{S231_NS}ExtensionBlock");
    for (key, ty, prefix) in [
        (full_key.as_str(), "S231:ExtensionBlock", false),
        ("S231:hasFmuPath", full_type.as_str(), false),
        (full_key.as_str(), full_type.as_str(), false),
        ("s231:hasFmuPath", "s231:ExtensionBlock", true),
    ] {
        assert_eq!(respelled(key, ty, prefix), compact, "{key} / {ty}");
    }
}

#[test]
fn near_misses_are_not_extension_blocks() {
    // A class path whose last dotted segment is `ExtensionBlock`, and a term that only ends
    // with it, are unknown classes; a `hasFmuPath` key in another namespace passes through.
    for class in [
        "http://example.org#Buildings.Controls.OBC.Pkg.ExtensionBlock",
        "http://example.org#MyExtensionBlock",
    ] {
        let mut doc = registered_leaf();
        node_mut(&mut doc, LEAF).insert("@type".into(), json!(class));
        let diags = refusal(&doc);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagCode::ClassNotFound, "{class}");
    }
    let mut doc = registered_leaf();
    node_mut(&mut doc, LEAF).insert(format!("{EX}hasFmuPath"), json!("a.fmu"));
    assert_eq!(loads(&doc), 2);
}

#[test]
fn an_extension_block_type_in_the_retired_namespace_is_a_namespace_refusal() {
    // The namespace check reads typing tokens first (GitHub #527), so the precedence is pinned.
    let mut doc = parse(WITH_FMU_PATH);
    node_mut(&mut doc, LEAF).insert(
        "@type".into(),
        json!("http://data.ashrae.org/S231P#ExtensionBlock"),
    );
    let codes: Vec<DiagCode> = refusal(&doc).iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagCode::VocabularyNamespaceMismatch]);
}

#[test]
fn a_non_string_fmu_path_is_a_parse_refusal_and_null_reads_as_absent() {
    for value in [
        json!(7),
        json!(true),
        json!(["a.fmu"]),
        json!({ "@value": "a.fmu" }),
    ] {
        let mut doc = parse(WITH_FMU_PATH);
        node_mut(&mut doc, LEAF).insert("S231:hasFmuPath".into(), value.clone());
        assert!(
            matches!(
                import(&doc, &ResolveOptions::default()),
                Err(CxfError::Json(_))
            ),
            "{value}"
        );
    }
    let mut doc = parse(WITH_FMU_PATH);
    node_mut(&mut doc, LEAF).insert("S231:hasFmuPath".into(), Json::Null);
    assert_eq!(outcome(&doc), outcome(&parse(WITHOUT_FMU_PATH)));
}

#[test]
fn several_extension_blocks_are_each_refused_in_one_order_whatever_the_node_order() {
    let mut doc = parse(WITH_FMU_PATH);
    push(
        &mut doc,
        json!({ "@id": CLASS, "@type": "S231:ExtensionBlock" }),
    );
    push(
        &mut doc,
        json!({ "@id": format!("{EX}Fdd.yOut.alias"), "S231:hasFmuPath": "b.fmu" }),
    );
    let expected = vec![
        refused(LEAF, TYPED_WITH),
        refused("http://example.org#Fdd.yOut.alias", FMU_PATH_ONLY),
        refused(CLASS, TYPED_WITHOUT),
    ];
    assert_eq!(refusal(&doc), expected);
    let first = outcome(&doc);
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .reverse();
    for _ in 0..8 {
        assert_eq!(outcome(&doc), first);
    }
}

#[test]
fn deny_warnings_leaves_the_refusal_unchanged() {
    let mut opts = ResolveOptions::default();
    opts.deny_warnings = true;
    match import(&parse(WITH_FMU_PATH), &opts) {
        Err(CxfError::Validation(diags)) => assert_eq!(diags, vec![refused(LEAF, TYPED_WITH)]),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn withheld_declaration_findings_survive_an_extension_block_refusal() {
    // The root's declaration cycle is found by the specialize pass (the root has a conditional
    // child) and is otherwise reported from the lowering pass, which the refusal skips.
    let (root, ext) = (format!("{EX}R"), format!("{EX}R.ext"));
    let mut graph = vec![
        container(
            &root,
            "S231:Block",
            &[&ext, &format!("{root}.cond")],
            &[(
                "S231:hasParameter",
                json!([
                    { "@id": format!("{root}.a") },
                    { "@id": format!("{root}.b") },
                    { "@id": format!("{root}.haveX") }
                ]),
            )],
        ),
        json!({ "@id": format!("{root}.a"), "S231:value": "b + 1.0" }),
        json!({ "@id": format!("{root}.b"), "S231:value": "a + 1.0" }),
        json!({ "@id": format!("{root}.haveX"), "S231:value": false }),
    ];
    graph.extend(source(&ext, "S231:ExtensionBlock", &[fmu()]));
    graph.extend(source(
        &format!("{root}.cond"),
        CONSTANT,
        &[
            ("S231:isConditionalComponent", json!(true)),
            ("S231:conditionalExpression", json!("haveX")),
        ],
    ));
    assert_eq!(
        refusal(&document(graph)),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "composite/declaration-cycle: cycle in the block's own declaration references: \
                 http://example.org#R.a -> http://example.org#R.b -> http://example.org#R.a",
            )
            .with_subject("http://example.org#R.a"),
            refused(&ext, TYPED_WITH),
        ]
    );
}
