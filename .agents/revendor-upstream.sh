#!/usr/bin/env bash
set -euo pipefail

# P1 read-only, event-driven upstream reporter. Run from inside the repo checkout.
# It never regenerates or updates any pinned artifact.
# BUILDINGS_COMMIT: git -C /tmp/modelica-buildings rev-parse HEAD
# MODELICA_JSON_COMMIT: git -C /tmp/modelica-json rev-parse HEAD
# The subtree SHA is read from the anchored Rust constant at runtime.
# Read-only overrides: OCE_REVENDOR_ROOT, OCE_REVENDOR_MANIFEST,
# OCE_REVENDOR_RUST_CONSTS, OCE_REVENDOR_API_BASE.

BUILDINGS_COMMIT="a131864e4c4df22ebcd52bb8da439de0087ac365"
MODELICA_JSON_COMMIT="85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb"

repo_root=$(git rev-parse --show-toplevel)
vendor_root=${OCE_REVENDOR_ROOT:-"$repo_root/third_party/modelica-buildings-cdl"}
manifest_file=${OCE_REVENDOR_MANIFEST:-"$repo_root/tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json"}
rust_file=${OCE_REVENDOR_RUST_CONSTS:-"$repo_root/crates/oce-cxf/tests/third_party_manifest/mod.rs"}
api_base=${OCE_REVENDOR_API_BASE:-"https://api.github.com"}
override_active=0
if [ "${OCE_REVENDOR_ROOT+x}" = x ] ||
   [ "${OCE_REVENDOR_MANIFEST+x}" = x ] ||
   [ "${OCE_REVENDOR_RUST_CONSTS+x}" = x ] ||
   [ "${OCE_REVENDOR_API_BASE+x}" = x ]; then
  override_active=1
fi

scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/oce-revendor.XXXXXX")
trap 'rm -rf "$scratch_dir"' EXIT

if command -v shasum >/dev/null 2>&1; then
  find "$vendor_root" -type f -print0 | xargs -0 shasum -a 256 >"$scratch_dir/hashes" || true
else
  find "$vendor_root" -type f -print0 | xargs -0 sha256sum >"$scratch_dir/hashes" || true
fi
git grep -l "$BUILDINGS_COMMIT" >"$scratch_dir/blast-files" || true
git grep -n "$BUILDINGS_COMMIT" >"$scratch_dir/blast-lines" || true

extract_sha() {
  python3 - "$1" "$2" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as source:
        value = json.load(source)
    print(next((x.get("sha", "") for x in value.get("tree", [])
                if x.get("path") == sys.argv[2]), ""))
except (OSError, ValueError, TypeError):
    print("")
PY
}

fetch() {
  name=$1
  url=$2
  BODY=$(curl -sS -m 30 --compressed -D "$scratch_dir/$name.headers" \
    -o "$scratch_dir/$name.body" -w $'\n%{http_code}' "$url" \
    2>"$scratch_dir/$name.error" || echo $'\n000')
  code=$(printf '%s\n' "$BODY" | tail -n 1)
  case "$code" in [0-9][0-9][0-9]) ;; *) code=000 ;; esac
  printf '%s\n' "$code" >"$scratch_dir/$name.code"
}

fetch pin_root "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$BUILDINGS_COMMIT"
pin_buildings=$(extract_sha "$scratch_dir/pin_root.body" Buildings)
if [ -n "$pin_buildings" ]; then
  fetch pin_buildings "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$pin_buildings"
fi
pin_controls=$(extract_sha "$scratch_dir/pin_buildings.body" Controls)
if [ -n "$pin_controls" ]; then
  fetch pin_controls "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$pin_controls"
fi
pin_obc=$(extract_sha "$scratch_dir/pin_controls.body" OBC)
if [ -n "$pin_obc" ]; then
  fetch pin_cone "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$pin_obc?recursive=1"
fi

fetch master_root "$api_base/repos/lbl-srg/modelica-buildings/git/trees/master"
master_buildings=$(extract_sha "$scratch_dir/master_root.body" Buildings)
if [ -n "$master_buildings" ]; then
  fetch master_buildings "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$master_buildings"
fi
master_controls=$(extract_sha "$scratch_dir/master_buildings.body" Controls)
if [ -n "$master_controls" ]; then
  fetch master_controls "$api_base/repos/lbl-srg/modelica-buildings/git/trees/$master_controls"
fi
fetch modelica_master "$api_base/repos/lbl-srg/modelica-json/commits/master"

python3 - "$repo_root" "$vendor_root" "$manifest_file" "$rust_file" "$scratch_dir" \
  "$BUILDINGS_COMMIT" "$MODELICA_JSON_COMMIT" "$override_active" <<'PY'
import json, os, re, subprocess, sys
from collections import Counter

(repo, root, manifest_name, rust_name, scratch, buildings_pin,
 modelica_pin, override_text) = sys.argv[1:]
override = override_text == "1"
integrity = incomplete = trigger = False

def emit(number, name, status, detail):
    print(f"LEG {number} {name}: {status} — {detail}")

def sub(name, status, detail):
    print(f"  SUB {name}: {status} — {detail}")

def load(filename):
    try:
        with open(filename, encoding="utf-8") as source:
            return json.load(source), None
    except (OSError, ValueError, TypeError) as error:
        return None, str(error)

manifest, manifest_error = load(manifest_name)
try:
    with open(rust_name, encoding="utf-8") as source:
        rust = source.read()
except OSError:
    rust = ""

def constant(name):
    match = re.search(rf'^const {name}: &str = "([0-9a-f]{{40}})";$', rust, re.M)
    return match.group(1) if match else None

rust_buildings = constant("BUILDINGS_COMMIT")
rust_modelica = constant("MODELICA_JSON_COMMIT")
rust_subtree = constant("SUBTREE_TREE_SHA")
errors = []
for name, value in (("BUILDINGS_COMMIT", rust_buildings),
                    ("MODELICA_JSON_COMMIT", rust_modelica),
                    ("SUBTREE_TREE_SHA", rust_subtree)):
    if value is None:
        errors.append(f"constant not found in {rust_name}: {name}")
if manifest_error:
    errors.append(f"manifest unreadable: {manifest_error}")
else:
    for name, script_value, field, rust_value in (
        ("BUILDINGS_COMMIT", buildings_pin, "buildings_commit", rust_buildings),
        ("MODELICA_JSON_COMMIT", modelica_pin, "modelica_json_commit", rust_modelica),
    ):
        if script_value != manifest.get(field) or script_value != rust_value:
            errors.append(f"{name} mismatch: script={script_value}, "
                          f"manifest={manifest.get(field)}, rust={rust_value}")
    if manifest.get("subtree_tree_sha") != rust_subtree:
        errors.append(f"SUBTREE_TREE_SHA mismatch: manifest="
                      f"{manifest.get('subtree_tree_sha')}, rust={rust_subtree}")
if errors:
    integrity = True
    emit(1, "constants-cross-check", "RED", "; ".join(errors))
else:
    emit(1, "constants-cross-check", "OK",
         f"BUILDINGS_COMMIT={rust_buildings}; MODELICA_JSON_COMMIT={rust_modelica}; "
         f"SUBTREE_TREE_SHA={rust_subtree}")

local_errors = []
walked = {}
for directory, _, filenames in os.walk(root):
    for filename in filenames:
        full_name = os.path.join(directory, filename)
        relative = os.path.relpath(full_name, root).replace(os.sep, "/")
        walked[relative] = full_name
entries = {}
if manifest:
    entries = {entry["path"]: entry for entry in manifest.get("entries", [])
               if isinstance(entry, dict) and isinstance(entry.get("path"), str)}
for relative in sorted(set(walked) - set(entries)):
    local_errors.append(f"added file: {relative}")
for relative in sorted(set(entries) - set(walked)):
    local_errors.append(f"missing file: {relative}")
counts = Counter()
for relative in walked:
    if relative.startswith("Buildings/"):
        counts["upstream-buildings"] += 1
    elif relative.startswith("cxf/"):
        counts["generated-cxf"] += 1
    elif relative == "README.md":
        counts["repo-authored"] += 1
    else:
        local_errors.append(f"unrecognized vendored path: {relative}")
if manifest:
    for bucket in ("upstream-buildings", "generated-cxf", "repo-authored"):
        expected = manifest.get("bucket_counts", {}).get(bucket)
        if counts[bucket] != expected:
            local_errors.append(f"{bucket} walked count {counts[bucket]} != header {expected}")
hashes = {}
try:
    with open(os.path.join(scratch, "hashes"), encoding="utf-8") as source:
        for line in source:
            match = re.match(r"^([0-9a-f]{64})\s+\*?(.*)$", line.rstrip())
            if match:
                relative = os.path.relpath(os.path.abspath(match.group(2)), root)
                hashes[relative.replace(os.sep, "/")] = match.group(1)
except OSError as error:
    local_errors.append(f"hash output unreadable: {error}")
for relative in sorted(set(walked) & set(entries)):
    if hashes.get(relative) != entries[relative].get("sha256"):
        local_errors.append(f"byte mismatch: {relative} (sha256="
                            f"{hashes.get(relative, 'unavailable')}, expected="
                            f"{entries[relative].get('sha256')})")
if override and os.environ.get("OCE_REVENDOR_ROOT") is not None:
    anchor_status, anchor_detail = "SKIPPED", "SKIPPED (override active)"
else:
    try:
        tree = subprocess.check_output(
            ["git", "-C", repo, "rev-parse",
             "HEAD:third_party/modelica-buildings-cdl"],
            text=True, stderr=subprocess.DEVNULL).strip()
    except subprocess.CalledProcessError:
        tree = ""
    if tree == rust_subtree:
        anchor_status, anchor_detail = "OK", f"HEAD subtree matches {rust_subtree}"
    else:
        anchor_status = "RED"
        anchor_detail = f"HEAD subtree={tree or 'unavailable'} != Rust constant={rust_subtree}"
        local_errors.append("committed subtree anchor mismatch")
if local_errors:
    integrity = True
    emit(2, "local-tamper", "RED", "; ".join(local_errors))
else:
    emit(2, "local-tamper", "OK",
         f"{len(walked)} files match the manifest in both directions and by SHA-256")
sub("subtree-anchor", anchor_status, anchor_detail)

def response(name, side):
    code_name = os.path.join(scratch, name + ".code")
    if not os.path.exists(code_name):
        return None, f"{name} not evaluated because its parent was unavailable", "unavailable"
    with open(code_name, encoding="utf-8") as source:
        code = source.read().strip()
    def text(suffix):
        try:
            with open(os.path.join(scratch, name + suffix),
                      encoding="utf-8", errors="replace") as source:
                return source.read()
        except OSError:
            return ""
    headers, curl_error = text(".headers"), text(".error").strip()
    if code in ("404", "422") and side == "pin":
        return None, f"HTTP {code} during pin walk at {name}", "integrity"
    if code == "404" and side == "master":
        return (None,
                "master ref not found; the upstream default branch may have moved",
                "unavailable")
    if code == "403":
        remaining = re.search(r"(?im)^x-ratelimit-remaining:\s*(\d+)", headers)
        detail = ("HTTP 403: GitHub API rate limit exhausted"
                  if remaining and remaining.group(1) == "0"
                  else "HTTP 403 from GitHub API")
        return None, detail, "unavailable"
    if code == "429" or (code.isdigit() and int(code) >= 500):
        return None, f"HTTP {code} from GitHub API", "unavailable"
    if code != "200":
        detail = f"curl transport failure (HTTP {code})"
        if curl_error:
            detail += f": {curl_error}"
        return None, detail, "unavailable"
    value, error = load(os.path.join(scratch, name + ".body"))
    if error:
        return None, f"HTTP 200 body is not JSON at {name}: {error}", "unavailable"
    return value, None, None

pin = {}
problems = []
for name in ("pin_root", "pin_buildings", "pin_controls", "pin_cone"):
    value, detail, kind = response(name, "pin")
    pin[name] = value
    if kind:
        problems.append((kind, detail))
if any(kind == "integrity" for kind, _ in problems):
    integrity = True
    emit(3, "upstream-fidelity", "RED",
         "; ".join(detail for kind, detail in problems if kind == "integrity"))
elif problems:
    incomplete = True
    emit(3, "upstream-fidelity", "UNAVAILABLE",
         "; ".join(detail for _, detail in problems))
else:
    fidelity = []
    buildings_items = {x.get("path"): x for x in pin["pin_buildings"].get("tree", [])}
    cone = pin["pin_cone"]
    if cone.get("truncated") is True:
        fidelity.append("recursive OBC cone response has truncated=true")
    expected = {x["path"]: x for x in entries.values()
                if x.get("origin") == "upstream-buildings"}
    actual = {}
    if "legal.html" in buildings_items:
        actual["Buildings/legal.html"] = buildings_items["legal.html"].get("sha")
    for item in cone.get("tree", []):
        if item.get("type") == "blob" and isinstance(item.get("path"), str):
            actual["Buildings/Controls/OBC/" + item["path"]] = item.get("sha")
    # Fidelity is manifest-driven and one-directional: every vendored entry must exist
    # upstream with a matching blob OID. The upstream cone is a SUPERSET of the vendored
    # set by design (Validation/, package.mo, non-CDL subtrees are deliberately not
    # vendored), so upstream objects absent from the manifest are never a finding here;
    # additions INSIDE the vendored tree are leg 2's business (fail-closed local walk).
    for relative in sorted(set(expected) - set(actual)):
        fidelity.append(f"upstream object missing: {relative}")
    for relative in sorted(set(expected) & set(actual)):
        if actual[relative] != expected[relative].get("git_blob_oid"):
            fidelity.append(f"blob OID mismatch: {relative}")
    if fidelity:
        integrity = True
        emit(3, "upstream-fidelity", "RED", "; ".join(fidelity))
    else:
        emit(3, "upstream-fidelity", "OK",
             f"{len(expected)} manifest blob OIDs match upstream at the pin")

master = {}
problems = []
for name in ("master_root", "master_buildings", "master_controls"):
    value, detail, kind = response(name, "master")
    master[name] = value
    if kind:
        problems.append(detail)
if problems:
    incomplete = True
    emit(4, "drift-trigger", "UNAVAILABLE", "; ".join(problems))
else:
    pin_buildings_items = {x.get("path"): x for x in
                           (pin.get("pin_buildings") or {}).get("tree", [])}
    pin_controls_items = {x.get("path"): x for x in
                          (pin.get("pin_controls") or {}).get("tree", [])}
    master_buildings_items = {x.get("path"): x for x in
                              master["master_buildings"].get("tree", [])}
    master_controls_items = {x.get("path"): x for x in
                             master["master_controls"].get("tree", [])}
    pin_obc = pin_controls_items.get("OBC", {}).get("sha")
    master_obc = master_controls_items.get("OBC", {}).get("sha")
    pin_legal = pin_buildings_items.get("legal.html", {}).get("sha")
    master_legal = master_buildings_items.get("legal.html", {}).get("sha")
    if not pin_obc or not pin_legal:
        incomplete = True
        emit(4, "drift-trigger", "UNAVAILABLE", "pinned comparison objects unavailable")
    elif master_obc != pin_obc or master_legal != pin_legal:
        trigger = True
        emit(4, "drift-trigger", "RED",
             f"TRIGGER: subtree SHA divergence (OBC {pin_obc} -> {master_obc}; "
             f"legal.html {pin_legal} -> {master_legal}); policy triggers: needed class "
             "absent, upstream subtree SHA divergence, or intended CDL specification "
             "revision; any pin-advance PR runs the full gate first-hand — see the "
             "vendored README's Pin-advance policy section")
    else:
        emit(4, "drift-trigger", "OK",
             f"no trigger: OBC tree={pin_obc} and legal.html blob={pin_legal}")

modelica, detail, kind = response("modelica_master", "master")
def lines(filename):
    try:
        with open(os.path.join(scratch, filename), encoding="utf-8") as source:
            return [line.rstrip("\n") for line in source if line.rstrip("\n")]
    except OSError:
        return []
files, occurrences = lines("blast-files"), lines("blast-lines")
areas = Counter(line.split(":", 1)[0].split("/", 1)[0] for line in occurrences)
blast = (f"blast radius: {len(files)} files, {len(occurrences)} lines; " +
         ", ".join(f"{name}={count}" for name, count in sorted(areas.items())))
if kind:
    incomplete = True
    emit(5, "informational", "UNAVAILABLE", f"{detail}; {blast}")
else:
    upstream = modelica.get("sha")
    movement = ("toolchain unchanged" if upstream == modelica_pin
                else f"toolchain moved: {modelica_pin} -> {upstream}")
    emit(5, "informational", "OK", f"{movement}; {blast}")

if integrity:
    code, meaning = 3, ("integrity red; see findings and the vendored README's "
                        "Pin-advance policy section")
elif incomplete:
    code, meaning = 4, "run incomplete; NOT an all-clear"
elif trigger:
    code, meaning = 2, "drift trigger present; integrity legs green"
else:
    code, meaning = 0, "all legs evaluated, all green, no trigger"
if override:
    meaning += " (override run)"
print(f"SUMMARY: exit={code} — {meaning}")
sys.exit(code)
PY
