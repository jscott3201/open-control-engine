//! Fixed identities and row bits for the scoped Reliefs evidence.

pub(super) const CANONICAL_SHA: &str =
    "e1112f10ffe14a967cb73f81cd7ceb89edcc2b010627458497855663bdf54b2c";
pub(super) const TIME_BITS: &[&str] = &[
    "0000000000000000",
    "404e000000000eff",
    "405e000000000781",
    "40668000000003c1",
    "406e0000000003c1",
    "4072c000000003c1",
    "40768000000003c1",
];
pub(super) const U_T_SUP_BITS: &[&str] = &[
    "bfe0000000000000",
    "bfd0000000000000",
    "bfc0000000000000",
    "0000000000000000",
    "3fc0000000000000",
    "3fd0000000000000",
    "3fe0000000000000",
];
pub(super) const Y_OUT_BITS: &[&str] = &[
    "3fd0000000000000",
    "3fd0000000000000",
    "3fe2000000000000",
    "3fec000000000000",
    "3fec000000000000",
    "3fec000000000000",
    "3fec000000000000",
];
pub(super) const Y_RET_BITS: &[&str] = &[
    "3fe8000000000000",
    "3fe8000000000000",
    "3fe8000000000000",
    "3fe8000000000000",
    "3fdc000000000000",
    "3fc0000000000000",
    "3fc0000000000000",
];

pub(super) const SOURCE_FILES: &[(&str, &str, &str)] = &[
    (
        "buildings",
        "Buildings/package.mo",
        "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
    ),
    (
        "buildings",
        "Buildings/Controls/package.mo",
        "17f0bba8aa51f7051fa43d5cac6dcef1f33ca8f811fd6a6474bd3ed1263f61cd",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/package.mo",
        "a86253df85e5531235ccb81ece569eedac973d8c4eae52be912877e7bd0d321c",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/package.mo",
        "88b99ba4667c09e5a23c5ac21c88fe18e39af67c22cc2efc6dbab26db09e8e6b",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/package.mo",
        "ae1fe5bfca73fd59ad4253aaea5e8c927ce1e1824cdce9790db3a24a20853881",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.mo",
        "266b09bcb8a3266467c6728ee7a5d9872cdf3dad405af91bac14a697320176a2",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.mo",
        "de4908f31fb15838b54dc41473b82059201ace000c2615ded47a1071dd718560",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.mo",
        "290c0e49356bc000364b644cac4baf353fc4d4a4ed5c77cb5e1145cdf3ab56e7",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.mo",
        "41ee31e3ed5ec6fd88a46447b73a5d5c55cd3cce06a899c25df3aadcba5b3b3b",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.mo",
        "adebe030dcdd18a8777558b18e56084ed19546c375f678d083987a4480952216",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/package.mo",
        "0e2f3d3129ed06fc93655e75fda3597bba6f17f924117bb4b47c5dca7f3c3508",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo",
        "177fd5f2802bfd29072bc221756dd8846cd05b552f8fdf368a2c87a56593cb41",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/package.mo",
        "3ceda191a859e2513c4d3df322bec753ed8df406968cf4354c5488f4dcd79256",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Interfaces/package.mo",
        "a4b3a6831deb68e8209435e2b0f0067d227e3bff2be76845bd2f3690f13c82e4",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo",
        "0f4afeda8d50035b722a79e6d6b48c86034facd3adcfc7f95e2b15cbd1ddc87a",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo",
        "ba27a80bc46bf8b9550655b54a93679f5322b33786cc220daa59f7d39243d98f",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/package.mo",
        "3b9a58569701c9f7d44347d6304aeb60cea28902332fb16acc15e0fd61e19a8a",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/Line.mo",
        "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/Min.mo",
        "e5dcf1e50d752365d05e44bc54eb743c116b87de240c1253c2126fcdbcbcbb04",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/Max.mo",
        "499e5162b21fa776c61065a46c4ba5d646ed887b227adaae93214d97750efca1",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/Sources/package.mo",
        "373e79eb61b6ace1527a93253ad0a3dfeb520dab0a1b6644dd4b0dd7419c9b20",
    ),
    (
        "buildings",
        "Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo",
        "f3a131c5c6eb372ea48dec67ed5eb075eef1a485901143a338c4361511eed05e",
    ),
    (
        "modelica",
        "Complex.mo",
        "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
    ),
    (
        "modelica",
        "Modelica/package.mo",
        "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
    ),
    (
        "modelica",
        "Modelica/Blocks/Sources.mo",
        "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
    ),
    (
        "modelica",
        "ModelicaServices/package.mo",
        "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
    ),
];

pub(super) fn expected_artifacts() -> Vec<(String, String)> {
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/";
    let tool = "tools/openmodelica-reliefs-reference/";
    let mut output = vec![
        (
            "image_index_json".into(),
            format!("{fixture}image-index.json"),
        ),
        (
            "cross_architecture_log".into(),
            format!("{fixture}cross-architecture.log"),
        ),
    ];
    let native = [
        ("architecture_record", "architecture.json"),
        ("canonical_csv", "reliefs.canonical.csv"),
        ("raw_run_a_csv", "reliefs-run-a.raw.csv"),
        ("raw_run_b_csv", "reliefs-run-b.raw.csv"),
        ("run_a_log", "run-a.log"),
        ("run_b_log", "run-b.log"),
        (
            "parameter_control_canonical_csv",
            "parameter-control.canonical.csv",
        ),
        ("parameter_control_raw_csv", "parameter-control.raw.csv"),
        ("parameter_control_log", "parameter-control.log"),
        ("final_clamp_canonical_csv", "final-clamp.canonical.csv"),
        ("final_clamp_raw_csv", "final-clamp.raw.csv"),
        ("final_clamp_log", "final-clamp.log"),
        ("projection_mutation_log", "projection-mutation.log"),
        (
            "projection_keep_first_canonical_csv",
            "projection-keep-first.canonical.csv",
        ),
        (
            "projection_keep_first_metadata",
            "projection-keep-first.metadata",
        ),
        ("architecture_image_index_json", "image-index.json"),
        ("platform_image_manifest_json", "image-manifest.json"),
    ];
    for architecture in ["arm64", "amd64"] {
        output.extend(native.iter().map(|(role, file)| {
            (
                format!("{architecture}_{role}"),
                format!("{fixture}{architecture}/{file}"),
            )
        }));
    }
    output.extend([
        (
            "canonicalizer_source".into(),
            "crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs".into(),
        ),
        ("tool_cargo_lock".into(), format!("{tool}Cargo.lock")),
        ("tool_cargo_toml".into(), format!("{tool}Cargo.toml")),
        ("tool_main_source".into(), format!("{tool}src/main.rs")),
        (
            "wrapper_model".into(),
            format!("{tool}reliefs/ReliefsPilot.mo"),
        ),
        (
            "parameter_control_wrapper_model".into(),
            format!("{tool}reliefs/ReliefsParameterPilot.mo"),
        ),
        (
            "final_clamp_wrapper_model".into(),
            format!("{tool}reliefs/ReliefsClampPilot.mo"),
        ),
        ("runner_script".into(), format!("{tool}reliefs/runner.sh")),
        (
            "regeneration_script".into(),
            format!("{tool}reliefs/regenerate.sh"),
        ),
        (
            "assembly_script".into(),
            format!("{tool}reliefs/assemble.sh"),
        ),
        (
            "manifest_generator_script".into(),
            format!("{tool}reliefs/generate_manifest.py"),
        ),
        (
            "architecture_generator_script".into(),
            format!("{tool}reliefs/generate_architecture.py"),
        ),
        (
            "evidence_validator_script".into(),
            format!("{tool}reliefs/verify_evidence.py"),
        ),
        (
            "projection_validator_script".into(),
            format!("{tool}reliefs/projection_evidence.py"),
        ),
        (
            "safe_file_helper_script".into(),
            format!("{tool}reliefs/safe_files.py"),
        ),
        (
            "oci_materializer_script".into(),
            format!("{tool}reliefs/materialize_oci.py"),
        ),
        (
            "deadline_script".into(),
            format!("{tool}reliefs/deadline.sh"),
        ),
        (
            "deadline_test_script".into(),
            format!("{tool}reliefs/deadline_test.sh"),
        ),
        (
            "output_publish_script".into(),
            format!("{tool}reliefs/output_publish.py"),
        ),
        (
            "output_publish_test_script".into(),
            format!("{tool}reliefs/output_publish_test.sh"),
        ),
        (
            "container_cleanup_script".into(),
            format!("{tool}reliefs/container_cleanup.sh"),
        ),
        (
            "container_cleanup_test_script".into(),
            format!("{tool}reliefs/container_cleanup_test.sh"),
        ),
        (
            "oci_index_source".into(),
            format!("{tool}reliefs/image-index.json"),
        ),
        (
            "arm64_manifest_source".into(),
            format!("{tool}reliefs/image-manifest-arm64.json"),
        ),
        (
            "amd64_manifest_source".into(),
            format!("{tool}reliefs/image-manifest-amd64.json"),
        ),
        (
            "evidence_workflow".into(),
            ".github/workflows/openmodelica-reliefs-evidence.yml".into(),
        ),
    ]);
    output
}
