//! What reading a trend costs, in allocations, never in wall-clock time, on a source shaped like
//! one Building 33 file (75,578 rows of `Seconds_Elapsed, Unix_Time, Value`, 5 s steps then
//! 600 s; commissioning note, "Building 33 data").
//!
//! Measured with `allocation-counter` on the reading thread. The ceilings carry headroom over
//! what was measured (in brackets). Reading a file is bounded per byte of it; the merged inputs
//! are not: they hold `frames × (8 + the columns' widths)` bytes, frames up to the rows of every
//! input source together and columns one per mapped input point, so many sources with times of
//! their own cost the square of their count. That product is what `max_source_bytes` caps, from
//! the counts, before anything is allocated (the last test).

use allocation_counter::AllocationInfo;
use oce_verify::{SpecInputs, TrendLimits, TrendSources};

use super::{point, trend_spec_with};

const ROWS: usize = 75_578;

/// A synthetic trend shaped like a Building 33 file, as `combiTimeTable` or csv text.
fn building_33_shaped(combi: bool) -> Vec<u8> {
    let mut text = if combi {
        format!(
            "#1\n# Recorded trend: synthetic\n# Columns: Seconds_Elapsed, Unix_Time, Value\ndouble OA_Temp({ROWS}, 3)\n"
        )
    } else {
        "Seconds_Elapsed,Unix_Time,Value\n".to_owned()
    };
    let mut t = 0.0_f64;
    for row in 0..ROWS {
        let value =
            50.389_996_000_000_004 + f64::from(u32::try_from(row % 97).expect("fits")) * 0.01;
        text.push_str(&format!("{t:?}, {:?}, {value:?}\n", 1_528_354_800.0 + t));
        t += if row < 75_000 { 5.0 } else { 600.0 };
    }
    if !combi {
        text = text.replace(", ", ",");
    }
    text.into_bytes()
}

fn read(combi: bool, bytes: &[u8]) -> (AllocationInfo, TrendSources) {
    let (format, time, column) = if combi {
        ("combiTimeTable", "1", "3")
    } else {
        ("csv", r#""Seconds_Elapsed""#, "Value")
    };
    let spec = trend_spec_with(
        Some("2018-06-07T00:00:00-07:00"),
        &format!(
            "[{}]",
            point(
                ("TOut", Some("K"), "float"),
                ("OA Temp", Some("degF"), "float")
            )
        ),
        &format!(
            r#""inputs": [{{"path": "OA_Temp", "format": "{format}", "timeColumn": {time},
                "columns": {{"{column}": "OA Temp"}}, "hold": {{"maxAge": 10}}}}]"#
        ),
    );
    let inputs = SpecInputs {
        cxf: Vec::new(),
        files: vec![("OA_Temp".to_owned(), bytes.to_vec())],
    };
    let mut read = None;
    let info = allocation_counter::measure(|| {
        read = Some(TrendSources::read(
            &spec,
            0,
            &inputs,
            &TrendLimits::default(),
        ));
    });
    (
        info,
        read.expect("ran").expect("the Building 33 shape reads"),
    )
}

#[test]
fn a_building_33_shaped_combi_time_table_loads_within_a_pinned_allocation_bound() {
    let bytes = building_33_shaped(true);
    let (info, read) = read(true, &bytes);
    assert_eq!(read.inputs.times().len(), ROWS);
    assert_eq!(read.sources[0].rows, ROWS as u64);
    let length = bytes.len() as u64;
    // [peak 2,421,059 bytes: 32.0 a row, 0.84 of the file; 54 allocations]. The peak is the
    // matrix's two kept columns beside their converted copies, then the merged frames.
    assert!(info.bytes_max <= 40 * ROWS as u64, "{info:?}");
    assert!(info.bytes_max <= length, "{info:?} for {length} bytes");
    assert!(info.count_total <= 64, "{info:?}");
}

#[test]
fn a_building_33_shaped_csv_loads_within_a_pinned_allocation_bound() {
    let bytes = building_33_shaped(false);
    let (info, read) = read(false, &bytes);
    assert_eq!(read.inputs.times().len(), ROWS);
    let length = bytes.len() as u64;
    // [peak 2,711,293 bytes: 35.9 a row, 0.99 of the file; 149 allocations]. The lists read
    // from replay's reader grow by doubling and are shrunk to their rows once the file is read.
    assert!(info.bytes_max <= 44 * ROWS as u64, "{info:?}");
    assert!(
        info.bytes_max <= 5 * length / 4,
        "{info:?} for {length} bytes"
    );
    assert!(info.count_total <= 192, "{info:?}");
}

/// `sources` input sources of `rows` rows each, at times no two sources share, one Real column
/// each: the shape whose merge costs the most, `sources × rows` frames of `sources` columns.
fn disjoint_sources(sources: usize, rows: usize) -> (oce_verify::CommissioningSpec, SpecInputs) {
    let points: Vec<String> = (0..sources)
        .map(|s| {
            point(
                (&format!("c{s}"), None, "float"),
                (&format!("d{s}"), None, "float"),
            )
        })
        .collect();
    let declared: Vec<String> = (0..sources)
        .map(|s| {
            format!(
                r#"{{"path": "s{s}.csv", "format": "csv", "timeColumn": "t",
                    "columns": {{"v": "d{s}"}}}}"#
            )
        })
        .collect();
    let spec = trend_spec_with(
        None,
        &format!("[{}]", points.join(", ")),
        &format!(r#""inputs": [{}]"#, declared.join(", ")),
    );
    let files = (0..sources)
        .map(|s| {
            let mut text = String::from("t,v\n");
            for r in 0..rows {
                text.push_str(&format!("{},1\n", r * sources + s));
            }
            (format!("s{s}.csv"), text.into_bytes())
        })
        .collect();
    (
        spec,
        SpecInputs {
            cxf: Vec::new(),
            files,
        },
    )
}

#[test]
fn the_merged_inputs_cost_frames_times_columns_and_are_refused_past_the_cap_before_allocating() {
    let (sources, rows) = (64, 100);
    let (spec, inputs) = disjoint_sources(sources, rows);
    let input: u64 = inputs
        .files
        .iter()
        .map(|(_, bytes)| bytes.len() as u64)
        .sum();
    let frames = (sources * rows) as u64;
    // Each frame holds its time and one Real per source: 8 + 8 × 64 bytes.
    let merged = frames * (8 + 8 * sources as u64);
    let measure = |limits: &TrendLimits| {
        let mut result = None;
        let info = allocation_counter::measure(|| {
            result = Some(TrendSources::read(&spec, 0, &inputs, limits));
        });
        (info, result.expect("ran"))
    };
    let (info, read) = measure(&TrendLimits::default());
    let read = read.expect("reads");
    assert_eq!(read.inputs.times().len() as u64, frames);
    assert_eq!(read.inputs.columns().len(), sources);
    // The merge is quadratic in sources with times of their own: 3.3 MB held for 44 KB of files.
    assert!(merged > 60 * input, "{merged} {input}");
    // [peak 3,482,698 bytes: the 3,328,000 of the merged columns and the reads beside them]
    assert!(info.bytes_max >= merged, "{info:?}");
    assert!(
        info.bytes_max <= merged + 48 * input + 64 * 1024,
        "{info:?}"
    );
    // One byte under what the merge needs: refused from the counts, the merged columns never
    // allocated, so the peak is the reads' [143,256 bytes].
    let capped = TrendLimits {
        replay: oce_replay::ReplayLimits {
            max_source_bytes: merged - 1,
            ..oce_replay::ReplayLimits::default()
        },
        ..TrendLimits::default()
    };
    let (info, refused) = measure(&capped);
    assert_eq!(refused.expect_err("refused").code(), "too-many-bytes");
    assert!(info.bytes_max <= 48 * input + 64 * 1024, "{info:?}");
    let at = TrendLimits {
        replay: oce_replay::ReplayLimits {
            max_source_bytes: merged,
            ..oce_replay::ReplayLimits::default()
        },
        ..TrendLimits::default()
    };
    assert!(measure(&at).1.is_ok());
}
