//! Every bound the trend sources are read under, through the public reader: the limit admitted,
//! one past it refused with its own code, never truncated.

use oce_replay::ReplayLimits;
use oce_verify::{SpecInputs, TrendLimits, TrendSources};

use super::{loaded, point, trend_spec_with};

fn mapping(count: usize) -> String {
    let points: Vec<String> = (0..count)
        .map(|i| {
            point(
                (&format!("c{i}"), None, "float"),
                (&format!("d{i}"), None, "float"),
            )
        })
        .collect();
    format!("[{}]", points.join(", "))
}

/// Read `files` for `spec` under the replay caps `replay`.
fn read_under(
    spec: &oce_verify::CommissioningSpec,
    files: &[(&str, Vec<u8>)],
    replay: ReplayLimits,
) -> Result<TrendSources, oce_verify::SourceError> {
    let inputs = SpecInputs {
        cxf: Vec::new(),
        files: files
            .iter()
            .map(|(p, b)| ((*p).to_owned(), b.clone()))
            .collect(),
    };
    let limits = TrendLimits {
        replay,
        ..TrendLimits::default()
    };
    TrendSources::read(spec, 0, &inputs, &limits)
}

fn code(result: Result<TrendSources, oce_verify::SourceError>) -> &'static str {
    result.map_or_else(|error| error.code(), |_| "read")
}

/// One csv input source mapping `columns` device points.
fn csv_spec(columns: usize) -> oce_verify::CommissioningSpec {
    let map: Vec<String> = (0..columns).map(|i| format!(r#""d{i}": "d{i}""#)).collect();
    trend_spec_with(
        None,
        &mapping(columns),
        &format!(
            r#""inputs": [{{"path": "a.csv", "format": "csv", "timeColumn": "time",
                "columns": {{{}}}}}]"#,
            map.join(", ")
        ),
    )
}

/// A csv of `columns` point columns of `cell` and an unmapped column of `pad` bytes, so the file
/// can be larger than the values kept from it.
fn csv_file(columns: usize, rows: usize, cell: &str, pad: usize) -> Vec<u8> {
    let header: Vec<String> = (0..columns).map(|i| format!("d{i}")).collect();
    let mut text = format!("time,{},pad\n", header.join(","));
    for row in 0..rows {
        text.push_str(&format!(
            "{row},{},{}\n",
            vec![cell; columns].join(","),
            "x".repeat(pad)
        ));
    }
    text.into_bytes()
}

#[test]
fn a_csv_source_is_held_to_each_replay_cap() {
    let max = ReplayLimits::default();
    let spec = csv_spec(2);
    let file = csv_file(2, 3, "1", 40);
    let read = |replay| code(read_under(&spec, &[("a.csv", file.clone())], replay));
    assert_eq!(read(max), "read");
    assert_eq!(read(ReplayLimits { max_rows: 3, ..max }), "read");
    assert_eq!(read(ReplayLimits { max_rows: 2, ..max }), "too-many-rows");
    assert_eq!(
        read(ReplayLimits {
            max_columns: 4,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_columns: 3,
            ..max
        }),
        "too-many-columns"
    );
    // The file's bytes: three rows keep 72 bytes of values, fewer than the file's.
    let length = file.len() as u64;
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: length,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: length - 1,
            ..max
        }),
        "too-many-bytes"
    );
    // The values kept: three rows of a time and two Reals, 72 bytes.
    let small = csv_file(2, 3, "1", 0);
    let read_small = |replay| code(read_under(&spec, &[("a.csv", small.clone())], replay));
    assert!(small.len() < 72);
    assert_eq!(
        read_small(ReplayLimits {
            max_source_bytes: 72,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read_small(ReplayLimits {
            max_source_bytes: 71,
            ..max
        }),
        "too-many-bytes"
    );
    let wide = csv_file(2, 1, "1.25", 0);
    let read = |replay| code(read_under(&spec, &[("a.csv", wide.clone())], replay));
    assert_eq!(
        read(ReplayLimits {
            max_cell_bytes: 4,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_cell_bytes: 3,
            ..max
        }),
        "cell-too-long"
    );
}

#[test]
fn a_combi_time_table_source_is_held_to_each_replay_cap() {
    let spec = trend_spec_with(
        None,
        &mapping(1),
        r#""inputs": [{"path": "a.mos", "format": "combiTimeTable", "timeColumn": 1,
            "columns": {"2": "d0"}}]"#,
    );
    // A third column, not kept, makes the file larger than the 48 bytes of values kept.
    let file = b"#1\ndouble t(3,3)\n0 1 0.000000000000001\n1 2.25 0.000000000000001\n2 3 0.000000000000001\n".to_vec();
    let max = ReplayLimits::default();
    let read = |replay| code(read_under(&spec, &[("a.mos", file.clone())], replay));
    assert_eq!(read(ReplayLimits { max_rows: 3, ..max }), "read");
    assert_eq!(read(ReplayLimits { max_rows: 2, ..max }), "too-many-rows");
    assert_eq!(
        read(ReplayLimits {
            max_columns: 3,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_columns: 2,
            ..max
        }),
        "too-many-columns"
    );
    assert_eq!(
        read(ReplayLimits {
            max_cell_bytes: 17,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_cell_bytes: 16,
            ..max
        }),
        "cell-too-long"
    );
    let length = file.len() as u64;
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: length,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: length - 1,
            ..max
        }),
        "too-many-bytes"
    );
}

#[test]
fn the_merged_inputs_are_held_to_the_frame_column_and_byte_caps() {
    // Two sources of three rows at distinct times: six frames, each within three rows.
    let spec = trend_spec_with(
        None,
        &mapping(2),
        r#""inputs": [
          {"path": "a.csv", "format": "csv", "timeColumn": "time", "columns": {"d0": "d0"}},
          {"path": "b.csv", "format": "csv", "timeColumn": "time", "columns": {"d1": "d1"}}]"#,
    );
    let files = [
        ("a.csv", b"time,d0\n0,1\n2,1\n4,1\n".to_vec()),
        ("b.csv", b"time,d1\n1,1\n3,1\n5,1\n".to_vec()),
    ];
    let max = ReplayLimits::default();
    let read = |replay| code(read_under(&spec, &files, replay));
    assert_eq!(read(ReplayLimits { max_rows: 6, ..max }), "read");
    assert_eq!(read(ReplayLimits { max_rows: 5, ..max }), "too-many-rows");
    // Two point columns and the time: three merged columns, two per file.
    assert_eq!(
        read(ReplayLimits {
            max_columns: 3,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_columns: 2,
            ..max
        }),
        "too-many-columns"
    );
    // Six frames of a time and two Reals hold 144 bytes; each file's values far fewer.
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: 144,
            ..max
        }),
        "read"
    );
    assert_eq!(
        read(ReplayLimits {
            max_source_bytes: 143,
            ..max
        }),
        "too-many-bytes"
    );
}

#[test]
fn the_sources_of_a_reference_are_held_to_their_bound() {
    let sources = |count: usize| {
        let one = r#"{"path": "a.csv", "format": "csv", "timeColumn": "time", "columns": {}}"#;
        loaded(&format!(
            r#"{{"references": [{{"model": "M", "sequence": "s", "pointNameMapping": [],
                "expected": [{}]}}]}}"#,
            vec![one; count].join(", ")
        ))
    };
    let files = [("a.csv", b"time\n0\n".to_vec())];
    let max = ReplayLimits::default();
    assert_eq!(code(read_under(&sources(256), &files, max)), "read");
    let error = read_under(&sources(257), &files, max).expect_err("refused");
    assert_eq!(error.code(), "too-many-sources");
    // Lowered, and never raised.
    let inputs = SpecInputs {
        cxf: Vec::new(),
        files: vec![("a.csv".to_owned(), b"time\n0\n".to_vec())],
    };
    let lowered = TrendLimits {
        max_sources: 3,
        ..TrendLimits::default()
    };
    assert!(TrendSources::read(&sources(3), 0, &inputs, &lowered).is_ok());
    let error = TrendSources::read(&sources(4), 0, &inputs, &lowered).expect_err("refused");
    assert_eq!(error.code(), "too-many-sources");
    let raised = TrendLimits {
        max_sources: 1_000,
        ..TrendLimits::default()
    };
    let error = TrendSources::read(&sources(257), 0, &inputs, &raised).expect_err("refused");
    assert_eq!(error.code(), "too-many-sources");
}

#[test]
fn the_values_one_source_keeps_are_held_to_the_byte_cap_in_every_role() {
    // An `expected` source is never merged, so its own check is the one that refuses.
    let spec = trend_spec_with(
        None,
        &mapping(2),
        r#""expected": [{"path": "a.csv", "format": "csv", "timeColumn": "time",
            "columns": {"d0": "d0", "d1": "d1"}}]"#,
    );
    let file = csv_file(2, 3, "1", 0);
    // Three rows of a time and two Reals keep 72 bytes, more than the file's.
    assert!((file.len() as u64) < 71);
    let max = ReplayLimits::default();
    let read = |bytes| {
        code(read_under(
            &spec,
            &[("a.csv", file.clone())],
            ReplayLimits {
                max_source_bytes: bytes,
                ..max
            },
        ))
    };
    assert_eq!(read(72), "read");
    assert_eq!(read(71), "too-many-bytes");
    // A combiTimeTable keeps 8 bytes a value of the matrix, and 9 for an Integer once read.
    let ints = format!("[{}]", point(("n", None, "int"), ("N", None, "int")));
    let spec = trend_spec_with(
        None,
        &ints,
        r#""expected": [{"path": "a.mos", "format": "combiTimeTable", "timeColumn": 1,
            "columns": {"2": "N"}}]"#,
    );
    let file = b"#1\ndouble t(2,2)\n0 1\n1 2\n".to_vec();
    let read = |bytes| {
        code(read_under(
            &spec,
            &[("a.mos", file.clone())],
            ReplayLimits {
                max_source_bytes: bytes,
                ..max
            },
        ))
    };
    // The matrix's two kept columns hold 32 bytes; the converted rows 2 × (8 + 9) = 34.
    assert_eq!(read(34), "read");
    assert_eq!(read(33), "too-many-bytes");
}
