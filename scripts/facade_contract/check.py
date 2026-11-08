#!/usr/bin/env python3
"""Compile the supported facade and prove selected names cannot be used by a host.

Standard library only. Cargo JSON selects the exact artifacts from each isolated,
locked feature build; positive and negative controls use those SAME dependencies.
Only temporary consumer files are written, never production source or baselines.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
# `oce-api` declares no cargo features, so the default build is the only selection.
SELECTIONS = {
    "default": [],
}


@dataclass(frozen=True)
class Case:
    name: str
    statement: str
    code: str
    symbol: str


CASES = (
    Case("modelica_loader_absent", "let _ = Host::load_modelica;", "E0599", "load_modelica"),
    Case("semantic_loader_absent", "let _ = Host::load_from_semantic;", "E0599",
         "load_from_semantic"),
    Case("template_handle_absent", "let _: Option<oce_api::TemplateRef> = None;", "E0425",
         "TemplateRef"),
    Case("flat_query_absent", "let _: Option<oce_api::SemanticQuery> = None;", "E0425",
         "SemanticQuery"),
    Case("error_severity_absent", "let _ = oce_api::AssertLevel::Error;", "E0599", "Error"),
)

# The engine is pure (GitHub #390): no storage type, constructor, accessor, error or stage is
# reachable through the facade.
STORAGE_CASES = (
    Case("storage_reexport_absent", "let _: Option<oce_api::oce_store::DomainKey> = None;", "E0433",
         "oce_store"),
    Case("with_store_absent", "let _ = Host::with_store;", "E0599", "with_store"),
    Case("store_accessor_absent", "let _ = Host::store;", "E0599", "store"),
    Case("in_memory_absent", "let _ = Host::in_memory;", "E0599", "in_memory"),
    Case("store_error_absent", "let _ = oce_api::OcError::Store;", "E0599", "Store"),
) + tuple(
    Case(f"{name.lower()}_stage_absent", f"let _ = oce_api::DiagnosticStage::{name};", "E0599", name)
    for name in ("StoreRecovery", "StoreSave", "StoreInputs")
)

FRAME_CASES = tuple(
    Case(f"{name}_absent", f"let _ = Host::{name};", "E0599", name)
    for name in ("tick", "tick_with", "set_input", "simulate", "step_realtime", "outputs",
                 "set_realtime_epoch_unix_nanos", "realtime_epoch_unix_nanos")
) + tuple(
    Case(f"{name.lower()}_type_absent", f"let _: Option<oce_api::{name}> = None;", "E0425", name)
    for name in ("Outputs", "SimSpec", "SimMetrics", "StepReport", "InputSource", "OutputTrace",
                 "CollectSpec")
) + tuple(
    Case(f"{name.lower()}_variant_absent", f"let _ = oce_api::OcError::{name};", "E0599", name)
    for name in ("RealtimeEpochUnset", "RealtimeInstantUnrepresentable")
)

POSITIVE = """use oce_api::{AssertLevel, CompletedFrame, Engine, PreparedInputFrame, Value};
fn send_sync<T: Send + Sync>() {}
fn supported(bytes: &[u8]) -> Result<(), oce_api::OcError> {
    send_sync::<Engine>();
    send_sync::<PreparedInputFrame>();
    send_sync::<CompletedFrame>();
    let mut engine = Engine::new();
    let _: usize = engine.cxf_byte_limit();
    engine.set_cxf_byte_limit(oce_api::MAX_CXF_BYTES / 2)?;
    let bounded = oce_api::OcError::CxfTooLarge { actual_bytes: 2, limit_bytes: 1 };
    let invalid = oce_api::OcError::CxfByteLimitTooLarge {
        actual_bytes: usize::MAX, limit_bytes: oce_api::MAX_CXF_BYTES,
    };
    let _ = (bounded, invalid);
    let (report, loaded): (oce_api::LoadReport, oce_api::LoadedModel) = engine.load_cxf(bytes)?;
    let _: &oce_api::DomainKey = &report.model_id;
    let _: &oce_api::ResolvedModel = &loaded.model;
    let _: Vec<(&str, oce_api::PointValueType)> = loaded.input_points().collect();
    let _ = engine.point_list(None)?;
    let _ = AssertLevel::Warning;
    let prepared = engine.prepare_frame(0.0, &[("u", Value::Real(1.0))])?;
    let completed = engine.execute_frame(prepared)?;
    let _ = (completed.time(), completed.sequence(), completed.outputs(), completed.diagnostics());
    let _ = engine.get_output("y")?;
    let _ = engine.watch(&["y"])?;
    Ok(())
}
"""


class ContractError(ValueError):
    """A failed build, positive control or intended absence diagnostic."""


def artifact(messages: list[dict], name: str) -> Path:
    paths = {Path(filename) for item in messages
             if item.get("reason") == "compiler-artifact"
             and item.get("target", {}).get("name") == name
             for filename in item.get("filenames", []) if filename.endswith(".rlib")}
    if len(paths) != 1:
        raise ContractError(f"{name}: expected one Cargo-reported rlib, got {len(paths)}")
    return paths.pop()


def intended_refusal(status: int, diagnostics: list[dict], case: Case, source: Path) -> None:
    errors = [item for item in diagnostics if item.get("level") == "error" and item.get("code")]
    if status == 0:
        raise ContractError(f"{case.name}: removed surface unexpectedly compiled")
    if len(errors) != 1:
        raise ContractError(f"{case.name}: expected exactly one coded compiler error: {errors}")
    error = errors[0]
    located = any(span.get("is_primary") and span.get("file_name") == str(source)
                  and span.get("line_start") == span.get("line_end") == 3
                  and any(case.symbol in line.get("text", "") for line in span.get("text", []))
                  for span in error.get("spans", []))
    if (error["code"].get("code") != case.code
            or f"`{case.symbol}`" not in error.get("message", "") or not located):
        raise ContractError(f"{case.name}: wrong diagnostic or source location: {error}")


def compile_source(source: Path, dependencies: dict[str, Path]) -> subprocess.CompletedProcess:
    command = ["rustc", "--edition=2024", "--crate-type=lib", "--emit=metadata",
               "--error-format=json", "--crate-name", "facade_contract", str(source),
               "-o", str(source.with_suffix(".rmeta"))]
    for name, library in dependencies.items():
        command.extend(["--extern", f"{name}={library}"])
    directories = {library.parent for library in dependencies.values()}
    # A Cargo root rlib lives above deps/. Sole-facade consumers still need the transitive
    # search directory, without granting additional direct --extern dependencies.
    directories |= {directory / "deps" for directory in directories
                    if (directory / "deps").is_dir()}
    for directory in sorted(directories):
        command.extend(["-L", f"dependency={directory}"])
    return subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=120,
                          check=False)


def check_selection(selection: str, cases: tuple[Case, ...]) -> list[str]:
    target = ROOT / "target" / "facade-contract" / selection
    command = ["cargo", "build", "-p", "oce-api", "--lib", "--locked", "--message-format=json",
               "--target-dir", str(target), *SELECTIONS[selection]]
    build = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=900,
                           check=False)
    if build.returncode:
        raise ContractError(f"{selection}: dependency build failed\n{build.stderr}\n{build.stdout}")
    messages = [json.loads(line) for line in build.stdout.splitlines()]
    dependencies = {name: artifact(messages, name) for name in ("oce_api",)}
    failures = []
    with tempfile.TemporaryDirectory(prefix="consumers-", dir=target) as temporary:
        directory = Path(temporary)
        positive = directory / "supported.rs"
        positive.write_text(POSITIVE, encoding="utf-8")
        result = compile_source(positive, dependencies)
        if result.returncode:
            raise ContractError(f"{selection}: same-dependency positive control failed\n{result.stderr}")
        print(f"{selection}: supported same-dependency control PASS", flush=True)
        catalog_source = directory / "catalog_consumer.rs"
        catalog_source.write_bytes((ROOT / "crates/oce-api/tests/fixtures/catalog_consumer.rs").read_bytes())
        result = compile_source(catalog_source, {"oce_api": dependencies["oce_api"]})
        if result.returncode:
            raise ContractError(f"{selection}: sole-facade catalog/receipt consumer failed\n{result.stderr}")
        print(f"{selection}: sole-facade catalog/receipt consumer PASS", flush=True)
        for case in cases:
            source = directory / f"{case.name}.rs"
            source.write_text(f"type Host = oce_api::Engine;\nfn contract() {{\n"
                              f"    {case.statement}\n}}\n", encoding="utf-8")
            result = compile_source(source, dependencies)
            diagnostics = [json.loads(line) for line in result.stderr.splitlines()]
            try:
                intended_refusal(result.returncode, diagnostics, case, source)
                print(f"{selection}: {case.name} PASS ({case.code}, consumer line 3)", flush=True)
            except ContractError as error:
                failures.append(f"{selection}: {error}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--group", choices=("all", "deferred", "assertion", "frame", "storage"),
                        default="all",
                        help="focused test-first group; the gate always runs all")
    args = parser.parse_args()
    cases = {"all": CASES + FRAME_CASES + STORAGE_CASES, "deferred": CASES[:4],
             "assertion": CASES[4:], "frame": FRAME_CASES, "storage": STORAGE_CASES}[args.group]
    try:
        failures = [failure for selection in SELECTIONS
                    for failure in check_selection(selection, cases)]
        if failures:
            raise ContractError("\n".join(failures))
    except (ContractError, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"facade contract: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"facade contract: PASS ({len(SELECTIONS)} selections, {len(cases)} absences each)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
