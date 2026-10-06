#!/usr/bin/env python3
"""Materialize exact registry JSON bytes from a newline-terminated tracked source."""

import hashlib
import os
import pathlib
import stat
import sys

source, destination, expected = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
metadata = source.lstat()
if source.is_symlink() or not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 1024 * 1024:
    raise SystemExit("OCI source is not a bounded regular file")
data = source.read_bytes()
if not data.endswith(b"\n") or data.endswith(b"\n\n"):
    raise SystemExit("OCI source must carry exactly one publication newline")
payload = data[:-1]
if hashlib.sha256(payload).hexdigest() != expected:
    raise SystemExit("OCI payload digest mismatch")
descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
try:
    os.write(descriptor, payload)
finally:
    os.close(descriptor)
