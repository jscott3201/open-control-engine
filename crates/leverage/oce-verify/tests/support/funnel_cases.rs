//! The cases of the funnel differential fixture (`tests/fixtures/funnel/`): funnel's own 17 test
//! directories and the hostile set, each with its two series, its tolerances and what the pinned
//! pyfunnel made of it.

use std::path::{Path, PathBuf};

use oce_verify::{Curve, FunnelResult, SeriesEnd, Tolerances};

use super::fixture;

/// funnel's own test directories at the pinned commit, in upstream's order of kinds.
pub const UPSTREAM_CASES: [&str; 17] = [
    "fail1",
    "fail2",
    "fail3",
    "fail4",
    "fail5",
    "fail6",
    "should_succeed1",
    "success1",
    "success2",
    "success3",
    "success4",
    "success5",
    "success6",
    "success7",
    "success8",
    "success9",
    "success10",
];

/// The number of directories under `hostile/`. Pinned, so that a case the reader stops finding
/// fails a test instead of leaving the comparison shorter.
pub const HOSTILE_CASES: usize = 49;

/// The five files pyfunnel writes, in the order the port's result lists its curves.
pub const CURVE_FILES: [&str; 5] = [
    "reference.csv",
    "test.csv",
    "lowerBound.csv",
    "upperBound.csv",
    "errors.csv",
];

/// One case of the fixture.
pub struct Case {
    /// The directory name, unique across both sets.
    pub name: String,
    /// `upstream` or `hostile`.
    pub set: &'static str,
    /// The reference series: `x`, then `y`.
    pub reference: (Vec<f64>, Vec<f64>),
    /// The test series: `x`, then `y`.
    pub test: (Vec<f64>, Vec<f64>),
    /// The tolerances of its `param.json`; a missing one is zero, as pyfunnel reads it.
    pub tolerances: Tolerances,
}

/// What the pinned pyfunnel made of a case.
pub enum Expected {
    /// The text of its five files, in the order of [`CURVE_FILES`].
    Curves([String; 5]),
    /// funnel refused the case: the end at which the two series' `x` differ.
    Refused(SeriesEnd),
}

/// The fixture's root, `tests/fixtures/funnel`.
pub fn root() -> PathBuf {
    fixture("funnel")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// A two-column CSV file read as the capture script reads it: the first line is a header, every
/// other line is `x,y`, and a line that is not two numbers is an error. Line ends may be CRLF
/// (two of upstream's files are).
fn read_series(path: &Path) -> (Vec<f64>, Vec<f64>) {
    let text = read(path);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (index, line) in text.lines().enumerate().skip(1) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (x, y) = line
            .split_once(',')
            .unwrap_or_else(|| panic!("{} line {}: not two fields", path.display(), index + 1));
        let number = |field: &str| {
            field
                .parse::<f64>()
                .unwrap_or_else(|error| panic!("{} line {}: {error}", path.display(), index + 1))
        };
        xs.push(number(x));
        ys.push(number(y));
    }
    (xs, ys)
}

/// The case `name` of the set `set` under the fixture root `root`.
pub fn load_in(root: &Path, set: &'static str, name: &str) -> Case {
    let directory = root.join(set).join(name);
    let param: serde_json::Value = serde_json::from_str(&read(&directory.join("param.json")))
        .unwrap_or_else(|error| panic!("{set}/{name}/param.json: {error}"));
    let file = |key: &str| {
        directory.join(
            param[key]
                .as_str()
                .unwrap_or_else(|| panic!("{set}/{name}/param.json names its `{key}` file")),
        )
    };
    let tolerance = |key: &str| match &param[key] {
        serde_json::Value::Null => 0.0,
        value => value
            .as_f64()
            .unwrap_or_else(|| panic!("{set}/{name}/param.json: `{key}` is a number")),
    };
    Case {
        name: name.to_owned(),
        set,
        reference: read_series(&file("reference")),
        test: read_series(&file("test")),
        tolerances: Tolerances {
            atolx: tolerance("atolx"),
            atoly: tolerance("atoly"),
            rtolx: tolerance("rtolx"),
            rtoly: tolerance("rtoly"),
            ltolx: tolerance("ltolx"),
            ltoly: tolerance("ltoly"),
        },
    }
}

/// The names of the directories under `hostile/`, sorted.
pub fn hostile_names() -> Vec<String> {
    let directory = root().join("hostile");
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 case name")
        })
        .collect();
    names.sort();
    names
}

/// Every case: upstream's 17 in [`UPSTREAM_CASES`]' order, then the hostile set sorted by name.
pub fn cases() -> Vec<Case> {
    let hostile = hostile_names();
    assert_eq!(hostile.len(), HOSTILE_CASES, "the hostile set's size");
    UPSTREAM_CASES
        .iter()
        .map(|name| load_in(&root(), "upstream", name))
        .chain(hostile.iter().map(|name| load_in(&root(), "hostile", name)))
        .collect()
}

/// One case by name, from either set.
pub fn case(name: &str) -> Case {
    if UPSTREAM_CASES.contains(&name) {
        load_in(&root(), "upstream", name)
    } else {
        load_in(&root(), "hostile", name)
    }
}

/// What pyfunnel made of the case `name`: `expected/<name>/` holds either the five curve files
/// and nothing else, or `refused.txt` and nothing else.
pub fn expected(name: &str) -> Expected {
    let directory = root().join("expected").join(name);
    let mut files: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .collect();
    files.sort();
    if files == ["refused.txt"] {
        let line = read(&directory.join("refused.txt"));
        return Expected::Refused(match line.as_str() {
            "Error: Reference and test data minimum x values are different.\n" => SeriesEnd::First,
            "Error: Reference and test data maximum x values are different.\n" => SeriesEnd::Last,
            other => panic!("expected/{name}/refused.txt holds an unknown refusal: {other:?}"),
        });
    }
    let mut sorted = CURVE_FILES.map(str::to_owned);
    sorted.sort();
    assert_eq!(files, sorted, "expected/{name} holds the five curve files");
    Expected::Curves(CURVE_FILES.map(|file| read(&directory.join(file))))
}

/// The five curves of a result, in the order of [`CURVE_FILES`].
pub fn curves(result: &FunnelResult) -> [&Curve; 5] {
    [
        &result.reference,
        &result.test,
        &result.lower_bound,
        &result.upper_bound,
        &result.errors,
    ]
}

/// One curve as hex bits: a `[name] <points>` line, then one `<x bits> <y bits>` line per point,
/// each the 16 lowercase hex digits of the IEEE-754 binary64.
pub fn curve_bits(name: &str, curve: &Curve) -> String {
    let mut text = format!("[{name}] {}\n", curve.len());
    for (x, y) in curve.points() {
        text.push_str(&format!("{:016x} {:016x}\n", x.to_bits(), y.to_bits()));
    }
    text
}

/// An optional figure as hex bits, or `none`.
pub fn optional_bits(value: Option<f64>) -> String {
    value.map_or_else(
        || "none".to_owned(),
        |value| format!("{:016x}", value.to_bits()),
    )
}

/// Compare `actual` with the golden at `path` byte for byte, or rewrite it when the bless switch
/// is on. A failure names the first differing line, never the whole file: a bits golden can be
/// tens of thousands of lines.
pub fn compare_or_bless_bits(path: &Path, actual: &str) {
    if oce_bless::enabled(super::BLESS_SWITCH) {
        std::fs::create_dir_all(path.parent().expect("a golden has a directory"))
            .expect("create the golden directory");
        std::fs::write(path, actual).expect("write golden");
        return;
    }
    let expected = read(path);
    if expected == actual {
        return;
    }
    let line = expected
        .lines()
        .zip(actual.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
    panic!(
        "{} differs from the generated bits at line {}: golden {:?}, generated {:?}; rerun with \
         {}=1 if the change is intended",
        path.display(),
        line + 1,
        expected.lines().nth(line),
        actual.lines().nth(line),
        super::BLESS_SWITCH
    );
}
