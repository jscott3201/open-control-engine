//! Shared test support: the golden switch, repository paths and the vendored document.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::generator::DOCUMENT;

/// The golden switch (design note decision 3): `OCE_BLESS=1` rewrites `data/generated.json`.
pub const BLESS_SWITCH: &str = "OCE_BLESS";

/// The crate root.
pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A path under the repository root.
pub fn repository(relative: &str) -> PathBuf {
    crate_dir().join("../../..").join(relative)
}

/// The vendored tree's root.
pub fn vendored(relative: &str) -> PathBuf {
    repository("third_party/modelica-buildings-cdl").join(relative)
}

/// The bytes of the vendored CXF-Core document, read once.
pub fn core_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let path = repository(DOCUMENT);
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    })
}

/// The vendored Modelica source of an upstream class (`CDL.Reals.PID`).
pub fn modelica_source(class_path: &str) -> String {
    let path = vendored(&format!(
        "Buildings/Controls/OBC/{}.mo",
        class_path.replace('.', "/")
    ));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}
