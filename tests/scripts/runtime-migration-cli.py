#!/usr/bin/env python3
"""Binary cutover proof. All state and owned children live in one fresh fixture."""
import base64
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()
fixture_binary = Path(sys.argv[2]).resolve()
repo = Path(__file__).resolve().parents[2]
SCHEMA = "effigy.runtime-migration.v1"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root):
    rows = {}
    for path in [root, *sorted(root.rglob("*"))]:
        stat = path.lstat()
        rows[str(path.relative_to(root))] = (
            stat.st_mode, stat.st_size, stat.st_mtime_ns, stat.st_ino,
            digest(path) if path.is_file() and not path.is_symlink() else None,
        )
    return rows


def frame(value):
    if isinstance(value, dict):
        if value.get("schema") == SCHEMA:
            return value
        for child in value.values():
            found = frame(child)
            if found is not None:
                return found
    elif isinstance(value, list):
        for child in value:
            found = frame(child)
            if found is not None:
                return found
    elif isinstance(value, str) and value.startswith("{"):
        try:
            return frame(json.loads(value))
        except ValueError:
            pass
    return None


with tempfile.TemporaryDirectory(prefix="q010-cli-") as temporary:
    root = Path(temporary).resolve()
    home = root / "home"
    project = root / "project"
    home.mkdir()
    project.mkdir()
    (project / "effigy.toml").write_text("[tasks.noop]\nrun = 'true'\n")
    git = project / ".git"
    git.mkdir()
    (git / "effigy-checkout-id").write_text("07" * 16)
    (git / "effigy-runtime-scope").write_text("08" * 16)
    env = dict(os.environ, HOME=str(home), NO_COLOR="1")
    # A proof never inherits another run's admission/custody capabilities.
    for key in ["EFFIGY_ADMITTED", "EFFIGY_CUSTODY_NONCE", "EFFIGY_PROBE_NONCE"]:
        env.pop(key, None)
    child = None

    def command(*args, succeeds=True, migration=True):
        result = subprocess.run([str(binary), *args], cwd=project, env=env,
                                text=True, capture_output=True, timeout=40)
        assert (result.returncode == 0) == succeeds, (args, result.returncode, result.stdout, result.stderr)
        if migration:
            data = frame(json.loads(result.stdout))
            assert data is not None, (result.stdout, result.stderr)
            assert data["schema_version"] == 1
            return data
        return result

    try:
        before = snapshot(root)
        assert command("run", "migrate", "--plan", "--json")["state"] == "no-legacy-state"
        assert snapshot(root) == before, "absent planning created state"
        child = subprocess.Popen([str(fixture_binary), "migration-writer", str(home), "seed-aliases"],
                                 cwd=project, env=env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        end = time.monotonic() + 12
        ready = home / "writer.json"
        while not ready.exists():
            assert child.poll() is None, "writer exited before ready"
            assert time.monotonic() < end, "writer ready deadline"
            time.sleep(.01)
        target = json.loads(ready.read_text())
        assert target["process"]["pid"] == child.pid
        os.kill(child.pid, signal.SIGSTOP)  # exact child started above
        stopped_pid, status = os.waitpid(child.pid, os.WUNTRACED)
        assert stopped_pid == child.pid and os.WIFSTOPPED(status)
        before = snapshot(root)
        blocked = command("run", "migrate", "--plan", "--json", succeeds=False)
        assert blocked["state"] == "blocked" and blocked["staging_started"] is False
        assert snapshot(root) == before, "busy planning changed a source or staged a destination"
        blocker = next(row for row in blocked["blockers"]
                       if row["writer"]["role"] == "gate holder" and row["writer"]["run"] is None)
        assert blocker["stop_command"].startswith("effigy run migrate --stop writer:")
        # A JSON-contract row must observe a fresh HOME, not this frozen writer.
        contract = root / "contract"
        contract.mkdir()
        schema = contract / "payload.json"
        schema.write_bytes((repo / "docs/knowledge/contracts/payload-schemas/effigy.runtime-migration.v1.json").read_bytes())
        index = contract / "index.json"
        index.write_text(json.dumps({"version": 1, "payload_schemas": {SCHEMA: "payload.json"},
            "schemas": [{"schema": SCHEMA, "schema_version": 1,
                "command": "effigy run migrate --plan --json", "status": "active", "isolated_home": True}]}))
        command("--json", "contracts", "check-json", "--index", str(index), "--full", migration=False)
        before = snapshot(home)
        stale = json.loads(json.dumps(target))
        stale["process"]["pid"] += 1
        token = base64.urlsafe_b64encode(json.dumps(stale).encode()).decode().rstrip("=")
        refused = command("run", "migrate", "--stop", "writer:" + token, "--json", succeeds=False)
        assert refused["state"] == "stop-refused"
        assert snapshot(home) == before, "stale receipt changed source state"
        stop_args = shlex.split(blocker["stop_command"])[1:]
        assert command(*stop_args, "--json")["state"] == "stop-requested"
        assert child.wait(timeout=12) == 0, "verified stop did not drain the stopped writer"
        before = snapshot(home)
        assert command(*stop_args, "--json", succeeds=False)["state"] == "stop-refused"
        assert snapshot(home) == before, "free gate granted stale stop authority"
        planned = command("run", "migrate", "--plan", "--json")
        assert planned["state"] == "planned"
        assert len(planned["destinations"]) == 1
        assert len(planned["destinations"][0]["checkout_aliases"]) == 1
        before = snapshot(home)
        assert command("run", "migrate", "--json", succeeds=False)["state"] == "needs-quiescence"
        assert snapshot(home) == before, "missing quiescence assertion staged a destination"
        source = home / ".effigy/state/kernel.db"
        source_digest = digest(source)
        imported = command("run", "migrate", "--legacy-launchers-stopped", "--json")
        assert imported["state"] == "committed"
        assert digest(source) == source_digest, "import modified the legacy database"
        machine = home / ".effigy/state/machine.db"
        machine_digest = digest(machine)
        local = project / ".effigy/state/kernel.db"
        local_digest = digest(local)
        destinations = [entry for entry in imported["report"]["receipt"]["destinations"] if entry["checkout"] is not None]
        assert len(destinations) == 1 and len(destinations[0]["checkout_aliases"]) == 1
        assert destinations[0]["git_directory"] == str(git)
        assert destinations[0]["git_directory_identity"] is not None
        def resources(database):
            with sqlite3.connect("file:" + str(database) + "?mode=ro&immutable=1", uri=True) as connection:
                return connection.execute("SELECT name, hex(canonical) FROM resources ORDER BY name").fetchall()
        assert resources(source) == resources(local) and len(resources(local)) == 2
        retry = command("run", "migrate", "--legacy-launchers-stopped", "--json")
        assert retry["report"]["resumed"] is True
        assert digest(source) == source_digest and digest(machine) == machine_digest
        assert digest(local) == local_digest
        # Archived writer controls may never regain authority after cutover.
        command(*stop_args, "--json", succeeds=False, migration=False)
        assert digest(source) == source_digest and digest(machine) == machine_digest
        git.rename(project / ".git.old")
        git.mkdir()
        (git / "effigy-checkout-id").write_text("07" * 16)
        (git / "effigy-runtime-scope").write_text("08" * 16)
        before = snapshot(root)
        command("run", "migrate", "--legacy-launchers-stopped", "--json", succeeds=False, migration=False)
        assert snapshot(root) == before, "copied tokens let a replacement Git generation change sealed state"
        print(json.dumps({"schema": "q010.cli-proof.v1", "physical_aliases_coalesced": True,
            "canonical_resource_ids_preserved": True, "replacement_git_generation_refused": True, "busy_before_staging": True,
            "writer_without_run_id": True, "verified_stop_and_resume": True,
            "stale_and_free_refused": True, "json_schema_and_isolated_home": True,
            "source_unchanged": True, "retry_without_reimport": True,
            "elapsed_ms": imported["report"]["elapsed_ms"],
            "destinations": imported["report"]["receipt"]["destinations"]}, sort_keys=True))
    finally:
        if child is not None:
            if child.poll() is None:
                child.kill()  # exact owned child; SIGKILL also ends a stopped child
            child.wait(timeout=12)
