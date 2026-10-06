//! Linux-only strict candidates, conditional on the fail-closed native matrix gate.
//!
//! This is not a macOS qualification or a libm accuracy promise. The retained inventory is the
//! only signal list; schema/coverage/provenance and raw-bit controls run in `strict_bits`.

use super::{BlockCase, Port, SignalKind};
use std::sync::OnceLock;

pub(super) fn exact_candidate(case: &BlockCase, output: &Port) -> bool {
    if output.kind != SignalKind::Real {
        return false; // aligned comparison already compares discrete values exactly
    }
    static INVENTORY: OnceLock<serde_json::Value> = OnceLock::new();
    let inventory = INVENTORY.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/strict_bits/corpus.json"))
            .expect("strict-bit inventory")
    });
    let id = format!("{}.{}", case.slug, output.name);
    let signals = inventory["signals"].as_array().expect("signal inventory");
    let matches: Vec<_> = signals.iter().filter(|signal| signal["id"] == id).collect();
    assert_eq!(
        matches.len(),
        1,
        "{id}: exactly one inventory entry required"
    );
    let signal = matches[0];
    assert_eq!(signal["class"], case.class_path);
    assert_eq!(signal["output"], output.name);
    assert_eq!(signal["other_regime"], "aligned-atoly-rtoly-ltoly-1e-12");
    assert_eq!(signal["linux_regime"], "exact-candidate-pending-matrix");
    cfg!(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))
}
