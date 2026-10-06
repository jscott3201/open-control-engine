"""Observe bounded clone-visible files, not prose or Rust semantics."""

import hashlib
import importlib
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import sys

from model import (API_TEST, BLOCK_TEST, BINDINGS, INDEX, OUTPUT,
                   digest_text, integer, keys, parse, require, text, validate)

ROOT = Path(__file__).resolve().parents[2]
PENDING = {INDEX, OUTPUT, API_TEST, BLOCK_TEST} | {
    f"scripts/authority_claims/{name}" for name in
    ("model.py", "observe.py", "render.py", "check.py", "test_check.py", "native.rs")
}


class Repository:
    """Read-only file boundary. No history, remote, archive, or dynamic expressions."""

    def __init__(self, root):
        self.root = root.resolve()
        def git_paths(*args):
            result = subprocess.run(["git", "ls-files", "-z", *args], cwd=root,
                                    check=True, capture_output=True)
            return {p.decode("utf-8") for p in result.stdout.split(b"\0") if p}
        self.tracked = git_paths("--cached")
        self.pending = git_paths("--others", "--exclude-standard") & PENDING

    def path(self, name, directory=False, output=False):
        text(name)
        parts = PurePosixPath(name).parts
        require(not name.startswith("/") and "\\" not in name and
                all(p not in (".", "..") for p in parts) and
                "/".join(parts) == name, "path: repository relative, normalized")
        require(parts[0] in {"docs", "crates", "tools", "scripts", ".github", "TESTING.md"}, "path: root")
        path = self.root
        for part in parts:
            path = path / part
            require(not path.is_symlink(), "path: symlink")
        if output and name == OUTPUT and not path.exists():
            ignored = subprocess.run(["git", "check-ignore", "--no-index", "-q", "--", name], cwd=self.root)
            require(ignored.returncode == 1, "output: ignored")
            return path
        if directory:
            require(path.is_dir(), "path: directory missing")
        else:
            require(name in self.tracked | self.pending, "path: not clone-visible")
            require(path.exists() and stat.S_ISREG(path.stat().st_mode), "path: regular file missing")
        return path

    def raw(self, name):
        return self.path(name).read_bytes()

    def json(self, name):
        return parse(self.raw(name))


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def corpus(repo, identity):
    """Count *.prov.json recursively; seal the COMPLETE root's path/byte universe.

    All members must be tracked regular .csv or .prov.json files, plus the fixed
    Tier-A MANIFEST.txt. Missing tracked
    files and untracked/ignored additions fail. Only universal tier/dependency
    metadata is interpreted; other JSON fields are opaque, but byte-witnessed.
    """
    root = BINDINGS[identity][1]
    base = repo.path(root, directory=True)
    members = []
    records = 0
    tier, dependency = ("A", False) if identity == "tier-a-records" else ("2", True)
    for parent, dirs, files in os.walk(base, followlinks=False):
        dirs.sort()
        for name in dirs:
            require(not (Path(parent) / name).is_symlink(), "corpus: symlink directory")
        for name in sorted(files):
            relative = (Path(parent) / name).relative_to(repo.root).as_posix()
            require(relative in repo.tracked, "corpus: untracked/ignored member")
            require(name.endswith((".csv", ".prov.json")) or
                    relative == "tools/golden-gen/goldens/MANIFEST.txt", f"corpus: unexpected member {relative}")
            raw = repo.raw(relative)
            members.append((relative, sha(raw)))
            if name.endswith(".prov.json"):
                record = parse(raw)
                require(type(record) is dict, "corpus: record object")
                require(record.get("tier") == tier, "corpus: tier")
                require(record.get("depends_on_oce_blocks") is dependency, "corpus: dependency metadata")
                records += 1
    expected = {p for p in repo.tracked if p.startswith(root + "/")}
    require({p for p, _ in members} == expected, "corpus: missing tracked member")
    require(records > 0, "corpus: empty")
    enumeration = "".join(f"{digest}  {path}\n" for path, digest in sorted(members))
    return {"records": records, "members": len(members), "tier": tier,
            "depends_on_oce_blocks": dependency, "inventory_sha256": sha(enumeration.encode())}


def public_descriptors(value):
    """Shape-check descriptors for display; native owner checks row semantics."""
    keys(value, {"schema", "authority", "range_encoding", "statuses", "baselines", "groups", "entries"}, "public ledger")
    require(value["schema"] == "oce-public-surface-ledger/v1" and
            value["authority"] == "docs/public-surface-contract.md", "public ledger: identity")
    text(value["range_encoding"])
    require(type(value["statuses"]) is list and bool(value["statuses"]), "public ledger: statuses")
    for status in value["statuses"]:
        text(status)
    require(type(value["baselines"]) is list and len(value["baselines"]) == 2, "public ledger: baselines")
    paths = {"oce-api": "crates/oce-api/tests/public-api.txt", "oce-store": "crates/oce-store/tests/public-api.txt"}
    seen = set()
    for row in value["baselines"]:
        keys(row, {"id", "path", "sha256", "item_count"}, "baseline")
        require(type(row["id"]) is str and row["id"] in paths and row["id"] not in seen, "baseline: id")
        seen.add(row["id"])
        require(row["path"] == paths[row["id"]], "baseline: path binding")
        digest_text(row["sha256"], 64)
        require(integer(row["item_count"]) > 0, "baseline: empty")
    require(type(value["groups"]) is dict and bool(value["groups"]), "public ledger: groups")
    for group, row in value["groups"].items():
        text(group)
        keys(row, {"status", "surface", "provenance", "rationale"}, "public group")
        for field in row.values():
            text(field)
        require(row["status"] in value["statuses"], "public group: status")
    require(type(value["entries"]) is list and bool(value["entries"]), "public ledger: entries")
    for row in value["entries"]:
        keys(row, {"baseline", "group", "ranges"}, "public entry")
        require(type(row["baseline"]) is str and row["baseline"] in paths and
                type(row["group"]) is str and row["group"] in value["groups"], "public entry: binding")
        require(type(row["ranges"]) is list and bool(row["ranges"]), "public entry: ranges")
        for interval in row["ranges"]:
            text(interval)
    return value


def observe(repo):
    index = repo.json(INDEX)
    facts = validate(index)
    observed = {}
    for identity, row in sorted(facts.items()):
        repo.path(row["verifier"])
        if row["mode"] == "corpus":
            observed[identity] = corpus(repo, identity)
        elif row["mode"] == "delegated":
            value = repo.json(row["source"])
            if identity == "packages":
                require(type(value) is dict, "package ledger: object")
                # Reuse the owner's schema, never its package matrix/closure implementation.
                owner_path = str(ROOT / "scripts/package_policy")
                if owner_path not in sys.path:
                    sys.path.insert(0, owner_path)
                package_owner = importlib.import_module("validate")
                package_owner.validate_ledger(value)
            elif identity == "public-surface":
                public_descriptors(value)
                for baseline in value["baselines"]:
                    repo.path(baseline["path"])
            elif identity == "catalog-source":
                keys(value, {"schema_version", "catalog", "repository", "branch", "commit", "fetched_at",
                             "catalog_fingerprint", "package_order_files", "structural_source_files",
                             "drift_source_files", "external_source_files", "notes"}, "catalog source")
                require(type(value.get("schema_version")) is int and value["schema_version"] == 1, "catalog source: schema")
                digest_text(value.get("commit"), 40)
                digest_text(value.get("catalog_fingerprint"), 16)
                for field in ("catalog", "repository", "branch", "fetched_at"):
                    text(value[field])
                for field in ("package_order_files", "structural_source_files", "drift_source_files", "external_source_files", "notes"):
                    require(type(value[field]) is list and bool(value[field]), "catalog source: array")
            else:
                require(type(value) is list and bool(value), "registry: nonempty array")
                for entry in value:
                    require(type(entry) is dict, "registry: object")
                    named = {"input_names", "output_names"} if entry.get("port_naming") == "named" else set()
                    keys(entry, {"class_path", "inputs", "outputs", "width_driven", "param_rules", "port_naming",
                                 "stateful", "reserved", "param_defaults"} | named, "registry entry")
                    text(entry.get("class_path"))
                    text(entry["port_naming"])
                    for field in ("reserved", "stateful", "width_driven"):
                        require(type(entry[field]) is bool, "registry: bool")
                    for field in {"inputs", "outputs", "param_rules", "param_defaults"} | named:
                        require(type(entry[field]) is list, "registry: array")
            observed[identity] = {"value": value, "sha256": sha(repo.raw(row["source"]))}
        else:
            repo.path(row["source"])
    for row in index["supersession"]:
        if row["authority_path"] is not None:
            repo.path(row["authority_path"])
    return index, observed
