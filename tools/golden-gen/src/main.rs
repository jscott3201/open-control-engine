//! golden-gen — Tier-A oracle generator.
//!
//! Emits closed-form CDL reference goldens as Modelica `CombiTimeTable` CSV under
//! `tools/golden-gen/goldens/<class_path>/<signal>.csv`, a sibling `<signal>.prov.json` per golden,
//! one per-block `reference.csv` containing `time`, machine-readable inputs, and all outputs, and
//! a crate-root `oracle.lock` toolchain/version pin skeleton.
//!
//! ANTI-TAUTOLOGY: all reference math is re-derived independently from `_spec/03`, `_spec/02`,
//! `_spec/01`, `_spec/07` (format only) and CDL §7.x. This crate has ZERO dependency on
//! `oce-blocks` (the implementation under test) and never reads it.

mod csv;
mod discrete_sources;
mod integers_conversions;
mod integers_stage;
mod logical;
mod logical_proof;
mod logical_variable_pulse;
mod oracle;
mod reals;
mod reals_pid;
mod reals_ramp;
mod reals_scalar_arithmetic;
mod reals_sources;
mod reals_transcendental;
mod sequences;
mod source_pulse;

use std::fs;
use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use csv::{DataColumn, SignalColumn};
use oracle::{Golden, InputSeries, Sample, ValueKind};

type GoldenGroupKey = (&'static str, Option<&'static str>);

/// Generator version, recorded in every provenance record and `oracle.lock`.
///
/// Derived from `CARGO_PKG_VERSION` so the advertised version can never skew from the crate manifest.
const GENERATOR_VERSION: &str = concat!("golden-gen ", env!("CARGO_PKG_VERSION"));

fn main() {
    // The crate root is this file's directory parent (`tools/golden-gen`). Use CARGO_MANIFEST_DIR
    // so the tool is invariant to the working directory it is launched from.
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let goldens_root = crate_root.join("goldens");

    // Re-derive every golden (closed-form spec math).
    let mut goldens: Vec<Golden> = Vec::new();
    goldens.extend(reals::goldens());
    goldens.extend(reals_scalar_arithmetic::goldens());
    goldens.extend(reals_sources::goldens());
    goldens.extend(reals_ramp::goldens());
    goldens.extend(reals_transcendental::goldens());
    goldens.extend(source_pulse::goldens());
    goldens.extend(reals_pid::goldens());
    goldens.extend(logical::goldens());
    goldens.extend(logical_proof::goldens());
    goldens.extend(logical_variable_pulse::goldens());
    goldens.extend(integers_conversions::goldens());
    goldens.extend(integers_stage::goldens());
    goldens.extend(discrete_sources::goldens());
    goldens.extend(sequences::goldens());

    assert_integer_csv_cells_are_exact(&goldens);

    // Clean and recreate the goldens tree so removed entries never linger (deterministic output).
    if goldens_root.exists() {
        fs::remove_dir_all(&goldens_root).expect("clear goldens tree");
    }
    fs::create_dir_all(&goldens_root).expect("create goldens root");

    let grouped = group_by_class(&goldens);
    let mut manifest_lines: Vec<String> = Vec::new();
    for g in &goldens {
        let dir_rel = golden_dir_relative(g.class_path, g.scenario);
        let dir = goldens_root.join(&dir_rel);
        fs::create_dir_all(&dir).expect("create golden dir");

        // Table name: a Modelica-identifier-safe slug of class_path + signal.
        let table_name = table_name(g.class_path, g.scenario, g.signal);
        let col = SignalColumn {
            name: g.signal.to_string(),
            time: g.time.clone(),
            values: g.samples.iter().map(|s| s.encode()).collect(),
        };
        let csv_text = csv::to_csv_string(&table_name, &col);
        let csv_path = dir.join(format!("{}.csv", g.signal));
        fs::write(&csv_path, csv_text).expect("write golden csv");

        let prov_path = dir.join(format!("{}.prov.json", g.signal));
        fs::write(
            &prov_path,
            prov_json(g, grouped.get(&golden_key(g)).expect("grouped golden")),
        )
        .expect("write prov json");

        manifest_lines.push(format!(
            "{} {} -> goldens/{}/{}.csv",
            golden_manifest_name(g.class_path, g.scenario),
            g.signal,
            dir_rel,
            g.signal
        ));
    }
    write_reference_csvs(&goldens_root, &grouped, &mut manifest_lines);

    // Non-steppable fold-time references (CDL.Constants, CDL.Types): provenance only, no CSV.
    write_constants_types(&goldens_root, &mut manifest_lines);
    write_deferred_provenance(&goldens_root, &mut manifest_lines);

    // oracle.lock skeleton (toolchain / version pins).
    fs::write(crate_root.join("oracle.lock"), oracle_lock()).expect("write oracle.lock");

    // Deterministic manifest of everything emitted.
    manifest_lines.sort();
    let mut manifest = String::new();
    manifest.push_str("# golden-gen manifest (class_path signal -> file). Auto-generated.\n");
    for line in &manifest_lines {
        manifest.push_str(line);
        manifest.push('\n');
    }
    fs::write(goldens_root.join("MANIFEST.txt"), &manifest).expect("write manifest");

    assert_provenance_json_is_strict(&goldens_root);

    println!("golden-gen: emitted {} signal goldens", goldens.len());
    print!("{manifest}");
}

/// Group output-signal goldens by block class path and scenario, preserving family emission order.
fn group_by_class(goldens: &[Golden]) -> BTreeMap<GoldenGroupKey, Vec<&Golden>> {
    let mut grouped: BTreeMap<GoldenGroupKey, Vec<&Golden>> = BTreeMap::new();
    for golden in goldens {
        grouped.entry(golden_key(golden)).or_default().push(golden);
    }
    grouped
}

fn golden_key(golden: &Golden) -> GoldenGroupKey {
    (golden.class_path, golden.scenario)
}

/// Emit one self-contained driver-ready reference table per steppable block.
fn write_reference_csvs(
    goldens_root: &Path,
    grouped: &BTreeMap<GoldenGroupKey, Vec<&Golden>>,
    manifest_lines: &mut Vec<String>,
) {
    for (&(class_path, scenario), group) in grouped {
        let dir_rel = golden_dir_relative(class_path, scenario);
        let dir = goldens_root.join(&dir_rel);
        let first = group.first().expect("non-empty golden group");
        assert_group_consistent(group);

        let mut cols = Vec::new();
        for input in &first.inputs {
            cols.push(DataColumn {
                name: input.name.to_string(),
                values: input.samples.iter().map(|s| s.encode()).collect(),
            });
        }
        for golden in group {
            cols.push(DataColumn {
                name: golden.signal.to_string(),
                values: golden.samples.iter().map(|s| s.encode()).collect(),
            });
        }

        let table_name = table_name(class_path, scenario, "reference");
        let csv_text = csv::to_table_csv_string(&table_name, &first.time, &cols);
        fs::write(dir.join("reference.csv"), csv_text).expect("write reference csv");
        manifest_lines.push(format!(
            "{} reference -> goldens/{}/reference.csv",
            golden_manifest_name(class_path, scenario),
            dir_rel
        ));
    }
}

/// Multi-output blocks must share one time grid and one input replay table.
fn assert_group_consistent(group: &[&Golden]) {
    let first = group.first().expect("non-empty golden group");
    for golden in group.iter().skip(1) {
        assert_eq!(
            first.time.len(),
            golden.time.len(),
            "{} output {} time length mismatch",
            golden.class_path,
            golden.signal
        );
        for (idx, (a, b)) in first.time.iter().zip(&golden.time).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{} output {} time[{idx}] mismatch",
                golden.class_path,
                golden.signal
            );
        }
        assert_eq!(
            first.inputs.len(),
            golden.inputs.len(),
            "{} output {} input column count mismatch",
            golden.class_path,
            golden.signal
        );
        for (left, right) in first.inputs.iter().zip(&golden.inputs) {
            assert_eq!(
                left.name, right.name,
                "{} output {} input name mismatch",
                golden.class_path, golden.signal
            );
            assert_eq!(
                left.kind, right.kind,
                "{} output {} input kind mismatch for {}",
                golden.class_path, golden.signal, left.name
            );
            assert_samples_bit_eq(left, right, golden);
        }
    }
}

fn assert_samples_bit_eq(left: &InputSeries, right: &InputSeries, golden: &Golden) {
    assert_eq!(
        left.samples.len(),
        right.samples.len(),
        "{} output {} input {} length mismatch",
        golden.class_path,
        golden.signal,
        left.name
    );
    for (idx, (a, b)) in left.samples.iter().zip(&right.samples).enumerate() {
        assert_eq!(
            sample_bits(a),
            sample_bits(b),
            "{} output {} input {} sample[{idx}] mismatch",
            golden.class_path,
            golden.signal,
            left.name
        );
    }
}

fn sample_bits(sample: &Sample) -> u64 {
    sample.encode().to_bits()
}

/// Map `CDL.Reals.Add` -> `CDL/Reals/Add` for the on-disk directory layout.
fn class_path_to_dir(class_path: &str) -> String {
    class_path.replace('.', "/")
}

fn golden_dir_relative(class_path: &str, scenario: Option<&str>) -> String {
    match scenario {
        Some(scenario) => format!("{}/{}", class_path_to_dir(class_path), scenario),
        None => class_path_to_dir(class_path),
    }
}

fn table_name(class_path: &str, scenario: Option<&str>, suffix: &str) -> String {
    let raw = match scenario {
        Some(scenario) => format!("{class_path}_{scenario}_{suffix}"),
        None => format!("{class_path}_{suffix}"),
    };
    sanitize_table_name(&raw)
}

fn golden_manifest_name(class_path: &str, scenario: Option<&str>) -> String {
    match scenario {
        Some(scenario) => format!("{class_path}/{scenario}"),
        None => class_path.to_string(),
    }
}

/// Produce a Modelica-identifier-safe table name (letters/digits/underscore only).
fn sanitize_table_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

/// Minimal JSON string escaper for provenance string fields.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_string_array(values: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (idx, value) in values.into_iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&json_escape(&value));
        out.push('"');
    }
    out.push(']');
    out
}

fn json_sample_values(samples: &[Sample]) -> String {
    let mut out = String::from("[");
    for (idx, sample) in samples.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&json_sample_value(sample));
    }
    out.push(']');
    out
}

fn json_sample_value(sample: &Sample) -> String {
    match sample {
        Sample::Real(x) if !x.is_finite() => {
            if x.is_nan() {
                "\"NaN\"".to_string()
            } else if x.is_sign_positive() {
                "\"inf\"".to_string()
            } else {
                "\"-inf\"".to_string()
            }
        }
        _ => csv::format_f64(sample.encode()),
    }
}

fn input_series_json(inputs: &[InputSeries]) -> String {
    if inputs.is_empty() {
        return "[]".to_string();
    }
    let mut out = String::from("[\n");
    for (idx, input) in inputs.iter().enumerate() {
        if idx > 0 {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            concat!(
                "    {{ \"name\": \"{name}\", \"value_kind\": \"{kind}\", \"values\": {values} }}"
            ),
            name = json_escape(input.name),
            kind = input.kind.as_str(),
            values = json_sample_values(&input.samples),
        ));
    }
    out.push_str("\n  ]");
    out
}

fn reference_columns(group: &[&Golden]) -> Vec<String> {
    let first = group.first().expect("non-empty golden group");
    let mut columns = Vec::with_capacity(1 + first.inputs.len() + group.len());
    columns.push("time".to_string());
    columns.extend(first.inputs.iter().map(|input| input.name.to_string()));
    columns.extend(group.iter().map(|golden| golden.signal.to_string()));
    columns
}

fn provenance_source(g: &Golden) -> &'static str {
    if g.class_path == "G36" {
        "hand-derived G36 sequence reference from fixture topology plus CDL / Buildings .mo semantics; independent re-derivation"
    } else if matches!(g.class_path, "CDL.Reals.PID" | "CDL.Reals.PIDWithReset") {
        "closed-form from _spec/03 R-REALS-2 plus Buildings CDL.Reals.PID.mo/PIDWithReset.mo wiring; independent re-derivation"
    } else if g.class_path == "CDL.Reals.Ramp" {
        "closed-form discrete recurrence from Buildings CDL.Reals.Ramp.mo plus the project-wide implicit Reals dynamics convention; independent re-derivation"
    } else {
        "closed-form from CDL spec (_spec/03,02,01; CDL §7.x); independent re-derivation"
    }
}

fn extra_provenance_json(g: &Golden) -> String {
    let mut out = String::new();
    for (key, value) in &g.extra_provenance {
        out.push_str(&format!(
            "  \"{key}\": \"{value}\",\n",
            key = json_escape(key),
            value = json_escape(value),
        ));
    }
    out
}

fn is_pid_recurrence(g: &Golden) -> bool {
    matches!(g.class_path, "CDL.Reals.PID" | "CDL.Reals.PIDWithReset")
}

fn is_reals_transcendental(g: &Golden) -> bool {
    matches!(
        g.class_path,
        "CDL.Reals.Acos"
            | "CDL.Reals.Asin"
            | "CDL.Reals.Atan"
            | "CDL.Reals.Atan2"
            | "CDL.Reals.Cos"
            | "CDL.Reals.Exp"
            | "CDL.Reals.Log"
            | "CDL.Reals.Log10"
            | "CDL.Reals.Sin"
            | "CDL.Reals.Tan"
    )
}

fn assert_integer_csv_cells_are_exact(goldens: &[Golden]) {
    for golden in goldens {
        // This conversion intentionally probes i64 -> f64 rounding beyond 2^53; its provenance
        // documents the CSV input loss and the unit tier pins the exact integer source behavior.
        if golden.class_path == "CDL.Conversions.IntegerToReal" {
            continue;
        }
        for (idx, sample) in golden.samples.iter().enumerate() {
            assert_integer_sample_is_csv_safe(sample, golden, golden.signal, idx);
        }
        for input in &golden.inputs {
            for (idx, sample) in input.samples.iter().enumerate() {
                assert_integer_sample_is_csv_safe(sample, golden, input.name, idx);
            }
        }
    }
}

fn assert_integer_sample_is_csv_safe(
    sample: &Sample,
    golden: &Golden,
    signal: &str,
    sample_idx: usize,
) {
    let Sample::Integer(value) = sample else {
        return;
    };
    const MAX_EXACT_CSV_INTEGER: i64 = 9_007_199_254_740_992;
    assert!(
        *value >= -MAX_EXACT_CSV_INTEGER && *value <= MAX_EXACT_CSV_INTEGER,
        "{} {} sample {sample_idx} integer {value} is outside the exact f64 CSV range",
        golden.class_path,
        signal
    );
}

/// Build the per-golden provenance JSON record.
fn prov_json(g: &Golden, group: &[&Golden]) -> String {
    // Describe non-finite Real samples by IEEE class so the provenance documents the compare regime.
    let has_non_finite = g
        .samples
        .iter()
        .any(|s| matches!(s, Sample::Real(x) if !x.is_finite()));
    let compare = if g.class_path == "G36" {
        match g.kind {
            ValueKind::Real => {
                "Exact for dyadic Tier-A Reals; zero-tolerance masked funnel for VAV heating-branch constants"
            }
            ValueKind::Integer => "exact encoded integer",
            ValueKind::Boolean => "exact 0.0/1.0",
        }
    } else if is_pid_recurrence(g) {
        "bit-exact Tier-1 f64 recurrence oracle (Value::bit_eq)"
    } else if is_reals_transcendental(g) {
        "aligned finite-Real tolerance for transcendental outputs; Inf/NaN by IEEE class"
    } else {
        match g.kind {
            ValueKind::Real if has_non_finite => {
                "bit-exact for finite; Inf/NaN by IEEE class (Value::bit_eq)"
            }
            ValueKind::Real => "Real banded/bit-exact (rtoly~1e-9; exact f64 ops -> bit-exact)",
            ValueKind::Integer => "exact (atoly=0)",
            ValueKind::Boolean => "exact 0.0/1.0 (atoly=0)",
        }
    };
    let ref_columns = reference_columns(group);
    let scenario_line = g.scenario.map_or_else(String::new, |scenario| {
        format!("  \"scenario\": \"{}\",\n", json_escape(scenario))
    });
    let extra_provenance = extra_provenance_json(g);
    format!(
        concat!(
            "{{\n",
            "  \"class_path\": \"{class_path}\",\n",
            "{scenario_line}",
            "  \"signal\": \"{signal}\",\n",
            "  \"tier\": \"A\",\n",
            "  \"source\": \"{source}\",\n",
            "  \"value_kind\": \"{value_kind}\",\n",
            "  \"compare_regime\": \"{compare}\",\n",
            "  \"n_samples\": {n_samples},\n",
            "  \"n_reference_columns\": {n_reference_columns},\n",
            "  \"reference_csv\": \"reference.csv\",\n",
            "  \"reference_columns\": {reference_columns},\n",
            "  \"inputs\": {inputs},\n",
            "  \"input\": \"{input}\",\n",
            "  \"reference_rule\": \"{rule}\",\n",
            "{extra_provenance}",
            "  \"format\": \"Modelica CombiTimeTable CSV (_spec/07 §9); signal CSV is time+one output; reference.csv is time+inputs+all outputs; ryu shortest-round-trip f64\",\n",
            "  \"generator\": \"{generator}\",\n",
            "  \"depends_on_oce_blocks\": false\n",
            "}}\n"
        ),
        class_path = json_escape(g.class_path),
        scenario_line = scenario_line,
        signal = json_escape(g.signal),
        source = json_escape(provenance_source(g)),
        value_kind = g.kind.as_str(),
        compare = json_escape(compare),
        n_samples = g.samples.len(),
        n_reference_columns = ref_columns.len(),
        reference_columns = json_string_array(ref_columns),
        inputs = input_series_json(&g.inputs),
        input = json_escape(&g.input_desc),
        rule = json_escape(&g.rule_desc),
        extra_provenance = extra_provenance,
        generator = GENERATOR_VERSION,
    )
}

/// Emit provenance-only records for the non-steppable fold-time references (CDL.Constants/Types).
///
/// These are NOT registry/steppable blocks (`_spec/03` §4.9/§4.10): they carry fold-time literal /
/// ordinal references with no tick trace, so there is no CombiTimeTable CSV — only a prov.json that
/// gives an evaluator (oce-expr) its reference values.
fn write_constants_types(goldens_root: &Path, manifest_lines: &mut Vec<String>) {
    // CDL.Constants: the canonical Buildings CDL.Constants.mo values (eps=1E-15, small=1E-37,
    // pi=2*asin(1.0)). Modelica.Constants.{eps,small} aliases resolve to these CDL constants, so
    // the CDL values are authoritative — NOT the Modelica standard-library machine values
    // (eps=2.22e-16, small=1e-60). `inf` is not defined in Buildings CDL.Constants;
    // its value is Modelica.Constants.inf = f64::MAX (largest representable FINITE real, not IEEE
    // +Inf), surfaced via the §7.3 alias whitelist.
    let constants_dir = goldens_root.join("CDL/Constants");
    fs::create_dir_all(&constants_dir).expect("create CDL/Constants dir");
    let pi = std::f64::consts::PI; // bit-identical f64 to Buildings 2*asin(1.0)
    let eps = 1e-15_f64; // Buildings CDL.Constants.eps (biggest x such that 1.0 + x == 1.0)
    let small = 1e-37_f64; // Buildings CDL.Constants.small
    let inf = f64::MAX; // Modelica.Constants.inf = largest representable finite real (§7.3 alias)
    let constants_json = format!(
        concat!(
            "{{\n",
            "  \"class_path\": \"CDL.Constants\",\n",
            "  \"tier\": \"A\",\n",
            "  \"source\": \"Buildings CDL.Constants.mo canonical values (eps=1E-15, small=1E-37, pi=2*asin(1.0)); Modelica.Constants.eps/small are _spec/02 §7.3 aliases resolving to these\",\n",
            "  \"value_kind\": \"Real\",\n",
            "  \"steppable\": false,\n",
            "  \"values\": {{\n",
            "    \"pi\": {pi},\n",
            "    \"eps\": {eps},\n",
            "    \"small\": {small},\n",
            "    \"inf\": {inf}\n",
            "  }},\n",
            "  \"note\": \"inf is NOT defined in Buildings CDL.Constants.mo; value is Modelica.Constants.inf = f64::MAX (largest representable FINITE real, not IEEE +Inf), surfaced via the _spec/02 §7.3 Modelica-alias whitelist.\",\n",
            "  \"generator\": \"{generator}\",\n",
            "  \"depends_on_oce_blocks\": false\n",
            "}}\n"
        ),
        pi = csv::format_f64(pi),
        eps = csv::format_f64(eps),
        small = csv::format_f64(small),
        inf = csv::format_f64(inf),
        generator = GENERATOR_VERSION,
    );
    fs::write(constants_dir.join("constants.prov.json"), constants_json)
        .expect("write constants prov");
    manifest_lines.push(
        "CDL.Constants (fold-time, no CSV) -> goldens/CDL/Constants/constants.prov.json".into(),
    );

    // CDL.Types: enumeration ordinals (1-based, declaration order).
    let types_dir = goldens_root.join("CDL/Types");
    fs::create_dir_all(&types_dir).expect("create CDL/Types dir");
    // The generator field uses GENERATOR_VERSION (CARGO_PKG_VERSION) like every other emitter, so the
    // advertised version can never skew from the crate manifest. The static JSON prefix stays a
    // `concat!` (its inner `{`/`}` are object literals, not format placeholders); only the dynamic
    // generator + closing brace are interpolated.
    let types_json = format!(
        "{prefix}  \"generator\": \"{generator}\",\n  \"depends_on_oce_blocks\": false\n}}\n",
        prefix = concat!(
            "{\n",
            "  \"class_path\": \"CDL.Types\",\n",
            "  \"tier\": \"A\",\n",
            "  \"source\": \"1-based enum ordinals in declaration order (_spec/02 §2.2; _spec/03 §4.9)\",\n",
            "  \"value_kind\": \"Integer\",\n",
            "  \"steppable\": false,\n",
            "  \"enums\": {\n",
            "    \"SimpleController\": { \"P\": 1, \"PI\": 2, \"PD\": 3, \"PID\": 4 },\n",
            "    \"Smoothness\": { \"LinearSegments\": 1, \"ConstantSegments\": 2 },\n",
            "    \"Extrapolation\": { \"HoldLastPoint\": 1, \"LastTwoPoints\": 2, \"Periodic\": 3 },\n",
            "    \"ZeroTime\": { \"UnixTimeStamp\": 1, \"UnixTimeStampGMT\": 2, \"Custom\": 3, \"NY2010\": 4, \"NY2011\": 5, \"NY2012\": 6, \"NY2013\": 7, \"NY2014\": 8, \"NY2015\": 9, \"NY2016\": 10, \"NY2017\": 11, \"NY2018\": 12, \"NY2019\": 13, \"NY2020\": 14, \"NY2021\": 15, \"NY2022\": 16, \"NY2023\": 17, \"NY2024\": 18, \"NY2025\": 19, \"NY2026\": 20, \"NY2027\": 21, \"NY2028\": 22, \"NY2029\": 23, \"NY2030\": 24, \"NY2031\": 25, \"NY2032\": 26, \"NY2033\": 27, \"NY2034\": 28, \"NY2035\": 29, \"NY2036\": 30, \"NY2037\": 31, \"NY2038\": 32, \"NY2039\": 33, \"NY2040\": 34, \"NY2041\": 35, \"NY2042\": 36, \"NY2043\": 37, \"NY2044\": 38, \"NY2045\": 39, \"NY2046\": 40, \"NY2047\": 41, \"NY2048\": 42, \"NY2049\": 43, \"NY2050\": 44 }\n",
            "  },\n",
            "  \"note\": \"ZeroTime ordinals are pinned through NY2050 from the source-verified Buildings CDL.Types.ZeroTime.mo file.\",\n",
        ),
        generator = GENERATOR_VERSION,
    );
    fs::write(types_dir.join("types.prov.json"), types_json).expect("write types prov");
    manifest_lines
        .push("CDL.Types (fold-time, no CSV) -> goldens/CDL/Types/types.prov.json".into());
}

fn write_deferred_provenance(goldens_root: &Path, manifest_lines: &mut Vec<String>) {
    for record in sequences::deferred_provenance(GENERATOR_VERSION) {
        let path = goldens_root.join(record.relative_path);
        fs::create_dir_all(path.parent().expect("deferred provenance has parent"))
            .expect("create deferred provenance dir");
        fs::write(&path, record.contents).expect("write deferred provenance JSON");
        manifest_lines.push(record.manifest_line.into());
    }
}

fn assert_provenance_json_is_strict(goldens_root: &Path) {
    let mut checked = 0;
    assert_provenance_json_dir(goldens_root, &mut checked);
    assert!(checked > 0, "golden-gen emitted no provenance JSON files");
}

fn assert_provenance_json_dir(dir: &Path, checked: &mut usize) {
    for entry in fs::read_dir(dir).expect("read provenance directory") {
        let path = entry.expect("read provenance directory entry").path();
        if path.is_dir() {
            assert_provenance_json_dir(&path, checked);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".prov.json"))
        {
            let text = fs::read_to_string(&path).expect("read provenance JSON");
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or_else(|err| panic!("invalid strict JSON in {}: {err}", path.display()));
            *checked += 1;
        }
    }
}

/// The crate-root `oracle.lock` skeleton (toolchain / version pins for reproducibility).
fn oracle_lock() -> String {
    format!(
        concat!(
            "# oracle.lock — Tier-A golden reproducibility pin (skeleton).\n",
            "#\n",
            "# Pins the toolchain and generator that produced the checked-in goldens so a future\n",
            "# run can be byte-compared. Fill the rust_toolchain from rust-toolchain.toml at release.\n",
            "\n",
            "generator = \"{generator}\"\n",
            "tier = \"A\"\n",
            "format = \"Modelica CombiTimeTable CSV (_spec/07 §9)\"\n",
            "float_format = \"ryu shortest-round-trip f64\"\n",
            "field_separator = \" \"\n",
            "rust_toolchain = \"<PIN: see rust-toolchain.toml>\"\n",
            "ryu_version = \"1.0\"\n",
            "source = \"closed-form CDL spec derivation; depends_on_oce_blocks = false\"\n",
        ),
        generator = GENERATOR_VERSION,
    )
}
