"""Inventory, scheduling and failure controls for the hosted libtest partitioner."""
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
ROOT_NAMES = {
    "runner::workspace_command::fixture_a",
    "runner::task_engine::fixture_b",
    "runner::tests::runner_core_tests::fixture_contract",
    "cli::fixture_c",
}
LISTS = {
    (): ROOT_NAMES,
    ("runner::workspace_command::",): {"runner::workspace_command::fixture_a"},
    ("runner::tests::runner_core_tests::",): {"runner::tests::runner_core_tests::fixture_contract"},
    ("runner::", "--skip", "runner::workspace_command::", "--skip", "runner::tests::runner_core_tests::"): {"runner::task_engine::fixture_b"},
    ("--skip", "runner::"): {"cli::fixture_c"},
}
PLAN = {"/synthetic/lib": "library", "/synthetic/cli": "cli",
        "/synthetic/gateway": "gateway", "/synthetic/future": "rest"}


def listing(args, **kwargs):
    names = LISTS[tuple(args[1:-3])]
    return "\n".join(f"{name}: test" for name in sorted(names)) + "\n"


class PartitionTests(unittest.TestCase):
    def runner(self, suite, binary, exit_code=0, inventory_only="0"):
        with patch.dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(PLAN),
                        EFFIGY_CI_INVENTORY_ONLY=inventory_only), \
                patch("subprocess.check_output", side_effect=listing), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=exit_code)) as run, \
                patch("sys.stdout", new_callable=io.StringIO):
            code = MODULE["run_binary"](suite, binary, [])
        return code, run.call_args_list

    def test_disjoint_complete_routing_and_original_thread_defaults(self):
        calls = []
        suites = ["library-workspace", "library-core", "library-runner", "library-other", "cli", "gateway", "rest"]
        for suite in suites:
            for binary in PLAN:
                code, executed = self.runner(suite, binary)
                self.assertEqual(code, 0)
                calls.extend(executed)
        self.assertEqual(len(calls), 7)
        library_calls = [c.args[0][1:] for c in calls if c.args[0][0] == "/synthetic/lib"]
        self.assertCountEqual(library_calls, [list(k) for k in LISTS if k])
        self.assertTrue(all(not any("test-threads" in arg for arg in c.args[0]) for c in calls))

    def test_runner_propagates_test_failure(self):
        self.assertEqual(self.runner("library-runner", "/synthetic/lib", 7)[0], 7)

    def test_unknown_binary_fails_closed(self):
        with self.assertRaises(KeyError):
            self.runner("cli", "/synthetic/unknown")

    def test_cargo_generated_doc_binary_runs_whole_in_rest(self):
        code, calls = self.runner("rest", "/synthetic/rustdoc-test", 7)
        self.assertEqual(code, 7)
        self.assertEqual(calls[0].args[0], ["/synthetic/rustdoc-test"])

    def test_inventory_only_does_not_execute_tests(self):
        self.assertEqual(self.runner("library-runner", "/synthetic/lib", inventory_only="1"), (0, []))

    def inventory(self, lists):
        def output(args, **kwargs):
            return "\n".join(f"{n}: test" for n in lists[tuple(args[1:-3])])
        with patch("subprocess.check_output", side_effect=output), \
                patch("sys.stdout", new_callable=io.StringIO):
            return MODULE["verify_library_inventory"]("/synthetic/lib", [])

    def test_inventory_rejects_a_missing_test(self):
        lists = {k: set(v) for k, v in LISTS.items()}
        lists[("--skip", "runner::")] = set()
        with self.assertRaisesRegex(RuntimeError, "missing/overlapping"):
            self.inventory(lists)

    def test_inventory_rejects_an_overlapping_test(self):
        lists = {k: set(v) for k, v in LISTS.items()}
        lists[("runner::workspace_command::",)].add("runner::task_engine::fixture_b")
        with self.assertRaisesRegex(RuntimeError, "missing/overlapping"):
            self.inventory(lists)

    def test_future_test_is_checked_and_assigned_automatically(self):
        lists = {k: set(v) for k, v in LISTS.items()}
        lists[()].add("future_module::new_test")
        lists[("--skip", "runner::")].add("future_module::new_test")
        self.assertIn("future_module::new_test", self.inventory(lists)["library-other"])

    def test_inventory_rejects_wrong_family_even_when_union_is_complete(self):
        lists = {k: set(v) for k, v in LISTS.items()}
        lists[("--skip", "runner::")], lists[("runner::workspace_command::",)] = \
            lists[("runner::workspace_command::",)], lists[("--skip", "runner::")]
        with self.assertRaisesRegex(RuntimeError, "disagrees with family"):
            self.inventory(lists)

    def build(self, build_exit=0, test_exit=0, suite="rest"):
        artifacts = []
        for name, kind in [("effigy", "lib"), ("effigy", "bin"),
                           ("cli_output_tests", "test"), ("gateway_private", "test"),
                           ("new_future_target", "test")]:
            artifacts.append({"reason": "compiler-artifact", "profile": {"test": True},
                              "target": {"name": name, "kind": [kind]},
                              "executable": f"/synthetic/{name}-{kind}"})
        build = SimpleNamespace(stdout=io.StringIO("".join(json.dumps(a) + "\n" for a in artifacts)),
                                wait=lambda: build_exit, returncode=build_exit)
        with patch("subprocess.Popen", return_value=build) as compile_call, \
                patch("subprocess.check_output", return_value="host: synthetic-host\n"), \
                patch("subprocess.run", return_value=SimpleNamespace(returncode=test_exit)) as run:
            code = MODULE["main"]([suite])
        return code, compile_call.call_args, run.call_args_list

    def test_build_failure_stops_execution(self):
        code, _, calls = self.build(build_exit=7)
        self.assertEqual((code, calls), (7, []))

    def test_workspace_features_and_future_targets_are_preserved(self):
        code, compile_call, calls = self.build()
        self.assertEqual(code, 0)
        self.assertIn("--workspace", compile_call.args[0])
        self.assertIn("--workspace", calls[0].args[0])
        env = calls[0].kwargs["env"]
        plan = json.loads(env["EFFIGY_CI_TEST_PARTITIONS"])
        self.assertEqual(len(plan), 5)
        self.assertEqual(plan["/synthetic/new_future_target-test"], "rest")
        self.assertEqual(env["EFFIGY_CI_INVENTORY_ONLY"], "0")
        self.assertEqual(compile_call.args[0][2:3], ["--workspace"])
        self.assertNotIn("--tests", compile_call.args[0])
        self.assertNotIn("--tests", calls[0].args[0])

    def test_library_build_supplies_cli_without_unassigned_integration_binaries(self):
        code, compile_call, calls = self.build(suite="library-workspace")
        self.assertEqual(code, 0)
        self.assertEqual(compile_call.args[0][2:6], ["--workspace", "--lib", "--test", "bootstrap_cli_tests"])
        self.assertEqual(calls[0].args[0][2:6], compile_call.args[0][2:6])
        self.assertEqual(len(calls), 1)

    def test_failure_is_retained_with_default_examples_and_doc_tests(self):
        code, _, calls = self.build(test_exit=1)
        self.assertEqual(code, 1)
        self.assertEqual(len(calls), 1)
        self.assertIn("--no-fail-fast", calls[0].args[0])
        self.assertNotIn("--tests", calls[0].args[0])
        self.assertNotIn("--lib", calls[0].args[0])

    def test_inventory_mode_is_explicit_and_cannot_run_tests(self):
        _, _, calls = self.build(suite="inventory")
        self.assertEqual(calls[0].kwargs["env"]["EFFIGY_CI_INVENTORY_ONLY"], "1")
        self.assertEqual(len(calls), 1)


if __name__ == "__main__":
    unittest.main()
