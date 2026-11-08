#!/usr/bin/env python3
"""Hostile controls for ci_ok.py: every way a skipped or failed job could read as green."""

import os
from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_ok  # noqa: E402

LANES = {"determinism-matrix": "core", "leverage-lane": "leverage", "gate": "gate_scripts"}


def outputs(core="true", leverage="true", gate_scripts="false", docs_only="false"):
    return {"core": core, "leverage": leverage, "gate_scripts": gate_scripts,
            "docs_only": docs_only}


def needs(lane_outputs, **results):
    data = {"changes": {"result": "success", "outputs": lane_outputs}, "docs": {"result": "success"}}
    for job, result in results.items():
        data[job.replace("_", "-")] = {"result": result}
    return data


class ResultCheck(unittest.TestCase):
    def assert_refused(self, data, message, lanes=LANES):
        with self.assertRaisesRegex(ci_ok.CheckError, message):
            ci_ok.check(data, lanes)

    def test_full_lane_all_green_passes(self):
        data = needs(outputs(gate_scripts="true"), determinism_matrix="success",
                     leverage_lane="success", gate="success")
        self.assertIn("gate: success", ci_ok.check(data, LANES))

    def test_leverage_only_skips_are_accepted(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", gate="skipped")
        report = ci_ok.check(data, LANES)
        self.assertIn("determinism-matrix: skipped by the lane filter (core=false)", report)

    def test_docs_only_skips_every_filtered_job(self):
        data = needs(outputs(core="false", leverage="false", docs_only="true"),
                     determinism_matrix="skipped", leverage_lane="skipped", gate="skipped")
        ci_ok.check(data, LANES)

    def test_skip_while_lane_ran_fails(self):
        data = needs(outputs(), determinism_matrix="skipped", leverage_lane="success",
                     gate="skipped")
        self.assert_refused(data, r"determinism-matrix.*skipped while core=true")

    def test_skip_of_unfiltered_job_fails(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     gate="skipped")
        data["docs"] = {"result": "skipped"}
        self.assert_refused(data, r"'docs': 'skipped'")

    def test_failure_and_cancellation_fail_even_when_filtered(self):
        for result in ("failure", "cancelled"):
            data = needs(outputs(core="false"), determinism_matrix=result,
                         leverage_lane="success", gate="skipped")
            self.assert_refused(data, f"determinism-matrix': '{result}'")

    def test_missing_result_fails(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     gate="skipped")
        data["docs"] = {}
        self.assert_refused(data, "'docs': 'None'")

    def test_failed_classifier_admits_no_skip(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", gate="skipped")
        data["changes"]["result"] = "failure"
        self.assert_refused(data, "changes job did not succeed")
        del data["changes"]
        self.assert_refused(data, "changes job did not succeed")

    def test_missing_or_malformed_outputs_fail(self):
        for broken in ({}, {**outputs(), "core": ""}, {**outputs(), "leverage": "False"}):
            data = needs(broken, determinism_matrix="skipped", leverage_lane="skipped",
                         gate="skipped")
            self.assert_refused(data, "missing or not true/false")

    def test_inconsistent_outputs_fail(self):
        cases = [
            (outputs(core="false", gate_scripts="true"), "gate_scripts=true requires core=true"),
            (outputs(leverage="false"), "core=true requires leverage=true"),
            (outputs(core="false", docs_only="true"), "docs_only=true contradicts"),
        ]
        for lane_outputs, message in cases:
            data = needs(lane_outputs, determinism_matrix="success", leverage_lane="success",
                         gate="success")
            self.assert_refused(data, message)

    def test_lane_map_must_name_needed_jobs(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     gate="success")
        self.assert_refused(data, "names jobs CI OK does not need",
                            {**LANES, "renamed-job": "core"})

    def test_empty_needs_fails(self):
        self.assert_refused({}, "no gating job results reported")


class LaneMap(unittest.TestCase):
    def test_parses_pairs(self):
        self.assertEqual(ci_ok.parse_lanes(" a=core\n b=leverage c=gate_scripts "),
                         {"a": "core", "b": "leverage", "c": "gate_scripts"})

    def test_refuses_malformed_unknown_duplicate_and_empty(self):
        for text in ("a", "a=", "=core", "a=docs_only", "a=nightly", "a=core a=core", "", "  "):
            with self.assertRaises(ci_ok.CheckError, msg=text):
                ci_ok.parse_lanes(text)


class Entrypoint(unittest.TestCase):
    def run_script(self, env):
        return subprocess.run([sys.executable, str(Path(ci_ok.__file__))], env=env,
                              capture_output=True, text=True, check=False)

    def test_exit_status_follows_the_check(self):
        import json
        good = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", gate="skipped")
        env = {**os.environ, "NEEDS_JSON": json.dumps(good),
               "JOB_LANES": "determinism-matrix=core leverage-lane=leverage gate=gate_scripts"}
        self.assertEqual(self.run_script(env).returncode, 0)
        bad = needs(outputs(), determinism_matrix="skipped", leverage_lane="success",
                    gate="skipped")
        self.assertEqual(self.run_script({**env, "NEEDS_JSON": json.dumps(bad)}).returncode, 1)
        self.assertEqual(self.run_script({**env, "NEEDS_JSON": "not json"}).returncode, 1)
        missing = {k: v for k, v in env.items() if k != "NEEDS_JSON"}
        self.assertEqual(self.run_script(missing).returncode, 1)
        self.assertEqual(self.run_script({**env, "JOB_LANES": ""}).returncode, 1)


if __name__ == "__main__":
    unittest.main()
