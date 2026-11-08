#!/usr/bin/env bash
# Fixtures for install-mdbook.sh's verify-or-reinstall branch. A stub `curl` on PATH records every
# download attempt and fails, so no case touches the network: a destination binary whose digest is
# not the pinned one must provoke a download attempt (never be trusted), and one whose digest is
# pinned must be accepted without any download.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

# A throwaway repo root (the script locates site/ relative to itself) with a stub curl.
root="$tmp/root"
mkdir -p "$root/scripts/docs" "$root/site" "$tmp/stub"
cp scripts/docs/install-mdbook.sh "$root/scripts/docs/"
cp site/mdbook-version "$root/site/"
cat > "$tmp/stub/curl" <<'STUB'
#!/usr/bin/env bash
echo "curl $*" >> "$STUB_CURL_LOG"
exit 22
STUB
chmod +x "$tmp/stub/curl"

printf 'pinned-fixture-binary\n' > "$tmp/good-mdbook"
good="$(sha256_of "$tmp/good-mdbook")"
version="$(tr -d '[:space:]' < site/mdbook-version)"
{
  for triple in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-gnu; do
    archive="mdbook-v${version}-${triple}.tar.gz"
    printf '%064d  %s\n' 0 "$archive"
    printf '%s  %s:mdbook\n' "$good" "$archive"
  done
} > "$root/site/mdbook-checksums.txt"

run_install() {
  dest="$1"
  : > "$tmp/curl.log"
  set +e
  STUB_CURL_LOG="$tmp/curl.log" PATH="$tmp/stub:$PATH" \
    bash "$root/scripts/docs/install-mdbook.sh" "$dest" > "$tmp/out" 2>&1
  status=$?
  set -e
}

fail() {
  echo "FAIL: $1"
  cat "$tmp/out"
  exit 1
}

# 1. A wrong binary at the destination is refused: the script goes to the network, and the
#    stubbed download failure makes the run fail rather than trust the file.
mkdir -p "$tmp/wrong"
printf 'tampered\n' > "$tmp/wrong/mdbook"
chmod +x "$tmp/wrong/mdbook"
run_install "$tmp/wrong"
[ "$status" -ne 0 ] || fail "a wrong cached binary was accepted"
[ -s "$tmp/curl.log" ] || fail "a wrong cached binary was not replaced by a download attempt"
grep -q 'already present' "$tmp/out" && fail "a wrong cached binary was reported as verified"

# 2. A binary with the pinned digest is accepted without any download.
mkdir -p "$tmp/right"
cp "$tmp/good-mdbook" "$tmp/right/mdbook"
run_install "$tmp/right"
[ "$status" -eq 0 ] || fail "a pinned-digest binary was refused"
[ ! -s "$tmp/curl.log" ] || fail "a verified binary still contacted the network"
grep -q "mdbook v${version} already present and verified" "$tmp/out" || fail "missing verified message"

# 3. An absent binary downloads.
mkdir -p "$tmp/empty"
run_install "$tmp/empty"
[ -s "$tmp/curl.log" ] || fail "an absent binary did not download"

# 4. A target with no pinned binary digest is a hard error.
sed -i.bak '/:mdbook$/d' "$root/site/mdbook-checksums.txt"
run_install "$tmp/right"
[ "$status" -ne 0 ] || fail "a missing binary digest pin was accepted"
grep -q 'no pinned binary checksum' "$tmp/out" || fail "missing-pin error not reported"

echo "OK: install-mdbook verify-or-reinstall fixtures passed."
