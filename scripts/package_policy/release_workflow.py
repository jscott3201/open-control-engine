#!/usr/bin/env python3
"""Validate release.yml with the repository's restricted YAML workflow style."""

from __future__ import annotations

import re
from dataclasses import dataclass


ALLOWED_BLOCK_STYLES = frozenset({"|", "|-", "|+", ">", ">-", ">+"})
MAPPING = re.compile(r"(?P<key>[A-Za-z_][A-Za-z0-9_-]*):(?P<rest>.*)")
JOB_ID = re.compile(r"[A-Za-z_][A-Za-z0-9_-]*")


class WorkflowError(ValueError):
    """A deterministic release-workflow structure or policy violation."""


@dataclass(frozen=True)
class MappingEntry:
    """One plain-key mapping entry in the supported YAML subset."""

    key: str
    value: str
    sequence: bool


@dataclass(frozen=True)
class WorkflowLine:
    """One structural YAML line with any following block scalar attached."""

    number: int
    indent: int
    text: str
    block_style: str | None = None
    block_body: str | None = None


@dataclass(frozen=True)
class Section:
    """One real top-level workflow mapping entry and its structural children."""

    key: str
    value: str
    line: WorkflowLine
    body: tuple[WorkflowLine, ...]


@dataclass(frozen=True)
class Job:
    """One plain-key job from the real top-level jobs mapping."""

    name: str
    line: WorkflowLine
    body: tuple[WorkflowLine, ...]


@dataclass(frozen=True)
class RunCommand:
    """One active step run scalar and its owning real job."""

    job: str
    line: int
    style: str
    script: str


def _indent(raw: str, number: int) -> int:
    leading_length = len(raw) - len(raw.lstrip(" \t"))
    leading = raw[:leading_length]
    if "\t" in leading:
        raise WorkflowError(f"release workflow uses tab indentation at line {number}")
    return leading_length


def _mapping_entry(text: str) -> MappingEntry | None:
    sequence = text.startswith("- ")
    candidate = text[2:] if sequence else text
    match = MAPPING.fullmatch(candidate)
    if match is None:
        return None
    rest = match.group("rest")
    if rest and not rest.startswith((" ", "\t")):
        return None
    return MappingEntry(match.group("key"), rest.strip(), sequence)


def _block_style(line: WorkflowLine) -> str | None:
    entry = _mapping_entry(line.text)
    if entry is None or not entry.value.startswith(("|", ">")):
        return None
    match = re.fullmatch(r"(?P<style>[|>][+-]?)(?:\s+#.*)?", entry.value)
    if match is None or match.group("style") not in ALLOWED_BLOCK_STYLES:
        raise WorkflowError(
            f"release workflow uses an unsupported block scalar at line {line.number}"
        )
    return match.group("style")


def _consume_block(
    physical: list[str], start: int, parent_indent: int
) -> tuple[str, int]:
    cursor = start + 1
    leading_blanks = 0
    while cursor < len(physical) and not physical[cursor].strip():
        leading_blanks += 1
        cursor += 1
    if cursor == len(physical):
        return "\n" * leading_blanks, cursor

    content_indent = _indent(physical[cursor], cursor + 1)
    if content_indent <= parent_indent:
        return "\n" * leading_blanks, cursor

    body = [""] * leading_blanks
    while cursor < len(physical):
        raw = physical[cursor]
        if not raw.strip():
            body.append("")
            cursor += 1
            continue
        indent = _indent(raw, cursor + 1)
        if indent < content_indent:
            break
        body.append(raw[content_indent:])
        cursor += 1
    return "\n".join(body), cursor


def _structural_lines(workflow: str) -> list[WorkflowLine]:
    physical = workflow.splitlines()
    result: list[WorkflowLine] = []
    cursor = 0
    while cursor < len(physical):
        raw = physical[cursor]
        indent = _indent(raw, cursor + 1)
        text = raw[indent:]
        if not text or text.startswith("#"):
            cursor += 1
            continue
        line = WorkflowLine(cursor + 1, indent, text)
        style = _block_style(line)
        if style is None:
            result.append(line)
            cursor += 1
            continue
        body, cursor = _consume_block(physical, cursor, indent)
        result.append(WorkflowLine(line.number, indent, text, style, body))
    return result


def _required_mapping(line: WorkflowLine, label: str) -> MappingEntry:
    entry = _mapping_entry(line.text)
    if entry is None:
        raise WorkflowError(
            f"release workflow has unsupported {label} syntax at line {line.number}"
        )
    return entry


def _top_level_sections(lines: list[WorkflowLine]) -> dict[str, Section]:
    starts: list[tuple[int, MappingEntry]] = []
    for index, line in enumerate(lines):
        if line.indent == 0:
            entry = _required_mapping(line, "top-level key")
            if entry.sequence:
                raise WorkflowError(
                    f"release workflow root must be a mapping at line {line.number}"
                )
            starts.append((index, entry))
        elif not starts:
            raise WorkflowError(
                f"release workflow has content before a top-level key at line {line.number}"
            )

    sections: dict[str, Section] = {}
    for position, (start, entry) in enumerate(starts):
        if entry.key in sections:
            raise WorkflowError(f"release workflow has duplicate top-level key {entry.key}")
        end = starts[position + 1][0] if position + 1 < len(starts) else len(lines)
        sections[entry.key] = Section(
            entry.key, entry.value, lines[start], tuple(lines[start + 1 : end])
        )
    return sections


def _require_manual_trigger(sections: dict[str, Section]) -> None:
    trigger_section = sections.get("on")
    if trigger_section is None or trigger_section.value or trigger_section.line.block_style:
        raise WorkflowError("release workflow has no supported top-level trigger mapping")

    manual: list[MappingEntry] = []
    seen_trigger = False
    for line in trigger_section.body:
        if line.indent == 2:
            entry = _required_mapping(line, "trigger key")
            if entry.sequence:
                raise WorkflowError(
                    f"release workflow has unsupported trigger syntax at line {line.number}"
                )
            seen_trigger = True
            if entry.key == "workflow_dispatch":
                manual.append(entry)
        elif line.indent < 2 or not seen_trigger:
            raise WorkflowError(
                f"release workflow has unsupported trigger indentation at line {line.number}"
            )
    if len(manual) != 1 or manual[0].value:
        raise WorkflowError("release workflow has no exact manual dispatch trigger")


def _parse_jobs(section: Section) -> dict[str, Job]:
    if section.value or section.line.block_style:
        raise WorkflowError("release workflow jobs must use a top-level mapping")

    collected: list[tuple[str, WorkflowLine, list[WorkflowLine]]] = []
    current: tuple[str, WorkflowLine, list[WorkflowLine]] | None = None
    for line in section.body:
        if line.indent == 2:
            entry = _required_mapping(line, "job key")
            if entry.sequence or entry.value or JOB_ID.fullmatch(entry.key) is None:
                raise WorkflowError(
                    f"release workflow has unsupported job-key shape at line {line.number}"
                )
            current = (entry.key, line, [])
            collected.append(current)
        elif line.indent < 2 or current is None:
            raise WorkflowError(
                f"release workflow has unsupported jobs indentation at line {line.number}"
            )
        else:
            current[2].append(line)

    jobs: dict[str, Job] = {}
    for name, line, body in collected:
        if name in jobs:
            raise WorkflowError(f"release workflow has duplicate job {name}")
        jobs[name] = Job(name, line, tuple(body))
    for required in ("verify", "publish"):
        if required not in jobs:
            raise WorkflowError(f"release workflow is missing real job {required}")
    return jobs


def _job_fields(job: Job) -> list[tuple[int, MappingEntry]]:
    fields: list[tuple[int, MappingEntry]] = []
    for index, line in enumerate(job.body):
        if line.indent == 4:
            entry = _required_mapping(line, f"{job.name} job-level key")
            if entry.sequence:
                raise WorkflowError(
                    f"release workflow job {job.name} has unsupported mapping syntax "
                    f"at line {line.number}"
                )
            fields.append((index, entry))
        elif line.indent < 4 or not fields:
            raise WorkflowError(
                f"release workflow job {job.name} has unsupported indentation at line {line.number}"
            )
    return fields


def _require_job_value(
    job: Job,
    fields: list[tuple[int, MappingEntry]],
    key: str,
    expected: str,
    violation: str,
) -> None:
    values = [entry.value for _, entry in fields if entry.key == key]
    nested = []
    for line in job.body:
        if line.indent <= 4:
            continue
        entry = _mapping_entry(line.text)
        if entry is not None and entry.key == key:
            nested.append(line.number)
    if nested:
        raise WorkflowError(f"{violation} (nested {key})")
    if len(values) > 1:
        raise WorkflowError(f"{violation} (duplicate job-level {key})")
    if len(values) != 1 or values[0] != expected:
        raise WorkflowError(violation)


def _run_command(job: Job, line: WorkflowLine, entry: MappingEntry) -> RunCommand:
    if line.block_style is not None:
        return RunCommand(job.name, line.number, line.block_style, line.block_body or "")
    if not entry.value:
        raise WorkflowError(f"release workflow has an empty run scalar at line {line.number}")
    if entry.value.startswith(("*", "&", "!", "[", "{")):
        raise WorkflowError(
            f"release workflow has an unsupported run scalar at line {line.number}"
        )
    return RunCommand(job.name, line.number, "inline", entry.value)


def _job_runs(job: Job, fields: list[tuple[int, MappingEntry]]) -> list[RunCommand]:
    steps = [(index, entry) for index, entry in fields if entry.key == "steps"]
    if not steps:
        return []
    if len(steps) != 1 or steps[0][1].value:
        raise WorkflowError(f"release workflow job {job.name} has unsupported steps mapping")

    start = steps[0][0] + 1
    end = next(
        (index for index in range(start, len(job.body)) if job.body[index].indent == 4),
        len(job.body),
    )
    runs: list[RunCommand] = []
    current_step = -1
    current_has_run = False
    for line in job.body[start:end]:
        entry = _mapping_entry(line.text)
        if line.indent == 6:
            if entry is None or not entry.sequence:
                raise WorkflowError(
                    f"release workflow has unsupported step item at line {line.number}"
                )
            current_step += 1
            current_has_run = False
        elif line.indent == 8:
            if current_step < 0 or entry is None or entry.sequence:
                raise WorkflowError(
                    f"release workflow has unsupported step mapping at line {line.number}"
                )
        elif line.indent > 8:
            if entry is None:
                raise WorkflowError(
                    f"release workflow has unsupported multiline scalar at line {line.number}"
                )
            if entry.key == "run":
                raise WorkflowError(
                    f"release workflow has a run scalar outside a step root at line {line.number}"
                )
            continue
        else:
            raise WorkflowError(
                f"release workflow has unsupported step indentation at line {line.number}"
            )

        if entry.key != "run":
            continue
        if current_has_run:
            raise WorkflowError(
                f"release workflow has duplicate run scalars in one step at line {line.number}"
            )
        current_has_run = True
        runs.append(_run_command(job, line, entry))
    return runs


def _mentions_cargo_publish(script: str) -> bool:
    compact = re.sub(r"[^a-z]", "", script.casefold())
    return "cargo" in compact and "publish" in compact


def validate(
    workflow: str,
    *,
    validator_command: str,
    dry_run_command: str,
    publish_command: str,
    publish_event: str,
) -> None:
    """Validate release structure, authorization scope, and every active run body."""

    lines = _structural_lines(workflow)
    sections = _top_level_sections(lines)
    jobs_section = sections.get("jobs")
    if jobs_section is None:
        raise WorkflowError("release workflow is missing one real top-level jobs mapping")
    _require_manual_trigger(sections)
    jobs = _parse_jobs(jobs_section)
    fields = {name: _job_fields(job) for name, job in jobs.items()}
    runs = [command for name, job in jobs.items() for command in _job_runs(job, fields[name])]

    verify_runs = [command for command in runs if command.job == "verify"]
    publish_runs = [command for command in runs if command.job == "publish"]
    if not all(
        any(command.style == "inline" and command.script == expected for command in verify_runs)
        for expected in (validator_command, dry_run_command)
    ):
        raise WorkflowError(
            "release verify job does not validate and dry-run the exact workspace selection"
        )
    if not any(
        command.style == "inline" and command.script == publish_command
        for command in publish_runs
    ):
        raise WorkflowError("release publish job does not use the exact workspace selection")

    publish_job = jobs["publish"]
    _require_job_value(
        publish_job,
        fields["publish"],
        "needs",
        "verify",
        "release publish job is not guarded by verify",
    )
    _require_job_value(
        publish_job,
        fields["publish"],
        "environment",
        "release",
        "release publish job is not bound to environment: release",
    )
    _require_job_value(
        publish_job,
        fields["publish"],
        "if",
        f"github.event_name == '{publish_event}'",
        "release publication is not restricted to manual dispatch",
    )

    expected_publish_runs = {("verify", dry_run_command), ("publish", publish_command)}
    actual_publish_runs: list[tuple[str, str]] = []
    for command in runs:
        if not _mentions_cargo_publish(command.script):
            continue
        identity = (command.job, command.script)
        if command.style != "inline" or identity not in expected_publish_runs:
            raise WorkflowError(
                f"release workflow has an unapproved cargo publish run at line {command.line}"
            )
        actual_publish_runs.append(identity)
    if sorted(actual_publish_runs) != sorted(expected_publish_runs):
        raise WorkflowError("release workflow cargo publish command set drifted")
