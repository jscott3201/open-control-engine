//! The three-file trend fixture (`tests/fixtures/commissioning/trends/`): a `combiTimeTable` with
//! a decoy matrix, a `csv` of seconds and a `csv` of instants against a `-07:00` origin, merged
//! into nine frames with two at `t = 10`.
//!
//! - **Golden**: the merged columns (times, cells, validity flags), the source records and the
//!   unit rows, written out and compared byte for byte (`merged.golden.txt`, blessed with
//!   `OCE_BLESS_VERIFY=1`).
//! - **Oracle**: `trends_oracle.txt`, written by `derive_trends.py`, which shares no code with the
//!   crate: the digests, the merged cells, and what replay feeds the engine at each frame under
//!   the sources' holds. The last is checked against a real replay of the `signals` subject.
//! - **Determinism**: the order of `files` and of the input sources' declarations changes no
//!   byte of the result.

use oce_replay::{Column, FrameStatus, TraceColumn};
use oce_verify::{SeriesValues, TrendLimits, TrendSources};

use super::support;
use super::{TREND_FILES, trend_file, trend_inputs, trend_spec};

fn read_fixture(names: &[&str]) -> TrendSources {
    TrendSources::read(
        &trend_spec(),
        0,
        &trend_inputs(names),
        &TrendLimits::default(),
    )
    .unwrap_or_else(|error| panic!("the fixture reads: {} {error}", error.code()))
}

fn hex(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

/// One merged cell as the oracle writes it: Real bits, an integer, `true`/`false`, or
/// `missing`.
fn oracle_cell(column: &Column, frame: usize) -> String {
    match column {
        Column::Real(values) if values[frame].is_nan() => "missing".to_owned(),
        Column::Real(values) => hex(values[frame]),
        Column::Integer { values, valid } => match valid {
            Some(valid) if !valid[frame] => "missing".to_owned(),
            _ => values[frame].to_string(),
        },
        Column::Boolean { values, valid } => match valid {
            Some(valid) if !valid[frame] => "missing".to_owned(),
            _ => values[frame].to_string(),
        },
        Column::Enum { .. } => unreachable!("the reader builds no Enum column"),
    }
}

/// The whole result as text: every bit of the merged inputs and every record.
fn dump(read: &TrendSources) -> String {
    let inputs = &read.inputs;
    let mut out =
        String::from("# merged inputs of tests/fixtures/commissioning/trends/spec.json\n");
    out.push_str(&format!(
        "origin\t{}\n",
        inputs.origin().map_or("none", |origin| origin.as_str())
    ));
    out.push_str(&format!("frames\t{}\n", inputs.times().len()));
    let times: Vec<String> = inputs.times().iter().map(|t| hex(*t)).collect();
    out.push_str(&format!("times\t{}\n", times.join("\t")));
    for column in inputs.columns() {
        let hold = column.hold_max_age_s().map_or("none".to_owned(), hex);
        out.push_str(&format!(
            "column\t{}\tdevice={}\tkind={:?}\thold={hold}\n",
            column.name(),
            column.device(),
            column.kind()
        ));
        let cells: Vec<String> = match column.values() {
            Column::Real(values) => values.iter().map(|x| hex(*x)).collect(),
            Column::Integer { values, valid } => values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{v}/{}", valid.as_ref().map_or(1, |f| u8::from(f[i]))))
                .collect(),
            Column::Boolean { values, valid } => values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    format!(
                        "{}/{}",
                        u8::from(*v),
                        valid.as_ref().map_or(1, |f| u8::from(f[i]))
                    )
                })
                .collect(),
            Column::Enum { .. } => unreachable!("the reader builds no Enum column"),
        };
        out.push_str(&format!("cells\t{}\n", cells.join("\t")));
    }
    for spec in inputs.column_specs() {
        out.push_str(&format!("spec\t{}\t{:?}\n", spec.name, spec.kind));
    }
    for (target, policy) in inputs.overrides() {
        out.push_str(&format!("override\t{target}\t{policy:?}\n"));
    }
    for (column, target) in inputs.column_map().entries {
        out.push_str(&format!("map\t{column}\t{target}\n"));
    }
    for source in &read.sources {
        out.push_str(&format!(
            "source\t{}\t{}\ttable={}\t{}\trows={}\n",
            source.role.as_str(),
            source.path,
            source.table.as_deref().unwrap_or("none"),
            source.digest,
            source.rows
        ));
    }
    if let Some(mapping) = &read.mapping {
        out.push_str(&format!("mapping\t{}\t{}\n", mapping.path, mapping.digest));
    }
    for unit in &read.unit_columns {
        out.push_str(&format!(
            "units\t{}\t{}\t{}\t{}\n",
            unit.device, unit.cdl, unit.from, unit.to
        ));
    }
    for series in read.expected.iter().chain(&read.indicators) {
        let values = match &series.values {
            SeriesValues::Real(values) => values.iter().map(|x| hex(*x)).collect::<Vec<_>>(),
            SeriesValues::Integer(values) => values.iter().map(i32::to_string).collect(),
            SeriesValues::Boolean(values) => values.iter().map(bool::to_string).collect(),
        };
        out.push_str(&format!("series\t{}\t{}\n", series.name, values.join("\t")));
    }
    out
}

/// The oracle's rows of one kind, each split on tabs after the kind.
fn oracle(kind: &str) -> Vec<Vec<String>> {
    String::from_utf8(trend_file("trends_oracle.txt"))
        .expect("UTF-8")
        .lines()
        .filter_map(|line| line.strip_prefix(&format!("{kind}\t")))
        .map(|rest| rest.split('\t').map(str::to_owned).collect())
        .collect()
}

#[test]
fn the_merged_fixture_matches_its_golden_byte_for_byte() {
    let read = read_fixture(&TREND_FILES);
    let path = support::fixture("commissioning/trends/merged.golden.txt");
    support::compare_or_bless(&path, &dump(&read));
}

#[test]
fn the_source_digests_match_the_oracles_and_name_the_files_in_declaration_order() {
    let read = read_fixture(&TREND_FILES);
    let digests: Vec<(String, String)> = oracle("digest")
        .into_iter()
        .map(|row| (row[0].clone(), row[1].clone()))
        .collect();
    let read_digests: Vec<(String, String)> = read
        .sources
        .iter()
        .map(|source| (source.path.clone(), source.digest.clone()))
        .collect();
    assert_eq!(read_digests, digests);
    let rows: Vec<u64> = read.sources.iter().map(|source| source.rows).collect();
    // The decoy matrix's two rows are not the source's: the matrix read has six.
    assert_eq!(rows, [6, 4, 8]);
    assert_eq!(read.sources[0].table.as_deref(), Some("ahu"));
    assert!(read.sources.iter().all(|s| s.role.as_str() == "inputs"));
}

#[test]
fn the_merged_timeline_and_cells_match_the_independent_derivation() {
    let read = read_fixture(&TREND_FILES);
    let names: Vec<String> = oracle("columns").remove(0);
    let columns: Vec<&str> = read.inputs.columns().iter().map(|c| c.name()).collect();
    assert_eq!(columns, names);
    let frames = oracle("frame");
    assert_eq!(read.inputs.times().len(), frames.len());
    for (index, row) in frames.iter().enumerate() {
        assert_eq!(row[0], index.to_string());
        assert_eq!(hex(read.inputs.times()[index]), row[1], "frame {index}");
        for (slot, column) in read.inputs.columns().iter().enumerate() {
            assert_eq!(
                oracle_cell(column.values(), index),
                row[2 + slot],
                "frame {index} column {}",
                column.name()
            );
        }
    }
}

#[test]
fn replay_feeds_the_engine_what_the_independent_derivation_holds() {
    let read = read_fixture(&TREND_FILES);
    let trace = super::replay_signals(read.inputs);
    assert_eq!(
        trace.meta().origin.as_ref().map(|o| o.as_str()),
        Some("2018-06-07T00:00:00-07:00"),
        "the origin is carried once, in the header"
    );
    let rows = oracle("replay");
    assert_eq!(trace.rows(), rows.len());
    let fed = trace.inputs().expect("the inputs are retained");
    for (index, row) in rows.iter().enumerate() {
        let status = FrameStatus::from_byte(trace.status()[index]).expect("a status");
        let expected = match row[1].as_str() {
            "executed" => FrameStatus::Executed,
            "substituted" => FrameStatus::Substituted,
            _ => FrameStatus::RefusedMissing,
        };
        assert_eq!(status, expected, "frame {index}");
        if expected == FrameStatus::RefusedMissing {
            continue;
        }
        for (slot, (path, column)) in fed.iter().enumerate() {
            let value = match column {
                TraceColumn::Real(values) => hex(values[index]),
                TraceColumn::Integer(values) => values[index].to_string(),
                TraceColumn::Boolean(values) => (values[index] == 1).to_string(),
                TraceColumn::Enum(_) => unreachable!("no Enum input"),
            };
            assert_eq!(value, row[2 + slot], "frame {index} input {path}");
        }
    }
}

#[test]
fn neither_file_order_nor_source_order_changes_a_byte() {
    let reference = dump(&read_fixture(&TREND_FILES));
    // Every order of the four files handed in.
    let mut orders = vec![TREND_FILES.to_vec()];
    for first in 0..4 {
        for second in 0..4 {
            if first != second {
                let mut order = TREND_FILES.to_vec();
                order.swap(first, second);
                order.rotate_left(first);
                orders.push(order);
            }
        }
    }
    for order in &orders {
        assert_eq!(dump(&read_fixture(order)), reference, "{order:?}");
    }
    // Every order of the three input sources' declarations: the merged inputs are the same;
    // only the receipt's `sources` list follows the declaration order, as the note says.
    let merged = |read: &TrendSources| {
        dump(read)
            .lines()
            .filter(|line| !line.starts_with("source\t"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        super::loaded(&spec_in_order([0, 1, 2])).spec_id(),
        trend_spec().spec_id(),
        "the declarations below are the fixture's"
    );
    let expected = merged(&read_fixture(&TREND_FILES));
    for order in [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        let spec = super::loaded(&spec_in_order(order));
        let read = TrendSources::read(
            &spec,
            0,
            &trend_inputs(&TREND_FILES),
            &TrendLimits::default(),
        )
        .expect("reads");
        assert_eq!(merged(&read), expected, "{order:?}");
        let paths: Vec<&str> = read.sources.iter().map(|s| s.path.as_str()).collect();
        let declared: Vec<&str> = order
            .iter()
            .map(|&i| ["ahu.mos", "status.csv", "limits.csv"][i])
            .collect();
        assert_eq!(paths, declared);
    }
}

/// The fixture's three input sources, as `spec.json` declares them.
const DECLARATIONS: [&str; 3] = [
    r#"{ "path": "ahu.mos", "format": "combiTimeTable", "table": "ahu", "timeColumn": 1,
         "columns": { "2": "AHU Temp", "3": "Zone Temp" }, "hold": { "maxAge": 10 } }"#,
    r#"{ "path": "status.csv", "format": "csv", "timeColumn": "time",
         "columns": { "Count": "Count", "Enable": "Enable" }, "hold": { "maxAge": 30 } }"#,
    r#"{ "path": "limits.csv", "format": "csv", "timeColumn": "stamp",
         "columns": { "Limit": "Limit" } }"#,
];

/// The fixture's spec with its input sources declared in `order`.
fn spec_in_order(order: [usize; 3]) -> String {
    let inputs: Vec<&str> = order.iter().map(|&i| DECLARATIONS[i]).collect();
    format!(
        r#"{{"schema": "oce/commissioning-spec/v1",
            "time": {{ "origin": "2018-06-07T00:00:00-07:00" }},
            "references": [{{ "model": "Signals", "sequence": "sig",
              "pointNameMapping": "points.json", "inputs": [{}] }}]}}"#,
        inputs.join(", ")
    )
}
