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
    if !matches!(command.as_str(), "canonicalize" | "canonicalize-inspect") {
        return Err("usage: oce-openmodelica-line-reference canonicalize[-inspect] INPUT OUTPUT TABLE_NAME [METADATA] | schedule-mismatches INPUT".into());
    }
    let input = arguments.next().ok_or("missing input path")?;
    let output = arguments.next().ok_or("missing output path")?;
    let table_name = arguments.next().ok_or("missing table name")?;
    let metadata = if command == "canonicalize-inspect" {
        Some(arguments.next().ok_or("missing metadata path")?)
    } else {
        None
    };
    if arguments.next().is_some() {
        return Err("unexpected trailing argument".into());
    }
    let canonical = canonicalizer::canonicalize_path(Path::new(&input), &table_name)
        .map_err(|error| error.to_string())?;
    write_new(&output, &canonical.bytes)?;
    if let Some(metadata) = metadata {
        let joined = |values: Vec<String>| values.join(",");
        write_new(
            &metadata,
            format!(
                "raw_rows={}\ncanonical_rows={}\ngroup_sizes={}\nraw_time_bits={}\ncanonical_time_bits={}\n",
                canonical.raw_rows.len(),
                canonical.rows.len(),
                joined(canonical.group_sizes.iter().map(usize::to_string).collect()),
                joined(canonical.raw_rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
                joined(canonical.rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
            )
            .as_bytes(),
        )?;
    }
    Ok(())
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
