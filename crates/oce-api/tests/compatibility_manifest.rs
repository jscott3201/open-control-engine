//! Published release compatibility manifest, derived from the facade and held current here.
//!
//! `docs/compatibility-manifest.json` is the artifact attached to a tagged release so a host or a
//! rule library can decide, without building the engine, whether its pinned CXF content still
//! means what it meant: which CXF import contract and CDL source the engine accepts, which
//! contract schema revisions it publishes, which content identities it mints, and which state and
//! replay formats it reads.
//!
//! Every value is derived here, not copied: the canonical [`CompatibilityDescriptor`], each
//! [`contract_descriptors`] revision plus an FNV-1a-128 tag over its packaged schema bytes, the
//! composite rule ids from the reference catalog, the vendored CDL source commits, and a live
//! load-and-export of every document in the swept G36 CXF corpus. State and replay revisions are
//! taken from the release-compatibility matrix, whose own checker derives them from source; this
//! test cross-checks that matrix's descriptor against the live one so the two cannot drift.
//!
//! The file is compared byte-for-byte. Regenerate it with `OCE_BLESS=1` only together with the
//! change that moved a fact, and review the diff: a changed content id churns every downstream
//! card that recorded the old one.
//!
//! The manifest is a description, not an authority. It grants no build, restore, replay or
//! release compatibility beyond the [release-compatibility policy](../../../docs/release-compatibility.md),
//! and its FNV tags are non-cryptographic integrity tags, not signatures.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use oce_api::{
    CompatibilityDescriptor, ContentIdError, ContractDomain, Engine, MAX_CXF_BYTES,
    contract_descriptors,
};
use serde_json::{Value, json};

/// Identity of the manifest's own shape; bump when keys or their meaning change.
const MANIFEST_SCHEMA: &str = "oce-compatibility-manifest/v1";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_path() -> PathBuf {
    repo_root().join("docs/compatibility-manifest.json")
}

/// FNV-1a-128 as 32 lowercase hex characters, implemented independently of the engine's hasher
/// with the constants `ExportReport::content_id_complete` documents.
fn fnv1a128_hex(bytes: &[u8]) -> String {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:032x}")
}

fn domain_name(domain: ContractDomain) -> &'static str {
    match domain {
        ContractDomain::Catalog => "catalog",
        ContractDomain::Diagnostics => "diagnostics",
        ContractDomain::Io => "io",
        ContractDomain::Values => "values",
        ContractDomain::Parameters => "parameters",
        ContractDomain::Assertions => "assertions",
        ContractDomain::ExecutionProfile => "execution-profile",
    }
}

fn contract_schemas() -> Value {
    let rows = contract_descriptors()
        .iter()
        .map(|descriptor| {
            let name = domain_name(descriptor.domain);
            let packaged = repo_root().join(format!("crates/oce-api/contracts/{name}.schema.json"));
            assert_eq!(
                fs::read_to_string(&packaged).expect("packaged contract schema"),
                descriptor.schema,
                "{name}: the packaged schema path no longer holds the served bytes"
            );
            json!({
                "domain": name,
                "revision": descriptor.revision,
                "path": format!("crates/oce-api/contracts/{name}.schema.json"),
                "fnv1a128": fnv1a128_hex(descriptor.schema.as_bytes()),
            })
        })
        .collect();
    Value::Array(rows)
}

/// The descriptor's ten canonical lines as an ordered field list, plus its exact text.
fn descriptor() -> (String, Value) {
    let text = CompatibilityDescriptor::current(None)
        .expect("descriptor without export")
        .to_string();
    let fields = text
        .lines()
        .map(|line| {
            let (key, value) = line.split_once(':').expect("labeled descriptor line");
            json!([key, value])
        })
        .collect();
    (text, Value::Array(fields))
}

fn composite_rule_ids() -> Value {
    let path = repo_root().join("tools/reference-catalog/oce-cxf.composite-rules.json");
    let catalog: serde_json::Map<String, Value> =
        serde_json::from_slice(&fs::read(path).expect("composite rule catalog"))
            .expect("composite rule catalog JSON");
    Value::Array(catalog.keys().cloned().map(Value::String).collect())
}

/// Commit pins recorded in the vendored CDL README: `| Commit | ... |` and the modelica-json row.
fn cdl_source() -> Value {
    let readme =
        fs::read_to_string(repo_root().join("third_party/modelica-buildings-cdl/README.md"))
            .expect("vendored CDL README");
    let buildings = readme
        .lines()
        .find_map(|line| line.strip_prefix("| Commit | `"))
        .and_then(|rest| rest.strip_suffix("` |"))
        .expect("Modelica Buildings commit row");
    let modelica_json = readme
        .split("`modelica-json` commit `")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .expect("modelica-json commit");
    for sha in [buildings, modelica_json] {
        assert!(sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    json!({
        "modelica_buildings_commit": buildings,
        "modelica_json_commit": modelica_json,
    })
}

/// Live load-and-export of the swept CXF corpus, sorted by fixture stem.
fn reference_corpus() -> Value {
    let directory = repo_root().join("crates/oce-cxf/tests/fixtures/g36");
    let mut fixtures = fs::read_dir(&directory)
        .expect("swept CXF corpus")
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 47, "swept CXF corpus size moved");
    let rows = fixtures
        .iter()
        .map(|fixture| {
            let stem = fixture.file_stem().and_then(|s| s.to_str()).expect("stem");
            let mut engine = Engine::in_memory();
            engine
                .load_cxf(&fs::read(fixture).expect("fixture bytes"))
                .unwrap_or_else(|error| panic!("{stem} loads: {error:?}"));
            let export = engine
                .export_cxf()
                .unwrap_or_else(|error| panic!("{stem} exports: {error:?}"));
            let digest = fnv1a128_hex(&export.bytes);
            match export.content_id_complete() {
                Ok(id) => {
                    assert_eq!(id, format!("cxf:fnv1a128:{digest}"), "{stem}");
                    json!({"fixture": stem, "export": "complete", "content_id": id})
                }
                Err(ContentIdError::Incomplete { warning_count, .. }) => json!({
                    "fixture": stem,
                    "export": "incomplete",
                    "deferral_warnings": warning_count,
                    "bytes_fnv1a128": digest,
                }),
                Err(other) => panic!("{stem}: unexpected content-id error {other:?}"),
            }
        })
        .collect();
    Value::Array(rows)
}

/// State and replay facts from the release-compatibility matrix, after proving its descriptor is
/// the live one.
fn matrix_facts(descriptor_fields: &Value) -> (Value, Value, Value) {
    let matrix: Value = serde_json::from_slice(
        &fs::read(repo_root().join("docs/release-compatibility.json")).expect("matrix"),
    )
    .expect("matrix JSON");
    let facts = &matrix["current_facts"];
    let live: serde_json::Map<String, Value> = descriptor_fields
        .as_array()
        .expect("fields")
        .iter()
        .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
        .collect();
    assert_eq!(
        facts["descriptor"],
        Value::Object(live),
        "release-compatibility matrix descriptor is not the live descriptor"
    );
    assert_eq!(facts["package"], env!("CARGO_PKG_VERSION"));
    let state = json!({
        "format_revision": facts["state_format"],
        "execution_abi_revision": facts["execution_abi"],
        "placements": facts["placement"],
    });
    let replay = json!({
        "format_revision": facts["replay_format"],
        "exactness": facts["replay_exactness"],
    });
    let policy = json!({
        "matrix": "docs/release-compatibility.json",
        "policy": matrix["policy"],
        "implementation_baseline": matrix["candidates"]["current"]["implementation_baseline"],
        "implementation_sha256": matrix["candidates"]["current"]["implementation"]["sha256"],
        "strict_bit_class": facts["strict_bit_class"],
    });
    (state, replay, policy)
}

/// One top-level key per line and one array element per line, so a release diff reads as a list
/// of moved facts.
fn render() -> String {
    let (descriptor_text, descriptor_fields) = descriptor();
    let (state, replay, policy) = matrix_facts(&descriptor_fields);
    let cxf = json!({
        "import_contract": "docs/cxf-composite-subset.md",
        "max_document_bytes": MAX_CXF_BYTES,
        "cdl_source": cdl_source(),
        "content_id_scheme": "cxf:fnv1a128:<32 lowercase hex> over complete ExportReport::bytes; minted only when the export has no deferral warnings",
        "catalog_content_id": descriptor_fields[2][1],
    });
    let sections: Vec<(&str, Value)> = vec![
        ("schema", json!(MANIFEST_SCHEMA)),
        ("package", json!(env!("CARGO_PKG_VERSION"))),
        (
            "generated_by",
            json!("crates/oce-api/tests/compatibility_manifest.rs (OCE_BLESS=1 regenerates)"),
        ),
        ("descriptor_text", json!(descriptor_text)),
        ("descriptor", descriptor_fields),
        ("contract_schemas", contract_schemas()),
        ("cxf", cxf),
        ("composite_rule_ids", composite_rule_ids()),
        ("reference_corpus", reference_corpus()),
        ("state", state),
        ("replay", replay),
        ("release_compatibility", policy),
    ];
    let mut out = String::from("{\n");
    for (index, (key, value)) in sections.iter().enumerate() {
        let body = match value {
            Value::Array(rows) if !rows.is_empty() => {
                let rows: Vec<String> = rows
                    .iter()
                    .map(|row| format!("    {}", serde_json::to_string(row).unwrap()))
                    .collect();
                format!("[\n{}\n  ]", rows.join(",\n"))
            }
            other => serde_json::to_string(other).unwrap(),
        };
        let comma = if index + 1 == sections.len() { "" } else { "," };
        writeln!(out, "  \"{key}\": {body}{comma}").unwrap();
    }
    out.push_str("}\n");
    out
}

#[test]
fn published_compatibility_manifest_is_current_and_repeatable() {
    let rendered = render();
    assert_eq!(
        rendered,
        render(),
        "manifest rendering is not deterministic"
    );
    let parsed: Value = serde_json::from_str(&rendered).expect("manifest is valid JSON");
    assert_eq!(parsed["schema"], MANIFEST_SCHEMA);
    if oce_bless::enabled("OCE_BLESS") {
        fs::write(manifest_path(), &rendered).expect("write compatibility manifest");
        return;
    }
    let checked_in = fs::read_to_string(manifest_path())
        .expect("read docs/compatibility-manifest.json (regenerate with OCE_BLESS=1)")
        .replace("\r\n", "\n");
    assert_eq!(
        checked_in, rendered,
        "docs/compatibility-manifest.json is stale; review the moved facts, then regenerate with OCE_BLESS=1"
    );
}
