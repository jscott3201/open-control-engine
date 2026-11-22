//! Commissioning runs laid out in scratch directories for `oce verify --commission` (issue
//! #664). Every file is copied in from `oce-verify`'s own fixtures, read in place, so the CLI is
//! held to the library's goldens: the run fixture (`tests/fixtures/commissioning/run/`, whose
//! `pass`, `fail` and `no_data` receipts and exceedance columns are golden there) and the
//! `signals` subject. A run starts in the scratch directory and names every file relatively, so
//! no host path reaches a transcript.

use std::fs;
use std::path::{Path, PathBuf};

use super::{crate_dir, scratch};

/// `oce-verify`'s fixture directory, relative to this crate.
pub const VERIFY_FIXTURES: &str = "../oce-verify/tests/fixtures";

/// The run fixture's files, beside the spec.
pub const RUN_FILES: [&str; 4] = ["points.json", "inputs.csv", "outputs.csv", "fan.csv"];

/// The bytes of a file of `oce-verify`'s run fixture.
pub fn run_file(name: &str) -> Vec<u8> {
    let path = crate_dir()
        .join(VERIFY_FIXTURES)
        .join("commissioning/run")
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// The bytes of a subject of `oce-verify`'s fixtures (`plant`, `signals`).
pub fn subject(name: &str) -> Vec<u8> {
    let path = crate_dir()
        .join(VERIFY_FIXTURES)
        .join(format!("subjects/{name}.cxf.json"));
    fs::read(&path).unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// The library's golden receipt of the run fixture's case `name` (`pass`, `fail`, `no_data`).
pub fn golden_receipt(name: &str) -> Vec<u8> {
    run_file(&format!("receipts/{name}.receipt.json"))
}

/// The library's golden exceedance columns of case `name`: per output, in the receipt's order,
/// its canonical path and its points as bits.
pub fn golden_curves(name: &str) -> Vec<(String, Vec<(f64, f64)>)> {
    let text = String::from_utf8(run_file(&format!("receipts/{name}.curves.txt"))).expect("UTF-8");
    let mut curves: Vec<(String, Vec<(f64, f64)>)> = Vec::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let (left, right) = line.split_once('\t').expect("two cells");
        if left == "output" {
            curves.push((right.to_owned(), Vec::new()));
            continue;
        }
        let bits = |hex: &str| f64::from_bits(u64::from_str_radix(hex, 16).expect("hex bits"));
        curves
            .last_mut()
            .expect("an output first")
            .1
            .push((bits(left), bits(right)));
    }
    curves
}

/// A curve as pyfunnel writes one, from its points: `x,y`, then `%.16g` pairs.
pub fn csv_of(points: &[(f64, f64)]) -> String {
    let mut text = String::from("x,y\n");
    for (x, y) in points {
        text.push_str(&format!(
            "{},{}\n",
            oce_verify::format_g16(*x),
            oce_verify::format_g16(*y)
        ));
    }
    text
}

/// A scratch directory holding the run fixture as the library's case `case` reads it: its spec as
/// `spec.json`, its four files and the `plant` subject as `plant.cxf.json`. `pass` is the fixture
/// as it is; `fail` trends the oracle's outputs; `no_data` a fan that never runs.
pub fn run_layout(name: &str, case: &str) -> PathBuf {
    let dir = scratch(name);
    fs::write(dir.join("spec.json"), run_file("spec.json")).expect("the spec");
    for file in RUN_FILES {
        fs::write(dir.join(file), run_file(file)).expect("a run file");
    }
    fs::write(dir.join("plant.cxf.json"), subject("plant")).expect("the subject");
    match case {
        "pass" => {}
        "fail" => fs::write(dir.join("outputs.csv"), run_file("oracle_outputs.csv")).expect("fail"),
        "no_data" => {
            fs::write(dir.join("fan.csv"), b"time,Fan Status\n0,false\n").expect("no data");
        }
        other => panic!("no case {other}"),
    }
    dir
}

/// A scratch directory holding the library's oracle case (`oracle_spec.json`: two input sources,
/// one frame refused for a missing sample, an off-grid stop) as `spec.json`, under `envelope`.
pub fn oracle_layout(name: &str, envelope: &str) -> PathBuf {
    let dir = scratch(name);
    let spec = String::from_utf8(run_file("oracle_spec.json"))
        .expect("UTF-8")
        .replace(
            r#""model": "Plant","#,
            &format!(r#""model": "Plant", "envelope": "{envelope}","#),
        );
    fs::write(dir.join("spec.json"), spec).expect("the spec");
    for file in [
        "points.json",
        "oracle_main.csv",
        "oracle_limit.csv",
        "oracle_outputs.csv",
        "fan.csv",
    ] {
        fs::write(dir.join(file), run_file(file)).expect("an oracle file");
    }
    fs::write(dir.join("plant.cxf.json"), subject("plant")).expect("the subject");
    dir
}

/// A scratch directory holding a run of the `signals` subject (`s.json`) with an inline mapping:
/// one row `(t, ahu.T, zone.T, b)` per frame, `n` at 1 and `lim` at 5, and the trended `ratio`
/// always 1. `ratio` is `zone.T / ahu.T`, so an `ahu.T` of zero makes the engine's sample an
/// infinity; a `b` of false fires the subject's `Assert`, a run-time warning.
pub fn signals_layout(name: &str, rows: &[(u32, f64, f64, bool)]) -> PathBuf {
    let dir = scratch(name);
    let point = |name: &str, kind: &str| {
        format!(
            r#"{{"cdl": {{"name": "{name}", "type": "{kind}"}}, "device": {{"name": "{name}", "type": "{kind}"}}}}"#
        )
    };
    let mapping = ["ahu.T", "zone.T", "lim", "ratio"]
        .iter()
        .map(|name| point(name, "float"))
        .chain([point("n", "int"), point("b", "bool")])
        .collect::<Vec<_>>()
        .join(", ");
    let spec = format!(
        r#"{{"tolerances": {{"atolx": 0.5, "atoly": 0.01}},
  "references": [{{"model": "S", "sequence": "sig", "cxf": "s.json",
    "pointNameMapping": [{mapping}],
    "window": {{"start": 0, "stop": {}}}, "sampling": 1,
    "inputs": [{{"path": "in.csv", "format": "csv", "timeColumn": "time",
      "columns": {{"ahu.T": "ahu.T", "zone.T": "zone.T", "n": "n", "b": "b", "lim": "lim"}}}}],
    "expected": [{{"path": "out.csv", "format": "csv", "timeColumn": "time",
      "columns": {{"ratio": "ratio"}}}}]}}]}}
"#,
        rows.last().map_or(0, |row| row.0)
    );
    let mut input = String::from("time,ahu.T,zone.T,n,b,lim\n");
    let mut output = String::from("time,ratio\n");
    for (t, ahu, zone, b) in rows {
        input.push_str(&format!("{t},{ahu:?},{zone:?},1,{b},5\n"));
        output.push_str(&format!("{t},1\n"));
    }
    fs::write(dir.join("spec.json"), spec).expect("the spec");
    fs::write(dir.join("in.csv"), input).expect("in.csv");
    fs::write(dir.join("out.csv"), output).expect("out.csv");
    fs::write(dir.join("s.json"), subject("signals")).expect("the subject");
    dir
}

/// Replace `old` with `new` in the file `name` of `dir`; an `old` that does not occur fails.
pub fn edit(dir: &Path, name: &str, old: &str, new: &str) {
    let path = dir.join(name);
    let text = fs::read_to_string(&path).expect("the file to edit");
    assert!(text.contains(old), "{name} holds `{old}`");
    fs::write(&path, text.replacen(old, new, 1)).expect("write the edit");
}

/// Every file under `dir`, as `(path relative to dir with `/`, bytes)`, sorted by path.
pub fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(at).expect("list") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((relative, fs::read(&path).expect("read")));
            }
        }
    }
    let mut out = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}
