#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
HELPER="$SCRIPT_DIR/output_publish.py"
SYSTEM_TEMP=$(env -u TMPDIR -u TMP -u TEMP python3 -c 'import pathlib,tempfile; print(pathlib.Path(tempfile.gettempdir()).resolve())')
PRIVATE_PARENT="$SYSTEM_TEMP/oce-line-publish-tests-$(id -u)"
if ! mkdir -m 700 "$PRIVATE_PARENT" 2>/dev/null; then
  python3 -c 'import os,pathlib,stat,sys; value=pathlib.Path(sys.argv[1]); metadata=value.lstat(); assert stat.S_ISDIR(metadata.st_mode) and not value.is_symlink() and metadata.st_uid == os.getuid() and stat.S_IMODE(metadata.st_mode) == 0o700' "$PRIVATE_PARENT"
fi
nonce=0
while :; do
  ROOT_DESTINATION="$PRIVATE_PARENT/case-$$-$nonce"
  if ROOT_RECORD=$(python3 "$HELPER" claim "$ROOT_DESTINATION" 2>/dev/null); then break; fi
  nonce=$((nonce + 1)); test "$nonce" -lt 1024
done
ROOT=$(printf '%s' "$ROOT_RECORD" | cut -f1)
ROOT_DEVICE=$(printf '%s' "$ROOT_RECORD" | cut -f2)
ROOT_INODE=$(printf '%s' "$ROOT_RECORD" | cut -f3)
cleanup() {
  env -u TMPDIR -u TMP -u TEMP python3 "$HELPER" cleanup "$ROOT" "$ROOT_DEVICE" "$ROOT_INODE"
  rmdir "$PRIVATE_PARENT" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

claim() {
  record=$(python3 "$HELPER" claim "$1")
  path=$(printf '%s' "$record" | cut -f1)
  device=$(printf '%s' "$record" | cut -f2)
  inode=$(printf '%s' "$record" | cut -f3)
  parent_device=$(printf '%s' "$record" | cut -f4)
  parent_inode=$(printf '%s' "$record" | cut -f5)
  destination=$(printf '%s' "$record" | cut -f6)
}

claim "$ROOT/published"
printf '%s\n' evidence > "$path/marker"
python3 "$HELPER" publish "$path" "$device" "$inode" "$parent_device" "$parent_inode" "$destination"
test "$(cat "$ROOT/published/marker")" = evidence

mkdir "$ROOT/unsafe"; chmod 0777 "$ROOT/unsafe"
if python3 "$HELPER" claim "$ROOT/unsafe/output" >/dev/null 2>&1; then exit 1; fi
mkdir "$ROOT/sticky"; chmod 1777 "$ROOT/sticky"
if python3 "$HELPER" claim "$ROOT/sticky/output" >/dev/null 2>&1; then exit 1; fi
TAB=$(printf '\t')
if python3 "$HELPER" claim "$ROOT/bad${TAB}output" >/dev/null 2>&1; then exit 1; fi

claim "$ROOT/replaced-output"
rmdir "$path"; mkdir "$ROOT/replacement"; printf '%s\n' protected > "$ROOT/replacement/marker"; ln -s "$ROOT/replacement" "$path"
if python3 "$HELPER" cleanup "$path" "$device" "$inode" >/dev/null 2>&1; then exit 1; fi
test "$(cat "$ROOT/replacement/marker")" = protected
rm "$path"

claim "$ROOT/blocked-output"
ln -s "$ROOT/replacement" "$destination"
if python3 "$HELPER" publish "$path" "$device" "$inode" "$parent_device" "$parent_inode" "$destination" >/dev/null 2>&1; then exit 1; fi
test -d "$path"
rm "$destination"
python3 "$HELPER" cleanup "$path" "$device" "$inode"
printf '%s\n' 'output publication test passed'
