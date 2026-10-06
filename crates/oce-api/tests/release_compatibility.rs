//! Retained release-pair evidence runs in the existing facade test gate, without workflow changes.

use std::path::Path;
use std::process::Command;

#[test]
fn retained_release_matrix_and_hostile_controls_are_enforced() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for script in ["check.py", "test_check.py"] {
        let output = Command::new("python3")
            .arg(root.join("scripts/release_compatibility").join(script))
            .current_dir(&root)
            .output()
            .expect("repository gates require Python 3.11+ with its standard library");
        assert!(
            output.status.success(),
            "{script}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if script == "test_check.py" {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("Ran 16 tests"),
                "hostile controls must not vanish: {:?}",
                output
            );
        } else {
            assert_eq!(
                output.stdout,
                b"release compatibility: OK (36 directed rows; current/current only; no N-1)\n"
            );
        }
    }
}
