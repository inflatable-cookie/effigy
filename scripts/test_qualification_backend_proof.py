#!/usr/bin/env python3
"""Self-test for scripts/qualification_backend_proof.py.

The fixture is a fake candidate executable that answers the ordinary Effigy
command surface the proof uses, so these cases prove the proof's own flow and
refusals with no container engine, checkout or native effect.
"""

from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import qualification_backend_proof as proof  # noqa: E402

FAKE = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

state = Path(os.environ["FAKE_STATE"])
running = state.read_text().strip() == "running"
backend = os.environ.get("FAKE_BACKEND", "containerd")
argv = sys.argv[1:]

expected = os.environ.get("FAKE_EXPECTED_BACKEND", "colima")
actual = os.environ.get("EFFIGY_COMPOSE_BACKEND")
if actual != expected:
    print(json.dumps({"schema": "effigy.command.v1", "ok": False,
                      "error": {"message": "EFFIGY_COMPOSE_BACKEND is "
                      + repr(actual) + ", expected " + repr(expected)}}))
    sys.exit(9)


def envelope(result):
    print(json.dumps({"schema": "effigy.command.v1", "schema_version": 1, "ok": True,
                      "result": result}))


def exec_envelope(stdout, code=0, ok=True, stderr=""):
    envelope({"schema": "effigy.exec.v1", "ok": ok, "code": code,
              "stdout": stdout, "stderr": stderr})


def guest_envelope(result):
    return json.dumps({"schema": "effigy.command.v1", "schema_version": 1,
                       "ok": True, "result": result})


if argv == ["--json", "version"]:
    envelope({"schema": "effigy.version.v1", "schema_version": 1, "ok": True,
              "version": "0.15.0"})
elif argv[:2] == ["container", "status"] and "--global" in argv:
    live = ["web"] if running else []
    envelope({"schema": "effigy.container.status-global.v1", "ok": True,
              "environments": [{"container": name, "project_name": "p",
                                "services": [{"container_name": name + "-1"}]}
                               for name in live]})
elif argv[:3] == ["container", "volume", "list"]:
    live = ["data"] if running and state.read_text().strip() != "retired" else []
    envelope({"schema": "effigy.container.volume-list.v1", "ok": True,
              "volumes": [{"name": name, "backend": "colima"} for name in live]})
elif argv == ["container", "up", "--detach", "--json"]:
    state.write_text("running")
    envelope({"schema": "effigy.container.up.v1", "ok": True})
elif argv[:2] == ["container", "down"]:
    envelope({"schema": "effigy.container.down.v1", "ok": True})
elif argv[:2] == ["container", "retire"]:
    state.write_text("retired")
    envelope({"schema": "effigy.container.retire.v1", "ok": True})
elif argv[:2] == ["container", "status"]:
    envelope({"schema": "effigy.container.status.v1", "ok": True, "backend": backend,
              "manifest_driver": "colima", "runtime_running": True})
elif argv == ["exec", "--json", "--", "effigy", "--json", "version"]:
    exec_envelope(guest_envelope({"schema": "effigy.version.v1", "schema_version": 1,
                                  "ok": True, "version": "0.15.0"}))
elif argv == ["exec", "--json", "--", "effigy", "--json", "tasks"]:
    tasks = json.loads(os.environ.get("FAKE_TASKS", '["hello"]'))
    exec_envelope(guest_envelope({"schema": os.environ.get(
        "FAKE_TASK_SCHEMA", "effigy.workspace-client.v1"), "schema_version": 1,
        "ok": True, "tasks": tasks}))
else:
    print(json.dumps({"schema": "effigy.command.v1", "ok": False,
                      "error": {"message": "unexpected " + repr(argv)}}))
    sys.exit(9)
"""


class Harness(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="qualification-proof-")
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.evidence = self.root / "evidence"
        self.state = self.root / "state"
        self.state.write_text("stopped")
        self.candidate = self.root / "effigy"
        self.candidate.write_text(FAKE)
        self.candidate.chmod(0o755)
        self.environment = {
            "FAKE_STATE": str(self.state),
            "FAKE_BACKEND": "containerd",
            "FAKE_EXPECTED_BACKEND": "colima",
            "TMPDIR": str(self.root / "run-scratch"),
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_proof(self, **overrides) -> tuple[int, Path]:
        arguments = {
            "backend": "colima",
            "repo": str(self.repo),
            "effigy": str(self.candidate),
            "task": "tasks",
            "run": "case-1",
            "evidence_root": str(self.evidence),
        }
        arguments.update(overrides)
        argv = []
        for key, value in arguments.items():
            argv.extend([f"--{key.replace('_', '-')}", value])
        previous = {key: os.environ.get(key) for key in self.environment}
        os.environ.update(self.environment)
        try:
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                return proof.main(argv), self.evidence / arguments["run"]
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def verdict(self, run: str = "case-1") -> dict:
        return json.loads((self.evidence / run / "colima.verdict.json").read_text())

    def test_passing_proof_writes_one_verdict_and_restores_the_inventory(self) -> None:
        code, _ = self.run_proof()
        self.assertEqual(code, 0)
        files = sorted(path.name for path in (self.evidence / "case-1").iterdir())
        self.assertEqual(files, ["colima.verdict.json"])
        verdict = self.verdict()
        self.assertTrue(verdict["ok"])
        self.assertEqual(verdict["schema"], proof.SCHEMA)
        self.assertEqual(
            verdict["inventory_before"], verdict["inventory_after"]
        )
        names = [step["name"] for step in verdict["steps"]]
        self.assertLess(names.index("container.up"), names.index("workspace.task"))
        self.assertLess(names.index("workspace.task"), names.index("container.retire"))
        self.assertTrue(Path(verdict["effigy"]).is_file())
        self.assertEqual(len(verdict["effigy_sha256"]), 64)
        workspace_steps = {
            step["name"]: step["argv"]
            for step in verdict["steps"]
            if step["name"].startswith("workspace.")
        }
        self.assertEqual(workspace_steps["workspace.version"][1:4], ["exec", "--json", "--"])
        self.assertEqual(workspace_steps["workspace.task"][1:4], ["exec", "--json", "--"])

    def test_tasks_v1_remains_accepted(self) -> None:
        self.environment["FAKE_TASK_SCHEMA"] = "effigy.tasks.v1"
        code, _ = self.run_proof()
        self.assertEqual(code, 0)

    def test_unknown_guest_task_schema_fails_the_proof(self) -> None:
        self.environment["FAKE_TASK_SCHEMA"] = "effigy.other.v1"
        code, _ = self.run_proof()
        self.assertEqual(code, 1)
        self.assertIn("did not answer its contract", self.verdict()["failure"])

    def test_workspace_client_requires_nonempty_task_list(self) -> None:
        self.environment["FAKE_TASKS"] = "[]"
        code, _ = self.run_proof()
        self.assertEqual(code, 1)
        self.assertIn("no workspace-client tasks", self.verdict()["failure"])

    def test_workspace_client_requires_task_list_shape(self) -> None:
        self.environment["FAKE_TASKS"] = "{\"name\": \"hello\"}"
        code, _ = self.run_proof()
        self.assertEqual(code, 1)
        self.assertIn("no workspace-client tasks", self.verdict()["failure"])

    def test_wrong_backend_fails_but_still_tears_down_and_writes(self) -> None:
        self.environment["FAKE_BACKEND"] = "docker"
        code, _ = self.run_proof()
        self.assertEqual(code, 1)
        verdict = self.verdict()
        self.assertFalse(verdict["ok"])
        self.assertIn("not colima", verdict["failure"])
        self.assertEqual(verdict["inventory_before"], verdict["inventory_after"])

    def test_leftover_inventory_fails_the_proof(self) -> None:
        self.candidate.write_text(
            FAKE.replace(
                'elif argv[:2] == ["container", "retire"]:\n    state.write_text("retired")',
                'elif argv[:2] == ["container", "retire"]:\n    pass',
            )
        )
        code, _ = self.run_proof()
        self.assertEqual(code, 1)
        self.assertIn("inventory did not return", self.verdict()["failure"])

    def test_evidence_root_inside_run_scratch_is_refused_before_effects(self) -> None:
        code, _ = self.run_proof(evidence_root=str(self.root / "run-scratch" / "evidence"))
        self.assertEqual(code, 2)
        self.assertFalse(self.evidence.exists())
        self.assertEqual(self.state.read_text().strip(), "stopped")

    def test_relative_evidence_root_is_refused(self) -> None:
        code, _ = self.run_proof(evidence_root="evidence")
        self.assertEqual(code, 2)
        self.assertEqual(self.state.read_text().strip(), "stopped")

    def test_unknown_backend_is_refused(self) -> None:
        code, _ = self.run_proof(backend="podman")
        self.assertEqual(code, 2)
        self.assertEqual(self.state.read_text().strip(), "stopped")


class Rules(unittest.TestCase):
    def test_run_labels(self) -> None:
        self.assertEqual(proof.run_label("2026-11-20-colima"), "2026-11-20-colima")
        for label in ("", "-leading", "a/b", "a" * 65, "a b"):
            with self.assertRaises(proof.Refusal, msg=label):
                proof.run_label(label)

    def test_evidence_root_must_be_absolute_and_outside_tmpdir(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qualification-run-") as scratch:
            with tempfile.TemporaryDirectory(prefix="qualification-evidence-") as durable:
                durable = Path(durable) / "evidence"
                self.assertEqual(proof.evidence_root(str(durable), scratch), durable)
                with self.assertRaises(proof.Refusal):
                    proof.evidence_root(str(Path(scratch) / "evidence"), scratch)
                nested = durable / "run"
                nested.mkdir(parents=True)
                with self.assertRaises(proof.Refusal):
                    proof.evidence_root(str(nested), str(durable))

    def test_symlinked_parent_into_run_scratch_is_refused(self) -> None:
        with tempfile.TemporaryDirectory(prefix="qualification-root-") as scratch:
            smuggled = Path(scratch) / "run" / "evidence"
            smuggled.mkdir(parents=True)
            entry = Path(scratch) / "durable"
            entry.symlink_to(Path(scratch) / "run")
            with self.assertRaises(proof.Refusal):
                proof.evidence_root(str(entry / "evidence"), str(Path(scratch) / "run"))

    def test_envelope_result_is_required(self) -> None:
        payload = {
            "schema": "effigy.command.v1",
            "ok": True,
            "result": {"schema": "effigy.version.v1"},
        }
        self.assertEqual(proof.result(payload, "version"), {"schema": "effigy.version.v1"})
        for refused in (
            {"schema": "other", "ok": True, "result": {}},
            {"schema": "effigy.command.v1", "ok": False, "error": {"message": "no"}},
            {"schema": "effigy.command.v1", "ok": True, "result": []},
        ):
            with self.assertRaises(proof.Refusal):
                proof.result(refused, "version")


if __name__ == "__main__":
    unittest.main()
