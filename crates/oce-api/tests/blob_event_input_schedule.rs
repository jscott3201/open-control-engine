//! A blob whose schedule fires an event input's reader before its producer is refused
//! (issue #768).
//!
//! The reset inputs of `PIDWithReset` (PI, PID), `IntegratorWithReset` and `OnCounter` feed
//! through since their events apply at the frame of the event, so the decoder's emit-before check
//! requires each such input's producer to fire first. A blob compiled before that change could
//! order a reset PID ahead of its trigger's producer, and running it would read last frame's
//! trigger; the decoder refuses it instead.
//!
//! `fixtures/blob_trigger_scheduled_after_its_reader.oceb` is the `thermal_zones_control_loops`
//! golden exactly as `development` encoded it before the change (at 84db2d76, path
//! `crates/oce-api/tests/fixtures/blob/thermal_zones_control_loops.oceb`; SHA-256
//! `18e46d3202b6fa65a8ac6facaab1f17526eea61bb7182e42780c682d3adcc6b7`). It is never
//! re-blessed. Its header carries this build's engine version (0.6.0 was already the version
//! after the v0.5.0 tag, so standing rule 10's bump was done and the version does not separate
//! the two meanings); the schedule check is what refuses it.
//!
//! `fixtures/blob_triggered_sampler_pre_trigger_false.oceb` is the `generic_time_suppression`
//! golden as `development` encoded it before the change (at 56c81a85, path
//! `crates/oce-api/tests/fixtures/blob/generic_time_suppression.oceb`; SHA-256
//! `09bb813f8f4b30afd954f43a47810f86432399f58fc439ab3643384db7dfd2d3`). Its `TriggeredSampler`
//! carries the initial previous-trigger word `false`; this build seeds it `true` (no event at frame
//! 0), so the initial-state CRC differs and the decoder refuses the blob with
//! `InitialStateMismatch`. It is never re-blessed.

use oce_api::BlobError;
use oce_graph::blob::{ENGINE_VERSION, ScheduleFault, decode};

const OLD_SCHEDULE: &[u8] = include_bytes!("fixtures/blob_trigger_scheduled_after_its_reader.oceb");
const CURRENT: &[u8] = include_bytes!("fixtures/blob/thermal_zones_control_loops.oceb");
const SAMPLER_SEEDED_FALSE: &[u8] =
    include_bytes!("fixtures/blob_triggered_sampler_pre_trigger_false.oceb");
const SAMPLER_CURRENT: &[u8] = include_bytes!("fixtures/blob/generic_time_suppression.oceb");

#[test]
fn a_blob_scheduling_a_reset_pid_before_its_trigger_producer_is_refused() {
    match decode(OLD_SCHEDULE) {
        Err(BlobError::InvalidSchedule {
            position: 0,
            fault: ScheduleFault::EmitBefore,
        }) => {}
        Err(other) => panic!("expected InvalidSchedule EmitBefore at position 0, got {other:?}"),
        Ok(_) => panic!("a blob with the trigger producer after its reader was accepted"),
    }
}

#[test]
fn the_refused_blob_differs_from_the_current_golden_only_by_its_schedule() {
    // Same length, same content id and engine version; the summaries agree on everything but
    // the decoder's verdict, so the refusal comes from the schedule and nothing else.
    assert_eq!(OLD_SCHEDULE.len(), CURRENT.len());
    assert_ne!(OLD_SCHEDULE, CURRENT);
    let old = oce_api::inspect_blob(OLD_SCHEDULE).expect("the old blob is well formed");
    let current = oce_api::inspect_blob(CURRENT).expect("the golden is well formed");
    assert_eq!(old.content_id, current.content_id);
    assert_eq!(old.engine_version, current.engine_version);
    assert_eq!(
        (
            old.engine_version.major,
            old.engine_version.minor,
            old.engine_version.patch
        ),
        (
            ENGINE_VERSION.major,
            ENGINE_VERSION.minor,
            ENGINE_VERSION.patch
        )
    );
    assert_eq!(old.classes, current.classes);
    assert_eq!(old.instances, current.instances);
    assert_eq!(
        old.decode_refusal,
        Some(BlobError::InvalidSchedule {
            position: 0,
            fault: ScheduleFault::EmitBefore,
        })
    );
    assert_eq!(current.decode_refusal, None);
    decode(CURRENT).expect("the re-blessed golden decodes");
}

#[test]
fn a_blob_whose_triggered_sampler_starts_with_pre_trigger_false_is_refused() {
    match decode(SAMPLER_SEEDED_FALSE) {
        Err(BlobError::InitialStateMismatch { .. }) => {}
        Err(other) => panic!("expected InitialStateMismatch, got {other:?}"),
        Ok(_) => panic!("a TriggeredSampler seeded with pre(trigger) = false was accepted"),
    }
    assert_eq!(SAMPLER_SEEDED_FALSE.len(), SAMPLER_CURRENT.len());
    let old = oce_api::inspect_blob(SAMPLER_SEEDED_FALSE).expect("well formed");
    let current = oce_api::inspect_blob(SAMPLER_CURRENT).expect("well formed");
    assert_eq!(old.content_id, current.content_id);
    assert!(matches!(
        old.decode_refusal,
        Some(BlobError::InitialStateMismatch { .. })
    ));
    assert_eq!(current.decode_refusal, None);
}
