//! A `controllerOutput` read in place of `expected` (the owner's ruling of 2026-11-21, OBC
//! §12.7.3): a csv whose first header column is the time in seconds and whose every other header
//! is a device point of the mapping, with no hold.

use oce_verify::{SeriesValues, SourceRole, TrendSources};

use super::{loaded, point, read_trends, support, trend_file};

const MAPPING: &str = "m.json";

fn mapping() -> String {
    format!(
        "[{}, {}, {}]",
        point(
            ("yCooVal", Some("1"), "float"),
            ("yCoo", Some("percent"), "float")
        ),
        point(
            ("TSup", Some("K"), "float"),
            ("TSup", Some("degF"), "float")
        ),
        point(("yFan", None, "bool"), ("uFan", None, "float")),
    )
}

/// A spec whose one reference reads its expected outputs from `out.csv`, with `expected` given
/// or not.
fn spec(with_expected: bool) -> oce_verify::CommissioningSpec {
    let expected = if with_expected {
        r#", "expected": [{"path": "e.csv", "format": "csv", "timeColumn": "time", "columns": {}}]"#
    } else {
        ""
    };
    loaded(&format!(
        r#"{{"references": [{{"model": "M", "sequence": "s", "pointNameMapping": "{MAPPING}",
            "controllerOutput": "out.csv"{expected}}}]}}"#
    ))
}

fn read(bytes: &[u8]) -> Result<TrendSources, oce_verify::SourceError> {
    let mapping = mapping();
    read_trends(
        &spec(false),
        &[(MAPPING, mapping.as_bytes()), ("out.csv", bytes)],
    )
}

fn dump(read: &TrendSources) -> String {
    let mut out = String::from("# expected series of controller_output.csv\n");
    for source in &read.sources {
        out.push_str(&format!(
            "source\t{}\t{}\t{}\trows={}\n",
            source.role.as_str(),
            source.path,
            source.digest,
            source.rows
        ));
    }
    for unit in &read.unit_columns {
        out.push_str(&format!(
            "units\t{}\t{}\t{}\t{}\n",
            unit.device, unit.cdl, unit.from, unit.to
        ));
    }
    for series in &read.expected {
        let times: Vec<String> = series
            .times
            .iter()
            .map(|t| format!("{:016x}", t.to_bits()))
            .collect();
        let values: Vec<String> = match &series.values {
            SeriesValues::Real(v) => v.iter().map(|x| format!("{:016x}", x.to_bits())).collect(),
            SeriesValues::Integer(v) => v.iter().map(i32::to_string).collect(),
            SeriesValues::Boolean(v) => v.iter().map(bool::to_string).collect(),
        };
        out.push_str(&format!(
            "series\t{}\tdevice={}\n",
            series.name, series.device
        ));
        out.push_str(&format!(
            "times\t{}\nvalues\t{}\n",
            times.join("\t"),
            values.join("\t")
        ));
    }
    out
}

#[test]
fn a_controller_output_is_read_as_the_expected_source_and_matches_its_golden() {
    let read = read(&trend_file("controller_output.csv")).expect("reads");
    assert_eq!(read.sources.len(), 1);
    assert_eq!(read.sources[0].role, SourceRole::Expected);
    assert_eq!(read.sources[0].path, "out.csv");
    assert_eq!(read.sources[0].rows, 5);
    let names: Vec<&str> = read.expected.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["TSup", "yCooVal", "yFan"]);
    // No hold, and nothing is merged: the inputs are empty.
    assert!(read.inputs.columns().is_empty());
    let path = support::fixture("commissioning/trends/controller_output.golden.txt");
    support::compare_or_bless(&path, &dump(&read));
}

#[test]
fn beside_an_expected_the_controller_output_is_not_read_and_not_named() {
    let mapping = mapping();
    let read = read_trends(
        &spec(true),
        &[(MAPPING, mapping.as_bytes()), ("e.csv", b"time\n0\n")],
    )
    .expect("reads");
    assert_eq!(read.sources.len(), 1);
    assert_eq!(read.sources[0].path, "e.csv");
    let error = read_trends(
        &spec(true),
        &[
            (MAPPING, mapping.as_bytes()),
            ("e.csv", b"time\n0\n"),
            ("out.csv", b"time\n"),
        ],
    )
    .expect_err("refused");
    assert_eq!(
        (error.code(), error.file()),
        ("file-unlisted", Some("out.csv"))
    );
}

#[test]
fn the_first_header_is_the_time_whatever_it_is_named() {
    let first = read(b"t_s,yCoo\n0,10\n5,20\n").expect("reads");
    assert_eq!(first.expected[0].times, [0.0, 5.0]);
    // Quoted, with a doubled quote inside, after a byte-order mark and blank lines.
    let quoted = read(b"\xef\xbb\xbf\n\r\n\"ti\"\"me\",yCoo\n0,10\n").expect("reads");
    assert_eq!(quoted.expected[0].times, [0.0]);
    // Replay trims the spaces and tabs around a header name (issue #733), the first one
    // included, so OBC's `time, uHea` names `uHea`.
    let padded = read(b" time\t, yCoo \n  0,\t10\n").expect("reads");
    assert_eq!(padded.expected[0].times, [0.0]);
    assert_eq!(padded.expected[0].device, "yCoo");
    // A quoted first header keeps its spaces, in this reader as in replay's, and is the time.
    let quoted = read(b"\" time \",yCoo\n0,10\n").expect("reads");
    assert_eq!(quoted.expected[0].times, [0.0]);
    // A quoted name keeps its spaces, so it names no mapped point.
    let error = read(b"time,\" yCoo\"\n0,10\n").expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("unmapped-point", Some(" yCoo"))
    );
}

#[test]
fn the_time_is_seconds_only_and_every_header_must_be_a_mapped_point() {
    let error = read(b"time,yCoo\n2018-06-07T00:00:00Z,1\n").expect_err("refused");
    assert_eq!((error.code(), error.row()), ("bad-time", Some(1)));
    let error = read(b"time,yCoo,Other\n0,1,2\n").expect_err("refused");
    assert_eq!(
        (error.code(), error.file(), error.column()),
        ("unmapped-point", Some("out.csv"), Some("Other"))
    );
}

#[test]
fn an_empty_file_a_time_header_alone_and_a_repeated_header_are_refused() {
    for empty in [&b""[..], b"\n\r\n", b"\xef\xbb\xbf"] {
        assert_eq!(
            read(empty).expect_err("refused").code(),
            "no-header",
            "{empty:?}"
        );
    }
    assert_eq!(
        read(b"time\n0\n").expect_err("refused").code(),
        "no-point-columns"
    );
    assert_eq!(
        read(b"time,yCoo,yCoo\n0,1,1\n")
            .expect_err("refused")
            .code(),
        "duplicate-column"
    );
    assert_eq!(
        read(b"time,yCoo,time\n0,1,1\n")
            .expect_err("refused")
            .code(),
        "duplicate-column"
    );
    let long = format!("{},yCoo\n0,1\n", "t".repeat(257));
    let error = read(long.as_bytes()).expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("cell-too-long", Some("#1"))
    );
    assert!(read(format!("{},yCoo\n0,1\n", "t".repeat(256)).as_bytes()).is_ok());
    let error = read(b"\xff,yCoo\n0,1\n").expect_err("refused");
    assert_eq!(error.code(), "not-utf8");
}
