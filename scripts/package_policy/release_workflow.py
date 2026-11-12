#!/usr/bin/env python3
"""Guard the approved release workflow's raw bytes, not YAML or shell semantics."""

import hashlib


# Approval witness for release.yml as re-pinned when its verify job began prefetching the tool
# workspaces the tests build offline (issue #610; previously when the leverage crate oce-verify
# became the eighth private member, issue #519, for oce-vocab, issue #499, oce-cli, issue #489,
# oce-author, issue #463, oce-replay, issue #445, the firmware image, GitHub #449, and when the
# member count fell to 12, GitHub #391).
# Intentional changes follow the procedure in docs/ci-and-the-gate.md (the package validator).
# Never derive the expected digest from the candidate workflow at validation time.
EXPECTED_SHA256 = "27d29381843ab05931f698b12853c739953e54c77d7520e5257adc653012bfce"


class WorkflowError(ValueError):
    """The candidate bytes differ from the approved release workflow."""


def validate(workflow: bytes) -> None:
    """Accept only the approved raw bytes; reject all drift without decoding."""

    if hashlib.sha256(workflow).hexdigest() != EXPECTED_SHA256:
        raise WorkflowError("release workflow bytes differ from the approved SHA-256")
