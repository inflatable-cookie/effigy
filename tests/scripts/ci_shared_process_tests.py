"""Coverage and failure controls for the hosted whole-binary partitioner."""
import io
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/ci-shared-process.py"


class PartitionTests(unittest.TestCase):
    def execute(self, suite, *, build_exit=0, failure=None):
        artifacts = []
        for name, kind in [("effigy", "lib"), ("effigy", "bin"),
                           ("cli_output_tests", "test"), ("gateway_private", "test"),
                           ("new_future_target", "test")]:
            artifacts.append({"reason": "compiler-artifact", "profile": {"test": True},
                              "target": {"name": name, "kind": [kind]},
                              "package_id": name, "executable": f"/synthetic/{name}-{kind}"})
        build = SimpleNamespace(stdout=io.StringIO("".join(json.dumps(a) + "\n" for a in artifacts)),
                                wait=lambda: build_exit, returncode=build_exit)
        calls = []

        def run(args):
            calls.append(args)
            return SimpleNamespace(returncode=int(failure is not None and failure in args))

        with patch("sys.argv", [str(SCRIPT), suite]), patch("subprocess.Popen", return_value=build), \
                patch("subprocess.run", side_effect=run), patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as result:
                runpy.run_path(str(SCRIPT), run_name="__main__")
        return result.exception.code, calls

    def test_every_binary_runs_once_and_keeps_its_entire_libtest_cohort(self):
        all_calls = []
        for suite in ["library", "cli", "gateway", "rest"]:
            code, calls = self.execute(suite)
            self.assertEqual(code, 0)
            all_calls.extend(calls)
        binaries = [c for c in all_calls if "--doc" not in c]
        self.assertEqual(len(binaries), 5)
        self.assertEqual(len({tuple(c) for c in binaries}), 5)
        self.assertTrue(all("--" not in c and "--test-threads" not in c for c in binaries))
        self.assertEqual([c for c in all_calls if "--doc" in c],
                         [["cargo", "test", "--workspace", "--doc", "--no-fail-fast"]])

    def test_build_failure_stops_execution(self):
        self.assertEqual(self.execute("rest", build_exit=7), (7, []))

    def test_failure_is_retained_and_remaining_binaries_still_run(self):
        code, calls = self.execute("rest", failure="--bin")
        self.assertEqual(code, 1)
        self.assertEqual(len(calls), 3)


if __name__ == "__main__":
    unittest.main()
