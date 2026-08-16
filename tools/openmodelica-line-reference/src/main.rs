//! Offline command boundary for canonicalizing retained Line CSV output.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

#[path = "../../../crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"]
mod canonicalizer;

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("oce-openmodelica-line-reference: {error}");
        std::process::exit(1);
    }
}

fn run(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let command = arguments.next().ok_or("missing command")?;
    if command == "schedule-mismatches" {
        let input = arguments.next().ok_or("missing canonical input path")?;
        if arguments.next().is_some() {
            return Err("unexpected trailing argument".into());
        }
        println!(
            "schedule_mismatch_rows={}",
            schedule_mismatches(Path::new(&input))?
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        return Ok(());
    }
    if command == "verify-architecture-canonical" {
        let directory = arguments.next().ok_or("missing architecture evidence path")?;
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
        return Err("usage: oce-openmodelica-line-reference canonicalize[-inspect] INPUT OUTPUT TABLE_NAME [METADATA] | canonicalize-first-inspect INPUT OUTPUT TABLE_NAME METADATA | schedule-mismatches INPUT | verify-architecture-canonical DIRECTORY".into());
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
    let canonical = canonicalizer::canonicalize_path_with_selection(
        Path::new(&input),
        &table_name,
        selection,
    )
    .map_err(|error| error.to_string())?;
    write_new(&output, &canonical.bytes)?;
    if let Some(metadata) = metadata {
        write_new(&metadata, &inspection_metadata(&canonical, selection))?;
    }
    Ok(())
}

fn verify_architecture_canonical(directory: &Path) -> Result<(), String> {
    let retained = canonicalizer::read_bounded_path(&directory.join("line.canonical.csv"))
        .map_err(|error| error.to_string())?;
    let control = canonicalizer::read_bounded_path(&directory.join("flag-control.canonical.csv"))
        .map_err(|error| error.to_string())?;
    for bytes in [&retained, &control] {
        if bytes.contains(&b'\r') || !bytes.ends_with(b"\n") {
            return Err("retained canonical output must use LF records and a final LF".into());
        }
    }
    for raw in ["line-run-a.raw.csv", "line-run-b.raw.csv"] {
        let reproduced = canonicalizer::canonicalize_path(
            &directory.join(raw),
            "openmodelica_reals_line",
        )
        .map_err(|error| format!("{raw}: {error}"))?;
        if reproduced.bytes != retained {
            return Err(format!(
                "{raw} does not reproduce retained strict canonical bytes"
            ));
        }
    }
    let reproduced = canonicalizer::canonicalize_path(
        &directory.join("flag-control.raw.csv"),
        "openmodelica_reals_line_flag_control",
    )
    .map_err(|error| format!("flag-control.raw.csv: {error}"))?;
    if reproduced.bytes != control {
        return Err("flag-control.raw.csv does not reproduce retained strict canonical bytes".into());
    }
    let mutation = canonicalizer::canonicalize_path_with_selection(
        &directory.join("line-run-a.raw.csv"),
        "openmodelica_reals_line",
        canonicalizer::ProjectionSelection::First,
    )
    .map_err(|error| format!("keep-first projection: {error}"))?;
    let retained_mutation = canonicalizer::read_bounded_path(
        &directory.join("projection-keep-first.canonical.csv"),
    )
    .map_err(|error| error.to_string())?;
    if mutation.bytes != retained_mutation || mutation.bytes == retained {
        return Err("retained keep-first output is not the executed projection control".into());
    }
    let retained_metadata =
        canonicalizer::read_bounded_path(&directory.join("projection-keep-first.metadata"))
            .map_err(|error| error.to_string())?;
    if inspection_metadata(&mutation, canonicalizer::ProjectionSelection::First)
        != retained_metadata
    {
        return Err("retained keep-first inspection metadata is not reproducible".into());
    }
    if schedule_mismatches(&directory.join("projection-keep-first.canonical.csv"))?
        != [2, 4, 6, 8]
    {
        return Err("keep-first schedule mismatches are not the pinned event rows".into());
    }
    Ok(())
}

fn inspection_metadata(
    canonical: &canonicalizer::Canonicalization,
    selection: canonicalizer::ProjectionSelection,
) -> Vec<u8> {
    let joined = |values: Vec<String>| values.join(",");
    format!(
        "selection={}\nraw_rows={}\ncanonical_rows={}\ngroup_sizes={}\nraw_time_bits={}\ncanonical_time_bits={}\n",
        if selection == canonicalizer::ProjectionSelection::First { "first" } else { "last" },
        canonical.raw_rows.len(),
        canonical.rows.len(),
        joined(canonical.group_sizes.iter().map(usize::to_string).collect()),
        joined(canonical.raw_rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
        joined(canonical.rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
    )
    .into_bytes()
}

fn schedule_mismatches(path: &Path) -> Result<Vec<usize>, String> {
    const EXPECTED_TIME: &[u64] = &[
        0x0000_0000_0000_0000,
        0x404e_0000_0000_0000,
        0x404e_0000_0000_0eff,
        0x405e_0000_0000_0000,
        0x405e_0000_0000_0781,
        0x4066_8000_0000_0000,
        0x4066_8000_0000_03c1,
        0x406e_0000_0000_0000,
        0x406e_0000_0000_03c1,
        0x4072_c000_0000_0000,
    ];
    const EXPECTED_U: &[u64] = &[
        0xc010_0000_0000_0000,
        0xc010_0000_0000_0000,
        0xc000_0000_0000_0000,
        0xc000_0000_0000_0000,
        0,
        0,
        0x4000_0000_0000_0000,
        0x4000_0000_0000_0000,
        0x4010_0000_0000_0000,
        0x4010_0000_0000_0000,
    ];
    let input = canonicalizer::read_bounded_path(path).map_err(|error| error.to_string())?;
    let text = std::str::from_utf8(&input).map_err(|error| error.to_string())?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != EXPECTED_TIME.len() + 3
        || lines[0] != "#1"
        || lines[1] != "# columns: time x1 f1 x2 f2 u yBoth yBelow yAbove yUnlimited"
    {
        return Err("canonical schedule has an unsupported shape or header".into());
    }
    let mut mismatches = Vec::new();
    for (index, line) in lines[3..].iter().enumerate() {
        let cells = line.split_ascii_whitespace().collect::<Vec<_>>();
        if cells.len() != 10 {
            return Err(format!("canonical schedule row {index} does not have ten cells"));
        }
        let values = cells[..6]
            .iter()
            .map(|cell| cell.parse::<f64>().map_err(|_| format!("canonical schedule row {index} is invalid")))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = [
            values[0].to_bits(),
            values[1].to_bits(),
            values[2].to_bits(),
            values[3].to_bits(),
            values[4].to_bits(),
            values[5].to_bits(),
        ];
        let expected = [
            EXPECTED_TIME[index],
            0xc000_0000_0000_0000,
            0x3ff4_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x400a_0000_0000_0000,
            EXPECTED_U[index],
        ];
        if actual != expected {
            mismatches.push(index);
        }
    }
    Ok(mismatches)
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
