#!/usr/bin/env bash
# Install the checksum-pinned upstream mdBook release binary into an untracked directory.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
version=$(tr -d '[:space:]' < "$repo_root/site/mdbook-version")
os_name=$(uname -s)
machine_arch=$(uname -m)

case "$os_name/$machine_arch" in
  Darwin/arm64)
    target_triple="aarch64-apple-darwin"
    ;;
  Darwin/x86_64)
    target_triple="x86_64-apple-darwin"
    ;;
  Linux/aarch64|Linux/arm64)
    target_triple="aarch64-unknown-linux-musl"
    ;;
  Linux/x86_64)
    target_triple="x86_64-unknown-linux-gnu"
    ;;
  *)
    echo "unsupported mdBook installation platform: $os_name/$machine_arch" >&2
    exit 1
    ;;
esac

archive="mdbook-v${version}-${target_triple}.tar.gz"
checksum_file="$repo_root/site/mdbook-checksums.txt"
expected=$(awk -v archive="$archive" '$2 == archive { print $1 }' "$checksum_file")
if [ -z "$expected" ]; then
  echo "no pinned checksum for $archive" >&2
  exit 1
fi

expected_binary=$(awk -v key="${archive}:mdbook" '$2 == key { print $1 }' "$checksum_file")
if [ -z "$expected_binary" ]; then
  echo "no pinned binary checksum for $archive" >&2
  exit 1
fi

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

destination=${1:-"$repo_root/target/mdbook-bin"}

# Idempotent and self-verifying: a binary already at the destination (a CI cache restore, or an
# earlier local run) is trusted only if its digest equals the pinned binary digest. Anything else
# falls through to a fresh download that is verified against the pinned archive digest.
if [ -f "$destination/mdbook" ] && [ "$(sha256_of "$destination/mdbook")" = "$expected_binary" ]; then
  echo "mdbook v${version} already present and verified"
  echo "verified SHA-256 $expected_binary  $destination/mdbook"
  exit 0
fi

download_dir=$(mktemp -d "${TMPDIR:-/tmp}/open-control-mdbook.XXXXXX")
trap 'rm -rf -- "$download_dir"' EXIT
archive_path="$download_dir/$archive"
download_url="https://github.com/rust-lang/mdBook/releases/download/v${version}/${archive}"

curl --fail --location --retry 3 --silent --show-error --output "$archive_path" "$download_url"
actual=$(sha256_of "$archive_path")

if [ "$actual" != "$expected" ]; then
  echo "mdBook checksum mismatch for $archive: expected $expected, got $actual" >&2
  exit 1
fi

tar -xzf "$archive_path" -C "$download_dir"
extracted=$(sha256_of "$download_dir/mdbook")
if [ "$extracted" != "$expected_binary" ]; then
  echo "mdBook binary checksum mismatch for $archive: expected $expected_binary, got $extracted" >&2
  exit 1
fi
mkdir -p "$destination"
install -m 0755 "$download_dir/mdbook" "$destination/mdbook"

reported=$("$destination/mdbook" --version)
if [ "$reported" != "mdbook v${version}" ]; then
  echo "mdBook version mismatch: expected mdbook v${version}, got $reported" >&2
  exit 1
fi

echo "$reported"
echo "verified SHA-256 $actual  $archive"
