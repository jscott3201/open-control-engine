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
SELECTIONS = {
    "default": [],
    "mem": ["--no-default-features", "--features", "mem"],
    "no-default": ["--no-default-features"],
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
    Case("csv_input_absent", 'let _ = oce_api::InputSource::Csv { path: "".into(), '
         'bindings: vec![] };', "E0599", "Csv"),
    Case("error_severity_absent", "let _ = oce_api::AssertLevel::Error;", "E0599", "Error"),
)

POSITIVE = """use oce_api::{AssertLevel, CollectSpec, Engine, InputSource, SimSpec, Value};
fn send_sync<T: Send + Sync>() {}
fn supported(bytes: &[u8]) -> Result<(), oce_api::OcError> {
    send_sync::<Engine>();
    send_sync::<SimSpec>();
    let mut engine = Engine::in_memory();
    let _ = engine.load_cxf(bytes)?;
    let _ = engine.point_list(None)?;
    let query = oce_api::oce_store::SemanticQuery::FuzzyText { query: "x".into(), k: 1 };
    let _: oce_store::SemanticQuery = query;
    let _ = AssertLevel::Warning;
    for inputs in [InputSource::None, InputSource::Constant(vec![("u".into(), Value::Real(1.0))]),
                   InputSource::Closure(Box::new(|t| vec![("u".into(), Value::Real(t))]))] {
        let _ = engine.simulate(&SimSpec {
            t_start: 0.0, t_stop: 1.0, step: 1.0, inputs, collect: CollectSpec::None,
        })?;
    }
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
    dependencies = {name: artifact(messages, name) for name in ("oce_api", "oce_store")}
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
    parser.add_argument("--group", choices=("all", "deferred", "assertion"), default="all",
                        help="focused test-first group; the gate always runs all")
    args = parser.parse_args()
    cases = CASES if args.group == "all" else CASES[:5] if args.group == "deferred" else CASES[5:]
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
