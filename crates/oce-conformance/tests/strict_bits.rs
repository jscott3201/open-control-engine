//! Pinned corpus evidence, not an independent mathematical oracle or macOS qualification.

mod block_harness;

#[path = "strict_bits/controls.rs"]
mod controls;
#[path = "strict_bits/evidence.rs"]
mod evidence;
#[path = "strict_bits/matrix.rs"]
mod matrix;

#[test]
fn retained_inventory_covers_every_real_signal_once() {
    let retained = evidence::read_retained();
    let actual = evidence::collect();
    evidence::same_contract(&retained, &actual).unwrap();
    let local = format!("{}-{}-", std::env::consts::OS, std::env::consts::ARCH);
    if retained.cell.starts_with(&local) {
        for (expected, actual) in retained.signals.iter().zip(&actual.signals) {
            assert_eq!(
                expected.actual, actual.actual,
                "{}: local profile observation drift",
                expected.id
            );
        }
    }
}

#[test]
fn capture_raw_bits_without_reblessing_the_oracle() {
    let first = evidence::collect();
    let second = evidence::collect();
    assert_eq!(first, second, "same-process repeat drift");
    if let Some(path) = std::env::var_os("OCE_STRICT_BITS_OUT") {
        let path = evidence::root().join(path);
        let refresh = std::env::var("OCE_STRICT_BITS_REFRESH").as_deref() == Ok("1");
        if refresh {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI cannot refresh retained evidence"
            );
            assert_eq!(
                path,
                evidence::root()
                    .join("crates/oce-conformance/tests/fixtures/strict_bits/corpus.json"),
                "only the local observation can be explicitly refreshed"
            );
        } else {
            assert!(!path.exists(), "never overwrite evidence implicitly");
        }
        std::fs::write(path, evidence::encode(&first)).unwrap();
    }
}
