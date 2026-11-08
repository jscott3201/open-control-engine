//! Reseed guard for the standalone G36 Generic.TrimAndRespond Tier-A reference.
//!
//! The pinned `Generic/TrimAndRespond.mo` documentation says the setpoint is `iniSet` while the
//! device is off and the reset starts `delTim` after device start. Its graph does something
//! narrower: `swi2` feeds `iniSet` into `uniDel.u` while the device is off, and the sampled
//! `UnitDelay` captures it only at a `samplePeriod` instant. A device-off window holding no
//! sample instant therefore never reseeds the chain, and after the re-armed TrueDelay the reset
//! resumes from the stale pre-trip value (GitHub issue #354).
//!
//! OCE executes the `.mo` graph, and the golden-gen reference models the same graph, so the engine
//! comparison in `g36_funnel` cannot tell the two rules apart on its own: a reference "fixed" to
//! reseed on any off edge would simply fail to match. This guard pins the rule in the reference
//! itself. It requires a short trip (no sample instant inside) and a boundary trip (a single off
//! tick on a sample instant), and checks that each release is one trim from the seed the graph
//! gives and not from the seed the other rule would give.

use std::path::{Path, PathBuf};

use oce_conformance::CombiTimeTable;
use serde_json::Value;

const REFERENCE: &str =
    "tools/golden-gen/goldens/G36/trim_and_respond_have_hol_false/reference.csv";
const FIXTURE: &str = "crates/oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld";
const FIXTURE_ROOT: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false";

/// The standalone fixture's literal parameters, read from the CXF fixture the engine loads.
#[derive(Clone, Copy, Debug)]
struct Parameters {
    /// `iniSet`.
    ini_set: f64,
    /// `triAmo`.
    trim_amount: f64,
    /// `numIgnReq`.
    ignored_requests: f64,
    /// `samplePeriod`, s.
    sample_period: f64,
    /// `delTim + samplePeriod`, s: the TrueDelay re-armed by each restart.
    true_delay: f64,
}

fn repository_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn parameters() -> Parameters {
    let path = repository_path(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {} failed: {err}", path.display()));
    let fixture: Value = serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("parse {} failed: {err}", path.display()));
    let graph = fixture["@graph"].as_array().expect("fixture graph");
    let literal = |name: &str| -> f64 {
        let id = format!("{FIXTURE_ROOT}.{name}");
        let node = graph
            .iter()
            .find(|node| node["@id"] == id.as_str())
            .unwrap_or_else(|| panic!("fixture has no parameter {id}"));
        let text = match &node["S231:value"] {
            Value::String(text) => text.as_str(),
            value => value["@value"]
                .as_str()
                .unwrap_or_else(|| panic!("{id} has no literal value")),
        };
        text.parse()
            .unwrap_or_else(|err| panic!("{id} literal {text} is not numeric: {err}"))
    };
    let sample_period = literal("samplePeriod");
    Parameters {
        ini_set: literal("iniSet"),
        trim_amount: literal("triAmo"),
        ignored_requests: literal("numIgnReq"),
        sample_period,
        true_delay: literal("delTim") + sample_period,
    }
}

/// One device-off window and what happened after the restart that ended it.
#[derive(Debug)]
struct Trip {
    stop_time: f64,
    off_rows: usize,
    holds_sample_instant: bool,
    /// Setpoint at the last sample instant before the stop: what the UnitDelay holds unless
    /// `swi2` is captured inside the off window.
    pre_trip_seed: f64,
    release_time: f64,
    release_value: f64,
}

fn reference() -> CombiTimeTable {
    let path = repository_path(REFERENCE);
    CombiTimeTable::read(&path)
        .unwrap_or_else(|err| panic!("read {} failed: {err}", path.display()))
}

fn column(reference: &CombiTimeTable, label: &str) -> (Vec<f64>, Vec<f64>) {
    let index = reference
        .col_names
        .as_ref()
        .and_then(|names| names.iter().position(|name| name == label))
        .unwrap_or_else(|| panic!("reference carries column {label}"));
    let column = reference
        .column_owned(index)
        .unwrap_or_else(|err| panic!("column {label} reads: {err}"));
    (column.x, column.y)
}

/// Every device-off window after the reset first left `iniSet`, with its re-armed release.
///
/// Two assumptions make the reference readable this way:
///
/// - The sample grid is anchored at t = 0: the Sampler and UnitDelay start at the first tick
///   (`t0 = 0` in this run, asserted below), so an instant is a multiple of `samplePeriod`.
/// - `pre_trip_seed` reads the published setpoint at the last instant before the stop. At that
///   instant the device is on and the TrueDelay has elapsed (both checked), so `swi` publishes
///   the same clamped value `swi2` stages into the UnitDelay. For a short trip no later instant
///   precedes the restart, so that staged value is what the UnitDelay still holds. When the stop
///   tick is itself an instant, `swi2` stages `iniSet` there instead; the seed is then only the
///   value the other rule would give, which is how the boundary-trip test uses it.
fn trips(parameters: Parameters) -> Vec<Trip> {
    let reference = reference();
    let (time, setpoint) = column(&reference, "setpoint");
    let (_, status) = column(&reference, "device_status");
    let (_, requests) = column(&reference, "request_count");
    assert_eq!(
        time.first().map(|t| t.to_bits()),
        Some(0.0f64.to_bits()),
        "the sample grid is anchored at the first tick, which must be t = 0"
    );
    let is_sample_instant = |t: f64| (t / parameters.sample_period).fract() == 0.0;
    let on = |row: usize| status[row] != 0.0;
    let reset_active = setpoint
        .iter()
        .position(|y| y.to_bits() != parameters.ini_set.to_bits())
        .expect("the reset leaves iniSet");

    let mut trips = Vec::new();
    let mut row = reset_active;
    while row < time.len() {
        if on(row) {
            row += 1;
            continue;
        }
        let stop = row;
        let restart = (stop..time.len())
            .find(|&row| on(row))
            .unwrap_or_else(|| panic!("device off at {} s never restarts", time[stop]));
        let release = (restart..time.len())
            .find(|&row| time[row] >= time[restart] + parameters.true_delay)
            .unwrap_or_else(|| panic!("run ends before the {} s restart releases", time[restart]));
        for held in stop..release {
            assert_eq!(
                setpoint[held].to_bits(),
                parameters.ini_set.to_bits(),
                "setpoint at {} s must be iniSet from the {} s stop through the re-armed delay",
                time[held],
                time[stop]
            );
        }
        let last_instant = (0..stop)
            .rev()
            .find(|&row| is_sample_instant(time[row]))
            .expect("a sample instant precedes the stop");
        assert!(
            on(last_instant)
                && setpoint[last_instant].to_bits() != parameters.ini_set.to_bits()
                && (last_instant..stop).all(&on),
            "the device must be on and released at the {} s instant before the {} s stop",
            time[last_instant],
            time[stop]
        );
        let sampled_at_release = (0..=release)
            .rev()
            .find(|&row| is_sample_instant(time[row]))
            .expect("a sample instant precedes the release");
        assert!(
            requests[sampled_at_release] <= parameters.ignored_requests,
            "the {} s release must be a pure trim so its seed is readable",
            time[release]
        );
        trips.push(Trip {
            stop_time: time[stop],
            off_rows: restart - stop,
            holds_sample_instant: (stop..restart).any(|row| is_sample_instant(time[row])),
            pre_trip_seed: setpoint[last_instant],
            release_time: time[release],
            release_value: setpoint[release],
        });
        row = restart;
    }
    trips
}

/// Assert the release is one trim from `seed` and not one trim from `other_seed`.
fn assert_released_from(trip: &Trip, trim_amount: f64, seed: f64, other_seed: f64, rule: &str) {
    assert_eq!(
        trip.release_value.to_bits(),
        (seed + trim_amount).to_bits(),
        "{rule}: the {} s release after the {} s stop must resume from {seed}, got {}",
        trip.release_time,
        trip.stop_time,
        trip.release_value
    );
    assert_ne!(
        trip.release_value.to_bits(),
        (other_seed + trim_amount).to_bits(),
        "{rule}: the {} s trip cannot tell the two seeding rules apart",
        trip.stop_time
    );
}

#[test]
fn short_trip_without_a_sample_instant_resumes_from_the_stale_value() {
    let parameters = parameters();
    let trips = trips(parameters);
    let short = trips
        .iter()
        .filter(|trip| !trip.holds_sample_instant)
        .collect::<Vec<_>>();
    assert!(
        !short.is_empty(),
        "the reference needs a device-off window strictly between sample instants: {trips:?}"
    );
    for trip in short {
        assert_released_from(
            trip,
            parameters.trim_amount,
            trip.pre_trip_seed,
            parameters.ini_set,
            "TrimAndRespond.mo reseeds only through the sampled UnitDelay",
        );
    }
}

#[test]
fn trip_holding_a_sample_instant_resumes_from_ini_set() {
    let parameters = parameters();
    let trips = trips(parameters);
    let reseeding = trips
        .iter()
        .filter(|trip| trip.holds_sample_instant)
        .collect::<Vec<_>>();
    assert!(
        reseeding.iter().any(|trip| trip.off_rows == 1),
        "the reference needs a boundary trip whose single off tick is a sample instant: {trips:?}"
    );
    for trip in reseeding {
        assert_released_from(
            trip,
            parameters.trim_amount,
            parameters.ini_set,
            trip.pre_trip_seed,
            "swi2 stages iniSet at a sample instant inside the off window",
        );
    }
}
