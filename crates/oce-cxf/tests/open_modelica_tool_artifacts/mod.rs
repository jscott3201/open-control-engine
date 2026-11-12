//! A private Cargo artifact directory for each run of an OpenModelica reference tool.
//!
//! # Why this exists (issues #640 and #642)
//!
//! The Line and Reliefs evidence tests run their tool workspace with `cargo run --manifest-path
//! tools/<tool>/Cargo.toml --offline --locked`, directly or through the tool's `assemble.sh`,
//! `candidate_assembly_test.sh` and `verify_evidence.py`. The recorded OpenModelica evidence pins
//! those scripts' bytes, so they keep that `cargo run`.
//!
//! Every `cargo run` puts the executable it is about to start at `<target-dir>/debug/<bin>` again,
//! even when nothing needs compiling. On macOS Cargo deletes that file and clones it afresh from
//! `deps/` on every run; on any host a rebuild replaces it. Cargo holds its build lock while it
//! does that and releases it before it starts the program. So while the parallel tests shared the
//! tool's one target directory, one `cargo run` could delete the executable while another was
//! starting it, and the test failed with `could not execute process … (never executed)` and
//! `No such file or directory`.
//!
//! [`ToolArtifactDir`] gives each run an artifact directory of its own (`CARGO_TARGET_DIR`), which
//! holds the only copy of the executable that run starts. It points the build directory
//! (`CARGO_BUILD_BUILD_DIR`) at the tool's own `target/`, which holds what Cargo compiled. Cargo
//! locks the build directory while it compiles and copies the executable out, so the tool is
//! still compiled once per change, and no run deletes an executable another run is starting. The
//! scripts inherit both variables, so their own `cargo run` is isolated the same way. This holds
//! under nextest (a process per test) and `cargo test` (a thread per test) alike, because each
//! run, not each process, claims its directory.

use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Line reference tool workspace, relative to the repository root.
pub(crate) const LINE_TOOL: &str = "tools/openmodelica-line-reference";

/// The Reliefs reference tool workspace, relative to the repository root.
pub(crate) const RELIEFS_TOOL: &str = "tools/openmodelica-reliefs-reference";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is below repository root")
        .to_path_buf()
}

/// One run's private Cargo artifact directory over its tool's shared build directory.
///
/// The directory is claimed fresh under the system temporary directory (mode `0o700`, never an
/// existing path) and removed on drop, after checking that the path still names the directory
/// that was claimed. [`Self::isolate`] only sets a command's environment, so keep the value alive
/// until the child process has exited.
pub(crate) struct ToolArtifactDir {
    build_dir: PathBuf,
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl ToolArtifactDir {
    /// Claims a fresh artifact directory for one run of `tool`, a tool workspace path relative to
    /// the repository root ([`LINE_TOOL`] or [`RELIEFS_TOOL`]).
    ///
    /// # Panics
    ///
    /// When the temporary directory cannot be resolved, or no fresh name is free after 4096
    /// attempts.
    pub(crate) fn claim(tool: &str) -> Self {
        let build_dir = repository_root().join(tool).join("target");
        let name = tool
            .rsplit('/')
            .next()
            .expect("tool path has a final segment");
        let root = std::env::temp_dir().canonicalize().unwrap();
        for nonce in 0_u32..4096 {
            let path = root.join(format!(
                "oce-{name}-artifacts-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let metadata = std::fs::symlink_metadata(&path).unwrap();
                    return Self {
                        build_dir,
                        path,
                        device: metadata.dev(),
                        inode: metadata.ino(),
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot claim a tool artifact directory: {error}"),
            }
        }
        panic!("cannot claim a tool artifact directory")
    }

    /// Points every `cargo` that `command` starts, and every one its children start, at this
    /// run's artifact directory and the tool's shared build directory.
    pub(crate) fn isolate<'command>(
        &self,
        command: &'command mut Command,
    ) -> &'command mut Command {
        command
            .env("CARGO_TARGET_DIR", &self.path)
            .env("CARGO_BUILD_BUILD_DIR", &self.build_dir)
    }

    /// The artifact directory: Cargo puts the executable it starts at `debug/<bin>` below it.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ToolArtifactDir {
    fn drop(&mut self) {
        let metadata = std::fs::symlink_metadata(&self.path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

/// The file at `path` as an identity that a delete-and-clone changes: device, inode, length and
/// modification time. `None` when nothing is there.
fn file_identity(path: &Path) -> Option<(u64, u64, u64, i64, i64)> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some((
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    ))
}

/// A run through [`ToolArtifactDir`] starts the executable in its own artifact directory and
/// leaves the one in the tool's `target/debug/` where it was: the property the parallel verifier
/// tests rely on. Without the isolation the start assertion fails on every host, and the
/// identity assertion on macOS, where each `cargo run` deletes and clones the executable again.
#[test]
fn tool_runs_start_a_private_executable_and_leave_the_shared_one_in_place() {
    let root = repository_root();
    for (tool, binary, evidence) in [
        (
            LINE_TOOL,
            "oce-openmodelica-line-reference",
            "reals_line/arm64",
        ),
        (
            RELIEFS_TOOL,
            "oce-openmodelica-reliefs-reference",
            "g36_reliefs/arm64",
        ),
    ] {
        let shared = root.join(tool).join("target/debug").join(binary);
        let before = file_identity(&shared);
        // The first run may compile the tool. The second finds it fresh, which is when macOS
        // Cargo still deletes and clones the executable it is about to start.
        for attempt in ["first", "fresh"] {
            let artifacts = ToolArtifactDir::claim(tool);
            let mut command = Command::new("cargo");
            artifacts
                .isolate(&mut command)
                .args(["run", "--manifest-path"])
                .arg(root.join(tool).join("Cargo.toml"))
                .args([
                    "--offline",
                    "--locked",
                    "--",
                    "verify-architecture-canonical",
                ])
                .arg(
                    root.join("crates/oce-conformance/tests/fixtures/open_modelica")
                        .join(evidence),
                )
                .current_dir(&root)
                .env("CARGO_NET_OFFLINE", "true");
            let output = command.output().expect("execute cargo run");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{tool} {attempt}: {stderr}");
            assert_eq!(
                output.stdout, b"strict canonical boundary passed\n",
                "{tool} {attempt}"
            );
            let started = artifacts.path().join("debug").join(binary);
            assert!(
                stderr.contains(&format!(
                    "`{} verify-architecture-canonical ",
                    started.display()
                )),
                "{tool} {attempt} started another executable: {stderr}"
            );
            assert!(started.is_file(), "{tool} {attempt}");
        }
        assert_eq!(
            file_identity(&shared),
            before,
            "{tool}: a run replaced the shared executable"
        );
    }
}
