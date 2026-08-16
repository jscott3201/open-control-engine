//! Executed keep-first projection control and retained inspection evidence.

use std::fmt::Write as _;
use std::path::Path;

use super::canonicalizer::{self, Canonicalization, ProjectionSelection, RealRow};
use super::expectations::{TIME_BITS, U_BITS};
use super::schema::{Artifact, Manifest};

pub(super) fn validate(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let canonicalizer_digest = &artifact(manifest, "canonicalizer_source")?.sha256;
    for architecture in &manifest.architectures {
        let prefix = &architecture.name;
        let record = &architecture.projection_mutation;
        let raw = artifact(manifest, &format!("{prefix}_raw_run_a_csv"))?;
        let output = artifact(
            manifest,
            &format!("{prefix}_projection_keep_first_canonical_csv"),
        )?;
        let metadata = artifact(
            manifest,
            &format!("{prefix}_projection_keep_first_metadata"),
        )?;
        let log = artifact(manifest, &format!("{prefix}_projection_mutation_log"))?;
        if record.control != "explicit_keep_first"
            || record.input != "line-run-a.raw.csv"
            || record.input_sha256 != raw.sha256
            || record.input_sha256 != architecture.raw_run_a_sha256
            || record.canonical_output != "projection-keep-first.canonical.csv"
            || record.canonical_sha256 != output.sha256
            || record.metadata != "projection-keep-first.metadata"
            || record.metadata_sha256 != metadata.sha256
            || record.log != "projection-mutation.log"
            || record.log_sha256 != log.sha256
            || record.schedule_mismatch_rows != [2, 4, 6, 8]
        {
            return Err(format!("{prefix} projection mutation record is not closed"));
        }
        let raw_bytes = super::safe_read::read(root, &raw.path)?;
        let normal = canonicalizer::canonicalize_bytes(&raw_bytes, "openmodelica_reals_line")
            .map_err(|error| error.to_string())?;
        let mutation = canonicalizer::canonicalize_path_with_selection(
            &root.join(&raw.path),
            "openmodelica_reals_line",
            ProjectionSelection::First,
        )
        .map_err(|error| error.to_string())?;
        validate_execution(&normal, &mutation)?;
        if mutation.bytes != super::safe_read::read(root, &output.path)? {
            return Err(format!(
                "{prefix} retained keep-first bytes are not reproducible"
            ));
        }
        if metadata_bytes(&mutation) != super::safe_read::read(root, &metadata.path)? {
            return Err(format!("{prefix} keep-first metadata is not reproducible"));
        }
        let expected_log = log_bytes(record, &mutation, canonicalizer_digest);
        if expected_log != super::safe_read::read(root, &log.path)? {
            return Err(format!(
                "{prefix} keep-first execution log is not reproducible"
            ));
        }
    }
    Ok(())
}

fn validate_execution(
    normal: &Canonicalization,
    mutation: &Canonicalization,
) -> Result<(), String> {
    if mutation.raw_rows != normal.raw_rows
        || mutation.group_sizes != normal.group_sizes
        || mutation
            .rows
            .iter()
            .map(|row| row.time.to_bits())
            .ne(normal.rows.iter().map(|row| row.time.to_bits()))
    {
        return Err("keep-first execution changed raw rows, groups, or timestamp bits".into());
    }
    if mutation.bytes == normal.bytes || mutation.rows == normal.rows {
        return Err("keep-first execution is a no-op".into());
    }
    let mismatches = schedule_mismatch_rows(&mutation.rows);
    if mismatches != [2, 4, 6, 8] {
        return Err(format!(
            "keep-first schedule mismatch rows are {mismatches:?}"
        ));
    }
    Ok(())
}

fn schedule_mismatch_rows(rows: &[RealRow]) -> Vec<u64> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let actual = [
                row.time.to_bits(),
                row.x1.to_bits(),
                row.f1.to_bits(),
                row.x2.to_bits(),
                row.f2.to_bits(),
                row.u.to_bits(),
            ];
            let expected = [
                parse_bits(TIME_BITS[index]),
                0xc000_0000_0000_0000,
                0x3ff4_0000_0000_0000,
                0x4000_0000_0000_0000,
                0x400a_0000_0000_0000,
                parse_bits(U_BITS[index]),
            ];
            (actual != expected).then_some(index as u64)
        })
        .collect()
}

fn metadata_bytes(value: &Canonicalization) -> Vec<u8> {
    let joined = |values: Vec<String>| values.join(",");
    format!(
        "selection=first\nraw_rows={}\ncanonical_rows={}\ngroup_sizes={}\nraw_time_bits={}\ncanonical_time_bits={}\n",
        value.raw_rows.len(),
        value.rows.len(),
        joined(value.group_sizes.iter().map(usize::to_string).collect()),
        joined(value.raw_rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
        joined(value.rows.iter().map(|row| format!("{:016x}", row.time.to_bits())).collect()),
    )
    .into_bytes()
}

fn log_bytes(
    record: &super::schema::ProjectionMutation,
    mutation: &Canonicalization,
    canonicalizer_digest: &str,
) -> Vec<u8> {
    let raw_times = mutation
        .raw_rows
        .iter()
        .map(|row| format!("{:016x}", row.time.to_bits()))
        .collect::<Vec<_>>()
        .join(",");
    let canonical_times = mutation
        .rows
        .iter()
        .map(|row| format!("{:016x}", row.time.to_bits()))
        .collect::<Vec<_>>()
        .join(",");
    let groups = mutation
        .group_sizes
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut output = String::new();
    for line in [
        "projection_mutation=contiguous equal-time selection changed from last to first".into(),
        "execution_path=canonicalize-first-inspect".into(),
        format!("mutated_input={}", record.input),
        format!("mutated_input_sha256={}", record.input_sha256),
        format!("mutated_output={}", record.canonical_output),
        format!("mutated_output_sha256={}", record.canonical_sha256),
        format!("mutated_metadata={}", record.metadata),
        format!("mutated_metadata_sha256={}", record.metadata_sha256),
        format!("mutated_raw_rows={}", mutation.raw_rows.len()),
        format!("mutated_canonical_rows={}", mutation.rows.len()),
        format!("mutated_group_sizes={groups}"),
        format!("mutated_raw_time_bits={raw_times}"),
        format!("mutated_canonical_time_bits={canonical_times}"),
        "mutated_schedule_result=FAIL".into(),
        "mutated_schedule_mismatch_rows=2,4,6,8".into(),
        "mutated_schedule_first_mismatch_row=2".into(),
        "mutated_schedule_first_mismatch_time_bits=404e000000000eff".into(),
        "mutated_output_differs_from_keep_last=PASS".into(),
        "mutated_grouping_result=PASS".into(),
        "mutated_timestamp_bits_result=PASS".into(),
        "explicit_keep_first_execution=PASS".into(),
        format!("executed_canonicalizer_sha256={canonicalizer_digest}"),
    ] {
        let _ = writeln!(output, "{line}");
    }
    output.into_bytes()
}

fn artifact<'a>(manifest: &'a Manifest, role: &str) -> Result<&'a Artifact, String> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.role == role)
        .ok_or_else(|| format!("missing projection artifact role {role}"))
}

fn parse_bits(value: &str) -> u64 {
    u64::from_str_radix(value, 16).expect("fixed bit literal")
}
