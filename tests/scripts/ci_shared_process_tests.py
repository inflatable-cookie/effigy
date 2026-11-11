"""Selection, inventory and failure controls for hosted ambient replay."""
import io
import json
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/ci-shared-process.py"
MODULE = runpy.run_path(str(SCRIPT), run_name="ci_partition_tests")
AMBIENT = MODULE["AMBIENT_FILTERS"]
SKIPS = tuple(arg for f in AMBIENT for arg in ("--skip", f))
REMAINDER = {"runner::unrelated_test", "future_module::new_test"}
LISTS = {(): {*AMBIENT, *REMAINDER}, tuple(AMBIENT): set(AMBIENT), SKIPS: REMAINDER}
PLAN = {"/synthetic/lib": "library-ambient", "/synthetic/gateway": "gateway",
        "/synthetic/cli": None, "/synthetic/future": None}


def listing(args, **kwargs):
    return "\n".join(f"{name}: test" for name in LISTS[tuple(args[1:-3])]) + "\n"


class PartitionTests(unittest.TestCase):
    def runner(self, suite, binary, exit_code=0, inventory_only="0"):
        with patch.dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(PLAN),
                        EFFIGY_CI_INVENTORY_ONLY=inventory_only), \
                patch("subprocess.check_output", side_effect=listing), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=exit_code)) as run, \
                patch("sys.stdout", new_callable=io.StringIO):
            code = MODULE["run_binary"](suite, binary, [])
        return code, run.call_args_list

    def test_only_ambient_cohorts_replay_with_original_threads_and_failure_status(self):
        calls = []
        for suite in MODULE["SUITES"]:
            for binary in PLAN:
                code, executed = self.runner(suite, binary, 7)
                self.assertEqual(code, 7 if executed else 0)
                calls.extend(c.args[0] for c in executed)
        self.assertCountEqual(calls, [["/synthetic/lib", *AMBIENT], ["/synthetic/gateway"]])

    def test_unknown_binary_fails_closed(self):
        with self.assertRaises(KeyError):
            self.runner("gateway", "/synthetic/unknown")

    def test_inventory_lists_every_binary_without_running_tests(self):
        for binary in PLAN:
            self.assertEqual(self.runner("inventory", binary, inventory_only="1"), (0, []))

    def test_inventory_rejects_missing_overlapping_misrouted_stale_and_duplicate_names(self):
        for defect in ("missing", "overlapping", "misrouted", "stale", "duplicate"):
            with self.subTest(defect=defect):
                lists = {k: list(v) for k, v in LISTS.items()}
                if defect == "missing":
                    lists[SKIPS] = []
                elif defect == "overlapping":
                    lists[SKIPS].append(AMBIENT[0])
                elif defect == "misrouted":
                    lists[tuple(AMBIENT)], lists[SKIPS] = lists[SKIPS], lists[tuple(AMBIENT)]
                elif defect == "stale":
                    lists[()].remove(AMBIENT[0])
                    lists[tuple(AMBIENT)].remove(AMBIENT[0])
                else:
                    lists[()].append(AMBIENT[0])
                def output(args, **kwargs):
                    return "\n".join(f"{n}: test" for n in lists[tuple(args[1:-3])])
                with patch("subprocess.check_output", side_effect=output), \
                        self.assertRaises(RuntimeError):
                    MODULE["verify_library_inventory"]("/synthetic/lib", [])

    def build(self, build_exit=0, test_exit=0, suite="gateway"):
        artifacts = [{"reason": "compiler-artifact", "profile": {"test": True},
                      "target": {"name": name, "kind": [kind]},
                      "executable": f"/synthetic/{name}-{kind}"}
                     for name, kind in [("effigy", "lib"), ("effigy", "bin"),
                                        ("gateway_private", "test"), ("new_future_target", "test")]]
        build = SimpleNamespace(stdout=io.StringIO("".join(json.dumps(a) + "\n" for a in artifacts)),
                                wait=lambda: build_exit, returncode=build_exit)
        with patch("subprocess.Popen", return_value=build) as compile_call, \
                patch("subprocess.check_output", return_value="host: synthetic-host\n"), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=test_exit)) as run:
            code = MODULE["main"]([suite])
        return code, compile_call.call_args, run.call_args_list

    def test_build_failure_stops_execution(self):
        self.assertEqual(self.build(build_exit=7)[::2], (7, []))

    def test_identical_workspace_selection_includes_examples_and_future_targets(self):
        for suite in [*MODULE["SUITES"], "inventory"]:
            with self.subTest(suite=suite):
                code, compile_call, calls = self.build(test_exit=7, suite=suite)
                self.assertEqual(code, 1)
                self.assertEqual(compile_call.args[0][2:4], ["--workspace", "--all-targets"])
                self.assertEqual(calls[0].args[0][2:4], compile_call.args[0][2:4])
                self.assertIn("--no-fail-fast", calls[0].args[0])
                env = calls[0].kwargs["env"]
                self.assertEqual(json.loads(env["EFFIGY_CI_TEST_PARTITIONS"]), {
                    "/synthetic/effigy-lib": "library-ambient", "/synthetic/effigy-bin": None,
                    "/synthetic/gateway_private-test": "gateway", "/synthetic/new_future_target-test": None})
                self.assertEqual(env["EFFIGY_CI_INVENTORY_ONLY"], "1" if suite == "inventory" else "0")


if __name__ == "__main__":
    unittest.main()
