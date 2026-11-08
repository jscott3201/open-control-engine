#!/usr/bin/env python3
"""Guard the approved release workflow's raw bytes, not YAML or shell semantics."""

import hashlib


# Approval witness for release.yml as re-pinned when the reference WAL adapter was removed (GitHub #390).
# Intentional changes follow the procedure in docs/ci-and-the-gate.md (the package validator).
# Never derive the expected digest from the candidate workflow at validation time.
EXPECTED_SHA256 = "ea12ba3227925ddf2012741170704c25d11135185960baa21d9709b8dce59e28"


class WorkflowError(ValueError):
    """The candidate bytes differ from the approved release workflow."""


def validate(workflow: bytes) -> None:
    """Accept only the approved raw bytes; reject all drift without decoding."""

    if hashlib.sha256(workflow).hexdigest() != EXPECTED_SHA256:
        raise WorkflowError("release workflow bytes differ from the approved SHA-256")
