#!/usr/bin/env python3
"""Hostile behavioral controls for the aggregate index, not replacement owners."""

import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import check
from model import BINDINGS, INDEX, OUTPUT, ClaimError, parse, validate
from observe import ROOT, Repository, corpus, observe
from render import render


class IndexTests(unittest.TestCase):
    def setUp(self):
        self.index = parse((ROOT / INDEX).read_bytes())

    def rejects(self, value, message):
        with self.assertRaisesRegex(ClaimError, message):
            validate(value)

    def test_closed_root_and_projection(self):
        for field in self.index:
            with self.subTest(field=field):
                wrong = copy.deepcopy(self.index)
                del wrong[field]
                self.rejects(wrong, "fields")
        for field, value in [("schema", True), ("schema", "unknown"),
                             ("projections", []), ("projections", [{"id": "summary", "path": INDEX}])]:
            wrong = copy.deepcopy(self.index)
            wrong[field] = value
            self.rejects(wrong, "index:")
        self.rejects(self.index | {"extra": None}, "fields")
        for value in [None, [], True, 1, "index"]:
            self.rejects(value, "fields")

    def test_every_fact_required_unique_and_closed(self):
        for position, row in enumerate(self.index["facts"]):
            with self.subTest(fact=row["id"]):
                missing = copy.deepcopy(self.index)
                missing["facts"].pop(position)
                self.rejects(missing, "missing/unknown")
                duplicate = copy.deepcopy(self.index)
                duplicate["facts"].append(row)
                self.rejects(duplicate, "duplicate id")
                for field in row:
                    wrong = copy.deepcopy(self.index)
                    del wrong["facts"][position][field]
                    self.rejects(wrong, "fields|row/id")
                wrong = copy.deepcopy(self.index)
                wrong["facts"][position]["unexpected"] = 0
                self.rejects(wrong, "fields")
        for value in [[], None, {}, [None], [{"id": "unknown"}]]:
            self.rejects(self.index | {"facts": value}, "facts:")

    def test_every_binding_rejects_legitimate_but_unrelated_sources(self):
        for position, row in enumerate(self.index["facts"]):
            unrelated = "docs/README.md"
            for field, value in [("source", unrelated), ("verifier", unrelated),
                                 ("mode", "review-only" if row["mode"] != "review-only" else "native"),
                                 ("projection", "unknown")]:
                with self.subTest(fact=row["id"], field=field):
                    wrong = copy.deepcopy(self.index)
                    wrong["facts"][position][field] = value
                    self.rejects(wrong, "binding|fields|projection")

    def test_native_numbers_are_u32_not_boolean_or_coerced(self):
        for row in self.index["facts"]:
            if row["mode"] != "native":
                continue
            for value in [True, False, -1, 2**32, 1.0, "1", None, [], {}]:
                wrong = copy.deepcopy(self.index)
                next(r for r in wrong["facts"] if r["id"] == row["id"])["expected"] = value
                self.rejects(wrong, "integer")
            # Real comparison belongs to compiled owner tests, not Python constants.
            for value in [0, 2**32 - 1]:
                wrong = copy.deepcopy(self.index)
                next(r for r in wrong["facts"] if r["id"] == row["id"])["expected"] = value
                validate(wrong)

    def test_duplicate_json_keys_and_non_json_constants_fail(self):
        for raw in ['{"facts": [], "facts": []}', '{"x":{"a":1,"\\u0061":2}}',
                    '{', '{"x":NaN}', '{"x":Infinity}', b'\xff']:
            with self.assertRaisesRegex(ClaimError, "JSON"):
                parse(raw)

    def test_supersession_closure_status_null_and_cycles(self):
        for position, row in enumerate(self.index["supersession"]):
            for field in row:
                wrong = copy.deepcopy(self.index)
                del wrong["supersession"][position][field]
                self.rejects(wrong, "fields|row/id")
            for field, value in [("status", "current"), ("authority_path", INDEX),
                                 ("authority_path", "TESTING.md"), ("superseded_by", row["id"]),
                                 ("superseded_by", "unknown"), ("reason", ""),
                                 ("historical_locator", OUTPUT)]:
                wrong = copy.deepcopy(self.index)
                wrong["supersession"][position][field] = value
                self.rejects(wrong, "binding|cycle|unknown target|text|self reference")
            wrong = copy.deepcopy(self.index)
            wrong["supersession"].pop(position)
            self.rejects(wrong, "missing/unknown")
            wrong = copy.deepcopy(self.index)
            wrong["supersession"].append(row)
            self.rejects(wrong, "duplicate id")
        wrong = copy.deepcopy(self.index)
        wrong["supersession"][0]["superseded_by"] = wrong["supersession"][1]["id"]
        validate(wrong)
        wrong["supersession"][1]["superseded_by"] = wrong["supersession"][0]["id"]
        self.rejects(wrong, "cycle")
        for position in range(4):
            wrong = copy.deepcopy(self.index)
            wrong["supersession"][position]["authority_path"] = None
            self.rejects(wrong, "binding")


class SourceTests(unittest.TestCase):
    """Temporary exact input bytes with explicit inventory; never stage or edit Git."""

    @classmethod
    def setUpClass(cls):
        cls.live = Repository(ROOT)
        cls.index, cls.observed = observe(cls.live)
        cls.golden = (ROOT / OUTPUT).read_bytes()
        paths = {INDEX, OUTPUT, "crates/oce-api/tests/public-api.txt", "crates/oce-store/tests/public-api.txt"}
        for _, source, verifier in BINDINGS.values():
            paths.add(verifier)
            if (ROOT / source).is_file():
                paths.add(source)
            else:
                paths.update(p for p in cls.live.tracked if p.startswith(source + "/"))
        paths.update(r["authority_path"] for r in cls.index["supersession"] if r["authority_path"])
        cls.seed = {p: (ROOT / p).read_bytes() for p in paths}

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name, raw in self.seed.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        # File-boundary methods are real; only Git's inventory is fixture input.
        self.repo = object.__new__(Repository)
        self.repo.root = self.root
        self.repo.tracked = set(self.seed)
        self.repo.pending = set()

    def write_json(self, name, value):
        (self.root / name).write_text(json.dumps(value), encoding="utf-8")

    def test_live_projection_is_golden_and_twice_render_is_byte_identical(self):
        first, second = self.root / "first.md", self.root / "second.md"
        first.write_bytes(render(*observe(self.repo)))
        second.write_bytes(render(*observe(self.repo)))
        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(first.read_bytes(), self.golden)
        self.assertEqual(check.run(ROOT), self.golden)

    def test_sort_order_does_not_change_output(self):
        index = copy.deepcopy(self.index)
        index["facts"].reverse()
        index["supersession"].reverse()
        self.assertEqual(render(index, self.observed), self.golden)

    def test_each_native_expected_value_is_projected_without_claiming_execution(self):
        for row in self.index["facts"]:
            if row["mode"] == "native":
                index = copy.deepcopy(self.index)
                next(r for r in index["facts"] if r["id"] == row["id"])["expected"] += 1
                validate(index)
                self.assertNotEqual(render(index, self.observed), self.golden)

    def test_byte_stale_even_formatting_fails_without_writing(self):
        for suffix in [b" ", b"\n", b"<!-- harmless -->"]:
            wrong = self.golden + suffix
            (self.root / OUTPUT).write_bytes(wrong)
            with patch.object(check, "Repository", return_value=self.repo):
                with self.assertRaisesRegex(ClaimError, "stale bytes"):
                    check.run(self.root)
            self.assertEqual((self.root / OUTPUT).read_bytes(), wrong)

    def test_changed_owner_requires_explicit_regeneration_not_self_bless(self):
        name = BINDINGS["public-surface"][1]
        owner = self.repo.json(name)
        owner["baselines"][0]["item_count"] += 1
        self.write_json(name, owner)
        before = {p: (self.root / p).read_bytes() for p in self.seed}
        with patch.object(check, "Repository", return_value=self.repo):
            with self.assertRaisesRegex(ClaimError, "stale bytes"):
                check.run(self.root)
            check.run(self.root, write=True)
            check.run(self.root)
        after = {p: (self.root / p).read_bytes() for p in self.seed}
        self.assertEqual({p for p in before if before[p] != after[p]}, {OUTPUT})
        self.assertNotEqual(after[OUTPUT], self.golden)
        # Native/public owner validation remains REQUIRED even after --write.

    def test_every_delegated_fact_mutation_changes_projection(self):
        for identity, (mode, name, _) in BINDINGS.items():
            if mode != "delegated":
                continue
            with self.subTest(fact=identity):
                # Even semantically inert owner formatting is byte-witnessed.
                (self.root / name).write_bytes(self.seed[name] + b"\n")
                self.assertNotEqual(render(*observe(self.repo)), self.golden)
                (self.root / name).write_bytes(self.seed[name])

    def test_every_structured_source_rejects_malformed_or_incomplete_data(self):
        names = [source for mode, source, _ in BINDINGS.values() if mode == "delegated"]
        for name in names:
            for raw in [b"{", b"null", b"{}", b"[]", b'{"x":1,"x":2}']:
                with self.subTest(source=name, raw=raw):
                    (self.root / name).write_bytes(raw)
                    with self.assertRaises(ValueError):
                        observe(self.repo)
            (self.root / name).write_bytes(self.seed[name])

    def test_delegated_descriptor_fields_cannot_disappear_or_expand_silently(self):
        for identity, (mode, name, _) in BINDINGS.items():
            if mode != "delegated":
                continue
            owner = parse(self.seed[name])
            record = owner[0] if identity == "catalog-registry" else owner
            for field in [*record, "unexpected"]:
                changed = copy.deepcopy(owner)
                row = changed[0] if identity == "catalog-registry" else changed
                if field == "unexpected":
                    row[field] = None
                else:
                    del row[field]
                self.write_json(name, changed)
                with self.subTest(source=name, field=field), self.assertRaises(ValueError):
                    observe(self.repo)
            (self.root / name).write_bytes(self.seed[name])

    def test_public_descriptor_redirects_and_wrong_types_fail(self):
        name = BINDINGS["public-surface"][1]
        for field, value in [("path", "TESTING.md"), ("sha256", "0"),
                             ("item_count", True), ("id", "unknown")]:
            owner = parse(self.seed[name])
            owner["baselines"][0][field] = value
            self.write_json(name, owner)
            with self.assertRaises(ClaimError):
                observe(self.repo)

    def test_paths_reject_escape_ignored_untracked_and_symlinks(self):
        for name in ["/etc/passwd", "docs/../TESTING.md", "docs//x", "./TESTING.md",
                     "docs\\x", "_spec/hidden", "docs/unknown.json"]:
            with self.assertRaisesRegex(ClaimError, "path:"):
                self.repo.path(name)
        for name in [INDEX, BINDINGS["public-surface"][1]]:
            path = self.root / name
            path.unlink()
            with self.assertRaisesRegex(ClaimError, "regular file missing"):
                self.repo.path(name)
            path.symlink_to(self.root / "TESTING.md")
            with self.assertRaisesRegex(ClaimError, "symlink"):
                self.repo.path(name)
            path.unlink()
            path.write_bytes(self.seed[name])
        docs = self.root / "docs"
        docs.rename(self.root / "relocated")
        docs.symlink_to(self.root / "relocated", target_is_directory=True)
        with self.assertRaisesRegex(ClaimError, "symlink"):
            self.repo.path(INDEX)

    def test_nonregular_source_and_missing_corpus_root_fail(self):
        path = self.root / INDEX
        path.unlink()
        path.mkdir()
        with self.assertRaisesRegex(ClaimError, "regular file missing"):
            self.repo.path(INDEX)
        root = self.root / BINDINGS["tier-two-records"][1]
        shutil.rmtree(root)
        with self.assertRaisesRegex(ClaimError, "directory missing"):
            corpus(self.repo, "tier-two-records")

    def test_historical_locators_are_never_opened_or_linked(self):
        index = copy.deepcopy(self.index)
        locator = "_spec/absent [promote](https://invalid.example/) <b>not authority</b> `code` | text"
        for row in index["supersession"]:
            row["historical_locator"] = locator
        self.write_json(INDEX, index)
        output = render(*observe(self.repo)).decode()
        self.assertIn("&lt;b&gt;not authority&lt;/b&gt;", output)
        self.assertIn("&#96;code&#96; &#124;", output)
        self.assertFalse((self.root / "_spec").exists())
        self.assertNotIn("href=", output)

    def test_corpus_universal_metadata_is_required_and_typed(self):
        for identity in ["tier-a-records", "tier-two-records"]:
            name = next(p for p in sorted(self.seed) if p.startswith(BINDINGS[identity][1] + "/") and p.endswith(".prov.json"))
            for raw in [b"{}", b"[]", b"{", b'{"tier":"A","tier":"2"}']:
                (self.root / name).write_bytes(raw)
                with self.assertRaises(ClaimError):
                    corpus(self.repo, identity)
            for field in ["tier", "depends_on_oce_blocks"]:
                for value in [None, 0, 1, "wrong", [], {}]:
                    record = parse(self.seed[name])
                    record[field] = value
                    self.write_json(name, record)
                    with self.assertRaisesRegex(ClaimError, "corpus:"):
                        corpus(self.repo, identity)
            (self.root / name).write_bytes(self.seed[name])

    def test_corpus_complete_enumeration_catches_add_remove_rename_and_bytes(self):
        for identity in ["tier-a-records", "tier-two-records"]:
            prefix = BINDINGS[identity][1] + "/"
            name = next(p for p in sorted(self.seed) if p.startswith(prefix) and p.endswith(".prov.json"))
            original = corpus(self.repo, identity)
            source = self.root / name
            added_name = prefix + "added.prov.json"
            added = self.root / added_name
            added.write_bytes(self.seed[name])
            with self.assertRaisesRegex(ClaimError, "untracked/ignored member"):
                corpus(self.repo, identity)
            self.repo.tracked.add(added_name)
            self.assertEqual(corpus(self.repo, identity)["records"], original["records"] + 1)
            added.unlink()
            self.repo.tracked.remove(added_name)
            source.unlink()
            with self.assertRaisesRegex(ClaimError, "missing tracked member"):
                corpus(self.repo, identity)
            self.repo.tracked.remove(name)
            reduced = corpus(self.repo, identity)
            self.assertEqual(reduced["records"], original["records"] - 1)
            added.write_bytes(self.seed[name])
            self.repo.tracked.add(added_name)
            renamed = corpus(self.repo, identity)
            self.assertEqual(renamed["records"], original["records"])
            self.assertNotEqual(renamed["inventory_sha256"], original["inventory_sha256"])
            added.unlink()
            self.repo.tracked.remove(added_name)
            source.write_bytes(self.seed[name] + b"\n")
            self.repo.tracked.add(name)
            self.assertNotEqual(corpus(self.repo, identity), original)
            source.write_bytes(self.seed[name])
            unexpected = self.root / (prefix + "unexpected.json")
            unexpected.write_text("{}")
            self.repo.tracked.add(prefix + "unexpected.json")
            with self.assertRaisesRegex(ClaimError, "unexpected member"):
                corpus(self.repo, identity)
            unexpected.unlink()
            self.repo.tracked.remove(prefix + "unexpected.json")

    def test_corpus_symlink_directory_and_empty_records_fail(self):
        identity = "tier-two-records"
        base = self.root / BINDINGS[identity][1]
        (base / "escape").symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ClaimError, "symlink directory"):
            corpus(self.repo, identity)
        (base / "escape").unlink()
        for path in base.glob("*.prov.json"):
            self.repo.tracked.remove(path.relative_to(self.root).as_posix())
            path.unlink()
        with self.assertRaisesRegex(ClaimError, "empty"):
            corpus(self.repo, identity)


class WiringTests(unittest.TestCase):
    def test_docs_script_changes_cannot_bypass_the_site_trigger(self):
        workflow = (ROOT / ".github/workflows/docs-pages.yml").read_text()
        self.assertEqual(workflow.splitlines().count('      - "scripts/authority_claims/**"'), 2)
        self.assertIn("run: python3 scripts/authority_claims/check.py --check", workflow)

    def test_cli_requires_one_mode_and_rejects_paths(self):
        for args in [[], ["--write", "--check"], ["--check", "--output", "wrong.md"]]:
            result = subprocess.run([sys.executable, str(ROOT / "scripts/authority_claims/check.py"), *args], capture_output=True)
            self.assertEqual(result.returncode, 2)

    def test_real_gate_executes_both_commands_and_propagates_failure(self):
        """Execute actual gate shell, stubbing expensive children only, not step()."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".agents").mkdir()
            gate = (ROOT / ".agents/gate.sh").read_text()
            (root / ".agents/gate.sh").write_text(gate)
            environment = root / "environment.sh"
            environment.write_text('''cargo() { return 0; }
bash() { return 0; }
env() { return 0; }
python3() {
  case "$*" in
    "scripts/authority_claims/check.py --check") return "${CLAIM_STATUS:-0}" ;;
    "scripts/authority_claims/test_check.py") return "${CONTROL_STATUS:-0}" ;;
    *) return 0 ;;
  esac
}
''')
            for name in ["CLAIM_STATUS", "CONTROL_STATUS"]:
                result = subprocess.run(["bash", ".agents/gate.sh"], cwd=root, capture_output=True, text=True,
                                        env=os.environ | {"BASH_ENV": str(environment), name: "37"})
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("GATE FAILED", result.stdout)
            # Disabled required invocation must be detectable by the above control.
            disabled = gate.replace("step 'authority claim consistency'", "false && step 'authority claim consistency'")
            self.assertNotEqual(disabled, gate)
            (root / ".agents/gate.sh").write_text(disabled)
            result = subprocess.run(["bash", ".agents/gate.sh"], cwd=root, capture_output=True, text=True,
                                    env=os.environ | {"BASH_ENV": str(environment), "CLAIM_STATUS": "37"})
            self.assertEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()
