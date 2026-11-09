//! Engine-side helpers for the emitter tests: load, export and the identity gate item 1 compares.

use oce_api::{Engine, LoadReport};

/// The path of a committed G36 corpus fixture.
pub fn corpus_fixture(name: &str) -> std::path::PathBuf {
    super::crate_dir()
        .join("../../oce-cxf/tests/fixtures/g36")
        .join(format!("{name}.jsonld"))
}

/// Load `bytes` in a fresh engine, or the refusal's display.
pub fn load(bytes: &[u8]) -> Result<(Engine, LoadReport), String> {
    let mut engine = Engine::new();
    let (report, _) = engine.load_cxf(bytes).map_err(|e| e.to_string())?;
    Ok((engine, report))
}

/// Load `bytes` and require a warning-free import (acceptance precondition 4).
pub fn load_clean(bytes: &[u8]) -> Engine {
    let (engine, report) = load(bytes).unwrap_or_else(|e| panic!("load refused: {e}"));
    let codes: Vec<&str> = report.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, Vec::<&str>::new(), "the import warned");
    engine
}

/// The exported CXF bytes of a loaded engine, as text.
pub fn export_text(engine: &Engine) -> String {
    let export = engine.export_cxf().expect("export");
    String::from_utf8(export.bytes).expect("UTF-8 export")
}

/// The identity of a loaded model that gate item 1 compares — today the complete export content
/// id (`ExportReport::content_id_complete`), or, for a partial export that has none, the export
/// bytes with their deferral codes. Every comparison goes through this one function, so the
/// switch to the canonical content id (GitHub #470) is a change here only.
pub fn identity(engine: &Engine) -> String {
    let export = engine.export_cxf().expect("export");
    match export.content_id_complete() {
        Ok(id) => id,
        Err(_) => {
            let codes: Vec<&str> = export.warnings.iter().map(|w| w.code.as_str()).collect();
            format!(
                "partial[{}]:{}",
                codes.join(","),
                String::from_utf8_lossy(&export.bytes)
            )
        }
    }
}

/// The loaded executable as compiled-schedule blob bytes with the content id it records (and the
/// CRC-32C trailer that covers it) zeroed, so two executables compare without their export
/// identity. Like [`identity`], the one place the id format is known.
pub fn executable(engine: &Engine) -> Vec<u8> {
    let blob = engine.compile_blob().expect("blob");
    let mut bytes = blob.bytes;
    if let Some(hex) = blob
        .content_id
        .as_deref()
        .and_then(|id| id.strip_prefix("cxf:fnv1a128:"))
    {
        // The blob stores the 128-bit tag little-endian.
        let needle: Vec<u8> = (0..16)
            .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .rev()
            .collect();
        let at = bytes
            .windows(needle.len())
            .position(|w| w == needle.as_slice())
            .expect("the blob records its content id");
        bytes[at..at + needle.len()].fill(0);
    }
    let len = bytes.len();
    bytes[len - 4..].fill(0);
    bytes
}
