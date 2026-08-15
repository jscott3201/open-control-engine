#!/usr/bin/env python3
"""Claim, verify, clean, and atomically publish a private evidence directory."""

import os
import pathlib
import shutil
import stat
import sys
import tempfile


def fail(detail):
    raise ValueError(f"output publication failed: {detail}")


def inspect_directory(path, name):
    path = pathlib.Path(path).absolute()
    trusted = pathlib.Path(tempfile.gettempdir()).resolve()
    lexical_temp = pathlib.Path(tempfile.gettempdir()).absolute()
    try:
        lexical_relative = path.relative_to(lexical_temp)
    except ValueError:
        pass
    else:
        path = trusted / lexical_relative
    try:
        relative = path.relative_to(trusted)
    except ValueError:
        relative = pathlib.Path(*path.parts[1:])
        current = pathlib.Path(path.anchor)
    else:
        current = trusted
    for part in relative.parts:
        current /= part
        metadata = current.lstat()
        if current.is_symlink():
            fail(f"{name} contains a symlink component")
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode):
        fail(f"{name} is not a directory")
    return path, metadata


def safe_parent(destination):
    destination = pathlib.Path(destination).absolute()
    if any(delimiter in str(destination) for delimiter in "\t\r\n"):
        fail("output destination contains a record delimiter")
    parent, metadata = inspect_directory(destination.parent, "output parent")
    mode = stat.S_IMODE(metadata.st_mode)
    private = metadata.st_uid == os.getuid() and mode & 0o022 == 0
    if not private:
        fail("output parent must be owned by the current user and not group/other writable")
    try:
        destination.lstat()
    except FileNotFoundError:
        pass
    else:
        fail("output destination already exists")
    return destination, parent, metadata


def identity(path, expected_device, expected_inode):
    metadata = pathlib.Path(path).lstat()
    if pathlib.Path(path).is_symlink() or not stat.S_ISDIR(metadata.st_mode):
        fail("claimed path is no longer a directory")
    if (metadata.st_dev, metadata.st_ino) != (expected_device, expected_inode):
        fail("claimed path identity changed")
    return metadata


def claim(destination):
    destination, parent, parent_metadata = safe_parent(destination)
    path = pathlib.Path(tempfile.mkdtemp(prefix=".oce-toggle-evidence.", dir=parent))
    path.chmod(0o700)
    metadata = path.lstat()
    print("\t".join(map(str, [path, metadata.st_dev, metadata.st_ino, parent_metadata.st_dev, parent_metadata.st_ino, destination])))


def claim_child(parent, name):
    parent, _ = inspect_directory(parent, "private output")
    if "/" in name or name in ("", ".", ".."):
        fail("invalid private child name")
    path = parent / name
    os.mkdir(path, 0o700)
    metadata = path.lstat()
    print("\t".join(map(str, [path, metadata.st_dev, metadata.st_ino])))


def cleanup(path, device, inode):
    identity(path, int(device), int(inode))
    shutil.rmtree(path)


def publish(path, device, inode, parent_device, parent_inode, destination):
    identity(path, int(device), int(inode))
    destination, parent, metadata = safe_parent(destination)
    if (metadata.st_dev, metadata.st_ino) != (int(parent_device), int(parent_inode)):
        fail("output parent identity changed")
    os.rename(path, destination)


def main(arguments):
    command, *values = arguments
    if command == "claim" and len(values) == 1:
        claim(values[0])
    elif command == "claim-child" and len(values) == 2:
        claim_child(*values)
    elif command == "cleanup" and len(values) == 3:
        cleanup(*values)
    elif command == "publish" and len(values) == 6:
        publish(*values)
    else:
        fail("invalid command")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
