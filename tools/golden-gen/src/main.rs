//! golden-gen — Tier-A oracle generator.
//!
//! Emits closed-form CDL reference goldens as Modelica `CombiTimeTable` CSV under
//! `tools/golden-gen/goldens/<class_path>/<signal>.csv`, a sibling `<signal>.prov.json` per golden,
//! and a crate-root `oracle.lock` toolchain/version pin skeleton.
//!
//! ANTI-TAUTOLOGY: all reference math is re-derived independently from `_spec/03`, `_spec/02`,
//! `_spec/01`, `_spec/07` (format only) and CDL §7.x. This crate has ZERO dependency on
//! `oce-blocks` (the implementation under test) and never reads it.

mod csv;
mod discrete_sources;
mod integers_conversions;
mod logical;
mod oracle;
mod reals;

use std::fs;
use std::path::{Path, PathBuf};

use csv::SignalColumn;
use oracle::{Golden, Sample, ValueKind};

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
    goldens.extend(logical::goldens());
    goldens.extend(integers_conversions::goldens());
    goldens.extend(discrete_sources::goldens());

    // Clean and recreate the goldens tree so removed entries never linger (deterministic output).
    if goldens_root.exists() {
        fs::remove_dir_all(&goldens_root).expect("clear goldens tree");
    }
    fs::create_dir_all(&goldens_root).expect("create goldens root");

    let mut manifest_lines: Vec<String> = Vec::new();
    for g in &goldens {
        let dir = goldens_root.join(class_path_to_dir(g.class_path));
        fs::create_dir_all(&dir).expect("create golden dir");

        // Table name: a Modelica-identifier-safe slug of class_path + signal.
        let table_name = sanitize_table_name(&format!("{}_{}", g.class_path, g.signal));
        let col = SignalColumn {
            name: g.signal.to_string(),
            time: g.time.clone(),
            values: g.samples.iter().map(|s| s.encode()).collect(),
        };
        let csv_text = csv::to_csv_string(&table_name, &col);
        let csv_path = dir.join(format!("{}.csv", g.signal));
        fs::write(&csv_path, csv_text).expect("write golden csv");

        let prov_path = dir.join(format!("{}.prov.json", g.signal));
        fs::write(&prov_path, prov_json(g)).expect("write prov json");

        manifest_lines.push(format!(
            "{} {} -> goldens/{}/{}.csv",
            g.class_path,
            g.signal,
            class_path_to_dir(g.class_path),
            g.signal
        ));
    }

    // Non-steppable fold-time references (CDL.Constants, CDL.Types): provenance only, no CSV.
    write_constants_types(&goldens_root, &mut manifest_lines);

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

    println!("golden-gen: emitted {} signal goldens", goldens.len());
    print!("{manifest}");
}

/// Map `CDL.Reals.Add` -> `CDL/Reals/Add` for the on-disk directory layout.
fn class_path_to_dir(class_path: &str) -> String {
    class_path.replace('.', "/")
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

/// Build the per-golden provenance JSON record.
fn prov_json(g: &Golden) -> String {
    // Describe non-finite Real samples by IEEE class so the provenance documents the compare regime.
    let has_non_finite = g
        .samples
        .iter()
        .any(|s| matches!(s, Sample::Real(x) if !x.is_finite()));
    let compare = match g.kind {
        ValueKind::Real if has_non_finite => {
            "bit-exact for finite; Inf/NaN by IEEE class (Value::bit_eq)"
        }
        ValueKind::Real => "Real banded/bit-exact (rtoly~1e-9; exact f64 ops -> bit-exact)",
        ValueKind::Integer => "exact (atoly=0)",
        ValueKind::Boolean => "exact 0.0/1.0 (atoly=0)",
    };
    format!(
        concat!(
            "{{\n",
            "  \"class_path\": \"{class_path}\",\n",
            "  \"signal\": \"{signal}\",\n",
            "  \"tier\": \"A\",\n",
            "  \"source\": \"closed-form from CDL spec (_spec/03,02,01; CDL §7.x); independent re-derivation\",\n",
            "  \"value_kind\": \"{value_kind}\",\n",
            "  \"compare_regime\": \"{compare}\",\n",
            "  \"n_samples\": {n_samples},\n",
            "  \"input\": \"{input}\",\n",
            "  \"reference_rule\": \"{rule}\",\n",
            "  \"format\": \"Modelica CombiTimeTable CSV (_spec/07 §9); time col 0; ryu shortest-round-trip f64\",\n",
            "  \"generator\": \"{generator}\",\n",
            "  \"depends_on_oce_blocks\": false\n",
            "}}\n"
        ),
        class_path = json_escape(g.class_path),
        signal = json_escape(g.signal),
        value_kind = g.kind.as_str(),
        compare = json_escape(compare),
        n_samples = g.samples.len(),
        input = json_escape(&g.input_desc),
        rule = json_escape(&g.rule_desc),
        generator = GENERATOR_VERSION,
    )
}

/// Emit provenance-only records for the non-steppable fold-time references (CDL.Constants/Types).
///
/// These are NOT registry/steppable blocks (`_spec/03` §4.9/§4.10): they carry fold-time literal /
/// ordinal references with no tick trace, so there is no CombiTimeTable CSV — only a prov.json that
/// gives an evaluator (oce-expr) its reference values.
fn write_constants_types(goldens_root: &Path, manifest_lines: &mut Vec<String>) {
    // CDL.Constants: pi/eps/small/inf (Modelica.Constants). pi/eps exact f64; small/inf flagged.
    let constants_dir = goldens_root.join("CDL/Constants");
    fs::create_dir_all(&constants_dir).expect("create CDL/Constants dir");
    let pi = std::f64::consts::PI;
    let eps = f64::EPSILON; // 2^-52
    let constants_json = format!(
        concat!(
            "{{\n",
            "  \"class_path\": \"CDL.Constants\",\n",
            "  \"tier\": \"A\",\n",
            "  \"source\": \"fold-time literals from Modelica.Constants (_spec/03 §4.10; _spec/02 §6.1/§7.3)\",\n",
            "  \"value_kind\": \"Real\",\n",
            "  \"steppable\": false,\n",
            "  \"values\": {{\n",
            "    \"pi\": {pi},\n",
            "    \"eps\": {eps},\n",
            "    \"small\": 1e-60,\n",
            "    \"inf\": \"+inf (Modelica largest-representable; FLAG: verify against oce-expr constant table — may fold to f64::MAX instead)\"\n",
            "  }},\n",
            "  \"generator\": \"{generator}\",\n",
            "  \"depends_on_oce_blocks\": false\n",
            "}}\n"
        ),
        pi = csv::format_f64(pi),
        eps = csv::format_f64(eps),
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
    let types_json = concat!(
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
        "    \"ZeroTime\": { \"UnixTimeStamp\": 1, \"UnixTimeStampGMT\": 2, \"Custom\": 3, \"NY2010\": 4 }\n",
        "  },\n",
        "  \"note\": \"FLAG: ZeroTime members beyond NY2010 exist in the Buildings library; only the spec-cited prefix is pinned here.\",\n",
        "  \"generator\": \"golden-gen 0.1.0\",\n",
        "  \"depends_on_oce_blocks\": false\n",
        "}\n"
    );
    fs::write(types_dir.join("types.prov.json"), types_json).expect("write types prov");
    manifest_lines
        .push("CDL.Types (fold-time, no CSV) -> goldens/CDL/Types/types.prov.json".into());
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
