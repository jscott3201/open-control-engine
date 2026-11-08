#!/usr/bin/env bash
# Install the checksum-pinned wasmtime release binary into an untracked directory (issue #403).
#
#   bash scripts/ci/install-wasmtime.sh [destination]    # default: target/wasmtime-bin
#
# The blob round trip's wasm32 leg runs its test binary under this wasmtime through the cargo
# target runner (scripts/ci/wasmtime-runner.sh). The version and the SHA-256 of every supported
# platform's release archive and of the binary inside it are pinned below; the archive digests are
# the ones GitHub records for the release assets. Idempotent and self-verifying, like
# scripts/docs/install-mdbook.sh: a binary already at the destination (a CI cache restore or an
# earlier run) is kept only if its digest is the pinned one, and anything else is downloaded again
# and verified before it is installed. Bump the version by replacing every digest in one change.

set -euo pipefail

version="50.0.1"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)
    platform="x86_64-linux"
    archive_sha="685466455dd42a14774a86c36c8f2e45cbb9bd6d92a850139315df60a75498a5"
    binary_sha="983e57573a320cf44c93ccaa571a25640cf38b15e8e14160aa7bb919336caae8"
    ;;
  Linux/aarch64 | Linux/arm64)
    platform="aarch64-linux"
    archive_sha="d737e077416d78fbfb0a008229ec27d6a0fd5a4924874b8c38b293c850cf4094"
    binary_sha="c453a1ce8ea2b1d5808d1cd011625e7137444f0fbe68f2cf01ce3087e88295ad"
    ;;
  Darwin/arm64)
    platform="aarch64-macos"
    archive_sha="960e5216c7ebd7e9b45e6561d15821b8b801b7d54aee28c7b91921aeb30c65c8"
    binary_sha="249ec535c752c84a1a386a31ddfc3c7611f4819363bf5d3840d7dfabd1046534"
    ;;
  Darwin/x86_64)
    platform="x86_64-macos"
    archive_sha="ea1818fa8cbb51524847ab2ad22197e0051594fdc96cbd0cc2afb65832e11925"
    binary_sha="bfc0dc34bc670749d8cb218b4ebd8ff4606d415abd9ba550b9cde73c82eff410"
    ;;
  *)
    echo "unsupported wasmtime installation platform: $(uname -s)/$(uname -m)" >&2
    exit 1
    ;;
esac

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

destination=${1:-"$repo_root/target/wasmtime-bin"}

if [ -f "$destination/wasmtime" ] && [ "$(sha256_of "$destination/wasmtime")" = "$binary_sha" ]; then
  echo "wasmtime $version already present and verified"
else
  name="wasmtime-v${version}-${platform}"
  download_dir=$(mktemp -d "${TMPDIR:-/tmp}/open-control-wasmtime.XXXXXX")
  trap 'rm -rf -- "$download_dir"' EXIT
  archive_path="$download_dir/$name.tar.xz"
  curl --fail --location --retry 3 --silent --show-error --output "$archive_path" \
    "https://github.com/bytecodealliance/wasmtime/releases/download/v${version}/${name}.tar.xz"
  actual=$(sha256_of "$archive_path")
  if [ "$actual" != "$archive_sha" ]; then
    echo "wasmtime archive checksum mismatch for $name: expected $archive_sha, got $actual" >&2
    exit 1
  fi
  mkdir -p "$download_dir/extract"
  tar -xJf "$archive_path" -C "$download_dir/extract"
  extracted=$(sha256_of "$download_dir/extract/$name/wasmtime")
  if [ "$extracted" != "$binary_sha" ]; then
    echo "wasmtime binary checksum mismatch for $name: expected $binary_sha, got $extracted" >&2
    exit 1
  fi
  mkdir -p "$destination"
  install -m 0755 "$download_dir/extract/$name/wasmtime" "$destination/wasmtime"
  echo "verified SHA-256 $actual  $name.tar.xz"
fi

reported=$("$destination/wasmtime" --version)
case "$reported" in
  "wasmtime $version "* | "wasmtime $version") ;;
  *)
    echo "wasmtime version mismatch: expected wasmtime $version, got $reported" >&2
    exit 1
    ;;
esac
echo "$reported"
echo "verified SHA-256 $binary_sha  $destination/wasmtime"
