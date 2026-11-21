//! Capture the `aligned-grid` parity goldens from `oce-conformance`: the bounds, errors and
//! figures of its `build_bounds` and `compare` over the funnel fixture's small cases, as bits.
//!
//! `oce-verify` copies that arithmetic and may not depend on the crate it is copied from, so
//! this file is not part of any build. To run it, copy it into `crates/oce-conformance/examples/`
//! (it is a complete example of that crate), run
//!
//! ```text
//! cargo run -p oce-conformance --locked --example conformance_bounds -- \
//!     crates/leverage/oce-verify/tests/fixtures
//! ```
//!
//! from the repository root, and delete the copy. It rewrites
//! `aligned_grid/conformance/<case>.txt` for every case of `funnel/upstream` and
//! `funnel/hostile` whose reference has at most [`MAX_REFERENCE_POINTS`] points (the larger ones
//! would make megabytes of text). The goldens are regenerated only by an issue that says so;
//! `tests/aligned_grid.rs` compares the copy with them and never writes them.

use std::path::Path;

use oce_conformance::{Series, Tolerances, build_bounds, compare};

/// The largest reference a parity golden is taken for. `tests/aligned_grid.rs` applies the same
/// rule and pins the number of cases it selects.
const MAX_REFERENCE_POINTS: usize = 41;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// A two-column CSV: one header line, then `x,y` rows; CRLF or LF.
fn series(path: &Path) -> (Vec<f64>, Vec<f64>) {
    read(path)
        .lines()
        .skip(1)
        .map(|line| {
            let (x, y) = line.trim_end_matches('\r').split_once(',').expect("two fields");
            (x.parse::<f64>().expect("x"), y.parse::<f64>().expect("y"))
        })
        .unzip()
}

fn curve(name: &str, points: &[(f64, f64)]) -> String {
    let mut text = format!("[{name}] {}\n", points.len());
    for (x, y) in points {
        text.push_str(&format!("{:016x} {:016x}\n", x.to_bits(), y.to_bits()));
    }
    text
}

fn main() {
    let fixtures = std::env::args().nth(1).expect("the path of oce-verify's tests/fixtures");
    let fixtures = Path::new(&fixtures);
    let out = fixtures.join("aligned_grid/conformance");
    let mut written = 0;
    for set in ["upstream", "hostile"] {
        let mut names: Vec<String> = std::fs::read_dir(fixtures.join("funnel").join(set))
            .expect("the set's directory")
            .map(|entry| entry.expect("an entry").file_name().into_string().expect("UTF-8"))
            .filter(|name| !name.ends_with(".txt"))
            .collect();
        names.sort();
        for name in names {
            let directory = fixtures.join("funnel").join(set).join(&name);
            let param: serde_json::Value =
                serde_json::from_str(&read(&directory.join("param.json"))).expect("param.json");
            let file = |key: &str| directory.join(param[key].as_str().expect("a file name"));
            let tolerance = |key: &str| param[key].as_f64().unwrap_or(0.0);
            let (reference_x, reference_y) = series(&file("reference"));
            let (test_x, test_y) = series(&file("test"));
            if reference_x.len() > MAX_REFERENCE_POINTS {
                continue;
            }
            let tolerances = Tolerances {
                atolx: tolerance("atolx"),
                atoly: tolerance("atoly"),
                rtolx: tolerance("rtolx"),
                rtoly: tolerance("rtoly"),
                ltolx: tolerance("ltolx"),
                ltoly: tolerance("ltoly"),
            };
            let reference = Series {
                x: &reference_x,
                y: &reference_y,
            };
            let test = Series {
                x: &test_x,
                y: &test_y,
            };
            let (lower, upper) = build_bounds(reference, &tolerances);
            let result = compare(reference, test, &tolerances);
            assert_eq!((&result.lower_bound, &result.upper_bound), (&lower, &upper));
            let first = result
                .first_failure_x
                .map_or_else(|| "none".to_owned(), |x| format!("{:016x}", x.to_bits()));
            let text = format!(
                "# oce-conformance build_bounds and compare: {set}/{name}\n# Hex IEEE-754 bits, \
                 `x y` per point. Captured by aligned_grid/capture/conformance_bounds.rs, never \
                 blessed.\n{}{}{}[figures]\npassed {}\ncompared {}\nmax_error {:016x}\n\
                 first_error_x {first}\n",
                curve("lower_bound", &lower),
                curve("upper_bound", &upper),
                curve("errors", &result.errors),
                result.passed,
                result.compared_points,
                result.max_error.to_bits(),
            );
            std::fs::write(out.join(format!("{name}.txt")), text).expect("write the golden");
            written += 1;
        }
    }
    println!("wrote {written} parity goldens");
}
