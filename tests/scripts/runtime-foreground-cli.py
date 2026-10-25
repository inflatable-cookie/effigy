#!/usr/bin/env python3
"""Private binary proof: foreground commands, live views and hook custody."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()


def frame(value, schema):
    if isinstance(value, dict):
        if value.get("schema") == schema:
            return value
        for child in value.values():
            found = frame(child, schema)
            if found is not None:
                return found
    if isinstance(value, list):
        for child in value:
            found = frame(child, schema)
            if found is not None:
                return found
    return None


with tempfile.TemporaryDirectory(prefix="q010-foreground-") as temporary:
    base = Path(temporary).resolve()
    home = base / "home"
    home.mkdir()
    env = dict(os.environ, HOME=str(home), NO_COLOR="1", EFFIGY_TEST_BIN=str(binary))
    # The installed selector owns host admission. Fixture CLI children start
    # independent private runs; explicit tests below supply their own hook.
    for name in ["EFFIGY_ADMITTED", "EFFIGY_ADMISSION_HOOK", "EFFIGY_ADMISSION_HOOK_TIMEOUT_MS",
                 "EFFIGY_CUSTODY_NONCE", "EFFIGY_PROBE_NONCE",
                 "EFFIGY_MANAGED_RUN_TOKEN", "EFFIGY_LOCK_WAIT_MS"]:
        env.pop(name, None)
    projects = []
    for index in range(3):
        project = base / f"checkout-{index}"
        project.mkdir()
        (project / ".git").mkdir()
        text = "\n".join(f'[tasks.short{i}]\nrun = "printf short-{i}"' for i in range(8))
        text += '''
[tasks.diagnostics]
run = [{task = "tasks"}, {task = "help test"}]
[tasks.sequence]
run = [{run = "printf first"}, {task = "short0"}, {run = "printf last"}]
[tasks.outer]
admission = "heavy"
run = "\\\"$EFFIGY_TEST_BIN\\\" inner"
[tasks.inner]
admission = "heavy"
run = "touch inner.txt"
[tasks.hold]
admission = "heavy"
run = "touch ready; sleep 300"
[tasks.late]
run = "\\\"$EFFIGY_TEST_BIN\\\" --run-id forbidden short1"
[tasks.test_ref]
run = [{task = "test local"}]
[tasks.outer_test]
admission = "heavy"
run = "\\\"$EFFIGY_TEST_BIN\\\" test heavy"
[tasks.test_loop]
run = [{task = "test loop"}]
[test.suites.local]
run = [{run = "printf suite"}, {task = "short0"}]
[test.suites.heavy]
default = false
run = [{task = "inner"}]
[test.suites.loop]
default = false
run = [{task = "test_loop"}]
[test.suites.hold]
default = false
run = "touch suite-ready; sleep 300"
'''
        (project / "effigy.toml").write_text(text)
        projects.append(project)

    def run(project, *args, extra=None, success=True):
        result = subprocess.run([str(binary), *args], cwd=project, env=env | (extra or {}),
                                capture_output=True, text=True, timeout=30)
        assert (result.returncode == 0) == success, (args, result.returncode, result.stdout, result.stderr)
        return result

    def assert_memory_only():
        for project in projects:
            assert not list((project / ".git").iterdir()), "foreground minted Git identity"
            assert not (project / ".effigy/state/checkout.json").exists()
            assert not (project / ".effigy/state/runs").exists()
        assert not list(base.rglob("*.db*")), "foreground opened a database"
        assert not list(home.iterdir()), "foreground initialized machine state"

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=24) as pool:
        futures = [pool.submit(run, project, f"short{i}") for project in projects for i in range(8)]
        for future in futures:
            future.result()
    for project in projects:
        run(project, "sequence")
        run(project, "diagnostics")
        before = {str(path.relative_to(project)): path.read_bytes()
                  for path in project.rglob("*") if path.is_file()}
        run(project, "test", "--plan")
        run(project, "test", "--help")
        after = {str(path.relative_to(project)): path.read_bytes()
                 for path in project.rglob("*") if path.is_file()}
        assert after == before, "builtin test plan/help changed runtime files"
        run(project, "test", "local")
        run(project, "test_ref")
        run(project, "test", "loop", success=False)
    assert_memory_only()
    concurrent_seconds = round(time.monotonic() - started, 3)

    project = projects[0]
    hook = project / "hook.sh"
    hook.write_text('''#!/bin/sh
printf '%s\\n' "$$" >> hook-pids
echo admit
exec sleep 300
''')
    hook_env = {"EFFIGY_ADMISSION_HOOK": f"/bin/sh '{hook}'"}
    run(project, "outer", extra=hook_env)
    count = len((project / "hook-pids").read_text().splitlines())
    assert count == 1, "nested heavy task queued twice"
    run(project, "outer_test", extra=hook_env)
    assert len((project / "hook-pids").read_text().splitlines()) == count + 1, "suite worker acquired another heavy slot"
    assert (project / "inner.txt").exists()
    assert_memory_only()

    child = subprocess.Popen([str(binary), "hold"], cwd=project, env=env | hook_env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 10
        while not (project / "ready").exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline, "foreground workload did not start"
            time.sleep(0.02)
        result = run(project, "tasks", "status", "hold", "--json")
        view = frame(json.loads(result.stdout), "effigy.tasks-status.v3")
        assert view and view["state"] == "running", result.stdout
        current = view["current"]
        assert current["evidence_source"] == "live-foreground"
        assert current["pid"] == child.pid
        assert current["phase"] is None and current["started_at_epoch_ms"] is None
        assert current["profile"] is None and view["history"] == [] and view["latest"] is None
        run_id = current["run_id"]
        result = run(project, "runs", "--json")
        listing = frame(json.loads(result.stdout), "effigy.runs.v1")
        assert listing["coverage"]["complete"] and not listing["coverage"]["omitted"], listing
        live = [row for row in listing["runs"] if row["run_id"] == run_id]
        assert len(live) == 1 and live[0]["pid"] == child.pid, listing
        assert live[0]["evidence_source"] == "live-foreground" and live[0]["exit_code"] is None
        result = run(project, "run", "status", run_id, "--json")
        generic = frame(json.loads(result.stdout), "effigy.run.v1")["run"]
        assert generic["state"] == "running" and generic["pid"] == child.pid, generic
        result = run(project, "run", "logs", run_id, "--follow", "--json")
        logs = frame(json.loads(result.stdout), "effigy.run.logs.v1")
        assert not logs["logs_available"] and logs["reason"] == "foreground-no-retained-log", logs
        assert_memory_only()
        hook_pid = int((project / "hook-pids").read_text().splitlines()[-1])
        # This is the exact hook spawned by our owned fixture invocation.
        os.kill(hook_pid, signal.SIGKILL)
        out, err = child.communicate(timeout=15)
        assert child.returncode != 0 and "admission" in err, (out, err)
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=30)
    result = run(project, "tasks", "status", "hold", "--json")
    view = frame(json.loads(result.stdout), "effigy.tasks-status.v3")
    assert view["state"] == "not-recorded" and view["current"] is None, view
    result = run(project, "run", "status", run_id, "--json", success=False)
    assert "not-recorded" in result.stdout or "not recorded" in result.stderr, result
    run(project, "late", success=False)
    assert_memory_only()
    # Stop a separate foreground owner through its fresh gate identity. No
    # stop request, result, checkout token or database is retained.
    (project / "ready").unlink()
    child = subprocess.Popen([str(binary), "hold"], cwd=project, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 10
        while not (project / "ready").exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline, "stop workload did not start"
            time.sleep(0.02)
        result = run(project, "tasks", "status", "hold", "--json")
        run_id = frame(json.loads(result.stdout), "effigy.tasks-status.v3")["current"]["run_id"]
        result = run(project, "run", "stop", run_id, "--wait-ms", "10000", "--json")
        stopped = frame(json.loads(result.stdout), "effigy.run.v1")
        assert stopped["stop"] == "signalled-owner" and stopped["custody"] == "released", stopped
        assert stopped["outcome"] == "not-recorded" and stopped["run"]["exit_code"] is None, stopped
        child.communicate(timeout=15)
        assert_memory_only()
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=30)
    # Replacing Git metadata changes the checkout generation while its owner
    # is waiting. It must stop native custody before a successor can proceed.
    project = projects[2]
    child = subprocess.Popen([str(binary), "hold"], cwd=project, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 10
        while not (project / "ready").exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline, "replacement workload did not start"
            time.sleep(0.02)
        (project / ".git").rename(project / "git-retired")
        (project / ".git").mkdir()
        out, err = child.communicate(timeout=15)
        assert child.returncode != 0 and "filesystem authority changed" in err, (out, err)
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=30)
    run(project, "short0")
    project = projects[1]
    child = subprocess.Popen([str(binary), "test", "hold"], cwd=project, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 10
        while not (project / "suite-ready").exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline, "builtin suite did not start"
            time.sleep(0.02)
        result = run(project, "runs", "--json")
        listing = frame(json.loads(result.stdout), "effigy.runs.v1")
        owned = [row for row in listing["runs"] if row["task"] == "effigy test" and row.get("pid") == child.pid]
        assert len(owned) == 1, listing
        run(project, "run", "stop", owned[0]["run_id"], "--wait-ms", "10000", "--json")
        child.communicate(timeout=15)
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=30)
    assert_memory_only()
    # A declared script may launch a retained child. Its static ancestor is
    # durable before go, while all undeclared late-effect refusals above stay.
    declared = base / "declared-durable"
    declared.mkdir()
    (declared / "effigy.toml").write_text('''
[tasks.start]
run = [{task = "opaque"}]
[tasks.opaque]
durable = true
run = '"$EFFIGY_TEST_BIN" --run-id declared-child inner'
[tasks.inner]
run = 'touch completed'
''')
    run(declared, "start")
    assert (declared / "completed").exists()
    assert (declared / ".effigy/state/kernel.db").exists()
    assert not (home / ".effigy/state").exists()
    print(json.dumps({"schema": "q010.foreground-proof.v1", "short_runs": 24,
                      "checkouts": 3, "seconds": concurrent_seconds,
                      "database_files_created": 0, "retained_run_records": 0,
                      "sequence": "passed", "nested_hook_count": 1,
                      "builtin_suites": "passed", "suite_stop": "released", "suite_hook_count": 1,
                      "hook_loss": "stopped", "checkout_replacement": "stopped", "late_durable_effect": "refused", "declared_durable_descendant": "passed"}, sort_keys=True))
