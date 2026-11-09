#!/usr/bin/env python3
"""Guard the approved release workflow's raw bytes, not YAML or shell semantics."""

import hashlib


# Approval witness for release.yml as re-pinned when the leverage crate oce-author became the fourth
# private member (issue #463; previously for the firmware image, GitHub #449, and when the member
# count fell to 12, GitHub #391).
# Intentional changes follow the procedure in docs/ci-and-the-gate.md (the package validator).
# Never derive the expected digest from the candidate workflow at validation time.
EXPECTED_SHA256 = "56ca5bf1135e52cc4e0dc0f42757369630b5e6c05dfc3fd1039267689a0199f6"


class WorkflowError(ValueError):
    """The candidate bytes differ from the approved release workflow."""


def validate(workflow: bytes) -> None:
    """Accept only the approved raw bytes; reject all drift without decoding."""

    if hashlib.sha256(workflow).hexdigest() != EXPECTED_SHA256:
        raise WorkflowError("release workflow bytes differ from the approved SHA-256")
