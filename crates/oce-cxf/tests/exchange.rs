//! `oce-cxf`'s integration tests that nothing selects by name, compiled as one test binary.
//!
//! Cargo builds each file under `tests/` as its own binary by default, and under the release
//! profile (`lto = "thin"`, `codegen-units = 1`) every binary links the whole dependency graph
//! again (GitHub #504). Here each file is a module instead. It keeps its file name, so it still
//! mirrors the source module it exercises, and nextest reports its tests as
//! `oce-cxf::exchange <file>::<test>`.
//!
//! `Cargo.toml` sets `autotests = false` and lists every test binary with the reason it is one.
//! Eleven files stay their own binary because a gate step, a document, a recorded command or a
//! test's own message selects them by name. With discovery off a new file under `tests/` would
//! compile into nothing: the layout test at the end of this file refuses that, and a file
//! compiled more than once.
//!
//! The helpers several modules share (`bless`, `render`) are declared once here, and each module
//! that used them says `use crate::bless;` or `use crate::render;` on the line that declared them,
//! because a second declaration of the same file trips `clippy::duplicate_mod`. The binaries that
//! stay apart keep their own declarations.
//!
//! `open_modelica_reliefs_windows_boundary` begins with `#![cfg(windows)]`, which now gates the
//! module instead of a crate: its test compiles into this binary on Windows only, and the Windows
//! cell of `release-gate.yml` runs this binary.
//!
//! The check behind that test is `binary_layout`, a helper adapted from `oce-author`'s
//! (GitHub #551), with its controls beside it.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// The layout check of this crate's test binaries (used at the end of this file).
mod binary_layout;

// Helpers that several modules share, declared once for the whole binary.
mod bless;
mod render;

mod export_cascade_naming;
mod export_declared_output_attrs;
mod export_deferral;
mod export_deferral_diagnostics;
mod export_external_inputs;
mod export_g36_roundtrip;
mod export_pass_through;
mod export_pass_through_deferral;
mod export_roundtrip;
mod export_single_assignment;
mod export_totality;
mod export_translation_software;
mod g36_single_driver_sweep;
mod g36_specialization;
mod ingest_totality;
mod open_modelica_reliefs_windows_boundary;
mod resolve_array_nodes;
mod resolve_boundary_multiplicity;
mod resolve_composite;
mod resolve_composite_orientation;
mod resolve_composite_rules;
mod resolve_connection_orientation;
mod resolve_connector_identity_errors;
mod resolve_context_expansion;
mod resolve_context_import;
mod resolve_context_scope;
mod resolve_context_term_names;
mod resolve_declaration_scope;
mod resolve_declaration_scope_emission;
mod resolve_declaration_strings;
mod resolve_errors;
mod resolve_extension_blocks;
mod resolve_external_input_order;
mod resolve_g36_air_economizer_high_limits;
mod resolve_g36_cooling_only_active_air_flow;
mod resolve_g36_cooling_only_alarms;
mod resolve_g36_cooling_only_controller;
mod resolve_g36_cooling_only_dampers;
mod resolve_g36_cooling_only_system_requests;
mod resolve_g36_economizer_controller;
mod resolve_g36_economizer_enable;
mod resolve_g36_economizer_limits_common;
mod resolve_g36_economizer_modulations_reliefs;
mod resolve_g36_economizer_modulations_return_fan;
mod resolve_g36_freeze_protection;
mod resolve_g36_generic_time_suppression;
mod resolve_g36_outdoor_airflow_ahu;
mod resolve_g36_outdoor_airflow_sumzone;
mod resolve_g36_outdoor_airflow_title24_ahu;
mod resolve_g36_outdoor_airflow_title24_sumzone;
mod resolve_g36_plant_requests;
mod resolve_g36_reheat_overrides;
mod resolve_g36_relief_damper;
mod resolve_g36_relief_fan;
mod resolve_g36_relief_fan_group;
mod resolve_g36_return_fan_airflow_tracking;
mod resolve_g36_return_fan_direct_pressure;
mod resolve_g36_thermal_zones_control_loops;
mod resolve_g36_thermal_zones_zone_states;
mod resolve_g36_ventilation_zones_ashrae62_1_setpoints;
mod resolve_instance_interface;
mod resolve_load_bounds;
mod resolve_nominal_attribute;
mod resolve_ordering;
mod resolve_param_precedence;
mod resolve_parameter_attrs;
mod resolve_pass_through;
mod resolve_port_binding;
mod resolve_property_keys;
mod resolve_reference_keys;
mod resolve_term_shapes;
mod resolve_type_near_miss;
mod resolve_typing_controls;
mod resolve_unused_boundary_input;
mod resolve_vocabulary_namespace;
mod resolve_vocabulary_terms;

#[test]
fn every_test_file_is_compiled_into_exactly_one_binary() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let mut sources = BTreeMap::new();
    binary_layout::read_sources(crate_dir, "tests", &mut sources);
    assert!(
        [
            "tests/exchange.rs",
            "tests/resolve_errors.rs",
            "tests/open_modelica_reliefs_windows_boundary.rs",
            "tests/fixture_port_order.rs",
            "tests/bless/mod.rs",
            "tests/binary_layout/mod.rs",
        ]
        .iter()
        .all(|name| sources.contains_key(*name)),
        "the scan found the root, modules, a binary of its own and a helper, so it cannot pass \
         vacuously: {:?}",
        sources.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        binary_layout::layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "declare a new file as `mod <file>;` in tests/exchange.rs (with `use crate::bless;` or \
         `use crate::render;` in the file for the shared helpers), or give it a [[test]] target of \
         its own in Cargo.toml, with the reason, when something selects it by name"
    );
}
