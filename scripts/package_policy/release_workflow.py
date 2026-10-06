#!/usr/bin/env python3
"""Guard the approved release workflow's raw bytes, not YAML or shell semantics."""

import hashlib


# Approval witness for release.yml at 2bab88acbc96862f1808b34d305b795f521b3614.
# Intentional changes require the manual review procedure in the publication policy.
# Never derive the expected digest from the candidate workflow at validation time.
EXPECTED_SHA256 = "c74806d5183289662df6379adb22c1708a260e1c3f64f1cfe9314342415ee90b"


class WorkflowError(ValueError):
    """The candidate bytes differ from the approved release workflow."""


def validate(workflow: bytes) -> None:
    """Accept only the approved raw bytes; reject all drift without decoding."""

    if hashlib.sha256(workflow).hexdigest() != EXPECTED_SHA256:
        raise WorkflowError("release workflow bytes differ from the approved SHA-256")
