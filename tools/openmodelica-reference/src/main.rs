//! Offline command-line boundary for canonicalizing captured OpenModelica CSV output.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

#[path = "../../../crates/oce-cxf/tests/open_modelica_reference/canonicalizer.rs"]
mod canonicalizer;

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("oce-openmodelica-reference: {error}");
        std::process::exit(1);
    }
}

fn run(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if arguments.next().as_deref() != Some("canonicalize") {
        return Err(
            "usage: oce-openmodelica-reference canonicalize INPUT OUTPUT TABLE_NAME".into(),
        );
    }
    let input = arguments.next().ok_or("missing input path")?;
    let output = arguments.next().ok_or("missing output path")?;
    let table_name = arguments.next().ok_or("missing table name")?;
    if arguments.next().is_some() {
        return Err("unexpected trailing argument".into());
    }
    let canonical = canonicalizer::canonicalize_path(Path::new(&input), &table_name)
        .map_err(|error| error.to_string())?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| format!("cannot create {output}: {error}"))?;
    destination
        .write_all(&canonical.bytes)
        .map_err(|error| format!("cannot write {output}: {error}"))?;
    Ok(())
}
