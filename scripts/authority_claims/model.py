"""Closed cross-domain index schema; bindings are protocol, not copied facts."""

import json
import re
from typing import Any

INDEX = "docs/authority-claims.json"
OUTPUT = "docs/authority-claims.md"
SCHEMA = "oce-authority-claims/v1"
API_TEST = "crates/oce-api/src/tests/authority_claims_tests.rs"
BLOCK_TEST = "crates/oce-blocks/src/authority_claims_tests.rs"
CATALOG = "crates/oce-blocks/src/catalog.rs"
STATE = "crates/oce-api/src/state.rs"
BINDINGS = {
    "packages": ("delegated", "docs/package-publication-ledger.json", "scripts/package_policy/validate.py"),
    "public-surface": ("delegated", "docs/public-surface-ledger.json", "crates/oce-api/tests/public_surface_contract.rs"),
    "catalog-entries": ("native", CATALOG, BLOCK_TEST),
    "catalog-reserved": ("native", CATALOG, BLOCK_TEST),
    "catalog-source": ("delegated", "tools/reference-catalog/Buildings.Controls.OBC.CDL.prov.json", "crates/oce-blocks/src/registry/catalog_tests.rs"),
    "catalog-registry": ("delegated", "tools/reference-catalog/oce-blocks.registry-manifest.json", "crates/oce-blocks/src/catalog_tests.rs"),
    "state-format": ("native", STATE, API_TEST),
    "execution-abi": ("native", STATE, API_TEST),
    "tier-a-records": ("corpus", "tools/golden-gen/goldens", "scripts/authority_claims/observe.py"),
    "tier-two-records": ("corpus", "crates/oce-conformance/tests/fixtures/golden/g36_traces", "scripts/authority_claims/observe.py"),
    "host-tick": ("review-only", "docs/execution-profile.md", "crates/oce-api/src/tests/pre_execution_profile_tests.rs"),
    "platforms": ("review-only", "docs/architecture.md", ".github/workflows/ci.yml"),
    "deferred-capabilities": ("review-only", "docs/public-surface-contract.md", "docs/cdl-coverage.md"),
    "evidence-quality": ("review-only", "docs/verification-evidence.md", "TESTING.md"),
    "true-hold-extension": ("review-only", "crates/oce-blocks/src/logical_timing.rs", "crates/oce-blocks/src/port_names.rs"),
}
SUPERSESSION = {
    "dated-stability": ("historical", "docs/stability-baseline.md"),
    "local-public-plan": ("superseded", "docs/public-surface-contract.md"),
    "local-package-plan": ("superseded", "docs/package-publication-policy.md"),
    "local-execution-plan": ("superseded", "docs/execution-profile.md"),
    "true-hold-annotation": ("ambiguous", None),
}


class ClaimError(ValueError):
    """Fail-closed index, source, or projection error."""


def require(condition, label):
    if not condition:
        raise ClaimError(label)


def keys(value, expected, label):
    require(type(value) is dict and set(value) == set(expected), f"{label}: fields")


def text(value):
    require(type(value) is str and bool(value.strip()) and all(ord(c) >= 32 for c in value), "text: type/empty/control")
    return value


def integer(value):
    require(type(value) is int and 0 <= value <= 0xFFFFFFFF, "integer: expected u32, not bool")
    return value


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def parse(raw) -> Any:
    def invalid_constant(value):
        raise ClaimError(f"non-JSON constant: {value}")
    try:
        return json.loads(raw, object_pairs_hook=unique_pairs, parse_constant=invalid_constant)
    except (ValueError, UnicodeError) as error:
        raise ClaimError(f"JSON: {error}") from error


def rows(value: Any, label):
    require(type(value) is list and bool(value), f"{label}: nonempty array")
    result = {}
    for row in value:
        require(type(row) is dict and type(row.get("id")) is str, f"{label}: row/id")
        require(row["id"] not in result, f"{label}: duplicate id")
        result[row["id"]] = row
    return result


def validate(index):
    keys(index, {"schema", "projections", "facts", "supersession"}, "index")
    require(index["schema"] == SCHEMA, "index: schema")
    require(index["projections"] == [{"id": "summary", "path": OUTPUT}], "index: projections")
    facts = rows(index["facts"], "facts")
    require(set(facts) == set(BINDINGS), "facts: missing/unknown id")
    for identity, (mode, source, verifier) in BINDINGS.items():
        row = facts[identity]
        fields = {"id", "mode", "source", "verifier", "projection"}
        if mode == "native":
            fields.add("expected")
        if mode == "review-only":
            fields.add("limit")
        keys(row, fields, identity)
        require((row["mode"], row["source"], row["verifier"]) == (mode, source, verifier), f"{identity}: fixed binding")
        require(row["projection"] == "summary", f"{identity}: projection")
        if mode == "native":
            integer(row["expected"])
        if mode == "review-only":
            text(row["limit"])
    records = rows(index["supersession"], "supersession")
    require(set(records) == set(SUPERSESSION), "supersession: missing/unknown id")
    for identity, row in records.items():
        fields = {"id", "subject", "historical_locator", "status", "authority_path", "reason", "responsibility_owner"}
        if "superseded_by" in row:
            fields.add("superseded_by")
        keys(row, fields, identity)
        for field in ("subject", "historical_locator", "reason", "responsibility_owner"):
            text(row[field])
        require((row["status"], row["authority_path"]) == SUPERSESSION[identity], f"{identity}: authority/status binding")
        # A locator is inert text, never a path to resolve, even when it looks local.
        require(row["historical_locator"] not in (INDEX, OUTPUT, row["authority_path"]), "supersession: self reference")
        seen = {identity}
        cursor = row
        while "superseded_by" in cursor:
            target = cursor["superseded_by"]
            require(type(target) is str and target in records, "supersession: unknown target")
            require(target not in seen, "supersession: cycle")
            seen.add(target)
            cursor = records[target]
    return facts


def digest_text(value, length):
    require(type(value) is str and re.fullmatch(f"[0-9a-f]{{{length}}}", value) is not None, "digest: shape")
    return value
