//! Offline command boundary for canonicalizing retained Reliefs CSV output.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

#[path = "../../../crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs"]
mod canonicalizer;

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("oce-openmodelica-reliefs-reference: {error}");
        std::process::exit(1);
    }
}

fn run(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let command = arguments.next().ok_or("missing command")?;
    if command == "verify-architecture-canonical" {
        let directory = arguments
            .next()
            .ok_or("missing architecture evidence path")?;
        if arguments.next().is_some() {
            return Err("unexpected trailing argument".into());
        }
        verify_architecture_canonical(Path::new(&directory))?;
        println!("strict canonical boundary passed");
        return Ok(());
    }
    if !matches!(
        command.as_str(),
        "canonicalize" | "canonicalize-inspect" | "canonicalize-first-inspect"
    ) {
        return Err("usage: oce-openmodelica-reliefs-reference canonicalize[-inspect] INPUT OUTPUT TABLE_NAME [METADATA] | canonicalize-first-inspect INPUT OUTPUT TABLE_NAME METADATA | verify-architecture-canonical DIRECTORY".into());
    }
    let input = arguments.next().ok_or("missing input path")?;
    let output = arguments.next().ok_or("missing output path")?;
    let table_name = arguments.next().ok_or("missing table name")?;
    let metadata = if matches!(
        command.as_str(),
        "canonicalize-inspect" | "canonicalize-first-inspect"
    ) {
        Some(arguments.next().ok_or("missing metadata path")?)
    } else {
        None
    };
    if arguments.next().is_some() {
        return Err("unexpected trailing argument".into());
    }
    let selection = if command == "canonicalize-first-inspect" {
        canonicalizer::ProjectionSelection::First
    } else {
        canonicalizer::ProjectionSelection::Last
    };
    let canonical =
        canonicalizer::canonicalize_path_with_selection(Path::new(&input), &table_name, selection)
            .map_err(|error| error.to_string())?;
    write_new(&output, &canonical.bytes)?;
    if let Some(metadata) = metadata {
        write_new(&metadata, &inspection_metadata(&canonical, selection))?;
    }
    Ok(())
}

fn verify_architecture_canonical(directory: &Path) -> Result<(), String> {
    let retained = read(directory, "reliefs.canonical.csv")?;
    let parameter = read(directory, "parameter-control.canonical.csv")?;
    let clamp = read(directory, "final-clamp.canonical.csv")?;
    for bytes in [&retained, &parameter, &clamp] {
        if bytes.contains(&b'\r') || !bytes.ends_with(b"\n") {
            return Err("retained canonical output must use LF records and a final LF".into());
        }
    }
    for raw in ["reliefs-run-a.raw.csv", "reliefs-run-b.raw.csv"] {
        let reproduced =
            canonicalizer::canonicalize_path(&directory.join(raw), "openmodelica_g36_reliefs")
                .map_err(|error| format!("{raw}: {error}"))?;
        if reproduced.bytes != retained {
            return Err(format!(
                "{raw} does not reproduce retained strict canonical bytes"
            ));
        }
    }
    for (raw, table, expected) in [
        (
            "parameter-control.raw.csv",
            "openmodelica_g36_reliefs_parameter_control",
            parameter,
        ),
        (
            "final-clamp.raw.csv",
            "openmodelica_g36_reliefs_final_clamp",
            clamp,
        ),
    ] {
        let reproduced = canonicalizer::canonicalize_path(&directory.join(raw), table)
            .map_err(|error| format!("{raw}: {error}"))?;
        if reproduced.bytes != expected {
            return Err(format!(
                "{raw} does not reproduce retained strict canonical bytes"
            ));
        }
    }
    let mutation = canonicalizer::canonicalize_path_with_selection(
        &directory.join("reliefs-run-a.raw.csv"),
        "openmodelica_g36_reliefs",
        canonicalizer::ProjectionSelection::First,
    )
    .map_err(|error| format!("keep-first projection: {error}"))?;
    let retained_mutation = read(directory, "projection-keep-first.canonical.csv")?;
    if mutation.bytes != retained_mutation || mutation.bytes == retained {
        return Err("retained keep-first output is not the executed projection control".into());
    }
    if inspection_metadata(&mutation, canonicalizer::ProjectionSelection::First)
        != read(directory, "projection-keep-first.metadata")?
    {
        return Err("retained keep-first inspection metadata is not reproducible".into());
    }
    Ok(())
}

fn read(directory: &Path, name: &str) -> Result<Vec<u8>, String> {
    canonicalizer::read_bounded_path(&directory.join(name)).map_err(|error| error.to_string())
}

fn inspection_metadata(
    canonical: &canonicalizer::Canonicalization,
    selection: canonicalizer::ProjectionSelection,
) -> Vec<u8> {
    let joined = |values: Vec<String>| values.join(",");
    let row_bits = |row: &canonicalizer::ReliefRow| {
        let mut cells = vec![format!("{:016x}", row.time.to_bits())];
        cells.extend(row.input_bits().map(|bits| format!("{bits:016x}")));
        cells.extend(row.output_bits().map(|bits| format!("{bits:016x}")));
        cells.join(":")
    };
    format!(
        "selection={}\nraw_rows={}\ngrouped_rows={}\ncanonical_rows={}\ngroup_sizes={}\nraw_time_bits={}\nselected_source_rows={}\nselected_time_bits={}\nselected_rows={}\n",
        if selection == canonicalizer::ProjectionSelection::First {
            "first"
        } else {
            "last"
        },
        canonical.raw_rows.len(),
        canonical.grouped_rows.len(),
        canonical.rows.len(),
        joined(canonical.group_sizes.iter().map(usize::to_string).collect()),
        joined(
            canonical
                .raw_rows
                .iter()
                .map(|row| format!("{:016x}", row.time.to_bits()))
                .collect()
        ),
        joined(
            canonical
                .rows
                .iter()
                .map(|row| row.source_index.to_string())
                .collect()
        ),
        joined(
            canonical
                .rows
                .iter()
                .map(|row| format!("{:016x}", row.time.to_bits()))
                .collect()
        ),
        canonical.rows.iter().map(row_bits).collect::<Vec<_>>().join(";")
    )
    .into_bytes()
}

fn write_new(path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {path}: {error}"))?;
    destination
        .write_all(bytes)
        .map_err(|error| format!("cannot write {path}: {error}"))
}
