#!/usr/bin/env python3
"""The `CI OK` result check for .github/workflows/pr-gate.yml (issue #393).

Reads two environment variables the `ci-ok` job sets:

* ``NEEDS_JSON`` -- ``${{ toJSON(needs) }}``: every needed job's ``result`` and the ``changes``
  job's ``outputs``.
* ``JOB_LANES`` -- whitespace-separated ``job=lane`` pairs naming each lane-filtered job and the
  ``changes`` output that gates it (its ``if:`` is ``needs.changes.outputs.<lane> == 'true'``).
  ``check-workflow-gates.sh`` asserts that this list matches the jobs' ``if:`` lines exactly.

A job passes when its result is ``success``. A job may instead be ``skipped`` only when it is in
``JOB_LANES`` and its lane output is exactly ``false``: the filter skipped it. Anything else fails
-- ``failure``, ``cancelled``, a skip while its lane output was ``true`` (a job that did not start
although its lane ran, e.g. because a job it needs failed), a skip of an unfiltered job, or an
absent, malformed or inconsistent ``changes`` result. Standard library only.
"""

from __future__ import annotations

import json
import os
import sys

LANES = ("core", "leverage", "gate_scripts", "docs_only")
FILTERS = ("core", "leverage", "gate_scripts")


class CheckError(Exception):
    """The aggregate must not report success."""


def parse_lanes(text: str) -> dict[str, str]:
    lanes: dict[str, str] = {}
    for pair in text.split():
        job, sep, lane = pair.partition("=")
        if not sep or not job or lane not in FILTERS:
            raise CheckError(f"malformed JOB_LANES entry: {pair!r}")
        if job in lanes:
            raise CheckError(f"job listed twice in JOB_LANES: {job}")
        lanes[job] = lane
    if not lanes:
        raise CheckError("JOB_LANES is empty")
    return lanes


def lane_outputs(needs: dict) -> dict[str, str]:
    changes = needs.get("changes")
    if not isinstance(changes, dict) or changes.get("result") != "success":
        raise CheckError("the changes job did not succeed, so no job may count as filtered")
    outputs = changes.get("outputs") or {}
    values = {lane: outputs.get(lane) for lane in LANES}
    bad = {lane: value for lane, value in values.items() if value not in ("true", "false")}
    if bad:
        raise CheckError(f"changes outputs missing or not true/false: {bad}")
    if values["gate_scripts"] == "true" and values["core"] != "true":
        raise CheckError("gate_scripts=true requires core=true")
    if values["core"] == "true" and values["leverage"] != "true":
        raise CheckError("core=true requires leverage=true")
    if values["docs_only"] == "true" and "true" in (values["core"], values["leverage"]):
        raise CheckError("docs_only=true contradicts a code lane")
    return values


def check(needs: dict, job_lanes: dict[str, str]) -> list[str]:
    """Return one report line per job; raise CheckError if any job is not acceptable."""
    if not needs:
        raise CheckError("no gating job results reported")
    unknown = sorted(set(job_lanes) - set(needs))
    if unknown:
        raise CheckError(f"JOB_LANES names jobs CI OK does not need: {unknown}")
    outputs = lane_outputs(needs)
    report, bad = [], {}
    for job, data in sorted(needs.items()):
        result = data.get("result") if isinstance(data, dict) else None
        lane = job_lanes.get(job)
        if result == "success":
            report.append(f"{job}: success")
        elif result == "skipped" and lane is not None and outputs[lane] == "false":
            report.append(f"{job}: skipped by the lane filter ({lane}=false)")
        else:
            why = f" while {lane}={outputs[lane]}" if result == "skipped" and lane else ""
            report.append(f"{job}: {result}{why}")
            bad[job] = f"{result}{why}"
    if bad:
        raise CheckError(f"gating jobs not green: {bad}")
    return report


def main() -> int:
    try:
        needs = json.loads(os.environ["NEEDS_JSON"])
        report = check(needs, parse_lanes(os.environ.get("JOB_LANES", "")))
    except (CheckError, KeyError, ValueError) as error:
        print(f"CI OK: FAIL: {error}", file=sys.stderr)
        return 1
    print("\n".join(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
