//! Gate item 1 (authoring note; importer note, "Test plan"; issues #464, #475 and #496): every
//! G36 corpus fixture inside the M2 authoring subset imports into the model, emits, loads
//! warning-free and has the same identity as the committed fixture loaded directly; every other
//! fixture is refused by the importer with a typed error, listed with its kind.
//!
//! **Coverage.** Gate item 1 is met: 31 of the 34 in-subset fixtures ([`support::cxf::IMPORTED`]).
//! The other 15 ([`support::cxf::REFUSED`]) are refused under the fixed precedence: 12 are outside
//! the M2 subset (4 nested composites, 7 conditional components, 1 expression value) and 3
//! hand-written fixtures carry block-port bounds, which are not authorable in M2 (importer note,
//! decision 8; GitHub #486). Identity is the canonical content id ([`support::cxf::identity`],
//! GitHub #470), defined whether or not the export is partial.
//!
//! **Namespace.** The corpus names its nodes under `http://example.org#g36.source.` (or
//! `http://example.org#g36.`), which ends in `.`; an emitter namespace must end in `#`, `/` or
//! `:` (the content-id note, GitHub #470/#471), so the emitter cannot spell the corpus IRIs. The
//! comparison therefore re-stamps the committed fixture's root IRI `{namespace}{sequence}` to
//! `urn:oce:library:{sequence}` — a textual substitution of the root prefix only, after which no
//! occurrence of the corpus namespace may remain — and the test also proves that the re-stamp is
//! the only difference: the committed fixture's own export, with the same substitution, equals the
//! emitted document's export byte for byte, wherever block order agrees. The re-stamp stays until
//! library bundle v2, regenerated from the authoring form, removes it (importer note, decision 14).
//!
//! **Node order (GitHub #458).** The canonical content id ignores order, but the export bytes do
//! not. One flat fixture's `@graph` block order differs from its `containsBlock` order
//! (`multizone_vav_outdoor_airflow_sumzone`), which the emitter's layout cannot reproduce; it is
//! compared on its identity like the others and its export bytes are left out ([`ORDER_DIFFERS`]).
//! `node_order_alone_moves_the_sumzone_export_but_not_its_identity` pins the mismatch itself.

mod support;

use oce_api::Value;
use oce_author::{
    Bound, DroppedProperty, EmitOptions, ImportOptions, ParameterType, SignalType, emit, import_cxf,
};

use support::cxf::{IMPORTED, REFUSED, corpus_fixture, export_text, identity, load_clean};
use support::imported;

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;

/// The namespace the round trip emits under.
const LIBRARY_NAMESPACE: &str = "urn:oce:library:";

/// The imported fixtures whose `@graph` block order differs from `containsBlock` order, so the
/// emitter's export bytes cannot equal the committed fixture's (GitHub #458).
const ORDER_DIFFERS: &[&str] = &["multizone_vav_outdoor_airflow_sumzone"];

fn committed(name: &str) -> String {
    let path = corpus_fixture(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The root `@id` of a committed fixture: the node that carries `containsBlock`.
fn root_iri(text: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    value["@graph"]
        .as_array()
        .and_then(|g| g.iter().find(|n| n.get("S231:containsBlock").is_some()))
        .and_then(|n| n["@id"].as_str())
        .expect("a root")
        .to_owned()
}

#[test]
fn the_tables_partition_the_corpus() {
    let mut listed: Vec<&str> = IMPORTED
        .iter()
        .copied()
        .chain(REFUSED.iter().map(|(n, _)| *n))
        .collect();
    listed.sort_unstable();
    let mut deduped = listed.clone();
    deduped.dedup();
    assert_eq!(listed, deduped, "a fixture is listed twice");
    let mut corpus = blob_corpus::FIXTURES.to_vec();
    corpus.sort_unstable();
    assert_eq!(listed, corpus);
    assert_eq!((IMPORTED.len(), REFUSED.len()), (31, 15));
    let count = |code: &str| REFUSED.iter().filter(|(_, c)| *c == code).count();
    assert_eq!(
        [
            count("nested-composite"),
            count("conditional-component"),
            count("expression-value"),
            count("port-attribute"),
        ],
        [4, 7, 1, 3]
    );
    // The M2 subset is the 34 fixtures that are not nested, conditional or expression-valued.
    assert_eq!(IMPORTED.len() + count("port-attribute"), 34);
}

#[test]
fn every_covered_fixture_round_trips_to_the_committed_identity() {
    for name in IMPORTED {
        let text = committed(name);
        let sequence = imported(text.as_bytes());
        let root = root_iri(&text);
        let namespace = root
            .strip_suffix(sequence.name.as_str())
            .expect("the root ends in the sequence name");
        let restamped_root = format!("{LIBRARY_NAMESPACE}{}", sequence.name);
        let restamped = text.replace(&root, &restamped_root);
        assert!(
            !restamped.contains(namespace),
            "{name}: the re-stamp left `{namespace}`"
        );
        let emitted = emit(
            &sequence,
            &EmitOptions::new().with_namespace(LIBRARY_NAMESPACE),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let from_model = load_clean(&emitted);
        let reference = load_clean(restamped.as_bytes());
        assert_eq!(identity(&from_model), identity(&reference), "{name}");
        // The re-stamp is the only difference from the committed fixture loaded directly.
        let direct = load_clean(text.as_bytes());
        if ORDER_DIFFERS.contains(name) {
            continue;
        }
        let direct_export = export_text(&direct).replace(&root, &restamped_root);
        assert!(!direct_export.contains(namespace), "{name}");
        assert_eq!(direct_export, export_text(&from_model), "{name}");
    }
}

#[test]
fn every_other_fixture_is_refused_with_its_kind() {
    for (name, code) in REFUSED {
        match import_cxf(committed(name).as_bytes(), &ImportOptions::new()) {
            Err(error) => assert_eq!(error.code(), *code, "{name}: {error}"),
            Ok(_) => panic!("{name}: expected `{code}`, but it imported"),
        }
    }
}

#[test]
fn re_emitting_an_imported_document_is_byte_stable() {
    for name in IMPORTED {
        let sequence = imported(committed(name).as_bytes());
        let options = EmitOptions::new();
        let once = emit(&sequence, &options).expect("emit");
        let again = import_cxf(&once, &ImportOptions::new()).expect("re-import");
        // An emitted document drops nothing: no label, no root class, no parameter attribute.
        assert!(again.report.is_empty(), "{name}: {:?}", again.report);
        let again = again.sequence;
        assert_eq!(emit(&again, &options).expect("emit"), once, "{name}");
        // The model reads back whole; only connections regroup, by source node in the
        // emitter's layout (each source's targets keep their order).
        assert_eq!(
            (&again.params, &again.inputs, &again.outputs, &again.blocks),
            (
                &sequence.params,
                &sequence.inputs,
                &sequence.outputs,
                &sequence.blocks
            ),
            "{name}"
        );
        let spelled = |s: &oce_author::Sequence| {
            let mut all: Vec<String> = s.connections.iter().map(|c| c.spelling()).collect();
            all.sort_unstable();
            all
        };
        assert_eq!(spelled(&again), spelled(&sequence), "{name}");
    }
}

#[test]
fn every_imported_input_bound_is_the_engines_bound_bit_for_bit() {
    // The effective input domain is inside the content id (content-id note, decision 10): one ulp
    // of drift between the importer's conversion and the loader's would move it.
    let mut compared = 0usize;
    for name in IMPORTED {
        let text = committed(name);
        let sequence = imported(text.as_bytes());
        let engine = load_clean(text.as_bytes());
        let definitions = engine.input_definitions().expect("definitions");
        for input in &sequence.inputs {
            if input.min.is_none() && input.max.is_none() {
                continue;
            }
            let suffix = format!(".{}", input.name);
            let loaded = definitions
                .iter()
                .find(|d| d.path.ends_with(&suffix))
                .unwrap_or_else(|| panic!("{name}: no definition for {}", input.name));
            for (declared, engine_bound) in [(input.min, &loaded.min), (input.max, &loaded.max)] {
                let Some(declared) = declared else { continue };
                let same = match (input.value_type, declared, engine_bound) {
                    (SignalType::Real, Bound::Real(v), Some(Value::Real(e))) => {
                        v.to_bits() == e.to_bits()
                    }
                    (SignalType::Integer, Bound::Integer(v), Some(Value::Integer(e))) => v == *e,
                    _ => false,
                };
                assert!(
                    same,
                    "{name}.{}: {declared:?} vs {engine_bound:?}",
                    input.name
                );
                compared += 1;
            }
        }
    }
    assert!(compared >= 20, "{compared} bounds compared");
}

#[test]
fn the_package_typed_boundary_is_normalised_to_integer_and_reported() {
    // Decision 21: `uOpeMod` is an `S231:IntegerInput` typed by the constant package
    // `OperationModes`; the engine reads it as an Integer, so the importer does too.
    let text = committed("multizone_vav_economizer_limits_common");
    let read = import_cxf(text.as_bytes(), &ImportOptions::new()).expect("import");
    let normalised: Vec<(&str, &str, SignalType)> = read
        .report
        .normalised
        .iter()
        .map(|n| (n.subject.as_str(), n.data_type.as_str(), n.to))
        .collect();
    assert_eq!(
        normalised,
        [(
            "http://example.org#g36.source.multizone_vav_economizer_limits_common.uOpeMod",
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
            SignalType::Integer
        )]
    );
    let input = read
        .sequence
        .inputs
        .iter()
        .find(|b| b.name.as_str() == "uOpeMod")
        .expect("uOpeMod");
    assert_eq!(input.value_type, SignalType::Integer);
    let emitted =
        String::from_utf8(emit(&read.sequence, &EmitOptions::new()).expect("emit")).expect("UTF-8");
    let node = "{\n      \"@id\": \"urn:oce:author:multizone_vav_economizer_limits_common.uOpeMod\",\n      \"@type\": \"S231:IntegerInput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Integer\"\n      }";
    assert!(emitted.contains(node), "{emitted}");
    // The normalised boundary loads to the same identity as the package-typed one.
    let restamped = text.replace("http://example.org#g36.source.", "urn:oce:author:");
    assert_eq!(
        identity(&load_clean(emitted.as_bytes())),
        identity(&load_clean(restamped.as_bytes()))
    );
}

#[test]
fn an_unreferenced_enumeration_parameter_is_carried_and_emitted_as_the_corpus_writes_it() {
    // `eneStd` is read by no leaf: it does not survive flattening, but it is what the author
    // wrote, so the importer carries it with its enumeration type.
    let text = committed("generic_air_economizer_high_limits_ashrae_differential");
    let sequence = imported(text.as_bytes());
    let param = sequence
        .params
        .iter()
        .find(|p| p.name.as_str() == "eneStd")
        .expect("eneStd");
    assert_eq!(
        param.value_type,
        ParameterType::Enum("EnergyStandard".to_owned())
    );
    assert_eq!(param.value, support::member("EnergyStandard.ASHRAE90_1"));
    let emitted = emit(&sequence, &EmitOptions::new()).expect("emit");
    load_clean(&emitted);
    let text = String::from_utf8(emitted).expect("UTF-8");
    assert!(text.contains(
        "\"S231:isOfDataType\": {\n        \"@id\": \"http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard\"\n      },\n      \"S231:value\": \"Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard.ASHRAE90_1\""
    ));
}

#[test]
fn the_report_lists_what_the_corpus_import_dropped() {
    // The root's library class, labels that differ from the local name and descriptive parameter
    // attributes; never a property the engine reads.
    let text = committed("cooling_only_dampers");
    let read = import_cxf(text.as_bytes(), &ImportOptions::new()).expect("import");
    let root = "http://example.org#g36.source.cooling_only_dampers";
    let first = read.report.dropped.first().expect("an entry");
    assert_eq!(
        (first.subject.as_str(), first.property, first.value.as_str()),
        (
            root,
            DroppedProperty::RootType,
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Subsequences.Dampers"
        )
    );
    for entry in &read.report.dropped {
        assert!(
            entry.subject == root || entry.subject.starts_with(&format!("{root}.")),
            "{entry:?}"
        );
    }
    let kinds: std::collections::BTreeSet<DroppedProperty> =
        read.report.dropped.iter().map(|d| d.property).collect();
    assert!(kinds.contains(&DroppedProperty::Unit), "{kinds:?}");
    assert!(read.report.normalised.is_empty() && read.report.truncated.is_empty());
}

/// Reorder a fixture's `@graph` so block nodes follow `containsBlock` (keeping every other node
/// where it is relative to its block), the order the emitter's layout would write.
fn blocks_in_contains_order(text: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    let graph = value["@graph"].as_array().expect("graph").clone();
    let root = graph
        .iter()
        .find(|n| n.get("S231:containsBlock").is_some())
        .expect("root")
        .clone();
    let blocks: Vec<String> = root["S231:containsBlock"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|r| r["@id"].as_str().expect("id").to_owned())
        .collect();
    let owner = |id: &str| -> Option<usize> {
        blocks
            .iter()
            .position(|b| id == b || id.starts_with(&format!("{b}.")))
    };
    let mut head: Vec<serde_json::Value> = Vec::new();
    let mut per_block: Vec<Vec<serde_json::Value>> = vec![Vec::new(); blocks.len()];
    let mut tail: Vec<serde_json::Value> = Vec::new();
    for node in graph {
        let id = node["@id"].as_str().expect("id").to_owned();
        match owner(&id) {
            Some(b) => per_block[b].push(node),
            None if per_block.iter().all(Vec::is_empty) => head.push(node),
            None => tail.push(node),
        }
    }
    let reordered: Vec<serde_json::Value> = head
        .into_iter()
        .chain(per_block.into_iter().flatten())
        .chain(tail)
        .collect();
    value["@graph"] = serde_json::Value::Array(reordered);
    serde_json::to_string(&value).expect("write")
}

#[test]
fn node_order_alone_moves_the_sumzone_export_but_not_its_identity() {
    // GitHub #458: `multizone_vav_outdoor_airflow_sumzone` lists its blocks in `@graph` as
    // groFlo, groFlo1, groFlo2, groFlo3, booToRea_1, … while `containsBlock` reads groFlo,
    // groFlo1, booToRea_1, booToRea_2, mul_1, … (first divergence at the third block). Moving
    // only the block nodes into `containsBlock` order — the emitter's layout — moves the export
    // bytes; the canonical content id (GitHub #470) ignores order, so it does not move.
    let text = committed("multizone_vav_outdoor_airflow_sumzone");
    let reordered = blocks_in_contains_order(&text);
    let direct = support::cxf::load(text.as_bytes()).expect("load").0;
    let moved = support::cxf::load(reordered.as_bytes()).expect("load").0;
    assert_ne!(export_text(&direct), export_text(&moved));
    assert_eq!(identity(&direct), identity(&moved));
    // The same reordering leaves a fixture whose order already agrees unchanged.
    let agreeing = committed("reheat_overrides");
    let same = identity(&support::cxf::load(agreeing.as_bytes()).expect("load").0);
    let again = identity(
        &support::cxf::load(blocks_in_contains_order(&agreeing).as_bytes())
            .expect("load")
            .0,
    );
    assert_eq!(same, again);
}
