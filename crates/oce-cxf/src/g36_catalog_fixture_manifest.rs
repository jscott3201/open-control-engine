//! Fixture-only G36 catalog manifest validation helpers.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::bridge;
use crate::dto::{CxfDocument, Node};
use crate::g36_catalog_guard_data::{EXPECTED_REFERENCE_COMMIT, FixtureSource};
use crate::g36_catalog_guard_helpers::{array_field, local_name, str_field, string_set};

const G36_BRIDGE_PREFIX: &str = "ASHRAE.G36.";

pub(super) fn validate_fixture_records(
    catalog: &Value,
    prov: &Value,
    runtime_fixtures: &[FixtureSource],
    errors: &mut Vec<String>,
) {
    let entries = array_field(catalog, "fixture_only_sequences");
    let source_files = string_set(array_field(prov, "sequence_source_files"));
    let catalog_paths = entries
        .iter()
        .map(|entry| str_field(entry, "fixture").to_owned())
        .collect::<BTreeSet<_>>();
    let expected_paths = runtime_fixtures
        .iter()
        .map(|fixture| fixture.path.to_owned())
        .collect::<BTreeSet<_>>();
    if catalog_paths != expected_paths {
        errors.push("fixture-path-set-drift".to_owned());
    }

    for fixture in runtime_fixtures {
        let Some(entry) = entries
            .iter()
            .find(|entry| str_field(entry, "fixture") == fixture.path)
        else {
            errors.push(format!("fixture-missing-catalog-record: {}", fixture.path));
            continue;
        };
        if str_field(entry, "status") != "supported-fixture-only" {
            errors.push(format!("fixture-invalid-status: {}", fixture.name));
        }
        if str_field(entry, "source_mapping_status") != "source-reviewed-fragment" {
            errors.push(format!(
                "fixture-source-mapping-not-reviewed: {}",
                fixture.name
            ));
        }
        if str_field(entry, "canonical_g36_class_path_status")
            != "fragment-of-canonical-source-not-runtime-sequence"
        {
            errors.push(format!(
                "fixture-overclaims-canonical-source: {}",
                fixture.name
            ));
        }
        if str_field(entry, "source_commit") != EXPECTED_REFERENCE_COMMIT {
            errors.push(format!("fixture-source-commit-drift: {}", fixture.name));
        }
        for required in [
            "source_mapping",
            "supported_variant",
            "golden_trace",
            "determinism_provenance",
            "oracle_reference",
            "oracle_test",
            "oracle_explanation",
        ] {
            if str_field(entry, required).is_empty() {
                errors.push(format!(
                    "fixture-missing-evidence: {}:{required}",
                    fixture.name
                ));
            }
        }
        validate_fixture_source_list(entry, fixture, &source_files, errors);
        validate_nonempty_manifest_array(entry, "known_deferred_branches", fixture, errors);
        validate_nonempty_manifest_array(entry, "unsupported_variants", fixture, errors);
        if fixture.name == "vav_single_zone" && str_field(entry, "oracle_status") != "partial" {
            errors.push("vav-fixture-oracle-status-not-partial".to_owned());
        }
        if fixture.name != "vav_single_zone" && str_field(entry, "oracle_status") != "complete" {
            errors.push(format!(
                "fixture-oracle-status-not-complete: {}",
                fixture.name
            ));
        }
        if str_field(entry, "oracle_status") == "partial"
            && str_field(entry, "oracle_exception").is_empty()
        {
            errors.push(format!(
                "fixture-partial-oracle-missing-exception: {}",
                fixture.name
            ));
        }
    }
}

pub(super) fn validate_fixture_manifest(
    document: &CxfDocument,
    top: &Node,
    entry: &Value,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let by_id = document
        .graph
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();

    validate_ref_manifest(
        top.has_input.iter().map(|iri| iri.id.as_str()).collect(),
        entry,
        "required_inputs",
        "fixture-input-manifest-drift",
        fixture,
        errors,
    );
    validate_ref_manifest(
        top.has_output.iter().map(|iri| iri.id.as_str()).collect(),
        entry,
        "outputs",
        "fixture-output-manifest-drift",
        fixture,
        errors,
    );
    validate_ref_manifest(
        top.contains_block
            .iter()
            .map(|iri| iri.id.as_str())
            .collect(),
        entry,
        "active_child_components",
        "fixture-child-manifest-drift",
        fixture,
        errors,
    );

    let actual_parameters = document
        .graph
        .iter()
        .filter(|node| node.value.is_some())
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    validate_ref_manifest(
        actual_parameters,
        entry,
        "parameters",
        "fixture-parameter-manifest-drift",
        fixture,
        errors,
    );

    validate_point_kinds(document, entry, "required_inputs", fixture, errors);
    validate_point_kinds(document, entry, "outputs", fixture, errors);
    validate_child_components(&by_id, entry, fixture, errors);
}

fn validate_fixture_source_list(
    entry: &Value,
    fixture: &FixtureSource,
    source_files: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let Some(upstream_sources) = manifest_array(entry, "upstream_source_files", fixture, errors)
    else {
        return;
    };
    if upstream_sources.is_empty() {
        errors.push(format!("fixture-source-list-empty: {}", fixture.name));
    }
    for source in upstream_sources {
        let Some(path) = source.as_str() else {
            errors.push(format!("fixture-source-entry-not-string: {}", fixture.name));
            continue;
        };
        if !source_files.contains(path) {
            errors.push(format!(
                "fixture-source-not-in-provenance: {}:{path}",
                fixture.name
            ));
        }
    }
}

fn validate_nonempty_manifest_array(
    entry: &Value,
    field: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let Some(values) = manifest_array(entry, field, fixture, errors) else {
        return;
    };
    if values.is_empty() {
        errors.push(format!(
            "fixture-empty-manifest-array: {}:{field}",
            fixture.name
        ));
    }
}

fn validate_ref_manifest(
    actual: BTreeSet<&str>,
    entry: &Value,
    field: &str,
    error_tag: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let Some(manifest) = manifest_object_path_set(entry, field, fixture, errors) else {
        return;
    };
    let actual = actual
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if manifest != actual {
        errors.push(format!("{error_tag}: {}", fixture.name));
    }
}

fn validate_point_kinds(
    document: &CxfDocument,
    entry: &Value,
    field: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let Some(points) = manifest_array(entry, field, fixture, errors) else {
        return;
    };
    for point in points {
        let path = str_field(point, "path");
        let expected_kind = str_field(point, "kind");
        let Some(node) = document.graph.iter().find(|node| node.id == path) else {
            errors.push(format!(
                "fixture-point-missing-node: {}:{path}",
                fixture.name
            ));
            continue;
        };
        let Some(actual_kind) = connector_kind(node) else {
            errors.push(format!(
                "fixture-point-kind-missing: {}:{path}",
                fixture.name
            ));
            continue;
        };
        if expected_kind != actual_kind {
            errors.push(format!(
                "fixture-point-kind-drift: {}:{path}:{expected_kind}!={actual_kind}",
                fixture.name
            ));
        }
    }
}

fn validate_child_components(
    by_id: &BTreeMap<&str, &Node>,
    entry: &Value,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let Some(children) = manifest_array(entry, "active_child_components", fixture, errors) else {
        return;
    };
    for child in children {
        let path = str_field(child, "path");
        let Some(node) = by_id.get(path) else {
            errors.push(format!(
                "fixture-child-missing-node: {}:{path}",
                fixture.name
            ));
            continue;
        };
        let expected_name = str_field(child, "name");
        if expected_name != local_label(node) {
            errors.push(format!("fixture-child-name-drift: {}:{path}", fixture.name));
        }
        let expected_class = str_field(child, "class_path");
        let actual_class = node
            .r#type
            .as_ref()
            .and_then(|types| {
                types
                    .as_slice()
                    .iter()
                    .map(|type_iri| catalog_class_path(type_iri))
                    .find(|class_path| !class_path.starts_with("S231:"))
            })
            .unwrap_or_default();
        if expected_class != actual_class {
            errors.push(format!(
                "fixture-child-class-drift: {}:{path}:{expected_class}!={actual_class}",
                fixture.name
            ));
        }
    }
}

fn manifest_array<'a>(
    entry: &'a Value,
    field: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) -> Option<&'a Vec<Value>> {
    match entry.get(field).and_then(Value::as_array) {
        Some(values) => Some(values),
        None => {
            errors.push(format!(
                "fixture-missing-manifest-array: {}:{field}",
                fixture.name
            ));
            None
        }
    }
}

fn manifest_object_path_set(
    entry: &Value,
    field: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) -> Option<BTreeSet<String>> {
    let values = manifest_array(entry, field, fixture, errors)?;
    let mut paths = BTreeSet::new();
    for value in values {
        let path = str_field(value, "path");
        if path.is_empty() {
            errors.push(format!(
                "fixture-manifest-entry-missing-path: {}:{field}",
                fixture.name
            ));
        } else {
            paths.insert(path.to_owned());
        }
    }
    Some(paths)
}

fn connector_kind(node: &Node) -> Option<&'static str> {
    node.r#type
        .as_ref()
        .and_then(|types| {
            types.as_slice().iter().find_map(|kind| {
                if kind.ends_with("RealInput") || kind.ends_with("RealOutput") {
                    Some("Real")
                } else if kind.ends_with("IntegerInput") || kind.ends_with("IntegerOutput") {
                    Some("Integer")
                } else if kind.ends_with("BooleanInput") || kind.ends_with("BooleanOutput") {
                    Some("Boolean")
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            node.is_of_data_type.as_ref().and_then(|iri| {
                if iri.id.ends_with("Real") {
                    Some("Real")
                } else if iri.id.ends_with("Integer") {
                    Some("Integer")
                } else if iri.id.ends_with("Boolean") {
                    Some("Boolean")
                } else {
                    None
                }
            })
        })
}

fn local_label(node: &Node) -> &str {
    node.label
        .as_ref()
        .and_then(|labels| labels.as_slice().first().map(String::as_str))
        .unwrap_or_else(|| local_name(&node.id))
}

fn catalog_class_path(type_iri: &str) -> String {
    let class_path = bridge::class_path_of(type_iri);
    if class_path.starts_with(G36_BRIDGE_PREFIX) {
        format!("Buildings.Controls.OBC.{class_path}")
    } else {
        class_path.to_owned()
    }
}
