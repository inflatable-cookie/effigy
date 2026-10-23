#!/usr/bin/env python3
"""Checkout routing, nested admission and owning-client recovery, privately."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="q010-managed-") as temporary:
    base = Path(temporary).resolve()
    home, project = base / "home", base / "checkout"
    home.mkdir()
    project.mkdir()
    (project / ".git").mkdir()
    env = {key: value for key, value in os.environ.items() if not key.startswith("EFFIGY_")}
    env.update(HOME=str(home), NO_COLOR="1", EFFIGY_TEST_BIN=str(binary))
    hold = shlex.quote(sys.executable) + " -c " + shlex.quote(
        "import os,signal,time; from pathlib import Path; "
        "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
        "signal.signal(signal.SIGHUP,signal.SIG_IGN); "
        "Path('job-pid').write_text(str(os.getpid())); time.sleep(40)")
    (project / "effigy.toml").write_text(
        '[tasks.outer]\nadmission = "heavy"\nscratch = false\nrun = ' +
        json.dumps('"$EFFIGY_TEST_BIN" managed --headless') +
        '\n[tasks.managed]\nmode = "tui"\nadmission = "heavy"\nscratch = false\n' +
        'concurrent = [{name = "job", run = "touch completed"}]\n' +
        '[tasks.hold]\nmode = "tui"\nscratch = false\n' +
        'concurrent = [{name = "job", run = ' + json.dumps("exec " + hold) + '}]\n')

    def run(*args, extra=None):
        result = subprocess.run([str(binary), *args], cwd=project, env=env | (extra or {}),
                                capture_output=True, text=True, timeout=45)
        assert result.returncode == 0, (args, result.returncode, result.stdout, result.stderr)
        return result

    def assert_local():
        assert (project / ".effigy/state/kernel.db").is_file(), "managed journal missing"
        assert not list(home.rglob("*.db*")), "managed helper created a central database"
        assert not list(home.rglob("helper-*.log")), "managed helper log escaped checkout"

    run("managed", "--headless")
    assert (project / "completed").exists()
    assert_local()
    (project / "completed").unlink()
    hook = project / "hook.sh"
    hook.write_text("#!/bin/sh\nprintf '%s\\n' \"$$\" >> hook-pids\necho admit\nexec sleep 300\n")
    run("--run-id", "outer", "outer", extra={"EFFIGY_ADMISSION_HOOK": "/bin/sh " + shlex.quote(str(hook))})
    assert (project / "completed").exists()
    assert len((project / "hook-pids").read_text().splitlines()) == 1, "managed child reacquired host capacity"
    assert_local()

    child = subprocess.Popen([str(binary), "hold", "--headless"], cwd=project, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    job_pid = None
    started = time.monotonic()
    try:
        deadline = time.monotonic() + 20
        while not (project / "job-pid").exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline, "managed job readiness timeout"
            time.sleep(.02)
        job_pid = int((project / "job-pid").read_text())
        # The exact owning client created here; the supervisor must settle
        # the recorded resistant workload without a request from this client.
        os.kill(child.pid, signal.SIGKILL)
        child.communicate(timeout=40)
        deadline = time.monotonic() + 25
        while True:
            try:
                os.kill(job_pid, 0)
            except ProcessLookupError:
                break
            assert time.monotonic() < deadline, "managed workload survived owning-client death"
            time.sleep(.03)
        job_pid = None
        assert_local()
    finally:
        if child.poll() is None:
            child.terminate()
            child.communicate(timeout=40)
        if job_pid is not None:
            subprocess.run([str(binary), "hold", "stop"], cwd=project, env=env,
                           capture_output=True, timeout=40)
            # The fixture also exits itself after 40 seconds. If recovery
            # refused, wait for that bounded lifetime instead of signalling
            # any number without a native receipt.
            deadline = time.monotonic() + 45
            while True:
                try:
                    os.kill(job_pid, 0)
                except ProcessLookupError:
                    break
                assert time.monotonic() < deadline, "fixture cleanup did not finish"
                time.sleep(.05)
    print(json.dumps({"schema": "effigy.proof.runtime-managed.v1", "nested_hooks": 1,
                      "central_databases": 0, "owner_death_seconds": round(time.monotonic() - started, 3)}))
