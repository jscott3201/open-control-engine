//! What reading a trend costs, in allocations, never in wall-clock time, on a source shaped like
//! one Building 33 file (75,578 rows of `Seconds_Elapsed, Unix_Time, Value`, 5 s steps then
//! 600 s; commissioning note, "Building 33 data").
//!
//! Measured with `allocation-counter` on the reading thread. The ceilings carry headroom over
//! what was measured (in brackets); each is stated per byte of the file, so a file at replay's
//! byte cap (1 GiB) is bounded by the same factor.

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
    // [peak 3,308,983 bytes: 43.8 a row, 1.20 of the file; 146 allocations]. The columns read
    // from replay's reader grow by doubling, so their capacity is up to twice their length.
    assert!(info.bytes_max <= 56 * ROWS as u64, "{info:?}");
    assert!(
        info.bytes_max <= 3 * length / 2,
        "{info:?} for {length} bytes"
    );
    assert!(info.count_total <= 192, "{info:?}");
}
