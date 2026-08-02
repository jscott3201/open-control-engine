//! Export RT-2 over the **whole** G36 fixture corpus, not a hand-picked slice of it.
//!
//! Eight of the 46 fixtures used to be covered by eight individually-named tests. The other 38
//! were exercised by nothing on the export side, and — this is the part that made the gap
//! self-perpetuating — nothing tied the directory to any test, so a 47th fixture would have landed
//! uncovered with no signal. `EXPECTED_G36_FIXTURES` plus the listing assertion closes that: a new
//! fixture is either added to the list and swept, or CI goes red naming it.
//!
//! Two classes, distinguished at runtime by whether the export deferred anything, because the
//! right assertion differs:
//!
//! * **Enum-free** — the deferral pre-pass defers nothing, so the whole graph exports and RT-2
//!   holds for the FULL graph: `render(import(fixture)) == render(import(export(...)))` bit-exact,
//!   plus the second-order byte fixpoint.
//! * **Deferring** — the fixture carries `Value::Enum` *parameters*, so some blocks are omitted by
//!   design and whole-graph render equality is false on purpose (see `export`'s rustdoc). RT-2
//!   holds over the **survivor cone**, which is what [`assert_survivor_cone_rt2`] asserts.
//!
//! Note what the split is NOT. No fixture is outside the export subset: 46/46 import with zero
//! diagnostics and 46/46 export. There are zero enum-typed *connectors* in the entire corpus —
//! every deferral here comes from a parameter — and composites are flattened and arrays
//! pre-flattened before export ever sees the graph.
//!
//! The `import_ok` / `render` helpers are duplicated from `export_roundtrip.rs` rather than
//! factored out: each integration test file is its own binary, and the helpers are too thin to
//! justify a shared module. `render` itself IS shared via `mod render;`.

mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use oce_cxf::{ExportReport, ResolveOptions, export_with_report, import_cxf};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};
use render::{render, render_attrs, render_value};

/// Every `*.jsonld` under `tests/fixtures/g36/`, sorted. Checked in so that adding a fixture is a
/// deliberate, reviewed act rather than a silent expansion — and so that adding one without
/// touching this list fails loudly instead of going uncovered.
const EXPECTED_G36_FIXTURES: &[&str] = &[
    "ahu_economizer.jsonld",
    "ahu_supply_air_temp_reset.jsonld",
    "cooling_only_active_air_flow.jsonld",
    "cooling_only_alarms.jsonld",
    "cooling_only_controller.jsonld",
    "cooling_only_dampers.jsonld",
    "cooling_only_system_requests.jsonld",
    "generic_air_economizer_high_limits_ashrae_differential.jsonld",
    "generic_air_economizer_high_limits_ashrae_fixed_18.jsonld",
    "generic_air_economizer_high_limits_ashrae_fixed_21.jsonld",
    "generic_air_economizer_high_limits_ashrae_fixed_24.jsonld",
    "generic_air_economizer_high_limits_title24_differential_offset_0.jsonld",
    "generic_air_economizer_high_limits_title24_differential_offset_1.jsonld",
    "generic_air_economizer_high_limits_title24_differential_offset_2.jsonld",
    "generic_air_economizer_high_limits_title24_differential_offset_3.jsonld",
    "generic_air_economizer_high_limits_title24_fixed_21.jsonld",
    "generic_air_economizer_high_limits_title24_fixed_22.jsonld",
    "generic_air_economizer_high_limits_title24_fixed_23.jsonld",
    "generic_air_economizer_high_limits_title24_fixed_24.jsonld",
    "generic_time_suppression.jsonld",
    "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld",
    "multizone_vav_economizer_enable.jsonld",
    "multizone_vav_economizer_limits_common.jsonld",
    "multizone_vav_economizer_modulations_reliefs.jsonld",
    "multizone_vav_economizer_modulations_return_fan.jsonld",
    "multizone_vav_economizer_modulations_return_fan_relief_damper.jsonld",
    "multizone_vav_freeze_protection.jsonld",
    "multizone_vav_outdoor_airflow_ahu.jsonld",
    "multizone_vav_outdoor_airflow_sumzone.jsonld",
    "multizone_vav_outdoor_airflow_title24_ahu.jsonld",
    "multizone_vav_outdoor_airflow_title24_sumzone.jsonld",
    "multizone_vav_plant_requests.jsonld",
    "multizone_vav_relief_damper.jsonld",
    "multizone_vav_relief_fan.jsonld",
    "multizone_vav_relief_fan_group.jsonld",
    "multizone_vav_return_fan_airflow_tracking.jsonld",
    "multizone_vav_return_fan_direct_pressure.jsonld",
    "multizone_vav_supply_fan.jsonld",
    "multizone_vav_supply_signals.jsonld",
    "multizone_vav_supply_temperature.jsonld",
    "reheat_overrides.jsonld",
    "thermal_zones_control_loops.jsonld",
    "thermal_zones_zone_states.jsonld",
    "trim_and_respond_have_hol_false.jsonld",
    "vav_single_zone.jsonld",
    "ventilation_zones_ashrae62_1_setpoints.jsonld",
];

fn g36_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/g36")
}

/// Deterministically sorted `*.jsonld` listing of the G36 fixture directory.
fn sorted_fixture_listing() -> Vec<String> {
    let dir = g36_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("G36 fixture dir {} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("readable directory entry").file_name())
        .filter_map(|name| {
            let name = name.to_str().expect("UTF-8 fixture name").to_owned();
            name.ends_with(".jsonld").then_some(name)
        })
        .collect();
    names.sort();
    names
}

fn import_ok(fixture: &str, bytes: &[u8]) -> ModelGraph {
    let (g, report) = import_cxf(bytes, &ResolveOptions::default())
        .unwrap_or_else(|e| panic!("`{fixture}` must resolve without error: {e:?}"));
    assert!(
        report.is_empty(),
        "`{fixture}` expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn export_ok(fixture: &str, g: &ModelGraph) -> ExportReport {
    export_with_report(g)
        .unwrap_or_else(|e| panic!("`{fixture}` must be inside the export subset: {e:?}"))
}

/// The blocks a report says were deferred, read off the warning subjects.
fn deferred_subjects(report: &ExportReport) -> BTreeSet<String> {
    report
        .warnings
        .iter()
        .map(|d| {
            d.subject
                .as_deref()
                .expect("a deferral warning always names its block")
                .to_owned()
        })
        .collect()
}

/// One block's identity, keyed by `instance_iri`, in a form that survives id renumbering.
///
/// Deliberately NOT a subset of what `render` compares: class, every parameter by bit-exact value,
/// and every port's §7.4.1 attributes by bit-exact rendering, in port-list order. Dropping the
/// attributes would let a defect that corrupted a port's `unit` or `min` — while preserving class,
/// parameters and wiring — pass this check unnoticed.
fn block_profiles(g: &ModelGraph) -> BTreeMap<String, String> {
    g.blocks
        .iter()
        .filter_map(|b| {
            let iri = b.instance_iri.as_deref()?.to_owned();
            let params: Vec<String> = b
                .params
                .values
                .iter()
                .map(|(n, v)| format!("{n}={}", render_value(v)))
                .collect();
            let ports = |ids: &[oce_model::ConnectorId]| -> Vec<String> {
                ids.iter()
                    .map(|cid| {
                        let c = &g.connectors[cid.0 as usize];
                        // `iri` is in the key on purpose. It is the connector's own boundary
                        // identity, and without it a mutation that collapsed three distinct
                        // external-input IRIs onto one, or renamed the canonical input ids,
                        // reached the emitted bytes with the whole suite green.
                        format!(
                            "{:?}/iri={:?}/{}",
                            c.value_type,
                            c.iri.as_deref(),
                            render_attrs(&c.attrs)
                        )
                    })
                    .collect()
            };
            Some((
                iri,
                format!(
                    "class={} params=[{}] in=[{}] out=[{}]",
                    b.class_iri,
                    params.join(","),
                    ports(&b.inputs).join(","),
                    ports(&b.outputs).join(","),
                ),
            ))
        })
        .collect()
}

/// Every connection as `<owner-iri>.out<k> -> <owner-iri>.in<k>`, sorted, skipping any edge with
/// an endpoint owned by a block in `excluded`. Naming endpoints by owner IRI plus port position is
/// what makes the set comparable across a re-import that renumbers every `BlockId`/`ConnectorId`.
///
/// The exclusion is by resolved OWNER, never by substring on the rendered edge. G36 instance IRIs
/// are hierarchical and share prefixes — `…actAirSet.intEqu`, `…intEqu1`, `…intEqu2` all coexist —
/// so a `contains()` filter drops edges belonging to `intEqu1` whenever `intEqu` defers, and the
/// survivor edge set comes out short. That produced a failure that looked like an exporter defect
/// and was entirely an artifact of the check.
fn edge_set(g: &ModelGraph, excluded: &BTreeSet<String>) -> BTreeSet<String> {
    let endpoint = |cid: oce_model::ConnectorId| -> Option<(String, String)> {
        let c = g.connectors.get(cid.0 as usize)?;
        let b = g.blocks.get(c.block.0 as usize)?;
        let list = if c.dir == Dir::In {
            &b.inputs
        } else {
            &b.outputs
        };
        let k = list.iter().position(|x| *x == cid)?;
        let dir = if c.dir == Dir::In { "in" } else { "out" };
        let owner = b.instance_iri.as_deref()?.to_owned();
        let port = format!("{owner}.{dir}{k}");
        Some((owner, port))
    };
    g.connections
        .iter()
        .filter_map(|c| {
            let (from_owner, from_port) = endpoint(c.from)?;
            let (to_owner, to_port) = endpoint(c.to)?;
            if excluded.contains(&from_owner) || excluded.contains(&to_owner) {
                return None;
            }
            Some(format!("{from_port} -> {to_port}"))
        })
        .collect()
}

/// RT-2 for a fixture that deferred nothing: the whole graph round-trips bit-exactly.
fn assert_full_graph_rt2(fixture: &str, g1: &ModelGraph, report: &ExportReport) {
    let g2 = import_ok(fixture, &report.bytes);
    assert_eq!(render(g1), render(&g2), "`{fixture}` full-graph RT-2");
    assert_eq!(
        export_ok(fixture, &g2).bytes,
        report.bytes,
        "`{fixture}` second-order byte fixpoint"
    );
}

/// The deferred set recomputed from the graph alone, as the LEAST fixpoint reachable from blocks
/// that carry enumeration content: seed with the enum roots, then repeatedly add any block having a
/// driven input whose every driver is already in the set.
///
/// "Least fixpoint from the roots" is the whole point, and a weaker phrasing was the defect this
/// replaced. Checking each deferred block against the FINAL set only asks whether the set is
/// internally consistent, and a feedback CYCLE satisfies that vacuously: two blocks driving each
/// other are each "justified" by the other, so an arbitrary cycle can be added to the deferred set
/// and every block in it still passes — while tracing back to no enum root at all. A review
/// reproduced exactly that on `multizone_vav_supply_fan`, growing deferrals from 2 to 10 and
/// dropping a block from the emitted bytes, with the whole suite green. Growing outward from the
/// roots cannot admit a cycle that no root reaches.
fn least_deferred_closure(g: &ModelGraph) -> BTreeSet<String> {
    let iri_of = |bi: usize| -> Option<&str> { g.blocks.get(bi)?.instance_iri.as_deref() };
    let owner_of = |cid: oce_model::ConnectorId| -> Option<&str> {
        let c = g.connectors.get(cid.0 as usize)?;
        iri_of(c.block.0 as usize)
    };

    let mut set: BTreeSet<String> = g
        .blocks
        .iter()
        .filter(|b| {
            b.params
                .values
                .iter()
                .any(|(_, v)| matches!(v, Value::Enum { .. }))
                || b.inputs.iter().chain(b.outputs.iter()).any(|cid| {
                    g.connectors
                        .get(cid.0 as usize)
                        .is_some_and(|c| matches!(c.value_type, ValueType::Enum(_)))
                })
        })
        .filter_map(|b| b.instance_iri.as_deref().map(str::to_owned))
        .collect();

    let mut grew = true;
    while grew {
        grew = false;
        for b in &g.blocks {
            let Some(iri) = b.instance_iri.as_deref() else {
                continue;
            };
            if set.contains(iri) {
                continue;
            }
            let cascades = b.inputs.iter().any(|cid| {
                let drivers: Vec<&oce_model::Connection> =
                    g.connections.iter().filter(|c| c.to == *cid).collect();
                !drivers.is_empty()
                    && drivers
                        .iter()
                        .all(|c| owner_of(c.from).is_some_and(|o| set.contains(o)))
            });
            if cascades {
                set.insert(iri.to_owned());
                grew = true;
            }
        }
    }
    set
}

/// The warned set must equal the least closure exactly — no block omitted without a licence, and
/// none kept that the cascade should have reached.
///
/// This is the one assertion here that does not trust the warnings at all. Everything else derives
/// the survivor cone FROM the warning subjects, which makes those checks self-consistent under any
/// bug that merely shrinks the cone: a smaller cone still round-trips perfectly, so the profile,
/// edge and no-leak assertions all pass while blocks silently vanish from the document.
///
/// Equality, not containment, because the two directions catch different bugs: a superset is
/// over-deferral (the document loses blocks it should carry) and a subset is under-deferral (enum
/// content, or something downstream of it, survives into bytes that cannot re-import).
fn assert_deferred_set_is_the_least_closure(
    fixture: &str,
    g: &ModelGraph,
    deferred: &BTreeSet<String>,
) {
    let expected = least_deferred_closure(g);
    let extra: Vec<&String> = deferred.difference(&expected).collect();
    let missing: Vec<&String> = expected.difference(deferred).collect();
    assert!(
        extra.is_empty() && missing.is_empty(),
        "`{fixture}` deferred set is not the least closure from the enum roots.\n  \
         deferred without a licence (over-deferral, blocks lost from the document): {extra:?}\n  \
         reachable from an enum root but NOT deferred (under-deferral): {missing:?}"
    );
}

/// The graph's `external_inputs` named by owner IRI and port position, skipping entries on an
/// excluded block. The document's whole external interface: omit it and a mutation that collapses
/// several boundary entries into one, or drops one entirely, round-trips unnoticed — `edge_set`
/// covers block-to-block wiring only, and a boundary input is by definition undriven by any block.
fn boundary_set(g: &ModelGraph, excluded: &BTreeSet<String>) -> BTreeSet<String> {
    g.external_inputs
        .iter()
        .filter_map(|cid| {
            let c = g.connectors.get(cid.0 as usize)?;
            let b = g.blocks.get(c.block.0 as usize)?;
            let owner = b.instance_iri.as_deref()?;
            if excluded.contains(owner) {
                return None;
            }
            let k = b.inputs.iter().position(|x| x == cid)?;
            Some(format!("{owner}.in{k} <- {:?}", c.iri.as_deref()))
        })
        .collect()
}

/// RT-2 for a fixture whose export deferred blocks: equality over the survivor cone.
///
/// Whole-graph `render` equality is false here by design, so this asserts the four properties that
/// together say "the document is exactly the survivor cone and nothing else":
/// no deferred block leaked in, every survivor kept its full profile, the survivor-to-survivor
/// wiring is identical, and the bytes are a fixpoint.
fn assert_survivor_cone_rt2(fixture: &str, g1: &ModelGraph, report: &ExportReport) {
    let deferred = deferred_subjects(report);
    assert_deferred_set_is_the_least_closure(fixture, g1, &deferred);
    let g2 = import_ok(fixture, &report.bytes);

    let all_iris: BTreeSet<&str> = g1
        .blocks
        .iter()
        .filter_map(|b| b.instance_iri.as_deref())
        .collect();
    for subject in &deferred {
        assert!(
            all_iris.contains(subject.as_str()),
            "`{fixture}` deferred `{subject}`, which is not a block in the input graph"
        );
    }

    let survivors: BTreeMap<String, String> = block_profiles(g1)
        .into_iter()
        .filter(|(iri, _)| !deferred.contains(iri))
        .collect();
    assert!(
        !survivors.is_empty(),
        "`{fixture}` deferred everything; total deferral is a rejection, not a warning"
    );
    assert!(
        !deferred.is_empty(),
        "`{fixture}` reached the deferring branch with an empty deferred set"
    );

    let emitted = block_profiles(&g2);
    for iri in emitted.keys() {
        assert!(
            !deferred.contains(iri),
            "`{fixture}` leaked the deferred block `{iri}` into the emitted document"
        );
    }
    assert_eq!(
        emitted, survivors,
        "`{fixture}` survivor profiles must round-trip exactly"
    );

    assert_eq!(
        edge_set(&g2, &BTreeSet::new()),
        edge_set(g1, &deferred),
        "`{fixture}` survivor-to-survivor wiring must round-trip exactly"
    );

    assert_eq!(
        boundary_set(&g2, &BTreeSet::new()),
        boundary_set(g1, &deferred),
        "`{fixture}` survivor external inputs must round-trip exactly"
    );

    assert_eq!(
        export_ok(fixture, &g2).bytes,
        report.bytes,
        "`{fixture}` second-order byte fixpoint"
    );
    assert!(
        g2.blocks.iter().all(|b| {
            b.params
                .values
                .iter()
                .all(|(_, v)| !matches!(v, Value::Enum { .. }))
        }),
        "`{fixture}` re-imported document still carries an enum parameter"
    );
}

#[test]
fn the_least_closure_excludes_a_cycle_no_enum_root_reaches() {
    // The property that makes `least_deferred_closure` an oracle rather than a restatement, on a
    // hand-built graph because the G36 corpus has no cycle reachable this way.
    //
    // `spinA` and `spinB` drive each other and nothing else feeds them, so under a rule phrased as
    // "every deferred block has an input whose drivers are all deferred" the PAIR is
    // self-supporting: each is licensed by the other, and the set {spinA, spinB} passes while
    // tracing back to no enumeration anywhere. Growing outward from the enum roots cannot reach
    // them, which is why the closure is computed in that direction and compared for EQUALITY.
    let iri = |n: &str| Some(Arc::from(format!("http://example.org#Cyc.{n}").as_str()));
    let b = |id: u32, name: &str, ins: &[u32], outs: &[u32], params: Vec<(Arc<str>, Value)>| {
        BlockInstance {
            id: BlockId(id),
            class_iri: Arc::from("CDL.Reals.Abs"),
            inputs: ins.iter().copied().map(ConnectorId).collect(),
            outputs: outs.iter().copied().map(ConnectorId).collect(),
            params: ParamTable { values: params },
            decl_order: id,
            instance_iri: iri(name),
        }
    };
    let c = |id: u32, owner: u32, dir: Dir| {
        Connector::new(ConnectorId(id), BlockId(owner), dir, ValueType::Real, 0)
    };
    let enum_param: Vec<(Arc<str>, Value)> = vec![(
        Arc::from("controllerType"),
        Value::Enum {
            class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1,
        },
    )];

    let g = ModelGraph {
        blocks: vec![
            b(0, "enumroot", &[], &[0], enum_param),
            b(1, "downstream", &[1], &[2], vec![]),
            b(2, "spinA", &[3], &[4], vec![]),
            b(3, "spinB", &[5], &[6], vec![]),
        ],
        connectors: vec![
            c(0, 0, Dir::Out),
            c(1, 1, Dir::In),
            c(2, 1, Dir::Out),
            c(3, 2, Dir::In),
            c(4, 2, Dir::Out),
            c(5, 3, Dir::In),
            c(6, 3, Dir::Out),
        ],
        connections: vec![
            Connection {
                from: ConnectorId(0),
                to: ConnectorId(1),
            }, // enumroot -> downstream
            Connection {
                from: ConnectorId(4),
                to: ConnectorId(5),
            }, // spinA -> spinB
            Connection {
                from: ConnectorId(6),
                to: ConnectorId(3),
            }, // spinB -> spinA
        ],
        ..ModelGraph::new()
    };

    let closure = least_deferred_closure(&g);
    assert!(
        closure.contains("http://example.org#Cyc.enumroot")
            && closure.contains("http://example.org#Cyc.downstream"),
        "the enum root and its downstream cone are in the closure: {closure:?}"
    );
    assert!(
        !closure.contains("http://example.org#Cyc.spinA")
            && !closure.contains("http://example.org#Cyc.spinB"),
        "a self-supporting cycle no enum root reaches must NOT be in the closure: {closure:?}"
    );
}

#[test]
fn the_fixture_directory_and_the_swept_list_stay_one_to_one() {
    let on_disk = sorted_fixture_listing();
    let expected: Vec<String> = EXPECTED_G36_FIXTURES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

    let missing: Vec<&String> = expected.iter().filter(|n| !on_disk.contains(n)).collect();
    let unswept: Vec<&String> = on_disk.iter().filter(|n| !expected.contains(n)).collect();
    assert!(
        missing.is_empty() && unswept.is_empty(),
        "the G36 corpus on disk and EXPECTED_G36_FIXTURES must stay one-to-one.\n  \
         listed but absent from disk: {missing:?}\n  \
         on disk but NOT swept by this file (add them to EXPECTED_G36_FIXTURES): {unswept:?}"
    );
}

#[test]
fn every_g36_fixture_reaches_its_rt2_fixpoint() {
    let mut enum_free = 0usize;
    let mut deferring = 0usize;

    for fixture in EXPECTED_G36_FIXTURES {
        let bytes = std::fs::read(g36_dir().join(fixture))
            .unwrap_or_else(|e| panic!("`{fixture}` must be readable: {e}"));
        let g1 = import_ok(fixture, &bytes);
        let report = export_ok(fixture, &g1);

        // The corpus carries no enum-typed connectors at all; every deferral is on the parameter
        // axis. Pinned because it is what makes the two-branch split exhaustive.
        assert!(
            g1.connectors
                .iter()
                .all(|c| !matches!(c.value_type, ValueType::Enum(_))),
            "`{fixture}` grew an enum-typed connector; the deferral split needs revisiting"
        );

        if report.warnings.is_empty() {
            enum_free += 1;
            assert_full_graph_rt2(fixture, &g1, &report);
        } else {
            deferring += 1;
            assert_survivor_cone_rt2(fixture, &g1, &report);
        }
    }

    assert_eq!(
        enum_free + deferring,
        EXPECTED_G36_FIXTURES.len(),
        "every fixture takes exactly one branch"
    );
    assert!(
        enum_free > 0 && deferring > 0,
        "both branches must be exercised, got enum-free={enum_free} deferring={deferring}"
    );
}

#[test]
fn the_largest_deferring_fixtures_defer_exactly_the_expected_block_counts() {
    // The survivor-cone assertions are all derived from the warning subjects, so a cascade bug
    // that over-defers satisfies them vacuously — a smaller cone is still self-consistent. These
    // two graphs are where that would hurt most: an over-reaching cascade could delete a third of
    // the document while every other assertion in this file stayed green. The counts are the
    // tripwire.
    for (fixture, expected) in [
        ("cooling_only_controller.jsonld", 83usize),
        ("multizone_vav_relief_fan_group.jsonld", 63usize),
    ] {
        let bytes = std::fs::read(g36_dir().join(fixture)).expect("fixture is readable");
        let g1 = import_ok(fixture, &bytes);
        let report = export_ok(fixture, &g1);
        assert_eq!(
            deferred_subjects(&report).len(),
            expected,
            "`{fixture}` deferred-block count changed; a cascade that grew or shrank here needs \
             an explicit review, not a re-blessed number"
        );
    }
}
