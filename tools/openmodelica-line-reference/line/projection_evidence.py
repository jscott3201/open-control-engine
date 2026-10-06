#!/usr/bin/env python3
"""Validate retained Line keep-first output against raw equal-time groups."""

import struct


TIME_BITS = [
    "0000000000000000", "404e000000000000", "404e000000000eff",
    "405e000000000000", "405e000000000781", "4066800000000000",
    "40668000000003c1", "406e000000000000", "406e0000000003c1",
    "4072c00000000000",
]
U_BITS = [
    "c010000000000000", "c010000000000000", "c000000000000000",
    "c000000000000000", "0000000000000000", "0000000000000000",
    "4000000000000000", "4000000000000000", "4010000000000000",
    "4010000000000000",
]
GROUP_SIZES = [1, 1, 2, 1, 2, 1, 2, 1, 2, 2]


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def record(directory, raw_digest, digest):
    return {
        "control": "explicit_keep_first",
        "input": "line-run-a.raw.csv",
        "input_sha256": raw_digest,
        "canonical_output": "projection-keep-first.canonical.csv",
        "canonical_sha256": digest(directory / "projection-keep-first.canonical.csv"),
        "metadata": "projection-keep-first.metadata",
        "metadata_sha256": digest(directory / "projection-keep-first.metadata"),
        "log": "projection-mutation.log",
        "log_sha256": digest(directory / "projection-mutation.log"),
        "schedule_mismatch_rows": [2, 4, 6, 8],
    }


def validate(
    raw_rows,
    groups,
    normal_rows,
    mutation_rows,
    metadata,
    log,
    raw_digest,
    mutation_digest,
    metadata_digest,
    canonicalizer_digest,
):
    expected_mutation = [[bits(value) for value in group[1][0]] for group in groups]
    if mutation_rows != expected_mutation:
        raise ValueError("projection mutation output is not the raw keep-first projection")
    if mutation_rows == normal_rows:
        raise ValueError("projection mutation is a no-op keep-first control")
    group_sizes = [len(group[1]) for group in groups]
    raw_time_bits = [bits(row[0]) for row in raw_rows]
    canonical_time_bits = [row[0] for row in mutation_rows]
    if group_sizes != GROUP_SIZES or canonical_time_bits != TIME_BITS:
        raise ValueError("projection mutation changed groups or canonical timestamp bits")
    expected_metadata = [
        "selection=first",
        f"raw_rows={len(raw_rows)}",
        f"canonical_rows={len(mutation_rows)}",
        "group_sizes=" + ",".join(map(str, group_sizes)),
        "raw_time_bits=" + ",".join(raw_time_bits),
        "canonical_time_bits=" + ",".join(canonical_time_bits),
    ]
    if metadata.splitlines() != expected_metadata:
        raise ValueError("projection mutation inspection metadata is not reproducible")
    expected_inputs = [
        [TIME_BITS[index], "c000000000000000", "3ff4000000000000",
         "4000000000000000", "400a000000000000", U_BITS[index]]
        for index in range(10)
    ]
    mismatches = [
        index for index, (actual, expected) in enumerate(zip(mutation_rows, expected_inputs))
        if actual[:6] != expected
    ]
    if mismatches != [2, 4, 6, 8]:
        raise ValueError("projection mutation schedule mismatches are not the pinned event rows")
    expected_log = [
        "projection_mutation=contiguous equal-time selection changed from last to first",
        "execution_path=canonicalize-first-inspect",
        "mutated_input=line-run-a.raw.csv",
        f"mutated_input_sha256={raw_digest}",
        "mutated_output=projection-keep-first.canonical.csv",
        f"mutated_output_sha256={mutation_digest}",
        "mutated_metadata=projection-keep-first.metadata",
        f"mutated_metadata_sha256={metadata_digest}",
        f"mutated_raw_rows={len(raw_rows)}",
        f"mutated_canonical_rows={len(mutation_rows)}",
        "mutated_group_sizes=" + ",".join(map(str, group_sizes)),
        "mutated_raw_time_bits=" + ",".join(raw_time_bits),
        "mutated_canonical_time_bits=" + ",".join(canonical_time_bits),
        "mutated_schedule_result=FAIL",
        "mutated_schedule_mismatch_rows=2,4,6,8",
        "mutated_schedule_first_mismatch_row=2",
        "mutated_schedule_first_mismatch_time_bits=404e000000000eff",
        "mutated_output_differs_from_keep_last=PASS",
        "mutated_grouping_result=PASS",
        "mutated_timestamp_bits_result=PASS",
        "explicit_keep_first_execution=PASS",
        f"executed_canonicalizer_sha256={canonicalizer_digest}",
    ]
    if log.splitlines() != expected_log:
        raise ValueError("projection mutation execution log is not reproducible")
