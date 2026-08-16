#!/usr/bin/env python3
"""Descriptor-relative bounded reads and copies for Reliefs evidence files."""

import os
import pathlib
import stat

DIRECTORY_FLAGS = (
    os.O_RDONLY
    | getattr(os, "O_DIRECTORY", 0)
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_CLOEXEC", 0)
)
FILE_FLAGS = (
    os.O_RDONLY
    | getattr(os, "O_NONBLOCK", 0)
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_CLOEXEC", 0)
)


def open_directory(path):
    path = pathlib.Path(path)
    components = path.parts
    if path.is_absolute():
        descriptor = os.open(path.anchor, DIRECTORY_FLAGS)
        components = components[1:]
    else:
        descriptor = os.open(".", DIRECTORY_FLAGS)
    try:
        for component in components:
            if component in ("", ".", ".."):
                raise ValueError("directory path contains an unsupported component")
            following = os.open(component, DIRECTORY_FLAGS, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = following
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _read_entry(directory, name, limit):
    if not name or name in (".", "..") or "/" in name or "\x00" in name:
        raise ValueError("file name is not one descriptor-relative component")
    descriptor = os.open(name, FILE_FLAGS, dir_fd=directory)
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_nlink != 1
            or metadata.st_size > limit
        ):
            raise ValueError(f"entry is not a bounded regular file: {name}")
        chunks = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(65536, limit + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > limit:
                raise ValueError(f"entry exceeds its byte bound: {name}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def read_bounded(path, limit):
    path = pathlib.Path(path)
    directory = open_directory(path.parent)
    try:
        return _read_entry(directory, path.name, limit)
    finally:
        os.close(directory)


def read_closed_directory(path, expected_names, limit):
    directory = open_directory(path)
    try:
        observed = set(os.listdir(directory))
        expected = set(expected_names)
        if observed != expected:
            raise ValueError("architecture evidence entries are not closed")
        return {name: _read_entry(directory, name, limit) for name in sorted(expected)}
    finally:
        os.close(directory)


def copy_closed_directory(source, destination, expected_names, limit):
    payloads = read_closed_directory(source, expected_names, limit)
    directory = open_directory(destination)
    try:
        if os.listdir(directory):
            raise ValueError("architecture copy destination is not empty")
        for name, payload in payloads.items():
            descriptor = os.open(
                name,
                os.O_WRONLY
                | os.O_CREAT
                | os.O_EXCL
                | getattr(os, "O_NOFOLLOW", 0)
                | getattr(os, "O_CLOEXEC", 0),
                0o600,
                dir_fd=directory,
            )
            try:
                view = memoryview(payload)
                while view:
                    written = os.write(descriptor, view)
                    if written <= 0:
                        raise OSError("bounded architecture copy made no progress")
                    view = view[written:]
            finally:
                os.close(descriptor)
    finally:
        os.close(directory)
