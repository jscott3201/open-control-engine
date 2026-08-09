//! Corpus guard: a document-loaded point's identity is document-derived — an authored `@id`
//! from the document, or a `hasInstance`-derived member identity minted from one.
//!
//! JSON-LD `@graph` is an unordered set, so a positional connector-index path renumbers under
//! semantically identical re-serializations; a durable store keyed on one can graft a point's
//! samples onto another point's history with no error. CXF ingest rejects any connector node
//! without an `@id`, so every facade surface of a CXF-loaded model must carry document-derived
//! identities in canonical `@context`-expanded form, and the positional spelling survives only
//! as the in-crate fallback for hand-built, IRI-less models. Expansion is identity on this
//! corpus — every G36 identity slot is written absolute — which is why the membership oracle
//! below can keep comparing facade paths against the raw document's `@id` values. A
//! `hasInstance`-derived instance's node-less and padded connectors carry
//! `<owner @id>.<declared port name>` (`_spec/19` R19-5) — derived from an authored `@id` and
//! the class signature, never from a position, so the identity is exactly as durable — and the
//! oracle admits that one shape for `hasInstance`-carrying owners only; the authored dialect
//! keeps the strict authored-membership check.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use oce_api::oce_store::Store;
use oce_api::{Engine, PointDirection, PointInfo, Topology};

/// The positional fallback prefix, assembled with `concat!` so a repository-wide search for the
/// banned spelling finds genuine leak sites only, never this guard.
const POSITIONAL_PREFIX: &str = concat!("conn", "#");

/// Every `.jsonld` document in the G36 corpus, sorted for deterministic iteration order.
fn corpus_fixtures() -> Vec<PathBuf> {
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = fs::read_dir(fixture_dir)
        .expect("read G36 fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 47, "G36 corpus size moved");
    fixtures
}

/// Load one corpus document into a fresh in-memory engine, keeping the raw document bytes so a
/// test can read the fixture independently of everything the engine derived from it.
fn load_fixture(path: &PathBuf) -> (String, Vec<u8>, Engine<impl Store>) {
    let fixture_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("UTF-8 fixture stem")
        .to_owned();
    let bytes = fs::read(path).expect("read G36 fixture");
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(&bytes)
        .unwrap_or_else(|error| panic!("{fixture_name} loads: {error:?}"));
    (fixture_name, bytes, engine)
}

/// Every `@id` value appearing anywhere in the document, collected by a plain JSON walk with no
/// resolver involvement. This is the membership oracle: whatever selection and identity rules the
/// engine applies, a facade path that is not literally one of these values is an identity the
/// author never wrote — so any transformation of the path (prefix, suffix, rename, expansion)
/// fails here even though every facade surface agrees with every other.
fn document_id_values(bytes: &[u8]) -> BTreeSet<String> {
    fn collect(value: &serde_json::Value, ids: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(id)) = map.get("@id") {
                    ids.insert(id.clone());
                }
                for nested in map.values() {
                    collect(nested, ids);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect(item, ids);
                }
            }
            _ => {}
        }
    }
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("fixture is JSON");
    let mut ids = BTreeSet::new();
    collect(&document, &mut ids);
    ids
}

/// The `@id` values of `@graph` nodes carrying `S231:hasInstance` — the owners whose derived
/// member identities (`<owner @id>.<one segment>`) the membership oracle admits.
fn has_instance_owner_ids(bytes: &[u8]) -> BTreeSet<String> {
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("fixture is JSON");
    document["@graph"]
        .as_array()
        .map(|graph| {
            graph
                .iter()
                .filter(|node| node.get("S231:hasInstance").is_some())
                .filter_map(|node| node["@id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `path` is a derived member identity of one of `owners`: the owner's `@id`, one `.`,
/// one further segment — the R19-5 mint shape, and nothing else.
fn is_derived_member_identity(path: &str, owners: &BTreeSet<String>) -> bool {
    owners.iter().any(|owner| {
        path.strip_prefix(owner.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|segment| !segment.is_empty() && !segment.contains('.'))
    })
}

/// Every point path visible on the seven facade surfaces: point_list, topology block ports,
/// connection endpoints, external_inputs, pass_through pairs, boundary_outputs, and `to_map`
/// keys.
fn facade_surface_paths<S: Store>(engine: &Engine<S>) -> Vec<(&'static str, String)> {
    let mut paths = Vec::new();
    for point in engine.point_list(None).expect("point inventory") {
        paths.push(("point_list", point.path));
    }
    let topology: Topology = engine.topology();
    for block in &topology.blocks {
        for input in &block.inputs {
            paths.push(("block input", input.clone()));
        }
        for output in &block.outputs {
            paths.push(("block output", output.clone()));
        }
    }
    for connection in &topology.connections {
        paths.push(("connection from", connection.from.clone()));
        paths.push(("connection to", connection.to.clone()));
    }
    for input in &topology.external_inputs {
        paths.push(("external_input", input.clone()));
    }
    for pair in &topology.pass_through {
        paths.push(("pass_through input", pair.input.clone()));
        paths.push(("pass_through output", pair.output.clone()));
    }
    for output in &topology.boundary_outputs {
        paths.push(("boundary_output path", output.path.clone()));
        paths.push(("boundary_output driver", output.driver_path.clone()));
    }
    for (key, _) in engine.outputs().to_map() {
        paths.push(("to_map key", key));
    }
    paths
}

fn point_list_paths<S: Store>(engine: &Engine<S>, direction: PointDirection) -> BTreeSet<String> {
    engine
        .point_list(None)
        .expect("point inventory")
        .into_iter()
        .filter(|point: &PointInfo| point.direction == direction)
        .map(|point| point.path)
        .collect()
}

#[test]
fn no_facade_surface_of_a_loaded_document_emits_a_positional_connector_path() {
    let mut offending_fixtures = Vec::new();
    for fixture in corpus_fixtures() {
        let (fixture_name, _bytes, engine) = load_fixture(&fixture);
        let positional: Vec<(&str, String)> = facade_surface_paths(&engine)
            .into_iter()
            .filter(|(_, path)| path.starts_with(POSITIONAL_PREFIX))
            .collect();
        if !positional.is_empty() {
            offending_fixtures.push((fixture_name, positional.len(), positional));
        }
    }
    assert!(
        offending_fixtures.is_empty(),
        "{}/47 fixtures leak positional {POSITIONAL_PREFIX}<N> paths onto a facade surface: {:?}",
        offending_fixtures.len(),
        offending_fixtures
            .iter()
            .map(|(name, count, sample)| {
                (
                    name.as_str(),
                    *count,
                    sample
                        .first()
                        .map(|(surface, path)| (*surface, path.as_str())),
                )
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn external_inputs_are_a_subset_of_point_list_input_paths() {
    for fixture in corpus_fixtures() {
        let (fixture_name, _bytes, engine) = load_fixture(&fixture);
        let in_paths = point_list_paths(&engine, PointDirection::In);
        for input in &engine.topology().external_inputs {
            assert!(
                in_paths.contains(input),
                "{fixture_name}: external input {input} is missing from point_list In paths"
            );
        }
        // The reverse inclusion is false by construction: point_list enumerates every
        // non-String connector, external_inputs only the boundary inputs.
    }
}

#[test]
fn to_map_keys_and_point_list_output_paths_are_the_same_set() {
    for fixture in corpus_fixtures() {
        let (fixture_name, _bytes, engine) = load_fixture(&fixture);
        let out_paths = point_list_paths(&engine, PointDirection::Out);
        let to_map_keys: BTreeSet<String> = engine
            .outputs()
            .to_map()
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        // Only `point_list Out ⊆ to_map` holds by construction: to_map includes String-typed
        // output connectors while point_list excludes them (CDL §7.8 metadata-only). No G36
        // fixture declares a String output connector, so equality holds over this corpus.
        assert_eq!(
            to_map_keys, out_paths,
            "{fixture_name}: to_map keys diverge from point_list Out paths"
        );
    }
}

#[test]
fn every_facade_path_is_an_id_authored_in_the_source_document() {
    let mut offending_fixtures = Vec::new();
    for fixture in corpus_fixtures() {
        let (fixture_name, bytes, engine) = load_fixture(&fixture);
        let authored_ids = document_id_values(&bytes);
        let member_owners = has_instance_owner_ids(&bytes);
        let foreign: Vec<(&str, String)> = facade_surface_paths(&engine)
            .into_iter()
            .filter(|(_, path)| {
                !authored_ids.contains(path) && !is_derived_member_identity(path, &member_owners)
            })
            .collect();
        if !foreign.is_empty() {
            offending_fixtures.push((fixture_name, foreign.len(), foreign));
        }
    }
    // The cross-surface agreement tests above cannot see a transformation applied uniformly by
    // the shared path helper; this membership check can, because its ground truth never passes
    // through the resolver.
    assert!(
        offending_fixtures.is_empty(),
        "{}/47 fixtures emit facade paths that are not document-derived identities: {:?}",
        offending_fixtures.len(),
        offending_fixtures
            .iter()
            .map(|(name, count, sample)| {
                (
                    name.as_str(),
                    *count,
                    sample
                        .first()
                        .map(|(surface, path)| (*surface, path.as_str())),
                )
            })
            .collect::<Vec<_>>()
    );
}

/// The expected point-path sets `{prefix}.{name}` for a hand-pinned fixture.
fn expected_paths(prefix: &str, names: &[&str]) -> BTreeSet<String> {
    names
        .iter()
        .map(|name| format!("{prefix}.{name}"))
        .collect()
}

// Hand-derived from crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld, not from engine
// output. The root node `#g36.ahu_economizer` declares three `S231:hasInput` boundary inputs
// (return_air_temp, outdoor_air_temp, operating_mode); each drives exactly one block-level input
// (returnMinusOutdoor.u1, returnMinusOutdoor.u2, modeIdentity.u via `isConnectedTo`), so those
// three internal connectors surface under the boundary `@id`s and their own node `@id`s appear
// nowhere. The remaining twelve block-level inputs keep their own `@id`s. Every block-level
// output keeps its own `@id`; the root's four `S231:hasOutput` declarations (economizer_enabled,
// damper_command, operating_mode_real, oa_temperature_delta) — addressable on `get_output`/
// `watch`/`CollectSpec::Named` as read aliases and enumerated in `Topology.boundary_outputs` —
// are deliberately absent from point rows (the _spec/18 D2 fence) and must NOT appear here.
#[test]
fn economizer_point_sets_match_the_hand_derived_document_reading() {
    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "ahu_economizer")
        })
        .expect("ahu_economizer fixture present");
    let (_, _bytes, engine) = load_fixture(&fixture);
    let prefix = "http://example.org#g36.ahu_economizer";
    let expected_in = expected_paths(
        prefix,
        &[
            "return_air_temp",
            "outdoor_air_temp",
            "operating_mode",
            "favorable.u",
            "modeAllowed.u",
            "modeToReal.u",
            "candidate.u1",
            "candidate.u2",
            "delay.u",
            "notCandidate.u",
            "enableLatch.u",
            "enableLatch.clr",
            "damperSwitch.u1",
            "damperSwitch.u2",
            "damperSwitch.u3",
        ],
    );
    let expected_out = expected_paths(
        prefix,
        &[
            "returnMinusOutdoor.y",
            "favorable.y",
            "modeIdentity.y",
            "modeAllowed.y",
            "modeToReal.y",
            "candidate.y",
            "delay.y",
            "notCandidate.y",
            "enableLatch.y",
            "damperOpen.y",
            "damperMinimum.y",
            "damperSwitch.y",
        ],
    );
    assert_eq!(
        point_list_paths(&engine, PointDirection::In),
        expected_in,
        "ahu_economizer In point set moved from the document reading"
    );
    assert_eq!(
        point_list_paths(&engine, PointDirection::Out),
        expected_out,
        "ahu_economizer Out point set moved from the document reading"
    );
}

// Hand-derived from crates/oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking
// .jsonld. The root declares boundary inputs VAirSup_flow -> conErr.u1, VAirRet_flow -> conP.u_m,
// and u1SupFan, which FANS OUT: it drives the block input swi.u2 AND connects directly to the
// declared boundary output y1RetFan. The direct input->output edge lowers to a pass-through
// whose input carries u1SupFan and whose output carries y1RetFan — so u1SupFan feeds two
// internal connectors yet appears ONCE in the In set, and y1RetFan is the one declared boundary
// output that owns a real connector row. The other declared output, yRetFan (driven by swi.y),
// is addressable as a read alias and enumerated in `Topology.boundary_outputs`, but is
// deliberately absent from point rows (the _spec/18 D2 fence) and must not appear here.
#[test]
fn return_fan_airflow_tracking_point_sets_pin_the_boundary_fan_out_collapse() {
    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "multizone_vav_return_fan_airflow_tracking")
        })
        .expect("multizone_vav_return_fan_airflow_tracking fixture present");
    let (_, _bytes, engine) = load_fixture(&fixture);
    let prefix = "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking";
    let expected_in = expected_paths(
        prefix,
        &[
            "VAirSup_flow",
            "VAirRet_flow",
            "u1SupFan",
            "conP.u_s",
            "swi.u1",
            "swi.u3",
            "conErr.u2",
        ],
    );
    let expected_out = expected_paths(
        prefix,
        &[
            "conP.y", "swi.y", "conErr.y", "zerSpe.y", "difFlo.y", "y1RetFan",
        ],
    );
    assert_eq!(
        point_list_paths(&engine, PointDirection::In),
        expected_in,
        "return_fan_airflow_tracking In point set moved from the document reading"
    );
    assert_eq!(
        point_list_paths(&engine, PointDirection::Out),
        expected_out,
        "return_fan_airflow_tracking Out point set moved from the document reading"
    );
}

#[test]
fn external_inputs_preserve_shared_boundary_multiplicity() {
    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "cooling_only_controller")
        })
        .expect("cooling_only_controller fixture present");
    let (_, _bytes, engine) = load_fixture(&fixture);
    let external_inputs = engine.topology().external_inputs;
    let unique: BTreeSet<&String> = external_inputs.iter().collect();
    // A shared boundary IRI names one host point staged into several internal connectors; the
    // topology contract preserves that multiplicity rather than deduplicating paths.
    assert_eq!(
        external_inputs.len(),
        43,
        "external_inputs entry count moved"
    );
    assert_eq!(unique.len(), 14, "unique boundary path count moved");
}
