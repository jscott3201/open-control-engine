//! The CXF adapter held to the authoring diff (diff note, "Test plan", adapters; issue #666).
//!
//! **The claim.** For two authoring models `A` and `B` that emit and load, the CXF diff of the
//! two loaded documents equals the authoring diff of `grounded(A)` against `grounded(B)`
//! ([`support::grounding`]) on everything both levels see: the graph and interface sections
//! without `reordered` entries, their two summary flags, the block counts, the notes and the two
//! sequence names, with the rename pass on and off.
//!
//! **What each level cannot see.** The CXF level cannot see a sequence parameter as such (its
//! change is `reparameterized` on each leaf that references it, its rename nothing), a reference
//! against a literal of the same value, an Integer literal the emitter widened to a Real, unit
//! suffixes, comments, semantic fields other than `unit`, `derived_from`, authored order, an
//! input nothing reads, an output nothing drives and an output's bounds: so its `parameters` and
//! `metadata` are always empty and their flags `None`. The authoring level cannot see a value a
//! reference grounds to, nor an input's effective bounds where a port it feeds narrows them.
//! `grounded` writes the first list into the model; the corpus here has no narrowing port.
//!
//! The models are every tracked authoring fixture (the worked examples and the `every_kind`
//! pair, the valid wire fixtures of `oce-author`, the 31 G36 corpus documents the importer
//! reads), every ordered pair of them, and each against a set of deterministic edits of it.

mod support;

use oce_author::{
    Assignment, Bound, EmitOptions, Endpoint, Identifier, ImportOptions, Literal, ParamValue,
    Sequence, SignalType, emit, from_wire_str, import_cxf,
};
use oce_diff::{DiffOptions, diff, diff_topology};

use support::grounding::{grounded, seen_by_both};
use support::loaded::{Loaded, corpus, corpus_bytes, try_load};

/// A model emitted and loaded, or `None` when either step refuses it.
fn loadable(model: &Sequence) -> Option<Loaded> {
    let bytes = emit(model, &EmitOptions::default()).ok()?;
    try_load(&bytes).ok()
}

/// Assert the claim for one pair of loaded models.
fn agree(base: (&Sequence, &Loaded), head: (&Sequence, &Loaded), context: &str) {
    let (grounded_base, grounded_head) = (grounded(base.0), grounded(head.0));
    for renames in [true, false] {
        let mut options = DiffOptions::default();
        options.renames = renames;
        let cxf = diff_topology(&base.1.topology, &head.1.topology, &options)
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        let authoring = diff(&grounded_base, &grounded_head, &options);
        assert_eq!(
            seen_by_both(&cxf),
            seen_by_both(&authoring),
            "{context}, renames {renames}"
        );
        // What the CXF level reports beyond the graph and interface is a renamed root, as a
        // `sequence-renamed` entry, and `equal` is exactly what both levels see.
        assert!(cxf.parameters.is_empty(), "{context}");
        assert_eq!(cxf.summary.parameters_equal, None, "{context}");
        assert_eq!(
            cxf.summary.metadata_equal,
            (!cxf.metadata.is_empty()).then_some(false),
            "{context}"
        );
        assert_eq!(
            serde_json::json!(cxf.summary.equal),
            seen_by_both(&cxf)["seen_equal"],
            "{context}"
        );
        assert_eq!(
            cxf.metadata.is_empty(),
            cxf.base.sequence == cxf.head.sequence,
            "{context}"
        );
    }
}

/// Every tracked authoring fixture, by name.
fn tracked() -> Vec<(String, Sequence)> {
    let mut models = Vec::new();
    for name in support::EXAMPLES.iter().chain(&["every_kind"]) {
        let (base, head) = support::example(name);
        models.push((format!("{name}.base"), base));
        models.push((format!("{name}.head"), head));
    }
    let wire = support::fixture("../../../oce-author/tests/fixtures/wire");
    for name in ["econ_enable", "every_element", "note_example"] {
        let text = std::fs::read_to_string(wire.join(format!("{name}.json"))).expect("read");
        let model = from_wire_str(&text).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        models.push((format!("wire {name}"), model));
    }
    for (path, _) in corpus() {
        if let Ok(imported) = import_cxf(&corpus_bytes(&path), &ImportOptions::default()) {
            models.push((path, imported.sequence));
        }
    }
    models
}

#[test]
fn the_tracked_models_are_the_ones_named_and_load_but_two() {
    let models = tracked();
    // Ten example sides, three wire fixtures, 31 imported corpus documents.
    assert_eq!(models.len(), 10 + 3 + 31);
    let refused: Vec<&str> = models
        .iter()
        .filter(|(_, model)| loadable(model).is_none())
        .map(|(name, _)| name.as_str())
        .collect();
    // `every_kind.head` feeds an Integer input to a Real port: stage 1 passes, the load refuses.
    // The wire `note_example` links a block it does not declare, which stage 1 refuses.
    assert_eq!(refused, ["every_kind.head", "wire note_example"]);
}

#[test]
fn every_ordered_pair_of_tracked_models_agrees_across_the_levels() {
    let models: Vec<(String, Sequence, Loaded)> = tracked()
        .into_iter()
        .filter_map(|(name, model)| loadable(&model).map(|loaded| (name, model, loaded)))
        .collect();
    assert_eq!(models.len(), 42);
    for (base_name, base, base_loaded) in &models {
        for (head_name, head, head_loaded) in &models {
            agree(
                (base, base_loaded),
                (head, head_loaded),
                &format!("{base_name} against {head_name}"),
            );
        }
    }
}

/// A deterministic edit of a model, or `None` where it does not apply.
type Edit = fn(&Sequence) -> Option<Sequence>;

fn first_real(model: &mut Sequence) -> Option<&mut f64> {
    model
        .blocks
        .iter_mut()
        .flat_map(|block| block.assignments.iter_mut())
        .find_map(|assignment| match &mut assignment.value {
            ParamValue::Literal(Literal::Real(real)) => Some(real),
            _ => None,
        })
}

fn retune(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let real = first_real(&mut model)?;
    *real = if *real == 0.0 { -*real } else { *real + 1.0 };
    Some(model)
}

fn rename(model: &mut Sequence, from: &str, to: &str) {
    for block in &mut model.blocks {
        if block.name.as_str() == from {
            block.name = Identifier::unchecked(to);
        }
    }
    for connection in &mut model.connections {
        for end in [&mut connection.source, &mut connection.target] {
            if let Endpoint::Port { block, .. } = end
                && block.as_str() == from
            {
                *block = Identifier::unchecked(to);
            }
        }
    }
}

fn rename_one(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let from = model.blocks.first()?.name.as_str().to_owned();
    rename(&mut model, &from, &format!("{from}Renamed"));
    Some(model)
}

fn rename_all(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let names: Vec<String> = model
        .blocks
        .iter()
        .map(|block| block.name.as_str().to_owned())
        .collect();
    for name in names {
        rename(&mut model, &name, &format!("z{name}"));
    }
    (!model.blocks.is_empty()).then_some(model)
}

fn drop_block(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let gone = model.blocks.pop()?.name.as_str().to_owned();
    let touches =
        |end: &Endpoint| matches!(end, Endpoint::Port { block, .. } if block.as_str() == gone);
    model
        .connections
        .retain(|connection| !touches(&connection.source) && !touches(&connection.target));
    Some(model)
}

fn rewire(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let kind = |model: &Sequence, end: &Endpoint| -> Option<SignalType> {
        let Endpoint::Boundary(name) = end else {
            return None;
        };
        model
            .inputs
            .iter()
            .find(|input| input.name == *name)
            .map(|input| input.value_type)
    };
    let fed: Vec<usize> = (0..model.connections.len())
        .filter(|&at| kind(&model, &model.connections[at].source).is_some())
        .collect();
    let (a, b) = fed.iter().enumerate().find_map(|(at, &a)| {
        fed[at + 1..].iter().find_map(|&b| {
            let (left, right) = (&model.connections[a].source, &model.connections[b].source);
            (left != right && kind(&model, left) == kind(&model, right)).then_some((a, b))
        })
    })?;
    let source = model.connections[a].source.clone();
    model.connections[a].source = model.connections[b].source.clone();
    model.connections[b].source = source;
    Some(model)
}

fn relabel(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let input = model
        .inputs
        .iter_mut()
        .find(|input| input.value_type == SignalType::Real)?;
    input.semantic.unit = Some(if input.semantic.unit.as_deref() == Some("K") {
        "degC".to_owned()
    } else {
        "K".to_owned()
    });
    input.min = Some(Bound::Real(-1.5));
    input.display_unit = Some("degF".to_owned());
    Some(model)
}

fn explicit_default(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    for block in &mut model.blocks {
        let metadata = oce_api::class_metadata()
            .iter()
            .find(|metadata| metadata.class_path == block.class_path)?;
        let default = metadata.parameters.iter().find_map(|parameter| {
            let assigned = block
                .assignments
                .iter()
                .any(|assignment| assignment.name.as_str() == parameter.name);
            let literal = match parameter.default {
                oce_api::CatalogDefault::Real(real) => Literal::Real(real),
                oce_api::CatalogDefault::Integer(integer) => Literal::Integer(integer),
                oce_api::CatalogDefault::Boolean(boolean) => Literal::Boolean(boolean),
                _ => return None,
            };
            (!assigned && !parameter.name.contains('<')).then_some((parameter.name, literal))
        });
        if let Some((name, literal)) = default {
            block.assignments.push(Assignment::new(
                Identifier::unchecked(name),
                ParamValue::Literal(literal),
            ));
            return Some(model);
        }
    }
    None
}

fn retune_parameter(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    let referenced = |name: &str| {
        model.blocks.iter().any(|block| {
            block.assignments.iter().any(
                |assignment| matches!(&assignment.value, ParamValue::Ref(r) if r.as_str() == name),
            )
        })
    };
    let at = (0..model.params.len()).find(|&at| referenced(model.params[at].name.as_str()))?;
    let parameter = &mut model.params[at];
    parameter.value = match parameter.value {
        Literal::Real(real) => Literal::Real(real + 0.25),
        Literal::Integer(integer) => Literal::Integer(integer + 1),
        _ => return None,
    };
    Some(model)
}

fn rename_root(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    model.name = Identifier::unchecked("renamedRoot");
    Some(model)
}

fn reverse_order(model: &Sequence) -> Option<Sequence> {
    let mut model = model.clone();
    model.blocks.reverse();
    model.connections.reverse();
    model.inputs.reverse();
    model.outputs.reverse();
    for block in &mut model.blocks {
        block.assignments.reverse();
    }
    Some(model)
}

const EDITS: [(&str, Edit); 11] = [
    ("retune", retune),
    ("rename one block", rename_one),
    ("rename every block", rename_all),
    ("drop a block", drop_block),
    ("rewire", rewire),
    ("relabel an input", relabel),
    ("assign a default", explicit_default),
    ("retune a sequence parameter", retune_parameter),
    ("rename the root", rename_root),
    ("reverse every list", reverse_order),
    ("retune twice", |model| retune(&retune(model)?)),
];

#[test]
fn every_tracked_model_agrees_across_the_levels_with_each_edit_of_it() {
    let mut compared: Vec<(&str, usize)> = EDITS.iter().map(|&(name, _)| (name, 0)).collect();
    for (name, model) in tracked() {
        let Some(loaded) = loadable(&model) else {
            continue;
        };
        for (at, (edit_name, edit)) in EDITS.iter().enumerate() {
            let Some(edited) = edit(&model) else {
                continue;
            };
            let Some(edited_loaded) = loadable(&edited) else {
                continue;
            };
            let context = format!("{name}, {edit_name}");
            agree((&model, &loaded), (&edited, &edited_loaded), &context);
            agree((&edited, &edited_loaded), (&model, &loaded), &context);
            compared[at].1 += 1;
        }
    }
    // How many of the 42 loadable models each edit applied to and still loaded: pinned, so that
    // an edit that stops applying cannot pass by comparing nothing.
    assert_eq!(
        compared,
        [
            ("retune", 36),
            ("rename one block", 42),
            ("rename every block", 42),
            ("drop a block", 10),
            ("rewire", 24),
            ("relabel an input", 33),
            ("assign a default", 23),
            ("retune a sequence parameter", 18),
            ("rename the root", 42),
            ("reverse every list", 42),
            ("retune twice", 36),
        ]
    );
}
