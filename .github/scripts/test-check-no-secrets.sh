#!/usr/bin/env bash
# Gate-behavior fixtures for check-no-secrets.sh (issue #524). A gate that cannot fail is not a
# gate, and this one could not for its private-key pattern: the pattern begins with five dashes,
# so `git grep` read it as an option and exited 129, and the scan took that for a clean tree.
# Every fixture runs in a throwaway repository, never in the real tree, and holds the scan to
# four things:
#
#   1. A clean tree passes. Its tracked files are a copy of the scan and of this file, so neither
#      holds a match and the scan of this repository stays clean; that is why every planted value
#      below is built at run time from fragments.
#   2. Each pattern the scan runs catches a planted match: exit 1, one FAIL line, under that
#      pattern's label, naming the planted file. Every alternative of an alternation, and every
#      letter of a one-letter class, is planted on its own, at the shortest length the pattern
#      accepts. Removing the planted file passes again. The labels planted here must be exactly
#      the scan's, so a pattern added to the scan without a planted match, or a planted match
#      whose pattern was removed, fails.
#   3. A `git grep` that cannot search fails the scan: exit 2, a FAIL line naming the pattern and
#      git's exit status, and git's own error left visible. Two controls: outside any repository
#      (git exits 128 for every pattern), and under a git that exits 129 with its usage text when
#      handed a pattern that begins with five dashes, which is the shipped defect reproduced with
#      `-e` in place. There a planted match for another pattern must still be reported: one
#      pattern's error does not stop the rest of the scan.
#   4. Checks 1 to 3 fail on four copies of the scan with a defect put back: `-e` removed, git's
#      error sent to /dev/null, an error read as no match, and the scan as shipped before issue
#      #524. Each copy must fail a named check, and a copy whose edit no longer applies to the
#      scan fails this file, so the controls cannot go stale unnoticed.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT="$(pwd)/.github/scripts/check-no-secrets.sh"
FIXTURE="$(pwd)/.github/scripts/test-check-no-secrets.sh"
REAL_GIT="$(command -v git)"

# The throwaway repositories must see neither the caller's repository nor its git configuration:
# a git hook exports GIT_DIR and GIT_INDEX_FILE, a user's grep.* settings must not decide the
# result, and git's messages are matched in English.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_COMMON_DIR
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 LC_ALL=C

tmp="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT
# Git looks for a repository no higher than the scratch directory, so the outside-a-repository
# control cannot find one above it.
export GIT_CEILING_DIRECTORIES="$tmp"

# `repeat C N` prints C N times.
repeat() {
  local out="" i=0
  while [ "$i" -lt "$2" ]; do
    out="$out$1"
    i=$((i + 1))
  done
  printf '%s' "$out"
}

dashes="$(repeat - 5)"

# One line per planted match: the label the scan must report, a case name, and the line planted.
# Each value is assembled from fragments, so this file never contains one whole.
planted() {
  local letter kind
  printf '%s\t%s\t%s\n' 'AWS access key id' aws-access-key-id \
    "aws_access_key_id = AKI""A$(repeat Q 16)"
  for kind in RSA EC OPENSSH PGP; do
    printf '%s\t%s\t%s\n' 'private key block' "private-key-$kind" \
      "${dashes}BEGIN $kind PRIVATE KEY${dashes}"
  done
  for letter in a b p r; do
    printf '%s\t%s\t%s\n' 'Slack token' "slack-token-xox$letter" \
      "token: xo""x$letter-$(repeat 7 10)"
  done
  for letter in p o u s r; do
    printf '%s\t%s\t%s\n' 'GitHub token' "github-token-gh$letter" \
      "token: g""h${letter}_$(repeat A 36)"
  done
  printf '%s\t%s\t%s\n' 'API token (sk- prefix)' api-token-sk \
    "api_key = s""k-$(repeat b 20)"
  printf '%s\t%s\t%s\n' 'UUID (possible agent / namespace identity)' uuid \
    "id: $(printf '%s-%s-%s-%s-%s' 0123abcd 4567 89ab cdef 0123456789ab)"
  for kind in Users home; do
    printf '%s\t%s\t%s\n' 'absolute developer home-directory path' "home-path-$kind" \
      "see /""$kind/a/notes.txt"
  done
  printf '%s\t%s\t%s\n' 'Aionforge environment variable name' aionforge-variable \
    "export AIONFORGE""_AB=1"
}
planted > "$tmp/planted.tsv"

# A repository whose tracked files are a copy of the scan and of this file: the clean tree.
new_repo() {
  local root="$1"
  mkdir -p "$root/.github/scripts"
  cp "$SCRIPT" "$FIXTURE" "$root/.github/scripts/"
  git -C "$root" -c init.defaultBranch=main init -q
  git -C "$root" add .
}

# A git whose grep exits 129 with its usage text when handed a pattern that begins with five
# dashes, as git did while the scan passed its patterns without `-e`.
mkdir -p "$tmp/shim"
cat > "$tmp/shim/git" <<EOF
#!/usr/bin/env bash
for arg in "\$@"; do
  case "\$arg" in
    ${dashes}*)
      echo 'usage: git grep [<options>] [-e] <pattern> [<rev>...] [[--] <path>...]' >&2
      exit 129
      ;;
  esac
done
exec '$REAL_GIT' "\$@"
EOF
chmod +x "$tmp/shim/git"

mkdir -p "$tmp/not-a-repo"
if git -C "$tmp/not-a-repo" rev-parse --git-dir > /dev/null 2>&1; then
  echo "BUG: the scratch directory is inside a git repository; the outside-a-repository control cannot run"
  exit 1
fi

# `run_scan SCAN DIR [VAR=VALUE...]` runs SCAN in DIR and sets `out` (stdout and stderr) and
# `out_status`.
run_scan() {
  local scan="$1" dir="$2"
  shift 2
  out_status=0
  out="$(cd "$dir" && env "$@" bash "$scan" 2>&1 < /dev/null)" || out_status=$?
}

has_line() { printf '%s\n' "$out" | grep -Fxq -- "$1"; }
has_text() { printf '%s\n' "$out" | grep -Fq -- "$1"; }
count_lines() { printf '%s\n' "$out" | grep -c -- "$1" || true; }

# Record a failed expectation of the case being run, with the scan's output.
fail_case() {
  echo "FAIL: $1: $2"
  printf '%s\n' "$out" | sed 's/^/    | /'
  suite_failures=$((suite_failures + 1))
}

# Checks 1 to 3 against one copy of the scan. Prints a FAIL line per failed expectation and
# returns non-zero if there was one. It never relies on errexit: it runs as an `if` condition.
suite() {
  local scan="$1"
  local root label name line labels want
  suite_failures=0
  root="$tmp/run.$$"

  # The labels planted here are exactly the scan's, each scan line in the one shape read here.
  labels="$(awk -F"'" '/^scan / {
      if (NF == 5 && $1 == "scan " && $3 == " " && $5 == "") print $4
      else print "UNREADABLE: " $0
    }' "$scan")"
  out="$labels"
  if has_text 'UNREADABLE: '; then
    fail_case labels "a scan line is not scan '<pattern>' '<label>', which this file reads"
  fi
  if [ "$(printf '%s\n' "$labels" | sort)" != "$(printf '%s\n' "$labels" | sort -u)" ]; then
    fail_case labels "two patterns share a label, so a FAIL line no longer names one pattern"
  fi
  want="$(cut -f1 "$tmp/planted.tsv" | sort -u)"
  if [ "$(printf '%s\n' "$labels" | sort -u)" != "$want" ]; then
    out="$(diff <(printf '%s\n' "$want") <(printf '%s\n' "$labels" | sort -u) || true)"
    fail_case labels "the labels planted here (<) are not the scan's (>)"
  fi

  # 1. The clean tree passes.
  rm -rf "$root"
  new_repo "$root"
  run_scan "$scan" "$root"
  if [ "$out_status" -ne 0 ] || ! has_line 'OK: baseline no-secret scan clean.' || has_text 'FAIL:'; then
    fail_case clean-tree "the clean tree did not pass (exit $out_status)"
  fi

  # 2. Every planted match fails the scan under its own label, and removing it passes again, which
  # leaves the clean tree for the next one.
  while IFS=$'\t' read -r label name line; do
    mkdir -p "$root/deep/er"
    printf '%s\n' "$line" > "$root/deep/er/planted.txt"
    git -C "$root" add deep/er/planted.txt
    run_scan "$scan" "$root"
    if [ "$out_status" -ne 1 ]; then
      fail_case "$name" "a planted $label exited $out_status, not 1"
    elif [ "$(count_lines '^FAIL:')" != 1 ] || ! has_line "FAIL: matched $label pattern in tracked files:"; then
      fail_case "$name" "a planted $label was not the one FAIL line, under its own label"
    elif ! has_text 'deep/er/planted.txt:1:' || has_text 'OK:'; then
      fail_case "$name" "a planted $label was reported without naming the planted file"
    fi
    git -C "$root" rm -q -f deep/er/planted.txt
    run_scan "$scan" "$root"
    if [ "$out_status" -ne 0 ] || ! has_line 'OK: baseline no-secret scan clean.'; then
      fail_case "$name-removed" "the tree did not pass once the planted $label was removed (exit $out_status)"
    fi
  done < "$tmp/planted.tsv"

  # 3a. Outside any repository every pattern's search fails, and so does the scan.
  run_scan "$scan" "$tmp/not-a-repo"
  if [ "$out_status" -ne 2 ] || has_text 'OK:'; then
    fail_case outside-a-repository "the scan exited $out_status outside a repository, not 2"
  elif ! has_text 'not a git repository'; then
    fail_case outside-a-repository "git's own error was not shown"
  else
    while IFS=$'\t' read -r label name line; do
      if ! has_line "FAIL: git grep exited 128 searching for the $label pattern; the tracked files were not scanned for it (git's error is above)."; then
        fail_case outside-a-repository "no FAIL line names the $label pattern and git's status"
        break
      fi
    done < "$tmp/planted.tsv"
  fi

  # 3b. One pattern's search fails with a usage error; the other patterns are still searched.
  rm -rf "$root"
  new_repo "$root"
  awk -F '\t' '$1 == "AWS access key id" { print $3 }' "$tmp/planted.tsv" > "$root/planted.txt"
  git -C "$root" add planted.txt
  run_scan "$scan" "$root" "PATH=$tmp/shim:$PATH"
  if [ "$out_status" -ne 2 ] || has_text 'OK:'; then
    fail_case usage-error-on-one-pattern "a usage error on one pattern exited $out_status, not 2"
  elif ! has_text 'usage: git grep'; then
    fail_case usage-error-on-one-pattern "git's usage text was not shown"
  elif ! has_line "FAIL: git grep exited 129 searching for the private key block pattern; the tracked files were not scanned for it (git's error is above)."; then
    fail_case usage-error-on-one-pattern "no FAIL line names the private key block pattern and status 129"
  elif ! has_line 'FAIL: matched AWS access key id pattern in tracked files:'; then
    fail_case usage-error-on-one-pattern "the error stopped the scan before a planted AWS access key id was reported"
  fi

  rm -rf "$root"
  [ "$suite_failures" -eq 0 ]
}

if ! suite "$SCRIPT"; then
  echo
  echo "FAIL: check-no-secrets.sh does not hold to its fixtures (above)."
  exit 1
fi
cases="$(wc -l < "$tmp/planted.tsv" | tr -d ' ')"

# 4. Copies of the scan with a defect put back. Each must fail the named check.
mkdir -p "$tmp/mutants"

# The scan() body as shipped before issue #524, and the same body with `-e`: an error is
# read as no match and its message is discarded.
cat > "$tmp/scan-as-shipped.body" <<'EOF'
scan() {
  local pattern="$1"
  local label="$2"
  if git grep -nE "$pattern" >/dev/null 2>&1; then
    echo "FAIL: matched $label pattern in tracked files:"
    git grep -nE "$pattern" || true
    violations=$((violations + 1))
  fi
}
EOF
sed 's/git grep -nE "/git grep -nE -e "/' "$tmp/scan-as-shipped.body" > "$tmp/errors-read-as-no-match.body"

# `replace_scan BODY DEST` writes the scan with its scan() function replaced by BODY.
replace_scan() {
  awk -v body="$1" '
    /^scan\(\) \{$/ { while ((getline text < body) > 0) print text; skipping = 1; replaced++; next }
    skipping && /^\}$/ { skipping = 0; next }
    !skipping { print }
    END { if (replaced != 1) exit 1 }
  ' "$SCRIPT" > "$2"
}

mutant_failures=0
# `mutant NAME CHECK`: the copy at $tmp/mutants/NAME.sh must fail CHECK.
mutant() {
  local name="$1" check="$2" copy="$tmp/mutants/$1.sh"
  if cmp -s "$SCRIPT" "$copy"; then
    echo "FAIL: the $name edit no longer applies to check-no-secrets.sh; update it with the scan"
    mutant_failures=$((mutant_failures + 1))
    return
  fi
  if suite "$copy" > "$tmp/mutants/$name.log" 2>&1; then
    echo "FAIL: the fixtures passed a copy of the scan with the defect '$name' put back"
    mutant_failures=$((mutant_failures + 1))
  elif ! grep -q "^FAIL: $check: " "$tmp/mutants/$name.log"; then
    echo "FAIL: the copy with the defect '$name' did not fail the $check check"
    sed 's/^/    | /' "$tmp/mutants/$name.log"
    mutant_failures=$((mutant_failures + 1))
  fi
}

# The defect of issue #524: the pattern is an option again.
sed 's/git grep -nE -e "\$pattern"/git grep -nE "$pattern"/' "$SCRIPT" \
  > "$tmp/mutants/pattern-read-as-an-option.sh"
mutant pattern-read-as-an-option private-key-RSA
# git's error discarded, the status still checked.
sed 's/git grep -nE -e "\$pattern")"/git grep -nE -e "$pattern" 2>\/dev\/null)"/' "$SCRIPT" \
  > "$tmp/mutants/git-error-hidden.sh"
mutant git-error-hidden outside-a-repository
# `-e` kept, any failure read as no match. A body that cannot replace scan() leaves the copy equal
# to the scan, which `mutant` reports as an edit that no longer applies.
replace_scan "$tmp/errors-read-as-no-match.body" "$tmp/mutants/errors-read-as-no-match.sh" ||
  cp "$SCRIPT" "$tmp/mutants/errors-read-as-no-match.sh"
mutant errors-read-as-no-match outside-a-repository
# Both defects, as shipped.
replace_scan "$tmp/scan-as-shipped.body" "$tmp/mutants/scan-as-shipped.sh" ||
  cp "$SCRIPT" "$tmp/mutants/scan-as-shipped.sh"
mutant scan-as-shipped private-key-RSA

if [ "$mutant_failures" -ne 0 ]; then
  echo
  echo "FAIL: $mutant_failures defect control(s) above; the fixtures could not fail."
  exit 1
fi

echo "OK: no-secret scan fixtures passed: a clean tree, $cases planted matches each caught and" \
  "cleared, two git-error controls, and four copies with a defect put back, each refused."
