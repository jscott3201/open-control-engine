//! The Building 33 worked example through the built `oce` binary (issue #755): one day of the
//! six vendored LBNL Building 33 trends against `CoolingCoilValve` **authored from the vendored
//! model, not vendored** (`oce-verify`'s `tests/fixtures/commissioning/building33/`). The
//! verdict is about that transcription, not about upstream's translation, which does not load
//! (issue #760).
//!
//! - **Provenance**: the checked-in CXF is what `oce emit` writes for the authored text, byte for
//!   byte, and the text is already in `oce fmt`'s canonical form.
//! - **Oracle**: stdout of `oce verify --commission spec.json --data trended` is `oce-verify`'s
//!   golden receipt byte for byte; with `--out --bounds`, the five curve files the verb writes
//!   are byte for byte the files the pinned pyfunnel wrote for the Building 33 day of the funnel
//!   differential.
//! - **Transcript**: the run's exit status, stdout and stderr, regenerated with `OCE_BLESS_CLI=1`.

mod support;

use std::fs;

use support::building33::{building_33_layout, example_file, verify_fixture};
use support::{case_in, oce_in, scratch};

const DAY: [&str; 5] = ["verify", "--commission", "spec.json", "--data", "trended"];

#[test]
fn the_checked_in_cxf_is_what_oce_emit_writes_for_the_authored_text() {
    let dir = scratch("building_33_emit");
    fs::write(
        dir.join("cooling_coil_valve.oce"),
        example_file("cooling_coil_valve.oce"),
    )
    .expect("the authored text");
    let emitted = oce_in(&dir, &["emit", "cooling_coil_valve.oce"], b"");
    assert_eq!(emitted.code, 0, "{}", emitted.transcript());
    assert!(
        emitted.stdout == example_file("cooling_coil_valve.cxf.json"),
        "the checked-in CXF is not `oce emit`'s output; re-emit it from the .oce"
    );
    let formatted = oce_in(&dir, &["fmt", "cooling_coil_valve.oce"], b"");
    assert_eq!(formatted.code, 0, "{}", formatted.transcript());
    assert!(
        formatted.stdout == example_file("cooling_coil_valve.oce"),
        "the authored text is in canonical form"
    );
}

#[test]
fn the_day_prints_the_librarys_golden_receipt_and_exits_1_for_its_fail() {
    let dir = building_33_layout("building_33_day");
    case_in(&dir, "verify_commission_building_33_day", 1, &DAY);
    let run = oce_in(&dir, &DAY, b"");
    assert!(
        run.stdout == verify_fixture("commissioning/building33/receipts/day.receipt.json"),
        "stdout is not the library's golden receipt\n{}",
        run.stdout_text()
    );
    assert!(run.stderr.is_empty(), "{}", run.transcript());
}

#[test]
fn the_curve_files_of_the_day_are_the_files_pyfunnel_wrote_byte_for_byte() {
    let dir = building_33_layout("building_33_out");
    fs::create_dir(dir.join("out")).expect("out/");
    let mut args = DAY.to_vec();
    args.extend(["--out", "out", "--bounds"]);
    let run = oce_in(&dir, &args, b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    assert!(
        fs::read(dir.join("out/receipt.json")).expect("the receipt")
            == verify_fixture("commissioning/building33/receipts/day.receipt.json")
    );
    for file in [
        "reference.csv",
        "test.csv",
        "lowerBound.csv",
        "upperBound.csv",
        "errors.csv",
    ] {
        let ours = fs::read(dir.join("out/outputs/0").join(file)).expect("a curve file");
        let theirs = verify_fixture(&format!("funnel/expected/building_33_day/{file}"));
        assert!(ours == theirs, "{file}: not pyfunnel's bytes");
    }
}
