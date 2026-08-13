//! Offline command boundary for canonicalizing retained Toggle CSV output.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

#[path = "../../../crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs"]
mod canonicalizer;

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("oce-openmodelica-toggle-reference: {error}");
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
        return Err(
            "usage: oce-openmodelica-toggle-reference canonicalize[-inspect] INPUT OUTPUT TABLE_NAME [METADATA] | schedule-mismatches INPUT".into(),
        );
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
        let group_sizes = canonical
            .group_sizes
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let raw_time_bits = canonical
            .raw_rows
            .iter()
            .map(|row| format!("{:016x}", row.time.to_bits()))
            .collect::<Vec<_>>()
            .join(",");
        let canonical_time_bits = canonical
            .rows
            .iter()
            .map(|row| format!("{:016x}", row.time.to_bits()))
            .collect::<Vec<_>>()
            .join(",");
        write_new(
            &metadata,
            format!(
                "raw_rows={}\ncanonical_rows={}\ngroup_sizes={group_sizes}\nraw_time_bits={raw_time_bits}\ncanonical_time_bits={canonical_time_bits}\n",
                canonical.raw_rows.len(),
                canonical.rows.len(),
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}

fn schedule_mismatches(path: &Path) -> Result<Vec<usize>, String> {
    const EXPECTED: &[(u64, bool, bool)] = &[
        (0x0000_0000_0000_0000, true, false),
        (0x403e_0000_0000_1dff, false, false),
        (0x404e_0000_0000_0000, false, false),
        (0x4056_8000_0000_0780, true, false),
        (0x405e_0000_0000_0000, true, false),
        (0x4062_c000_0000_03c1, false, false),
        (0x4066_8000_0000_0000, false, false),
        (0x406a_4000_0000_03c1, true, false),
        (0x406e_0000_0000_0000, true, false),
        (0x4070_e000_0000_01e0, false, false),
        (0x4072_c000_0000_0000, false, false),
        (0x4073_6000_0000_0320, false, true),
        (0x4075_e000_0000_02d0, false, false),
        (0x4076_8000_0000_0000, false, false),
        (0x4078_6000_0000_03c1, true, true),
        (0x407a_4000_0000_0000, true, true),
        (0x407a_e000_0000_0320, true, false),
        (0x407c_2000_0000_03c1, false, false),
        (0x407e_0000_0000_0000, false, false),
        (0x407f_e000_0000_03c1, true, false),
        (0x4080_e000_0000_0000, true, false),
        (0x4082_c000_0000_0000, true, false),
    ];
    let input = canonicalizer::read_bounded_path(path).map_err(|error| error.to_string())?;
    let text = std::str::from_utf8(&input).map_err(|error| error.to_string())?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != EXPECTED.len() + 3
        || lines[0] != "#1"
        || lines[1] != "# columns: time u clr y"
    {
        return Err("canonical schedule has an unsupported shape or header".into());
    }
    let mut mismatches = Vec::new();
    for (index, (line, expected)) in lines[3..].iter().zip(EXPECTED).enumerate() {
        let cells = line.split_ascii_whitespace().collect::<Vec<_>>();
        if cells.len() != 4 {
            return Err(format!(
                "canonical schedule row {index} does not have four cells"
            ));
        }
        let time = cells[0]
            .parse::<f64>()
            .map_err(|_| format!("canonical schedule row {index} time is invalid"))?;
        let boolean = |value: &str| match value {
            "0.0" => Ok(false),
            "1.0" => Ok(true),
            _ => Err(format!("canonical schedule row {index} is not Boolean")),
        };
        let actual = (time.to_bits(), boolean(cells[1])?, boolean(cells[2])?);
        if actual != *expected {
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
