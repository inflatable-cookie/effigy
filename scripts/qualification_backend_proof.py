#!/usr/bin/env python3
"""One per-backend workspace proof for the v0.15 re-qualification.

Brings up a real workspace on one backend with ordinary Effigy commands, runs
one representative task through that workspace, tears the stack down, and
checks the engine inventory returned to its starting state. Writes exactly one
verdict file: ``<evidence-root>/<run>/<backend>.verdict.json``.

There are no sealed occurrences, one-shot markers or sidecar pins. The only
inputs are the backend, a disposable checkout, the candidate binary and a run
label.

Usage::

    scripts/qualification_backend_proof.py --backend colima \\
        --repo /path/to/disposable/checkout --run 2026-11-20-colima

Steps, in order:

1. Record the candidate binary's SHA-256 and ``effigy --json version``.
2. Snapshot the engine inventory: ``container status --global --json`` and
   ``container volume list --global --json``.
3. ``effigy container up --json`` in the checkout.
4. ``effigy container status --json``; the chosen backend must be the one this
   proof is for.
5. ``effigy exec -- effigy --json version`` and ``effigy exec -- effigy
   --json <task>`` (default ``tasks``); both need exit 0, and the task needs
   output.
6. ``effigy container down --json``, then ``effigy container retire --yes
   --json`` so the brings-up's containers, volumes and networks are gone.
7. Snapshot the inventory again; it must equal step 2 byte for byte.

A failure still tears down and still writes one verdict file, marked ``fail``
with the failing step. The evidence root must be absolute and outside the run's
TMPDIR (effigy#1041): a verdict never lands in disposable run scratch.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

SCHEMA = "effigy.qualification.backend.v1"
DEFAULT_EVIDENCE_ROOT = Path("/Volumes/Stash/scratch/effigy/evidence")
RUN_LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")


class Refusal(Exception):
    """A proof that refuses before any effect."""


@dataclass(frozen=True)
class Backend:
    name: str
    system: str
    driver: str
    compose_backend: str

    def identity_ok(self, status: dict) -> bool:
        return status.get("backend") == self.driver and platform.system() == self.system


BACKENDS = {
    "colima": Backend(
        name="colima", system="Darwin", driver="containerd", compose_backend="colima"
    ),
    "desktop": Backend(
        name="desktop", system="Darwin", driver="docker", compose_backend="docker"
    ),
    "linux": Backend(
        name="linux", system="Linux", driver="docker", compose_backend="docker"
    ),
}


def backend(name: str) -> Backend:
    try:
        return BACKENDS[name]
    except KeyError:
        raise Refusal(
            f"unknown backend `{name}`; use one of {', '.join(sorted(BACKENDS))}"
        ) from None


def evidence_root(value: str, tmpdir: str | None) -> Path:
    """The durable evidence root: absolute, outside the run's TMPDIR."""
    root = Path(value)
    if not root.is_absolute():
        raise Refusal(f"evidence root `{value}` must be absolute")
    resolved = root.resolve()
    if tmpdir:
        scratch = Path(tmpdir).resolve()
        if resolved == scratch or scratch in resolved.parents:
            raise Refusal(
                f"evidence root `{value}` is inside the run TMPDIR `{scratch}`; "
                "use a durable root outside disposable run scratch"
            )
    return resolved


def run_label(value: str) -> str:
    if not RUN_LABEL.match(value):
        raise Refusal(
            f"run label `{value}` must be 1-64 characters of letters, digits, "
            "dot, dash or underscore, and start with a letter or digit"
        )
    return value


def verdict_path(root: Path, run: str, backend_name: str) -> Path:
    return root / run / f"{backend_name}.verdict.json"


def digest(text: str | bytes) -> str:
    if isinstance(text, str):
        text = text.encode()
    return hashlib.sha256(text).hexdigest()


def parse_json(text: str, what: str) -> dict:
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise Refusal(f"{what} did not return JSON: {error}") from None
    if not isinstance(value, dict):
        raise Refusal(f"{what} did not return a JSON object")
    return value


def containers_inventory(payload: dict) -> list[str]:
    """Environment and service container identity, without volatile facts."""
    if payload.get("schema") != "effigy.container.status-global.v1":
        raise Refusal("container inventory did not answer its contract")
    entries = []
    for environment in payload.get("environments", []):
        project = f"{environment.get('container')}@{environment.get('project_name')}"
        entries.append(project)
        for service in environment.get("services", []):
            entries.append(f"{project}/{service.get('container_name')}")
    return sorted(entries)


def volumes_inventory(payload: dict) -> list[str]:
    """Named volume identity, never size or in-use drift."""
    if payload.get("schema") != "effigy.container.volume-list.v1":
        raise Refusal("volume inventory did not answer its contract")
    return sorted(
        f"{volume.get('name')}@{volume.get('backend')}"
        for volume in payload.get("volumes", [])
    )


def result(envelope: dict, what: str) -> dict:
    """The command envelope's own payload (`effigy.command.v1`)."""
    if envelope.get("schema") != "effigy.command.v1":
        raise Refusal(f"{what} did not answer the command envelope")
    if envelope.get("ok") is not True:
        raise Refusal(f"{what} refused: {envelope.get('error')}")
    payload = envelope.get("result")
    if not isinstance(payload, dict):
        raise Refusal(f"{what} did not return a result object")
    return payload


@dataclass
class Step:
    name: str
    argv: list[str]
    exit_code: int
    stdout: str
    stderr: str

    def ok(self) -> bool:
        return self.exit_code == 0

    def record(self) -> dict:
        return {
            "name": self.name,
            "argv": self.argv,
            "exit_code": self.exit_code,
            "stdout_sha256": digest(self.stdout),
            "stderr_sha256": digest(self.stderr),
            "stdout": self.stdout[-4000:],
            "stderr": self.stderr[-4000:],
        }


class Proof:
    """One backend proof. Every effect is an ordinary Effigy command."""

    def __init__(self, args: argparse.Namespace):
        self.backend = backend(args.backend)
        self.effigy = shutil.which(args.effigy) or args.effigy
        self.repo = Path(args.repo).resolve()
        self.task = args.task
        self.root = evidence_root(args.evidence_root, os.environ.get("TMPDIR"))
        self.run = run_label(args.run)
        self.destination = verdict_path(self.root, self.run, self.backend.name)
        self.steps: list[Step] = []
        self.failure: str | None = None
        self.inventory_before: dict[str, str] = {}
        self.inventory_after: dict[str, str] = {}

    def effigy_step(self, name: str, *argv: str) -> Step:
        command = [self.effigy, *argv]
        environment = {
            **os.environ,
            "EFFIGY_COMPOSE_BACKEND": self.backend.compose_backend,
        }
        try:
            done = subprocess.run(
                command,
                cwd=self.repo,
                capture_output=True,
                text=True,
                timeout=3600,
                env=environment,
            )
            step = Step(name, command, done.returncode, done.stdout, done.stderr)
        except subprocess.TimeoutExpired:
            step = Step(name, command, 124, "", f"{' '.join(command)} timed out")
        self.steps.append(step)
        if not step.ok() and self.failure is None:
            self.failure = f"{name} exited {step.exit_code}"
        return step

    def inventory(self) -> dict[str, list[str]]:
        snapshot: dict[str, list[str]] = {}
        readers = {
            "containers": ("container", "status", "--global", "--json"),
            "volumes": ("container", "volume", "list", "--global", "--json"),
        }
        for key, argv in readers.items():
            step = self.effigy_step(f"inventory.{key}", *argv)
            try:
                observed = result(
                    parse_json(step.stdout, f"inventory {key}"), f"inventory {key}"
                )
                reader = (
                    containers_inventory if key == "containers" else volumes_inventory
                )
                snapshot[key] = reader(observed)
            except Refusal as refusal:
                snapshot[key] = [f"refused: {refusal}"]
        return snapshot

    def fail(self, reason: str) -> None:
        if self.failure is None:
            self.failure = reason

    def prove(self) -> dict:
        self.inventory_before = self.inventory()
        up = self.effigy_step(
            "container.up", "container", "up", "--detach", "--json"
        )
        if self.failure is None:
            try:
                result(parse_json(up.stdout, "container up"), "container up")
            except Refusal as refusal:
                self.fail(str(refusal))
        status = self.effigy_step("container.status", "container", "status", "--json")
        if self.failure is None:
            try:
                observed = result(
                    parse_json(status.stdout, "container status"), "container status"
                )
            except Refusal as refusal:
                self.fail(str(refusal))
                observed = {}
            if not self.backend.identity_ok(observed):
                self.fail(
                    f"container status reports backend "
                    f"`{observed.get('backend')}` on {platform.system()}, "
                    f"not {self.backend.name}"
                )
            if observed.get("runtime_running") is not True:
                self.fail("container status does not report a running runtime")
        version = self.effigy_step(
            "workspace.version", "exec", "--", "effigy", "--json", "version"
        )
        if self.failure is None:
            try:
                guest = result(
                    parse_json(version.stdout, "workspace effigy version"),
                    "workspace effigy version",
                )
            except Refusal as refusal:
                self.fail(str(refusal))
                guest = {}
            if guest.get("schema") != "effigy.version.v1" or not guest.get("version"):
                self.fail("workspace effigy version is not the published contract")
        task = self.effigy_step(
            "workspace.task", "exec", "--", "effigy", "--json", self.task
        )
        if self.failure is None:
            try:
                answered = result(
                    parse_json(task.stdout, f"workspace task {self.task}"),
                    f"workspace task {self.task}",
                )
            except Refusal as refusal:
                self.fail(str(refusal))
                answered = {}
            if answered.get("schema") != "effigy.tasks.v1":
                self.fail(f"workspace task {self.task} did not answer its contract")
        self.teardown()
        return self.record()

    def teardown(self) -> None:
        self.effigy_step("container.down", "container", "down", "--json")
        self.effigy_step(
            "container.retire", "container", "retire", "--yes", "--json"
        )
        self.inventory_after = self.inventory()
        if self.inventory_after != self.inventory_before:
            self.fail("engine inventory did not return to its starting state")

    def record(self) -> dict:
        binary = Path(self.effigy)
        return {
            "schema": SCHEMA,
            "schema_version": 1,
            "ok": self.failure is None,
            "backend": self.backend.name,
            "platform": platform.system(),
            "repo": str(self.repo),
            "run": self.run,
            "effigy": str(binary),
            "effigy_sha256": digest(binary.read_bytes()) if binary.is_file() else None,
            "task": self.task,
            "collected_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "failure": self.failure,
            "inventory_before": self.inventory_before,
            "inventory_after": self.inventory_after,
            "steps": [step.record() for step in self.steps],
        }

    def write(self, verdict: dict) -> Path:
        self.destination.parent.mkdir(parents=True, exist_ok=True)
        self.destination.write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
        return self.destination


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--backend", required=True, help="colima, desktop or linux"
    )
    parser.add_argument(
        "--repo",
        default=os.getcwd(),
        help="disposable checkout to bring up (default: current directory)",
    )
    parser.add_argument(
        "--effigy", default="effigy", help="candidate Effigy executable"
    )
    parser.add_argument(
        "--task",
        default="tasks",
        help="representative task selector run inside the workspace",
    )
    parser.add_argument(
        "--run", required=True, help="evidence run label below the evidence root"
    )
    parser.add_argument(
        "--evidence-root",
        default=str(DEFAULT_EVIDENCE_ROOT),
        help=f"durable evidence root (default: {DEFAULT_EVIDENCE_ROOT})",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    try:
        proof = Proof(parse_args(argv))
    except Refusal as refusal:
        print(f"qualification: refused: {refusal}", file=sys.stderr)
        return 2
    verdict = proof.prove()
    destination = proof.write(verdict)
    print(destination)
    if verdict["ok"]:
        print(f"qualification: {proof.backend.name} passed")
        return 0
    print(f"qualification: {proof.backend.name} failed: {verdict['failure']}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
