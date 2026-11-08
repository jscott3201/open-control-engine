#!/usr/bin/env python3
"""Guard the approved release workflow's raw bytes, not YAML or shell semantics."""

import hashlib


# Approval witness for release.yml as re-pinned when the member count fell to 12 (GitHub #391).
# Intentional changes follow the procedure in docs/ci-and-the-gate.md (the package validator).
# Never derive the expected digest from the candidate workflow at validation time.
EXPECTED_SHA256 = "e51991413ede503f304146046aa535958f2f70728ce3a50ac666d0d1106f1b6c"


class WorkflowError(ValueError):
    """The candidate bytes differ from the approved release workflow."""


def validate(workflow: bytes) -> None:
    """Accept only the approved raw bytes; reject all drift without decoding."""

    if hashlib.sha256(workflow).hexdigest() != EXPECTED_SHA256:
        raise WorkflowError("release workflow bytes differ from the approved SHA-256")
