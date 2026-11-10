"""Coverage and failure controls for the hosted whole-binary partitioner."""
import io
import json
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/ci-shared-process.py"
PLAN = {"/synthetic/lib": "library", "/synthetic/cli": "cli",
        "/synthetic/gateway": "gateway", "/synthetic/future": "rest"}


class PartitionTests(unittest.TestCase):
    def runner(self, suite, binary, exit_code=0):
        with patch("sys.argv", [str(SCRIPT), "--run", suite, binary]), \
                patch.dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(PLAN)), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=exit_code)) as run, \
                patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as result:
                runpy.run_path(str(SCRIPT), run_name="__main__")
        return result.exception.code, run.call_args_list

    def test_disjoint_complete_whole_binary_cohorts(self):
        calls = []
        for suite in ["library", "cli", "gateway", "rest"]:
            for binary in PLAN:
                code, executed = self.runner(suite, binary)
                self.assertEqual(code, 0)
                calls.extend(executed)
        self.assertEqual(len(calls), 4)
        self.assertEqual({c.args[0][0] for c in calls}, set(PLAN))
        for call in calls:
            args = call.args[0]
            self.assertEqual(args[1:], [])

    def test_runner_propagates_test_failure(self):
        self.assertEqual(self.runner("library", "/synthetic/lib", 7)[0], 7)

    def test_unknown_binary_fails_closed(self):
        with self.assertRaises(KeyError):
            self.runner("rest", "/synthetic/unknown")

    def build(self, build_exit=0, test_exit=0):
        artifacts = []
        for name, kind in [("effigy", "lib"), ("effigy", "bin"),
                           ("cli_output_tests", "test"), ("gateway_private", "test"),
                           ("new_future_target", "test")]:
            artifacts.append({"reason": "compiler-artifact", "profile": {"test": True},
                              "target": {"name": name, "kind": [kind]},
                              "executable": f"/synthetic/{name}-{kind}"})
        build = SimpleNamespace(stdout=io.StringIO("".join(json.dumps(a) + "\n" for a in artifacts)),
                                wait=lambda: build_exit, returncode=build_exit)
        with patch("sys.argv", [str(SCRIPT), "rest"]), patch("subprocess.Popen", return_value=build) as compile_call, \
                patch("subprocess.check_output", return_value="host: synthetic-host\n"), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=test_exit)) as run, \
                patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as result:
                runpy.run_path(str(SCRIPT), run_name="__main__")
        return result.exception.code, compile_call.call_args, run.call_args_list

    def test_build_failure_stops_execution(self):
        code, _, calls = self.build(build_exit=7)
        self.assertEqual((code, calls), (7, []))

    def test_workspace_features_and_future_targets_are_preserved(self):
        code, compile_call, calls = self.build()
        self.assertEqual(code, 0)
        self.assertIn("--workspace", compile_call.args[0])
        self.assertIn("--workspace", calls[0].args[0])
        plan = json.loads(calls[0].kwargs["env"]["EFFIGY_CI_TEST_PARTITIONS"])
        self.assertEqual(len(plan), 5)
        self.assertEqual(plan["/synthetic/new_future_target-test"], "rest")
        self.assertIn("--doc", calls[1].args[0])

    def test_failure_is_retained_and_doc_tests_still_run(self):
        code, _, calls = self.build(test_exit=1)
        self.assertEqual(code, 1)
        self.assertEqual(len(calls), 2)


if __name__ == "__main__":
    unittest.main()
