#!/usr/bin/env python3
"""Hostile controls for ci_ok.py: every way a skipped or failed job could read as green."""

import os
from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_ok  # noqa: E402

LANES = {"determinism-matrix": "core", "leverage-lane": "leverage", "clippy": "core"}
# The README lane (issue #503): `facade-contract` is the one job filtered on `readme`.
README_LANES = {**LANES, "facade-contract": "readme"}


def outputs(core="true", leverage="true", gate_scripts="false", docs_only="false", readme=None):
    # `core` implies `readme`, as the classifier emits it; a case that needs another value says so.
    return {"core": core, "leverage": leverage, "readme": core if readme is None else readme,
            "gate_scripts": gate_scripts, "docs_only": docs_only}


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
                     leverage_lane="success", clippy="success")
        self.assertIn("clippy: success", ci_ok.check(data, LANES))

    def test_leverage_only_skips_are_accepted(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped")
        report = ci_ok.check(data, LANES)
        self.assertIn("determinism-matrix: skipped by the lane filter (core=false)", report)

    def test_docs_only_skips_every_filtered_job(self):
        data = needs(outputs(core="false", leverage="false", docs_only="true"),
                     determinism_matrix="skipped", leverage_lane="skipped", clippy="skipped")
        ci_ok.check(data, LANES)

    def test_skip_while_lane_ran_fails(self):
        data = needs(outputs(), determinism_matrix="skipped", leverage_lane="success",
                     clippy="success")
        self.assert_refused(data, r"determinism-matrix.*skipped while core=true")

    def test_leverage_lane_skipped_while_leverage_ran_fails(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="skipped", clippy="skipped")
        self.assert_refused(data, r"leverage-lane.*skipped while leverage=true")

    def test_gate_script_change_admits_no_skip(self):
        # gate_scripts implies core and gates nothing of its own (issue #502): a gate-script
        # change runs the whole determinism lane, so a determinism job skipped under it fails.
        data = needs(outputs(gate_scripts="true"), determinism_matrix="success",
                     leverage_lane="success", clippy="skipped")
        self.assert_refused(data, r"'clippy': 'skipped while core=true'")

    def test_readme_only_runs_the_readme_job_and_skips_both_other_lanes(self):
        data = needs(outputs(core="false", leverage="false", readme="true"),
                     determinism_matrix="skipped", leverage_lane="skipped", clippy="skipped",
                     facade_contract="success")
        report = ci_ok.check(data, README_LANES)
        self.assertIn("facade-contract: success", report)
        self.assertIn("determinism-matrix: skipped by the lane filter (core=false)", report)
        self.assertIn("leverage-lane: skipped by the lane filter (leverage=false)", report)

    def test_readme_with_leverage_skips_only_the_determinism_lane(self):
        data = needs(outputs(core="false", readme="true"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped", facade_contract="success")
        ci_ok.check(data, README_LANES)

    def test_readme_job_is_skipped_when_the_readme_did_not_change(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped", facade_contract="skipped")
        self.assertIn("facade-contract: skipped by the lane filter (readme=false)",
                      ci_ok.check(data, README_LANES))

    def test_readme_job_skipped_while_the_readme_lane_ran_fails(self):
        for lane_outputs in (outputs(core="false", leverage="false", readme="true"), outputs()):
            data = needs(lane_outputs, determinism_matrix="skipped", leverage_lane="skipped",
                         clippy="skipped", facade_contract="skipped")
            self.assert_refused(data, r"'facade-contract': 'skipped while readme=true'",
                                README_LANES)

    def test_broken_quickstart_fails_a_readme_only_change(self):
        # The acceptance case of issue #503: the README lane ran and its job failed.
        for result in ("failure", "cancelled"):
            data = needs(outputs(core="false", leverage="false", readme="true"),
                         determinism_matrix="skipped", leverage_lane="skipped", clippy="skipped",
                         facade_contract=result)
            self.assert_refused(data, f"'facade-contract': '{result}'", README_LANES)

    def test_readme_job_missing_from_the_lane_map_cannot_skip(self):
        # A job filtered on `readme` but absent from JOB_LANES is an unfiltered job to this
        # check: its skip fails, whatever the lane output says.
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped", facade_contract="skipped")
        self.assert_refused(data, r"'facade-contract': 'skipped'")

    def test_readme_output_is_required(self):
        for value in (None, "", "yes"):
            broken = outputs(core="false")
            if value is None:
                del broken["readme"]
            else:
                broken["readme"] = value
            data = needs(broken, determinism_matrix="skipped", leverage_lane="success",
                         clippy="skipped", facade_contract="skipped")
            self.assert_refused(data, "missing or not true/false.*readme", README_LANES)

    def test_gate_scripts_output_is_still_required(self):
        for value in (None, "", "yes"):
            broken = outputs()
            if value is None:
                del broken["gate_scripts"]
            else:
                broken["gate_scripts"] = value
            data = needs(broken, determinism_matrix="success", leverage_lane="success",
                         clippy="success")
            self.assert_refused(data, "missing or not true/false.*gate_scripts")

    def test_non_object_needs_fails(self):
        for data in ([], [{"result": "success"}], "success", 1, None, True):
            with self.subTest(data=data):
                self.assert_refused(data, "not an object of job results")

    def test_skip_of_unfiltered_job_fails(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     clippy="success")
        data["docs"] = {"result": "skipped"}
        self.assert_refused(data, r"'docs': 'skipped'")

    def test_failure_and_cancellation_fail_even_when_filtered(self):
        for result in ("failure", "cancelled"):
            data = needs(outputs(core="false"), determinism_matrix=result,
                         leverage_lane="success", clippy="skipped")
            self.assert_refused(data, f"determinism-matrix': '{result}'")

    def test_missing_result_fails(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     clippy="success")
        data["docs"] = {}
        self.assert_refused(data, "'docs': 'None'")

    def test_failed_classifier_admits_no_skip(self):
        data = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped")
        data["changes"]["result"] = "failure"
        self.assert_refused(data, "changes job did not succeed")
        del data["changes"]
        self.assert_refused(data, "changes job did not succeed")

    def test_missing_or_malformed_outputs_fail(self):
        for broken in ({}, {**outputs(), "core": ""}, {**outputs(), "leverage": "False"}):
            data = needs(broken, determinism_matrix="skipped", leverage_lane="skipped",
                         clippy="skipped")
            self.assert_refused(data, "missing or not true/false")

    def test_inconsistent_outputs_fail(self):
        cases = [
            (outputs(core="false", gate_scripts="true"), "gate_scripts=true requires core=true"),
            (outputs(leverage="false"), "core=true requires leverage=true"),
            (outputs(readme="false"), "core=true requires readme=true"),
            (outputs(core="false", docs_only="true"), "docs_only=true contradicts"),
            (outputs(core="false", leverage="false", readme="true", docs_only="true"),
             "docs_only=true contradicts"),
        ]
        for lane_outputs, message in cases:
            data = needs(lane_outputs, determinism_matrix="success", leverage_lane="success",
                         clippy="success")
            self.assert_refused(data, message)

    def test_lane_map_must_name_needed_jobs(self):
        data = needs(outputs(), determinism_matrix="success", leverage_lane="success",
                     clippy="success")
        self.assert_refused(data, "names jobs CI OK does not need",
                            {**LANES, "renamed-job": "core"})

    def test_empty_needs_fails(self):
        self.assert_refused({}, "no gating job results reported")


class LaneMap(unittest.TestCase):
    def test_parses_pairs(self):
        self.assertEqual(ci_ok.parse_lanes(" a=core\n b=leverage c=core d=readme "),
                         {"a": "core", "b": "leverage", "c": "core", "d": "readme"})

    def test_refuses_malformed_unknown_duplicate_and_empty(self):
        for text in ("a", "a=", "=core", "a=docs_only", "a=nightly", "a=core a=core", "", "  "):
            with self.assertRaises(ci_ok.CheckError, msg=text):
                ci_ok.parse_lanes(text)

    def test_gate_scripts_is_not_a_filter_lane(self):
        # `gate (light)` was the one job filtered on gate_scripts; with it retired (issue #502) no
        # job may be skipped on that output, so a lane map that names it is refused outright.
        for text in ("gate=gate_scripts", "a=core gate=gate_scripts"):
            with self.assertRaisesRegex(ci_ok.CheckError,
                                        "malformed JOB_LANES entry: 'gate=gate_scripts'"):
                ci_ok.parse_lanes(text)


class Entrypoint(unittest.TestCase):
    def run_script(self, env):
        return subprocess.run([sys.executable, str(Path(ci_ok.__file__))], env=env,
                              capture_output=True, text=True, check=False)

    def test_exit_status_follows_the_check(self):
        import json
        good = needs(outputs(core="false"), determinism_matrix="skipped",
                     leverage_lane="success", clippy="skipped")
        env = {**os.environ, "NEEDS_JSON": json.dumps(good),
               "JOB_LANES": "determinism-matrix=core leverage-lane=leverage clippy=core"}
        self.assertEqual(self.run_script(env).returncode, 0)
        bad = needs(outputs(), determinism_matrix="skipped", leverage_lane="success",
                    clippy="success")
        self.assertEqual(self.run_script({**env, "NEEDS_JSON": json.dumps(bad)}).returncode, 1)
        readme_env = {**env, "JOB_LANES": env["JOB_LANES"] + " facade-contract=readme"}
        readme_only = needs(outputs(core="false", leverage="false", readme="true"),
                            determinism_matrix="skipped", leverage_lane="skipped",
                            clippy="skipped", facade_contract="success")
        self.assertEqual(
            self.run_script({**readme_env, "NEEDS_JSON": json.dumps(readme_only)}).returncode, 0)
        readme_only["facade-contract"] = {"result": "failure"}
        self.assertEqual(
            self.run_script({**readme_env, "NEEDS_JSON": json.dumps(readme_only)}).returncode, 1)
        retired = {**env, "JOB_LANES": env["JOB_LANES"] + " gate=gate_scripts"}
        self.assertEqual(self.run_script(retired).returncode, 1)
        self.assertEqual(self.run_script({**env, "NEEDS_JSON": "not json"}).returncode, 1)
        for text in ("[]", "null", '"success"', "[{\"result\": \"success\"}]"):
            result = self.run_script({**env, "NEEDS_JSON": text})
            self.assertEqual(result.returncode, 1, text)
            self.assertIn("CI OK: FAIL: NEEDS_JSON is not an object", result.stderr, text)
            self.assertNotIn("Traceback", result.stderr, text)
        missing = {k: v for k, v in env.items() if k != "NEEDS_JSON"}
        self.assertEqual(self.run_script(missing).returncode, 1)
        self.assertEqual(self.run_script({**env, "JOB_LANES": ""}).returncode, 1)


if __name__ == "__main__":
    unittest.main()
