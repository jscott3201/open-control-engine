#![doc = "Windows compilation and filesystem boundary checks for retained Reliefs artifacts."]
#![cfg(windows)]

#[path = "open_modelica_reliefs_reference/safe_read.rs"]
mod safe_read;

#[test]
fn hardlinks_and_reparse_points_cannot_cross_the_artifact_boundary() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!(
        "oce-reliefs-windows-safe-read-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let regular = root.join("regular");
    std::fs::write(&regular, b"ok").unwrap();
    std::fs::hard_link(&regular, root.join("alias")).unwrap();
    assert!(safe_read::read(&root, "regular").is_err());
    std::fs::remove_file(root.join("alias")).unwrap();
    assert_eq!(safe_read::read(&root, "regular").unwrap(), b"ok");
    if symlink_file(&regular, root.join("reparse")).is_ok() {
        assert!(safe_read::read(&root, "reparse").is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
