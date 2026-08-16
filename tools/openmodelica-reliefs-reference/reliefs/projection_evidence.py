#!/usr/bin/env python3
"""Validate retained Reliefs keep-first output and provenance against raw groups."""

import struct

GROUP_SIZES = [1, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2]
RAW_TIME_BITS = """0000000000000000 404e000000000000 404e000000000eff
404e000000000eff 405e000000000000 405e000000000781 405e000000000781
4066800000000000 40668000000003c1 40668000000003c1 406e000000000000
406e0000000003c1 406e0000000003c1 4072c00000000000 4072c000000003c1
4072c000000003c1 4076800000000000 40768000000003c1 40768000000003c1
407a400000000000 407a400000000000""".split()
KEEP_LAST_SOURCES = [0, 3, 6, 9, 12, 15, 18]
KEEP_FIRST_SOURCES = [0, 4, 7, 10, 13, 16, 19]
KEEP_LAST_TIMES = [RAW_TIME_BITS[index] for index in KEEP_LAST_SOURCES]
KEEP_FIRST_TIMES = [RAW_TIME_BITS[index] for index in KEEP_FIRST_SOURCES]
U_T_SUP_BITS = "bfe0000000000000 bfd0000000000000 bfc0000000000000 0000000000000000 3fc0000000000000 3fd0000000000000 3fe0000000000000".split()
CONSTANT_INPUT_BITS = ["3fd0000000000000", "3fec000000000000", "3fc0000000000000", "3fe8000000000000"]


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def record(directory, raw_digest, digest):
    return {
        "control": "explicit_keep_first",
        "input": "reliefs-run-a.raw.csv",
        "input_sha256": raw_digest,
        "canonical_output": "projection-keep-first.canonical.csv",
        "canonical_sha256": digest(directory / "projection-keep-first.canonical.csv"),
        "metadata": "projection-keep-first.metadata",
        "metadata_sha256": digest(directory / "projection-keep-first.metadata"),
        "log": "projection-mutation.log",
        "log_sha256": digest(directory / "projection-mutation.log"),
        "selected_source_rows": KEEP_FIRST_SOURCES,
        "selected_time_bits": KEEP_FIRST_TIMES,
        "expected_source_rows": KEEP_LAST_SOURCES,
        "expected_time_bits": KEEP_LAST_TIMES,
    }


def selected(groups, keep_last):
    grouped = [group[1][-1 if keep_last else 0] for group in groups]
    rows = []
    for row in grouped:
        if not rows or input_bits(row) != input_bits(rows[-1]):
            rows.append(row)
    return rows


def input_bits(row):
    return [bits(value) for value in row[1:6]]


def row_bits(row):
    return [bits(value) for value in row[:8]]


def metadata_rows(rows):
    return ";".join(":".join(row_bits(row)) for row in rows)


def validate(raw_rows, groups, normal_rows, mutation_rows, metadata, log,
             raw_digest, mutation_digest, metadata_digest, canonicalizer_digest):
    expected_mutation_values = selected(groups, False)
    expected_mutation = [row_bits(row) for row in expected_mutation_values]
    if mutation_rows != expected_mutation or mutation_rows == normal_rows:
        raise ValueError("projection mutation is not a live raw keep-first control")
    if [len(group[1]) for group in groups] != GROUP_SIZES:
        raise ValueError("projection mutation group sizes drifted")
    if [bits(row[0]) for row in raw_rows] != RAW_TIME_BITS:
        raise ValueError("projection mutation raw timestamp bits drifted")
    if [row[8] for row in expected_mutation_values] != KEEP_FIRST_SOURCES:
        raise ValueError("projection mutation selected source rows drifted")
    expected_tuples = [[U_T_SUP_BITS[index], *CONSTANT_INPUT_BITS] for index in range(7)]
    if [input_bits(row) for row in expected_mutation_values] != expected_tuples:
        raise ValueError("projection mutation did not preserve authored input tuples")
    expected_metadata = [
        "selection=first", "raw_rows=21", "grouped_rows=14", "canonical_rows=7",
        "group_sizes=" + ",".join(map(str, GROUP_SIZES)),
        "raw_time_bits=" + ",".join(RAW_TIME_BITS),
        "selected_source_rows=" + ",".join(map(str, KEEP_FIRST_SOURCES)),
        "selected_time_bits=" + ",".join(KEEP_FIRST_TIMES),
        "selected_rows=" + metadata_rows(expected_mutation_values),
    ]
    if metadata.splitlines() != expected_metadata:
        raise ValueError("projection mutation inspection metadata is not reproducible")
    expected_log = [
        "projection_mutation=contiguous equal-time selection changed from last to first",
        "execution_path=canonicalize-first-inspect",
        "mutated_input=reliefs-run-a.raw.csv",
        f"mutated_input_sha256={raw_digest}",
        "mutated_output=projection-keep-first.canonical.csv",
        f"mutated_output_sha256={mutation_digest}",
        "mutated_metadata=projection-keep-first.metadata",
        f"mutated_metadata_sha256={metadata_digest}",
        "mutated_raw_rows=21", "mutated_grouped_rows=14", "mutated_canonical_rows=7",
        "mutated_group_sizes=" + ",".join(map(str, GROUP_SIZES)),
        "mutated_raw_time_bits=" + ",".join(RAW_TIME_BITS),
        "mutated_selected_source_rows=" + ",".join(map(str, KEEP_FIRST_SOURCES)),
        "mutated_selected_time_bits=" + ",".join(KEEP_FIRST_TIMES),
        "expected_selected_source_rows=" + ",".join(map(str, KEEP_LAST_SOURCES)),
        "expected_selected_time_bits=" + ",".join(KEEP_LAST_TIMES),
        "mutated_input_tuples_result=PASS",
        "mutated_selected_source_rows_result=FAIL",
        "mutated_selected_time_bits_result=FAIL",
        "mutated_output_differs_from_keep_last=PASS",
        "mutated_grouping_result=PASS", "explicit_keep_first_execution=PASS",
        f"executed_canonicalizer_sha256={canonicalizer_digest}",
    ]
    if log.splitlines() != expected_log:
        raise ValueError("projection mutation execution log is not reproducible")
