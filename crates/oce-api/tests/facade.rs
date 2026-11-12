//! The facade's integration tests, compiled as one test binary.
//!
//! Cargo builds each file under `tests/` as its own binary by default, and under the release
//! profile (`lto = "thin"`, `codegen-units = 1`) every binary links the whole dependency graph
//! again: that linking was most of the determinism subset's release build (GitHub #504). Here each
//! file is a module instead. It keeps its file name, so it still mirrors the source module it
//! exercises, and nextest reports its tests as `oce-api::facade <file>::<test>`.
//!
//! `Cargo.toml` sets `autotests = false` and lists every test binary with the reason it is one.
//! A few files stay their own binary because a gate step, a workflow or a documented command
//! selects them by name. The files that measure heap allocations are the modules of
//! `facade_allocation_census.rs` instead, which links `allocation-counter`'s counting global
//! allocator, so no test in either binary runs under a different allocator than it did alone.
//! With discovery off a new file under `tests/` would compile into nothing; the layout check at
//! the end of this file refuses that, and a file compiled more than once.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// Helpers that several modules share, declared once for the whole binary.
#[path = "support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/blob_image.rs"]
mod blob_image;
#[path = "support/blob_models.rs"]
mod blob_models;
#[path = "support/content_id_corpus.rs"]
mod content_id_corpus;
#[path = "support/frame_trace.rs"]
mod frame_trace;

mod array_extent;
mod authored_point_identity;
mod blob_fuzz;
mod blob_hostile_bytes;
mod blob_hostile_io_schedule;
mod blob_hostile_model;
mod blob_old_layout;
mod blob_refusal_precedence;
mod blob_semantic_seam;
mod boundary_output_attrs;
mod boundary_output_enumeration;
mod catalog_contract;
mod class_ports;
mod class_ports_import;
mod compatibility;
mod composite_import;
mod composition_identity;
mod content_id_behaviour;
mod content_id_cases;
mod content_id_changes;
mod content_id_permutations;
mod content_id_worked_examples;
mod context_expansion_identity;
mod core_crates_hash_free;
mod crossed_bounds;
mod cxf_namespace_and_label;
mod cxf_property_key_spellings;
mod declared_output_lookup;
mod derived_connector_types;
mod device_filtered_inventory;
mod double_driver_refusals;
mod element_name_spelling;
mod enumerations;
mod execute_frame;
mod execute_frame_preservation;
mod export_cxf;
mod finite_frame_inputs;
mod finite_time_step;
mod frame_assertions;
mod frame_driver_ownership;
mod frame_purity;
mod frame_refusals;
mod g36_air_economizer_high_limits;
mod g36_cooling_only_active_air_flow;
mod g36_cooling_only_alarms;
mod g36_cooling_only_controller;
mod g36_cooling_only_dampers;
mod g36_cooling_only_system_requests;
mod g36_economizer_controller;
mod g36_economizer_enable;
mod g36_economizer_limits_common;
mod g36_economizer_modulations_reliefs;
mod g36_economizer_modulations_return_fan;
mod g36_economizer_modulations_return_fan_relief_damper;
mod g36_freeze_protection;
mod g36_generic_time_suppression;
mod g36_outdoor_airflow_ahu;
mod g36_outdoor_airflow_sumzone;
mod g36_outdoor_airflow_title24_ahu;
mod g36_outdoor_airflow_title24_sumzone;
mod g36_plant_requests;
mod g36_reheat_overrides;
mod g36_relief_damper;
mod g36_relief_fan;
mod g36_relief_fan_group;
mod g36_return_fan_airflow_tracking;
mod g36_return_fan_direct_pressure;
mod g36_sequences;
mod g36_thermal_zones_control_loops;
mod g36_thermal_zones_zone_states;
mod g36_trim_and_respond;
mod g36_ventilation_zones_ashrae62_1_setpoints;
mod hash_order_independence;
mod integer_range_bindings;
mod library_rule_state_continuation;
mod load_cxf_trace;
mod native_pass_through;
mod non_finite_bounds;
mod non_finite_composite_parameters;
mod non_finite_input_policy;
mod non_finite_parameters;
mod prepare_frame_preservation;
mod release_fallback;
mod replay_matrix;
mod state_contract;
mod state_old_layout;
mod topology;
mod unused_boundary_input;

/// Every file Cargo would have discovered as a test target, keyed by its path under `tests/`
/// without the `.rs` (`name` for `tests/name.rs`, `name/main` for `tests/name/main.rs`).
fn discoverable_sources(tests: &Path) -> BTreeMap<String, String> {
    let mut sources = BTreeMap::new();
    for entry in fs::read_dir(tests).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let (key, file) = if path.is_dir() {
            (format!("{name}/main"), path.join("main.rs"))
        } else if let Some(stem) = name.strip_suffix(".rs") {
            (stem.to_owned(), path.clone())
        } else {
            continue;
        };
        if file.is_file() {
            sources.insert(key, fs::read_to_string(file).unwrap());
        }
    }
    sources
}

/// What is wrong with the layout: `manifest` is the package's `Cargo.toml` and `sources` the
/// discoverable files under `tests/`. A file must be exactly one `[[test]]` target or exactly one
/// module of a target, declared there without `#[path]` (a `#[path]` module is a helper).
fn layout_defects(manifest: &str, sources: &BTreeMap<String, String>) -> Vec<String> {
    let mut defects = Vec::new();
    if !manifest
        .lines()
        .any(|line| line.trim() == "autotests = false")
    {
        defects.push("Cargo.toml: autotests is not false, so Cargo builds every file alone".into());
    }
    let mut owners: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let targets = manifest.lines().filter_map(|line| {
        let path = line.trim().strip_prefix("path = \"tests/")?;
        path.strip_suffix(".rs\"")
    });
    for target in targets {
        owners
            .entry(target)
            .or_default()
            .push("a [[test]] target".into());
        let mut after_path_attribute = false;
        for line in sources.get(target).map_or("", String::as_str).lines() {
            let module = line
                .strip_prefix("mod ")
                .and_then(|rest| rest.strip_suffix(';'));
            if let Some(module) = module.filter(|_| !after_path_attribute) {
                owners
                    .entry(module)
                    .or_default()
                    .push(format!("a module of {target}"));
            }
            after_path_attribute = line.starts_with("#[path = ");
        }
    }
    for name in sources.keys() {
        match owners.get(name.as_str()) {
            None => defects.push(format!("tests/{name}.rs is never compiled")),
            Some(owner) if owner.len() > 1 => defects.push(format!(
                "tests/{name}.rs is compiled {} times: {}",
                owner.len(),
                owner.join(", ")
            )),
            Some(_) => {}
        }
    }
    defects
}

#[test]
fn every_test_file_is_compiled_into_exactly_one_binary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap();
    let sources = discoverable_sources(&manifest_dir.join("tests"));
    assert!(
        [
            "facade",
            "facade_allocation_census",
            "blob_goldens",
            "frame_purity"
        ]
        .iter()
        .all(|name| sources.contains_key(*name)),
        "the scan found the roots and a module, so it cannot pass vacuously: {:?}",
        sources.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "declare a new file as `mod <file>;` in tests/facade.rs (in \
         tests/facade_allocation_census.rs when it calls allocation_counter), or give it a \
         [[test]] target of its own in Cargo.toml, with the reason, when it is selected by name"
    );
}

#[test]
fn the_layout_check_refuses_an_orphan_a_double_build_and_discovery_left_on() {
    let manifest = "[package]\nautotests = false\n\n[[test]]\nname = \"root\"\n\
                    path = \"tests/root.rs\"\n\n[[test]]\nname = \"alone\"\n\
                    path = \"tests/alone.rs\"\n";
    let root = "#[path = \"support/helper.rs\"]\nmod helper;\nmod member;\n";
    let mut sources: BTreeMap<String, String> = ["root", "member", "alone"]
        .into_iter()
        .map(|name| (name.to_owned(), String::new()))
        .collect();
    sources.insert("root".into(), root.into());
    assert_eq!(
        layout_defects(manifest, &sources),
        Vec::<String>::new(),
        "positive control"
    );

    sources.insert("orphan".into(), String::new());
    sources.insert("nested/main".into(), String::new());
    sources.insert("root".into(), format!("{root}mod alone;\n"));
    assert_eq!(
        layout_defects(manifest, &sources),
        [
            "tests/alone.rs is compiled 2 times: a module of root, a [[test]] target",
            "tests/nested/main.rs is never compiled",
            "tests/orphan.rs is never compiled",
        ]
    );

    // A `#[path]` helper named like a top-level file does not compile that file.
    sources.insert("helper".into(), String::new());
    let defects = layout_defects(manifest, &sources);
    assert!(
        defects
            .iter()
            .any(|defect| defect == "tests/helper.rs is never compiled")
    );

    let discovered = manifest.replace("autotests = false\n", "");
    assert!(layout_defects(&discovered, &sources)[0].starts_with("Cargo.toml: autotests"));
}
