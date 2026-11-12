#!/usr/bin/env bash
# Gate-behavior fixtures for check-no-secrets.sh (issues #524 and #607). A gate that cannot fail
# is not a gate, and this one could not for its private-key pattern: the pattern began with five
# dashes, so `git grep` read it as an option and exited 129, and the scan took that for a clean
# tree. Every fixture runs in a throwaway repository, never in the real tree, and holds the scan to
# five things:
#
#   1. A clean tree passes. Its tracked files are a copy of the scan and of this file, so neither
#      holds a match and the scan of this repository stays clean (that is why every value below is
#      built at run time from fragments), and the near misses: one character short of every
#      quantifier of every pattern, each pattern in the wrong case, and the public-key and
#      certificate headers beside the private-key ones. None may match.
#   2. Each pattern catches every planted value: the values of one pattern, planted together, exit
#      1 as one FAIL block under that pattern's label, each reported in its own file, and nothing
#      else reported; removing them passes again. Between them the values of a pattern hold every
#      member of each character class where the pattern must read it (every alternative and every
#      letter of a one-letter class too), each value as short as its pattern accepts, so a class
#      narrowed at an end or with a hole in the middle misses one. Every value has text after it on
#      its line; most have text before it; one per pattern sits at the start of its line and one
#      follows a word character. The labels planted here, and the labels of the near misses, must
#      each be exactly the scan's, so a pattern added without both fails.
#   3. A `git grep` that cannot search fails the scan: exit 2, a FAIL line naming the pattern and
#      git's exit status, and git's own error left visible. Outside any repository (git exits 128),
#      under a git that exits 129 with its usage text when handed a pattern that begins with five
#      dashes (the defect of issue #524 reproduced with `-e` in place; a planted match for another
#      pattern must still be reported), and outside any repository with `grep.fallbackToNoIndex`
#      turned on by a config file, by `git -c` and by `GIT_CONFIG_COUNT`, where git would
#      otherwise search the empty directory and call it clean.
#   4. Checks 1 to 3 fail on copies of the scan with a defect put back, each of which must fail a
#      named check: `-e` removed, git's error sent to /dev/null, an error read as no match, the
#      scan as shipped before issue #524, the fallback no longer forced off, `-i`, and every pattern
#      weakened: a class narrowed at an end and with a hole in the middle, an alternative dropped,
#      a start anchor, a required non-word character before it, an end anchor, a quantifier
#      tightened (each of which misses a planted value), and a quantifier loosened (which matches a
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

upper=ABCDEFGHIJKLMNOPQRSTUVWXYZ
lower=abcdefghijklmnopqrstuvwxyz
digits=0123456789
hex="${digits}abcdefABCDEF"
dashes="$(repeat - 5)"
after=' (planted)'

# `window MEMBERS WIDTH K` prints the K-th run (from 1) of WIDTH characters of MEMBERS, wrapping
# round to the start, and `windows MEMBERS WIDTH` how many runs it takes to hold every member. A
# class read WIDTH characters at a time therefore meets each of its members in one of the runs.
window() {
  local members="$1$1" offset=$(( ($3 - 1) * $2 % ${#1} ))
  printf '%s' "${members:offset:$2}"
}
windows() { echo $(( (${#1} + $2 - 1) / $2 )); }

row() { printf '%s\t%s\t%s\n' "$1" "$2" "$3"; }
uuid_shape() { printf '%s-%s-%s-%s-%s' "$@"; }
# `slug LABEL` is the label as a case name: lower case, a hyphen between words.
slug() { printf '%s' "$1" | tr 'A-Z' 'a-z' | tr -cs 'a-z0-9' '-' | sed 's/-$//'; }

aws='AWS access key id'
key='private key block'
slack='Slack token'
github='GitHub token'
sk='API token (sk- prefix)'
uuid='UUID (possible agent / namespace identity)'
home='absolute developer home-directory path'
aionforge='Aionforge environment variable name'

# One line per planted value: the label the scan must report, a case name, and the line planted.
# Each value is assembled from fragments, so this file never contains one whole.
planted() {
  local k n letter word members
  # [0-9A-Z]{16}: three runs of sixteen hold all 36 members.
  members="$digits$upper"
  n="$(windows "$members" 16)"
  for k in $(seq "$n"); do
    row "$aws" "aws-access-key-id-members-$k" "aws_access_key_id = AKI""A$(window "$members" 16 "$k")$after"
  done
  row "$aws" aws-access-key-id-at-line-start "AKI""A$(window "$members" 16 1)$after"
  row "$aws" aws-access-key-id-after-a-word-character "keyAKI""A$(window "$members" 16 1)$after"

  # Every PEM private-key header form, two upper-case words, a word holding every member of the
  # class, and the armored OpenPGP block.
  row "$key" private-key-generic "key: ${dashes}BEGIN PRIVATE KEY${dashes}$after"
  for word in RSA EC DSA OPENSSH ENCRYPTED; do
    row "$key" "private-key-$word" "key: ${dashes}BEGIN $word PRIVATE KEY${dashes}$after"
  done
  row "$key" private-key-two-words "key: ${dashes}BEGIN SSH2 ENCRYPTED PRIVATE KEY${dashes}$after"
  row "$key" private-key-members "key: ${dashes}BEGIN $digits$upper PRIVATE KEY${dashes}$after"
  row "$key" private-key-PGP-block "key: ${dashes}BEGIN PGP PRIVATE KEY BLOCK${dashes}$after"
  row "$key" private-key-at-line-start "${dashes}BEGIN RSA PRIVATE KEY${dashes}$after"
  row "$key" private-key-after-a-word-character "pem${dashes}BEGIN RSA PRIVATE KEY${dashes}$after"

  # [A-Za-z0-9-]{10,} reads ten characters: seven runs, the prefix letter cycling through [abpr].
  members="$upper$lower$digits-"
  n="$(windows "$members" 10)"
  for k in $(seq "$n"); do
    letter="$(window abpr 1 "$k")"
    row "$slack" "slack-token-members-$k" "token: xo""x$letter-$(window "$members" 10 "$k")$after"
  done
  row "$slack" slack-token-at-line-start "xo""xb-$(window "$members" 10 1)$after"
  row "$slack" slack-token-after-a-word-character "slackxo""xb-$(window "$members" 10 1)$after"

  # [A-Za-z0-9]{36,}: two runs hold every member; five values, one per prefix letter.
  members="$upper$lower$digits"
  for k in 1 2 3 4 5; do
    letter="$(window pousr 1 "$k")"
    row "$github" "github-token-members-$k" "token: g""h${letter}_$(window "$members" 36 "$k")$after"
  done
  row "$github" github-token-at-line-start "g""hp_$(window "$members" 36 1)$after"
  row "$github" github-token-after-a-word-character "tokeng""hp_$(window "$members" 36 1)$after"

  # [A-Za-z0-9_-]{20,}: four runs of twenty.
  members="$upper$lower${digits}_-"
  n="$(windows "$members" 20)"
  for k in $(seq "$n"); do
    row "$sk" "api-token-members-$k" "api_key = s""k-$(window "$members" 20 "$k")$after"
  done
  row "$sk" api-token-at-line-start "s""k-$(window "$members" 20 1)$after"
  row "$sk" api-token-after-a-word-character "ta""s""k-$(window "$members" 20 1)$after"

  # Each group's class is read in full; the four-character groups take six runs to meet all 22.
  n="$(windows "$hex" 4)"
  for k in $(seq "$n"); do
    row "$uuid" "uuid-members-$k" "id: $(uuid_shape "$(window "$hex" 8 "$k")" "$(window "$hex" 4 "$k")" \
      "$(window "$hex" 4 "$k")" "$(window "$hex" 4 "$k")" "$(window "$hex" 12 "$k")")$after"
  done
  word="$(uuid_shape "$(window "$hex" 8 1)" "$(window "$hex" 4 1)" "$(window "$hex" 4 1)" \
    "$(window "$hex" 4 1)" "$(window "$hex" 12 1)")"
  row "$uuid" uuid-at-line-start "$word$after"
  row "$uuid" uuid-after-a-word-character "uuid_$word$after"

  # The component is read up to the closing slash, so one value holds every member; the shortest
  # component is one character.
  for word in Users home; do
    row "$home" "home-path-$word" "see /""$word/a/notes.txt"
  done
  row "$home" home-path-members "see /""Users/$upper$lower$digits._-/notes.txt"
  row "$home" home-path-at-line-start "/""Users/a/notes.txt"
  row "$home" home-path-after-a-word-character "x/""Users/a/notes.txt"

  # The pattern reads one character of each class after the underscore: 27 values, between them
  # every letter in the first place and every letter and the underscore in the second.
  for k in $(seq 27); do
    row "$aionforge" "aionforge-variable-members-$k" \
      "export AIONFORGE""_$(window "$upper" 1 "$k")$(window "${upper}_" 1 "$k")=1"
  done
  row "$aionforge" aionforge-variable-at-line-start "AIONFORGE""_AA=1"
  row "$aionforge" aionforge-variable-after-a-word-character "xAIONFORGE""_AA=1"
}

# One line per near miss, in the same shape: one character short of what a quantifier of the
# pattern requires (or of the dashes), the pattern in the wrong case, and the headers a private
# key's must not be mistaken for. None may match.
near_misses() {
  local m
  m="$digits$upper"
  row "$aws" near-miss-aws-access-key-id "aws_access_key_id = AKI""A$(window "$m" 15 1)$after"
  row "$aws" near-miss-aws-access-key-id-case "aws_access_key_id = aki""a$(window "$m" 16 1)$after"
  row "$key" near-miss-private-key-dashes-before "key: ${dashes%-}BEGIN RSA PRIVATE KEY${dashes}$after"
  row "$key" near-miss-private-key-dashes-after "key: ${dashes}BEGIN RSA PRIVATE KEY${dashes%-}$after"
  row "$key" near-miss-private-key-double-space "key: ${dashes}BEGIN  PRIVATE KEY${dashes}$after"
  row "$key" near-miss-private-key-lower-case-word "key: ${dashes}BEGIN rsa PRIVATE KEY${dashes}$after"
  row "$key" near-miss-private-key-case "key: ${dashes}begin rsa private key${dashes}$after"
  row "$key" near-miss-public-key "key: ${dashes}BEGIN PUBLIC KEY${dashes}$after"
  row "$key" near-miss-rsa-public-key "key: ${dashes}BEGIN RSA PUBLIC KEY${dashes}$after"
  row "$key" near-miss-pgp-public-key-block "key: ${dashes}BEGIN PGP PUBLIC KEY BLOCK${dashes}$after"
  row "$key" near-miss-certificate "key: ${dashes}BEGIN CERTIFICATE${dashes}$after"
  m="$upper$lower$digits-"
  row "$slack" near-miss-slack-token "token: xo""xb-$(window "$m" 9 1)$after"
  row "$slack" near-miss-slack-token-case "token: XO""XB-$(window "$m" 10 1)$after"
  m="$upper$lower$digits"
  row "$github" near-miss-github-token "token: g""hp_$(window "$m" 35 1)$after"
  row "$github" near-miss-github-token-case "token: G""HP_$(window "$m" 36 1)$after"
  m="$upper$lower${digits}_-"
  row "$sk" near-miss-api-token-sk "api_key = s""k-$(window "$m" 19 1)$after"
  row "$sk" near-miss-api-token-case "api_key = S""K-$(window "$m" 20 1)$after"
  row "$uuid" near-miss-uuid-first-group \
    "id: $(uuid_shape "$(window "$hex" 7 1)" 4567 89ab cdef 0123456789ab)$after"
  row "$uuid" near-miss-uuid-second-group \
    "id: $(uuid_shape 01234567 456 89ab cdef 0123456789ab)$after"
  row "$uuid" near-miss-uuid-third-group \
    "id: $(uuid_shape 01234567 4567 89a cdef 0123456789ab)$after"
  row "$uuid" near-miss-uuid-fourth-group \
    "id: $(uuid_shape 01234567 4567 89ab cde 0123456789ab)$after"
  row "$uuid" near-miss-uuid-last-group \
    "id: $(uuid_shape 01234567 4567 89ab cdef "$(window "$hex" 11 1)")$after"
  row "$home" near-miss-home-path "see /""Users//notes.txt"
  row "$home" near-miss-home-path-case "see /""USERS/a/notes.txt"
  row "$aionforge" near-miss-aionforge-variable "export AIONFORGE""_A=1"
  row "$aionforge" near-miss-aionforge-variable-case "export aionforge""_AA=1"
}
planted > "$tmp/planted.tsv"
near_misses > "$tmp/near-misses.tsv"
# One line per label, in the scan's order: the label and its slug.
cut -f1 "$tmp/planted.tsv" | awk '!seen[$0]++' | while IFS= read -r label; do
  printf '%s\t%s\n' "$label" "$(slug "$label")"
done > "$tmp/labels.tsv"

# The clean tree: a copy of the scan, of this file and the near misses, built once and copied for
# each run.
mkdir -p "$tmp/clean/.github/scripts" "$tmp/clean/near-miss"
cp "$SCRIPT" "$FIXTURE" "$tmp/clean/.github/scripts/"
while IFS=$'\t' read -r label name line; do
  printf '%s\n' "$line" > "$tmp/clean/near-miss/$name.txt"
done < "$tmp/near-misses.tsv"
git -C "$tmp/clean" -c init.defaultBranch=main init -q
git -C "$tmp/clean" add .
new_repo() { rm -rf "$1" && cp -R "$tmp/clean" "$1"; }

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

# A defect copy runs only the check it must fail (`suite_only`), and so only the run of the label
# that check belongs to (`only_label`); the scan itself runs every check.
suite_only=""
only_label=""
wants() { [ -z "$suite_only" ] || [ "$suite_only" = "$1" ]; }
wants_label() { [ -z "$suite_only" ] || [ "$only_label" = "$1" ]; }
label_of_check() {
  awk -F '\t' -v n="$1" '$2 == n { print $1; exit }' "$tmp/planted.tsv"
  awk -F '\t' -v n="$1" '$2 == n || $2 "-removed" == n { print $1; exit }' "$tmp/labels.tsv"
}

# Checks 1 to 3 against one copy of the scan. Prints a FAIL line per failed expectation and
# returns non-zero if there was one. It never relies on errexit: it runs as an `if` condition.
suite() {
  local scan="$1"
  local root label name line labels form slug_of want got
  suite_failures=0
  root="$tmp/run"
  only_label=""
  [ -z "$suite_only" ] || only_label="$(label_of_check "$suite_only" | head -n 1)"

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

  # 2. Each label's values, planted together, are one FAIL block under that label, each reported
  # in its own file and nothing else reported; removing them passes again.
  while IFS=$'\t' read -r label slug_of <&3; do
    wants_label "$label" || continue
    mkdir -p "$root/deep/er"
    want="$(awk -F '\t' -v l="$label" '$1 == l { print l "\tdeep/er/" $2 ".txt" }' "$tmp/planted.tsv" | sort)"
    while IFS=$'\t' read -r _ name line; do
      printf '%s\n' "$line" > "$root/deep/er/$name.txt"
    done < <(awk -F '\t' -v l="$label" '$1 == l' "$tmp/planted.tsv")
    git -C "$root" add deep
    run_scan "$scan" "$root"
    got="$(reported)"
    if [ "$out_status" -ne 1 ] || [ "$(count_lines '^FAIL:')" != 1 ] ||
      ! has_line "FAIL: matched $label pattern in tracked files:" || has_text 'OK:'; then
      fail_case "$slug_of" "the planted $label values were not one FAIL block under that label (exit $out_status)"
    fi
    while IFS=$'\t' read -r _ name; do
      fail_case "$(basename "$name" .txt)" "a planted $label was not reported under its own label"
    done < <(comm -23 <(printf '%s\n' "$want") <(printf '%s\n' "$got"))
    while IFS= read -r line; do
      [ -n "$line" ] && fail_case "$slug_of" "reported beside the planted $label values: $line"
    done < <(comm -13 <(printf '%s\n' "$want") <(printf '%s\n' "$got"))
    git -C "$root" rm -q -r -f deep
    run_scan "$scan" "$root"
    if [ "$out_status" -ne 0 ] || ! has_line 'OK: baseline no-secret scan clean.'; then
      fail_case "$slug_of-removed" "the tree did not pass once the planted $label values were removed (exit $out_status)"
    fi
  done 3< "$tmp/labels.tsv"

  # 3a. Outside any repository every pattern's search fails, and so does the scan.
  if wants outside-a-repository; then
    run_scan "$scan" "$tmp/not-a-repo"
    if [ "$out_status" -ne 2 ] || has_text 'OK:'; then
      fail_case outside-a-repository "the scan exited $out_status outside a repository, not 2"
    elif ! has_text 'not a git repository'; then
      fail_case outside-a-repository "git's own error was not shown"
    else
      while IFS=$'\t' read -r label slug_of; do
        if ! has_line "FAIL: git grep exited 128 searching for the $label pattern; the tracked files were not scanned for it (git's error is above)."; then
          fail_case outside-a-repository "no FAIL line names the $label pattern and git's status"
          break
        fi
      done < "$tmp/labels.tsv"
    fi
  fi

  # 3b. One pattern's search fails with a usage error; the other patterns are still searched.
  if wants usage-error-on-one-pattern; then
    awk -F '\t' '$1 == "AWS access key id" { print $3; exit }' "$tmp/planted.tsv" > "$root/planted.txt"
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
# Case ignored: every pattern also matches its near miss in the wrong case.
sed 's/grep -nE -e "\$pattern"/grep -inE -e "$pattern"/' "$SCRIPT" > "$tmp/mutants/case-ignored.sh"
mutant case-ignored near-miss-aws-access-key-id-case

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
# A character before the pattern that is not a word character, or the start of the line.
nonword='(^|[^A-Za-z0-9_])'

# The AWS access key id: a hole at each range, an end, both anchors, a required non-word
# character, and the quantifier both ways.
weaken narrowed-aws-class aws-access-key-id-members-3 "$aws" '[0-9A-Z]' '[0-9A-Y]'
weaken aws-letter-hole aws-access-key-id-members-2 "$aws" '[0-9A-Z]' '[0-9A-LN-Z]'
weaken aws-digit-hole aws-access-key-id-members-1 "$aws" '[0-9A-Z]' '[0-46-9A-Z]'
weaken start-anchored-aws aws-access-key-id-members-1 "$aws" 'AKIA' '^AKIA'
weaken word-boundary-aws aws-access-key-id-after-a-word-character "$aws" 'AKIA' "${nonword}AKIA"
weaken end-anchored-aws aws-access-key-id-members-1 "$aws" '{16}' '{16}$'
weaken tightened-aws aws-access-key-id-members-1 "$aws" '{16}' '{17}'
weaken loosened-aws near-miss-aws-access-key-id "$aws" '{16}' '{15}'
# The private-key header.
weaken narrowed-private-key-class private-key-members "$key" '[A-Z0-9]' '[A-Y0-9]'
weaken private-key-letter-hole private-key-members "$key" '[A-Z0-9]' '[A-LN-Z0-9]'
weaken private-key-block-dropped private-key-PGP-block "$key" '( BLOCK)?' ''
weaken private-key-label-required private-key-generic "$key" '+ )*' '+ )+'
weaken private-key-one-word-at-most private-key-two-words "$key" '+ )*' '+ )?'
weaken start-anchored-private-key private-key-generic "$key" "${dashes}BEGIN" "^${dashes}BEGIN"
weaken word-boundary-private-key private-key-after-a-word-character "$key" "${dashes}BEGIN" \
  "${nonword}${dashes}BEGIN"
weaken end-anchored-private-key private-key-generic "$key" "BLOCK)?$dashes" "BLOCK)?$dashes\$"
weaken loosened-private-key-dashes-before near-miss-private-key-dashes-before "$key" \
  "${dashes}BEGIN" "${dashes%-}BEGIN"
weaken loosened-private-key-dashes-after near-miss-private-key-dashes-after "$key" \
  "BLOCK)?$dashes" "BLOCK)?${dashes%-}"
weaken loosened-private-key-word near-miss-private-key-double-space "$key" ']+ )' ']* )'
# The Slack token.
weaken narrowed-slack-class slack-token-members-7 "$slack" '0-9-]' '0-9]'
weaken slack-letter-hole slack-token-members-4 "$slack" '[A-Za-z0-9-]' '[A-Za-ln-z0-9-]'
weaken narrowed-slack-prefix slack-token-members-4 "$slack" '[abpr]' '[abp]'
weaken start-anchored-slack slack-token-members-1 "$slack" 'xox' '^xox'
weaken word-boundary-slack slack-token-after-a-word-character "$slack" 'xox' "${nonword}xox"
weaken end-anchored-slack slack-token-members-1 "$slack" '{10,}' '{10,}$'
weaken tightened-slack slack-token-members-1 "$slack" '{10,}' '{11,}'
weaken loosened-slack near-miss-slack-token "$slack" '{10,}' '{9,}'
# The GitHub token.
weaken narrowed-github-class github-token-members-1 "$github" '[A-Za-z0-9]' '[A-Z0-9]'
weaken github-letter-hole github-token-members-2 "$github" '[A-Za-z0-9]' '[A-Za-pr-z0-9]'
weaken narrowed-github-prefix github-token-members-5 "$github" '[pousr]' '[pous]'
weaken start-anchored-github github-token-members-1 "$github" 'gh[' '^gh['
weaken word-boundary-github github-token-after-a-word-character "$github" 'gh[' "${nonword}gh["
weaken end-anchored-github github-token-members-1 "$github" '{36,}' '{36,}$'
weaken tightened-github github-token-members-1 "$github" '{36,}' '{37,}'
weaken loosened-github near-miss-github-token "$github" '{36,}' '{35,}'
# The sk- token.
weaken narrowed-sk-class api-token-members-4 "$sk" '9_-]' '9]'
weaken sk-letter-hole api-token-members-2 "$sk" '[A-Za-z0-9_-]' '[A-Za-eg-z0-9_-]'
weaken start-anchored-sk api-token-members-1 "$sk" 'sk-' '^sk-'
weaken word-boundary-sk api-token-after-a-word-character "$sk" 'sk-' "${nonword}sk-"
weaken end-anchored-sk api-token-members-1 "$sk" '{20,}' '{20,}$'
weaken tightened-sk api-token-members-1 "$sk" '{20,}' '{21,}'
weaken loosened-sk near-miss-api-token-sk "$sk" '{20,}' '{19,}'
# The UUID.
weaken narrowed-uuid-class uuid-members-5 "$uuid" '[0-9a-fA-F]{4}' '[0-9a-f]{4}'
weaken uuid-letter-hole uuid-members-4 "$uuid" '[0-9a-fA-F]{4}' '[0-9abd-fA-F]{4}'
weaken start-anchored-uuid uuid-members-1 "$uuid" '[0-9a-fA-F]{8}' '^[0-9a-fA-F]{8}'
weaken word-boundary-uuid uuid-after-a-word-character "$uuid" '[0-9a-fA-F]{8}' \
  "${nonword}[0-9a-fA-F]{8}"
weaken end-anchored-uuid uuid-members-1 "$uuid" '{12}' '{12}$'
weaken tightened-uuid uuid-members-1 "$uuid" '{12}' '{13}'
weaken loosened-uuid-first-group near-miss-uuid-first-group "$uuid" ']{8}' ']{7}'
weaken loosened-uuid-last-group near-miss-uuid-last-group "$uuid" '{12}' '{11}'
# The home-directory path.
weaken narrowed-home-path-class home-path-members "$home" '._-]' '_-]'
weaken home-path-letter-hole home-path-members "$home" '[A-Za-z0-9._-]' '[A-Za-jl-z0-9._-]'
weaken narrowed-home-path-alternation home-path-home "$home" '|home)' ')'
weaken start-anchored-home-path home-path-Users "$home" '/(Users' '^/(Users'
weaken word-boundary-home-path home-path-after-a-word-character "$home" '/(Users' "${nonword}/(Users"
weaken end-anchored-home-path home-path-Users "$home" ']+/' ']+/$'
weaken tightened-home-path home-path-Users "$home" ']+/' ']{2,}/'
weaken loosened-home-path near-miss-home-path "$home" ']+/' ']*/'
# The Aionforge variable name.
weaken narrowed-aionforge-class aionforge-variable-members-27 "$aionforge" '[A-Z_]' '[A-Z]'
weaken aionforge-letter-hole aionforge-variable-members-13 "$aionforge" '[A-Z_]' '[A-LN-Z_]'
weaken aionforge-first-letter-hole aionforge-variable-members-13 "$aionforge" '_[A-Z][' '_[A-LN-Z]['
weaken start-anchored-aionforge aionforge-variable-members-1 "$aionforge" 'AIONFORGE_' '^AIONFORGE_'
weaken word-boundary-aionforge aionforge-variable-after-a-word-character "$aionforge" 'AIONFORGE_' \
  "${nonword}AIONFORGE_"
weaken end-anchored-aionforge aionforge-variable-members-1 "$aionforge" '_]+' '_]+$'
weaken tightened-aionforge aionforge-variable-members-1 "$aionforge" '_]+' '_]{2,}'
weaken loosened-aionforge near-miss-aionforge-variable "$aionforge" '_]+' '_]*'

if [ "$mutant_failures" -ne 0 ]; then
  echo
  echo "FAIL: $mutant_failures defect control(s) above; the fixtures could not fail."
  exit 1
fi

echo "OK: no-secret scan fixtures passed: a clean tree with $misses near misses, $values planted values" \
  "each caught and cleared, three git-error controls, and $mutants copies with a defect put back," \
  "each refused by its named check."
