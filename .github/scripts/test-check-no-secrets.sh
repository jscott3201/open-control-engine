#!/usr/bin/env bash
# Gate-behavior fixtures for check-no-secrets.sh (issue #524). A gate that cannot fail is not a
# gate, and this one could not for its private-key pattern: the pattern begins with five dashes,
# so `git grep` read it as an option and exited 129, and the scan took that for a clean tree.
# Every fixture runs in a throwaway repository, never in the real tree, and holds the scan to
# five things:
#
#   1. A clean tree passes. Its tracked files are a copy of the scan and of this file, so neither
#      holds a match and the scan of this repository stays clean (that is why every value below is
#      built at run time from fragments), and one near miss per quantifier of every pattern: the
#      value one character short of what the quantifier requires, which must not match.
#   2. Each pattern catches every planted value: exit 1, reported as the one match, under that
#      pattern's label, naming the planted file. Every alternative of an alternation, and every
#      letter of a one-letter class, is planted on its own. Every pattern has values as short as
#      it accepts; between them they hold both ends of every range and every punctuation member of
#      each character class, where the pattern must read them; and each has more text after it on
#      its line. Removing
#      the planted file passes again. The labels planted here, and the labels of the near misses,
#      must each be exactly the scan's, so a pattern added without both fails.
#   3. A `git grep` that cannot search fails the scan: exit 2, a FAIL line naming the pattern and
#      git's exit status, and git's own error left visible. Outside any repository (git exits 128),
#      under a git that exits 129 with its usage text when handed a pattern that begins with five
#      dashes (the shipped defect reproduced with `-e` in place; a planted match for another
#      pattern must still be reported), and outside any repository with `grep.fallbackToNoIndex`
#      turned on by a config file, by `git -c` and by `GIT_CONFIG_COUNT`, where git would
#      otherwise search the empty directory and call it clean.
#   4. Checks 1 to 3 fail on copies of the scan with a defect put back, each of which must fail a
#      named check: `-e` removed, git's error sent to /dev/null, an error read as no match, the
#      scan as shipped before issue #524, the fallback no longer forced off, and every pattern
#      weakened four ways (a character class narrowed, an end anchor added, a quantifier
#      tightened, each of which misses a planted value, and a quantifier loosened, which matches a
#      near miss). A copy whose edit no longer applies to the scan fails this file, so the controls
#      cannot go stale unnoticed.
#   5. The caller's repository, git configuration and locale do not reach any of it: a git hook
#      exports GIT_DIR and GIT_INDEX_FILE, configuration arrives in files and in the environment
#      (`GIT_CONFIG_COUNT` with `GIT_CONFIG_KEY_<n>`, and `GIT_CONFIG_PARAMETERS` from `git -c`),
#      and git's messages are matched in English. The isolation is itself checked against config
#      injected in the environment.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT="$(pwd)/.github/scripts/check-no-secrets.sh"
FIXTURE="$(pwd)/.github/scripts/test-check-no-secrets.sh"
REAL_GIT="$(command -v git)"

isolate_git() {
  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_COMMON_DIR \
    GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 LC_ALL=C
}
isolate_git

tmp="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT
# Git looks for a repository no higher than the scratch directory, so the outside-a-repository
# controls cannot find one above it.
export GIT_CEILING_DIRECTORIES="$tmp"

# 5. Git reads config injected in the environment (else the next check proves nothing), and
# isolate_git removes it.
git -C "$tmp" -c init.defaultBranch=main init -q probe
inject_config() {
  export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=grep.fallbackToNoIndex GIT_CONFIG_VALUE_0=true \
    GIT_CONFIG_PARAMETERS="'grep.patternType'='fixed'"
}
injected="$( (inject_config; git -C "$tmp/probe" config --get-regexp '^grep\.' || true) | wc -l)"
if [ "$(printf '%s' "$injected" | tr -d ' ')" != 2 ]; then
  echo "BUG: git did not read the config injected in the environment; the isolation check cannot run"
  exit 1
fi
if [ -n "$( (inject_config; isolate_git; git -C "$tmp/probe" config --get-regexp '^grep\.' || true) )" ]; then
  echo "FAIL: isolate_git leaves git config passed in the environment in effect"
  exit 1
fi

# `repeat TEXT N` prints TEXT N times.
repeat() {
  local out="" i=0
  while [ "$i" -lt "$2" ]; do
    out="$out$1"
    i=$((i + 1))
  done
  printf '%s' "$out"
}

dashes="$(repeat - 5)"
after=' (planted)'
row() { printf '%s\t%s\t%s\n' "$1" "$2" "$3"; }
uuid_shape() { printf '%s-%s-%s-%s-%s' "$@"; }

# One line per planted value: the label the scan must report, a case name, and the line planted.
# Each value is assembled from fragments, so this file never contains one whole.
planted() {
  local letter kind
  row 'AWS access key id' aws-access-key-id "aws_access_key_id = AKI""A09AZ09AZ09AZ09AZ$after"
  for kind in RSA EC OPENSSH PGP; do
    row 'private key block' "private-key-$kind" "${dashes}BEGIN $kind PRIVATE KEY${dashes}$after"
  done
  for letter in a b p r; do
    row 'Slack token' "slack-token-xox$letter" "token: xo""x$letter-AZaz09-Za0$after"
  done
  for letter in p o u s r; do
    row 'GitHub token' "github-token-gh$letter" "token: g""h${letter}_$(repeat AZaz09 6)$after"
  done
  row 'API token (sk- prefix)' api-token-sk "api_key = s""k-$(repeat AZaz09_- 2)AZaz$after"
  # Each group's class is read in full, so the two values between them hold every range end in
  # every group.
  row 'UUID (possible agent / namespace identity)' uuid-lower-ends \
    "id: $(uuid_shape 09afAF09 09af 09af 09af 09afAF09afAF)$after"
  row 'UUID (possible agent / namespace identity)' uuid-upper-ends \
    "id: $(uuid_shape AF09af09 AF09 AF09 AF09 AF09af09afAF)$after"
  for kind in Users home; do
    row 'absolute developer home-directory path' "home-path-$kind" "see /""$kind/a/notes.txt"
  done
  row 'absolute developer home-directory path' home-path-range-ends "see /""Users/AZaz09._-/notes.txt"
  # The pattern reads two letters after the underscore and no more, so each end of each class has
  # a value of its own there.
  row 'Aionforge environment variable name' aionforge-variable-ZA "export AIONFORGE""_ZA=1"
  row 'Aionforge environment variable name' aionforge-variable-AZ "export AIONFORGE""_AZ=1"
  row 'Aionforge environment variable name' aionforge-variable-A_ "export AIONFORGE""_A_=1"
}

# One line per near miss, in the same shape: one character short of what one quantifier of the
# pattern requires (or of the five dashes), so none may match.
near_misses() {
  row 'AWS access key id' near-miss-aws-access-key-id "aws_access_key_id = AKI""A09AZ09AZ09AZ09A$after"
  row 'private key block' near-miss-private-key-block "${dashes%-}BEGIN RSA PRIVATE KEY${dashes}$after"
  row 'Slack token' near-miss-slack-token "token: xo""xb-AZaz09-Za$after"
  row 'GitHub token' near-miss-github-token "token: g""hp_$(repeat AZaz09 5)AZaz0$after"
  row 'API token (sk- prefix)' near-miss-api-token-sk "api_key = s""k-$(repeat AZaz09_- 2)AZa$after"
  row 'UUID (possible agent / namespace identity)' near-miss-uuid-first-group \
    "id: $(uuid_shape 09afAF0 09af 09af 09af 09afAF09afAF)$after"
  row 'UUID (possible agent / namespace identity)' near-miss-uuid-second-group \
    "id: $(uuid_shape 09afAF09 09a 09af 09af 09afAF09afAF)$after"
  row 'UUID (possible agent / namespace identity)' near-miss-uuid-third-group \
    "id: $(uuid_shape 09afAF09 09af 09a 09af 09afAF09afAF)$after"
  row 'UUID (possible agent / namespace identity)' near-miss-uuid-fourth-group \
    "id: $(uuid_shape 09afAF09 09af 09af 09a 09afAF09afAF)$after"
  row 'UUID (possible agent / namespace identity)' near-miss-uuid-last-group \
    "id: $(uuid_shape 09afAF09 09af 09af 09af 09afAF09afA)$after"
  row 'absolute developer home-directory path' near-miss-home-path "see /""Users//notes.txt"
  row 'Aionforge environment variable name' near-miss-aionforge-variable "export AIONFORGE""_A=1"
}
planted > "$tmp/planted.tsv"
near_misses > "$tmp/near-misses.tsv"

# A repository whose tracked files are a copy of the scan, of this file and the near misses: the
# clean tree.
new_repo() {
  local root="$1" label name line
  mkdir -p "$root/.github/scripts" "$root/near-miss"
  cp "$SCRIPT" "$FIXTURE" "$root/.github/scripts/"
  while IFS=$'\t' read -r label name line; do
    printf '%s\n' "$line" > "$root/near-miss/$name.txt"
  done < "$tmp/near-misses.tsv"
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

printf '[grep]\n\tfallbackToNoIndex = true\n' > "$tmp/fallback.gitconfig"
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
# Every match the last scan reported, as `label<TAB>path`, sorted.
reported() {
  printf '%s\n' "$out" | awk '
    /^FAIL: matched .* pattern in tracked files:$/ {
      label = $0
      sub(/^FAIL: matched /, "", label)
      sub(/ pattern in tracked files:$/, "", label)
      next
    }
    /^FAIL: / || /^$/ { label = ""; next }
    label != "" { print label "\t" substr($0, 1, index($0, ":") - 1) }
  ' | sort
}

# Record a failed expectation of the case being run, with the scan's output.
fail_case() {
  echo "FAIL: $1: $2"
  printf '%s\n' "$out" | sed 's/^/    | /'
  suite_failures=$((suite_failures + 1))
}

# A defect copy runs only the check it must fail (`suite_only`); the scan itself runs every check.
suite_only=""
wants() { [ -z "$suite_only" ] || [ "$suite_only" = "$1" ]; }

# Checks 1 to 3 against one copy of the scan. Prints a FAIL line per failed expectation and
# returns non-zero if there was one. It never relies on errexit: it runs as an `if` condition.
suite() {
  local scan="$1"
  local root label name line labels form
  suite_failures=0
  root="$tmp/run"

  # The labels planted here, and those of the near misses, are exactly the scan's, each scan line
  # in the one shape read here.
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
  for table in planted near-misses; do
    if [ "$(printf '%s\n' "$labels" | sort -u)" != "$(cut -f1 "$tmp/$table.tsv" | sort -u)" ]; then
      out="$(diff <(cut -f1 "$tmp/$table.tsv" | sort -u) <(printf '%s\n' "$labels" | sort -u) || true)"
      fail_case labels "the labels of the $table here (<) are not the scan's (>)"
    fi
  done

  rm -rf "$root"
  new_repo "$root"

  # 1. The clean tree passes, and no near miss in it matches.
  case "$suite_only" in
    '' | clean-tree | near-miss-*)
      run_scan "$scan" "$root"
      if [ "$out_status" -ne 0 ] || ! has_line 'OK: baseline no-secret scan clean.' || has_text 'FAIL:'; then
        fail_case clean-tree "the clean tree did not pass (exit $out_status)"
      fi
      while IFS=$'\t' read -r label name line; do
        if has_text "near-miss/$name.txt:"; then
          fail_case "$name" "a near miss of the $label pattern matched it"
        fi
      done < "$tmp/near-misses.tsv"
      ;;
  esac

  # 2. Every planted value is the one match, under its own label, and removing it passes again,
  # which leaves the clean tree for the next one.
  while IFS=$'\t' read -r label name line; do
    wants "$name" || wants "$name-removed" || continue
    mkdir -p "$root/deep/er"
    printf '%s\n' "$line" > "$root/deep/er/planted.txt"
    git -C "$root" add deep/er/planted.txt
    run_scan "$scan" "$root"
    if [ "$out_status" -ne 1 ]; then
      fail_case "$name" "a planted $label exited $out_status, not 1"
    elif [ "$(count_lines '^FAIL:')" != 1 ] || [ "$(reported)" != "$(printf '%s\t%s' "$label" deep/er/planted.txt)" ]; then
      fail_case "$name" "a planted $label was not the one match, under its own label, in the planted file"
    elif has_text 'OK:'; then
      fail_case "$name" "a planted $label was reported beside the OK line"
    fi
    git -C "$root" rm -q -f deep/er/planted.txt
    run_scan "$scan" "$root"
    if [ "$out_status" -ne 0 ] || ! has_line 'OK: baseline no-secret scan clean.'; then
      fail_case "$name-removed" "the tree did not pass once the planted $label was removed (exit $out_status)"
    fi
  done < "$tmp/planted.tsv"

  # 3a. Outside any repository every pattern's search fails, and so does the scan.
  if wants outside-a-repository; then
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
  fi

  # 3b. One pattern's search fails with a usage error; the other patterns are still searched.
  if wants usage-error-on-one-pattern; then
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
    git -C "$root" rm -q -f planted.txt
  fi

  # 3c. Outside any repository with the no-index fallback turned on, in each way git takes it.
  if wants no-index-fallback-outside-a-repository; then
    for form in config-file git-c environment; do
      case "$form" in
        config-file) run_scan "$scan" "$tmp/not-a-repo" "GIT_CONFIG_GLOBAL=$tmp/fallback.gitconfig" ;;
        git-c) run_scan "$scan" "$tmp/not-a-repo" "GIT_CONFIG_PARAMETERS='grep.fallbackToNoIndex'='true'" ;;
        environment)
          run_scan "$scan" "$tmp/not-a-repo" GIT_CONFIG_COUNT=1 \
            GIT_CONFIG_KEY_0=grep.fallbackToNoIndex GIT_CONFIG_VALUE_0=true
          ;;
      esac
      if [ "$out_status" -ne 2 ] || has_text 'OK:' || ! has_text 'not a git repository'; then
        fail_case no-index-fallback-outside-a-repository \
          "with grep.fallbackToNoIndex set by $form the scan exited $out_status outside a repository, not 2"
      fi
    done
  fi

  rm -rf "$root"
  [ "$suite_failures" -eq 0 ]
}

if ! suite "$SCRIPT"; then
  echo
  echo "FAIL: check-no-secrets.sh does not hold to its fixtures (above)."
  exit 1
fi
values="$(wc -l < "$tmp/planted.tsv" | tr -d ' ')"
misses="$(wc -l < "$tmp/near-misses.tsv" | tr -d ' ')"

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

mutants=0
mutant_failures=0
# `mutant NAME CHECK`: the copy at $tmp/mutants/NAME.sh must fail CHECK.
mutant() {
  local name="$1" check="$2" copy="$tmp/mutants/$1.sh"
  mutants=$((mutants + 1))
  if cmp -s "$SCRIPT" "$copy"; then
    echo "FAIL: the $name edit no longer applies to check-no-secrets.sh; update it with the scan"
    mutant_failures=$((mutant_failures + 1))
    return
  fi
  suite_only="$check"
  if suite "$copy" > "$tmp/mutants/$name.log" 2>&1; then
    echo "FAIL: the $check check passed a copy of the scan with the defect '$name' put back"
    mutant_failures=$((mutant_failures + 1))
  elif ! grep -q "^FAIL: $check: " "$tmp/mutants/$name.log"; then
    echo "FAIL: the copy with the defect '$name' did not fail the $check check"
    sed 's/^/    | /' "$tmp/mutants/$name.log"
    mutant_failures=$((mutant_failures + 1))
  fi
  suite_only=""
}

# The defect of issue #524: the pattern is an option again.
sed 's/grep -nE -e "\$pattern"/grep -nE "$pattern"/' "$SCRIPT" \
  > "$tmp/mutants/pattern-read-as-an-option.sh"
mutant pattern-read-as-an-option private-key-RSA
# git's error discarded, the status still checked.
sed 's/grep -nE -e "\$pattern")"/grep -nE -e "$pattern" 2>\/dev\/null)"/' "$SCRIPT" \
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
# The no-index fallback left to the caller's configuration.
sed 's/git -c grep\.fallbackToNoIndex=false grep/git grep/' "$SCRIPT" \
  > "$tmp/mutants/no-index-fallback-honoured.sh"
mutant no-index-fallback-honoured no-index-fallback-outside-a-repository

# `weaken NAME CHECK LABEL FROM TO`: a copy of the scan whose pattern for LABEL has its first FROM
# replaced by TO (literal text, not a regex) must fail CHECK.
weaken() {
  awk -F"'" -v OFS="'" -v label="$3" -v from="$4" -v to="$5" '
    /^scan / && $4 == label && !done {
      i = index($2, from)
      if (i) { $2 = substr($2, 1, i - 1) to substr($2, i + length(from)); done = 1 }
    }
    { print }
    END { exit !done }
  ' "$SCRIPT" > "$tmp/mutants/$1.sh" || cp "$SCRIPT" "$tmp/mutants/$1.sh"
  mutant "$1" "$2"
}

aws='AWS access key id'
key='private key block'
slack='Slack token'
github='GitHub token'
sk='API token (sk- prefix)'
uuid='UUID (possible agent / namespace identity)'
home='absolute developer home-directory path'
aionforge='Aionforge environment variable name'

# A narrowed class or alternation misses the planted value that holds what it dropped.
weaken narrowed-aws-class aws-access-key-id "$aws" '[0-9A-Z]' '[0-9A-Y]'
weaken narrowed-private-key-alternation private-key-PGP "$key" '|PGP)' ')'
weaken narrowed-slack-class slack-token-xoxa "$slack" '0-9-]' '0-9]'
weaken narrowed-github-class github-token-ghp "$github" '[A-Za-z0-9]' '[A-Z0-9]'
weaken narrowed-sk-class api-token-sk "$sk" '9_-]' '9]'
weaken narrowed-uuid-class uuid-upper-ends "$uuid" '[0-9a-fA-F]{4}' '[0-9a-f]{4}'
weaken narrowed-home-path-class home-path-range-ends "$home" '._-]' '_-]'
weaken narrowed-aionforge-class aionforge-variable-A_ "$aionforge" '[A-Z_]' '[A-Z]'
# An end anchor misses every planted value, because text follows each one.
weaken end-anchored-aws aws-access-key-id "$aws" '{16}' '{16}$'
weaken end-anchored-private-key private-key-RSA "$key" "KEY$dashes" "KEY$dashes\$"
weaken end-anchored-slack slack-token-xoxa "$slack" '{10,}' '{10,}$'
weaken end-anchored-github github-token-ghp "$github" '{36,}' '{36,}$'
weaken end-anchored-sk api-token-sk "$sk" '{20,}' '{20,}$'
weaken end-anchored-uuid uuid-lower-ends "$uuid" '{12}' '{12}$'
weaken end-anchored-home-path home-path-Users "$home" ']+/' ']+/$'
weaken end-anchored-aionforge aionforge-variable-ZA "$aionforge" '_]+' '_]+$'
# A tightened quantifier misses the planted value, which is as short as the pattern accepts.
weaken tightened-aws aws-access-key-id "$aws" '{16}' '{17}'
weaken tightened-slack slack-token-xoxa "$slack" '{10,}' '{11,}'
weaken tightened-github github-token-ghp "$github" '{36,}' '{37,}'
weaken tightened-sk api-token-sk "$sk" '{20,}' '{21,}'
weaken tightened-uuid uuid-lower-ends "$uuid" '{12}' '{13}'
weaken tightened-home-path home-path-Users "$home" ']+/' ']{2,}/'
weaken tightened-aionforge aionforge-variable-ZA "$aionforge" '_]+' '_]{2,}'
# A loosened quantifier matches the near miss one character short of it.
weaken loosened-aws near-miss-aws-access-key-id "$aws" '{16}' '{15}'
weaken loosened-private-key near-miss-private-key-block "$key" "${dashes}BEGIN" "${dashes%-}BEGIN"
weaken loosened-slack near-miss-slack-token "$slack" '{10,}' '{9,}'
weaken loosened-github near-miss-github-token "$github" '{36,}' '{35,}'
weaken loosened-sk near-miss-api-token-sk "$sk" '{20,}' '{19,}'
weaken loosened-uuid-first-group near-miss-uuid-first-group "$uuid" ']{8}' ']{7}'
weaken loosened-uuid-last-group near-miss-uuid-last-group "$uuid" '{12}' '{11}'
weaken loosened-home-path near-miss-home-path "$home" ']+/' ']*/'
weaken loosened-aionforge near-miss-aionforge-variable "$aionforge" '_]+' '_]*'

if [ "$mutant_failures" -ne 0 ]; then
  echo
  echo "FAIL: $mutant_failures defect control(s) above; the fixtures could not fail."
  exit 1
fi

echo "OK: no-secret scan fixtures passed: a clean tree with $misses near misses, $values planted values" \
  "each caught and cleared, three git-error controls, and $mutants copies with a defect put back," \
  "each refused by its named check."
