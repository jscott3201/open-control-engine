//! `oce-conformance`'s integration tests that nothing selects by name, compiled as one test
//! binary.
//!
//! Cargo builds each file under `tests/` as its own binary by default, and under the release
//! profile (`lto = "thin"`, `codegen-units = 1`) every binary links the whole dependency graph
//! again (GitHub #504). Here each file is a module instead. It keeps its file name, so it still
//! mirrors what it exercises, and nextest reports its tests as
//! `oce-conformance::conformance <file>::<test>`.
//!
//! `Cargo.toml` sets `autotests = false` and lists every test binary with the reason it is one.
//! Seven files stay their own binary because a gate step, a workflow or a recorded command selects
//! them by name: the strict-bit evidence tests (`strict_bits` and four `per_block_*` suites, which
//! change only when an issue says so), `blob_round_trip` and `open_modelica_nand`. With discovery
//! off a new file under `tests/` would compile into nothing: the layout test at the end of this
//! file refuses that, and a file compiled more than once.
//!
//! The helpers several modules share are declared once here, because a second declaration of the
//! same file trips `clippy::duplicate_mod`. Each module that declared one says `use crate::<name>;`
//! on the lines that declared it, with a comment, so no line of a module moved. Three modules
//! declared `block_harness` only so that their scoped OpenModelica helper could name
//! `crate::block_harness`; that line is now a comment. A helper that one module uses stays declared
//! in it, and the binaries that stay apart keep their own declarations. The one helper that named
//! its declaring file as the crate root, `g36_determinism/sequence_inputs.rs` (`use crate::*;` for
//! the point identities in `g36_determinism.rs`), now names it `super::*`.
//!
//! The check behind the layout test is `oce-cxf`'s (`crates/oce-cxf/tests/binary_layout/`,
//! GitHub #553), compiled here through `#[path]` instead of copied; its controls come with it.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// The layout check of a crate's test binaries, shared with `oce-cxf` (used at the end of this file).
#[path = "../../oce-cxf/tests/binary_layout/mod.rs"]
mod binary_layout;

// Helpers that several modules share, declared once for the whole binary.
mod block_harness;
#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;
#[path = "open_modelica_line/support.rs"]
mod scoped_open_modelica_line;
#[path = "g36_funnel_band/sequences.rs"]
mod sequences;
#[path = "g36_determinism/support.rs"]
mod support;

mod aligned_tolerance;
mod config;
mod csv;
mod driver;
mod driver_frames;
mod exact;
mod funnel;
mod g36_air_economizer_high_limits_determinism;
mod g36_air_economizer_high_limits_funnel;
mod g36_conformance_report;
mod g36_cooling_only_active_air_flow_determinism;
mod g36_cooling_only_active_air_flow_funnel;
mod g36_cooling_only_alarms_determinism;
mod g36_cooling_only_alarms_oracle;
mod g36_cooling_only_controller_determinism;
mod g36_cooling_only_controller_funnel;
mod g36_cooling_only_dampers_determinism;
mod g36_cooling_only_dampers_funnel;
mod g36_cooling_only_system_requests_determinism;
mod g36_cooling_only_system_requests_oracle;
mod g36_determinism;
mod g36_economizer_controller_determinism;
mod g36_economizer_controller_funnel;
mod g36_economizer_enable_determinism;
mod g36_economizer_enable_funnel;
mod g36_economizer_limits_common_determinism;
mod g36_economizer_limits_common_funnel;
mod g36_economizer_modulations_reliefs_determinism;
mod g36_economizer_modulations_reliefs_funnel;
mod g36_economizer_modulations_return_fan_determinism;
mod g36_economizer_modulations_return_fan_funnel;
mod g36_economizer_modulations_return_fan_relief_damper_determinism;
mod g36_economizer_modulations_return_fan_relief_damper_funnel;
mod g36_freeze_protection_determinism;
mod g36_freeze_protection_funnel;
mod g36_freeze_protection_heating_coil_continuous_time;
mod g36_funnel;
mod g36_funnel_band_routing;
mod g36_generic_time_suppression_determinism;
mod g36_generic_time_suppression_oracle;
mod g36_outdoor_airflow_sumzone_determinism;
mod g36_outdoor_airflow_sumzone_funnel;
mod g36_outdoor_airflow_title24_ahu_determinism;
mod g36_outdoor_airflow_title24_ahu_funnel;
mod g36_outdoor_airflow_title24_sumzone_determinism;
mod g36_outdoor_airflow_title24_sumzone_funnel;
mod g36_reference_frame_rate_labels;
mod g36_reference_provenance_labels;
mod g36_reheat_overrides_determinism;
mod g36_reheat_overrides_funnel;
mod g36_relief_damper_funnel;
mod g36_relief_fan_funnel;
mod g36_relief_fan_group_determinism;
mod g36_relief_fan_group_funnel;
mod g36_return_fan_airflow_tracking_funnel;
mod g36_return_fan_direct_pressure_funnel;
mod g36_thermal_zones_control_loops_determinism;
mod g36_thermal_zones_control_loops_funnel;
mod g36_thermal_zones_zone_states_determinism;
mod g36_thermal_zones_zone_states_oracle;
mod g36_trim_and_respond_determinism;
mod g36_trim_and_respond_reseed;
mod g36_ventilation_zones_ashrae62_1_setpoints_determinism;
mod g36_ventilation_zones_ashrae62_1_setpoints_funnel;
mod integer_reference_domain;
mod known_divergence;
mod mask;
mod open_modelica_line;
mod open_modelica_reliefs;
mod open_modelica_toggle;
mod per_block_conversions;
mod per_block_discrete;
mod per_block_integers;
mod per_block_logical;
mod per_block_reals;
mod per_block_reals_dynamics;
mod per_block_reals_matrix;
mod per_block_reals_vector_reductions;
mod per_block_routing_reals;
mod per_block_routing_typed;
mod per_block_source_timetable;
mod per_block_utilities_assert;
mod tiers;

/// The test binaries a workflow, a gate step or a recorded command selects by name. Each stays a
/// `[[test]]` target of its own: folded into this binary, `--test <name>` would fail, and the
/// Windows cell's `binary_id(..)` exclusion of the five strict-bit binaries would match nothing and
/// let them run there.
const SELECTED_BY_NAME: [&str; 7] = [
    "strict_bits",
    "per_block_reals_transcendental",
    "per_block_reals_sources_transcendental",
    "per_block_psychrometrics",
    "per_block_utilities",
    "blob_round_trip",
    "open_modelica_nand",
];

/// The names in [`SELECTED_BY_NAME`] that `manifest` does not give a `[[test]]` target of their own
/// at `tests/<name>.rs`.
fn targets_missing(manifest: &str) -> Vec<&'static str> {
    SELECTED_BY_NAME
        .into_iter()
        .filter(|name| {
            !manifest.contains(&format!(
                "[[test]]\nname = \"{name}\"\npath = \"tests/{name}.rs\"\n"
            ))
        })
        .collect()
}

#[test]
fn every_test_file_is_compiled_into_exactly_one_binary() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let mut sources = BTreeMap::new();
    binary_layout::read_sources(crate_dir, "tests", &mut sources);
    assert!(
        [
            "tests/conformance.rs",
            "tests/known_divergence.rs",
            "tests/strict_bits.rs",
            "tests/strict_bits/matrix.rs",
            "tests/block_harness/mod.rs",
            "tests/g36_determinism/support.rs",
        ]
        .iter()
        .all(|name| sources.contains_key(*name)),
        "the scan found the root, a module, a binary of its own and the helpers, so it cannot pass \
         vacuously: {:?}",
        sources.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        binary_layout::layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "declare a new file as `mod <file>;` in tests/conformance.rs (with `use crate::<helper>;` \
         in the file for a helper this root declares), or give it a [[test]] target of its own in \
         Cargo.toml, with the reason, when something selects it by name"
    );
}

#[test]
fn the_binaries_selected_by_name_stay_their_own_targets() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert_eq!(
        targets_missing(&manifest),
        Vec::<&str>::new(),
        "a workflow, gate step or recorded command selects each of these by name"
    );
    // Control: folding one of them into this binary is refused by name.
    let folded = manifest.replace(
        "[[test]]\nname = \"strict_bits\"\npath = \"tests/strict_bits.rs\"\n",
        "",
    );
    assert_eq!(targets_missing(&folded), ["strict_bits"]);
}
