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

destination=${1:-"$repo_root/target/mdbook-bin"}
download_dir=$(mktemp -d "${TMPDIR:-/tmp}/open-control-mdbook.XXXXXX")
trap 'rm -rf -- "$download_dir"' EXIT
archive_path="$download_dir/$archive"
download_url="https://github.com/rust-lang/mdBook/releases/download/v${version}/${archive}"

curl --fail --location --retry 3 --silent --show-error --output "$archive_path" "$download_url"
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive_path" | awk '{ print $1 }')
else
  actual=$(shasum -a 256 "$archive_path" | awk '{ print $1 }')
fi

if [ "$actual" != "$expected" ]; then
  echo "mdBook checksum mismatch for $archive: expected $expected, got $actual" >&2
  exit 1
fi

tar -xzf "$archive_path" -C "$download_dir"
mkdir -p "$destination"
install -m 0755 "$download_dir/mdbook" "$destination/mdbook"

reported=$("$destination/mdbook" --version)
if [ "$reported" != "mdbook v${version}" ]; then
  echo "mdBook version mismatch: expected mdbook v${version}, got $reported" >&2
  exit 1
fi

echo "$reported"
echo "verified SHA-256 $actual  $archive"
