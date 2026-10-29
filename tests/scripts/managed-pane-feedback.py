#!/usr/bin/env python3
"""Capture the supervisor's actual PTY stream, without a consumer runtime."""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="epane.") as directory:
    base = Path(directory)
    home = base / "home"
    root = base / "repo"
    home.mkdir()
    root.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.startswith("EFFIGY_")}
    env.update(HOME=str(home), TERM="xterm-256color", NO_COLOR="1", PANE_TEST_BIN=binary)
    env.pop("CI", None)
    (root / "effigy.toml").write_text('''[tasks.dev]
mode = "tui"
concurrent = [
  { name = "nested", run = "\\\"$PANE_TEST_BIN\\\" nested; sleep 10", start = 1 },
  { name = "adjacent", run = "printf 'adjacent-before\\\\n'; sleep 1; printf '\\\\033[31mVite error\\\\033[0m\\\\n'; printf 'adjacent-after\\\\n'; sleep 10", start = 1 },
]
[tasks.nested]
run = [{ task = "leaf" }]
[tasks.leaf]
run = ["printf 'child-before\\\\n'; sleep 2; printf 'child-after\\\\n'"]
''')

    def start(args):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        def own_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        child = subprocess.Popen([binary, *args], cwd=root, env=env, stdin=slave,
                                 stdout=slave, stderr=slave, preexec_fn=own_terminal)
        os.close(slave)
        return child, master

    def drain(master, output):
        if select.select([master], [], [], .05)[0]:
            try:
                output.extend(os.read(master, 65536))
                while output.count(b"\x1b[6n") > output.count(b"\x1b[1;1R"):
                    os.write(master, b"\x1b[1;1R")
                    output.extend(b"\x1b[1;1R")
            except OSError as error:
                if error.errno != errno.EIO:
                    raise

    child, master = start(["dev"])
    output = bytearray()
    try:
        deadline = time.monotonic() + 45
        pane = b""
        while time.monotonic() < deadline:
            drain(master, output)
            logs = list(base.rglob("*.stdout.*.log"))
            for log in logs:
                data = log.read_bytes()
                if b"child-after" in data:
                    pane = data
                    break
            if pane.count(b"step 1/1 `1` finished") == 2:
                break
            assert child.poll() is None, (child.returncode, bytes(output[-5000:]), logs)
        assert pane and b"finished" in pane, (bytes(output[-5000:]), pane, logs)
        assert b"EFFIGY" not in pane and b"\x1b" not in pane, pane
        assert b"\r" not in pane.replace(b"\r\n", b""), pane
        assert pane.count(b"Started: running nested step 1/1") == 1, pane
        assert pane.count(b"step 1/1 `1` finished") == 2, pane
        assert b"child-before\r\n" in pane and b"child-after\r\n" in pane, pane
        assert any(b"adjacent-before\r\n" in log.read_bytes() and b"adjacent-after\r\n" in log.read_bytes() and b"\x1b[31mVite error\x1b[0m" in log.read_bytes() for log in logs)
    finally:
        if child.poll() is None:
            child.send_signal(signal.SIGTERM)
            deadline = time.monotonic() + 30
            while child.poll() is None and time.monotonic() < deadline:
                drain(master, output)
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=15)
        os.close(master)

    child, master = start(["nested"])
    output = bytearray()
    try:
        deadline = time.monotonic() + 30
        while child.poll() is None and time.monotonic() < deadline:
            drain(master, output)
        assert child.poll() == 0, (child.poll(), bytes(output))
        for _ in range(5):
            drain(master, output)
        assert b"EFFIGY" in output and b"running nested step" in output and b"\x1b" in output, bytes(output)
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=10)
        os.close(master)
    result = subprocess.run([binary, "--json", "nested"], cwd=root, env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["ok"] is True
    assert b"Started:" not in result.stderr
    pane_env = dict(env, EFFIGY_INTERNAL_MANAGED_PANE="1")
    result = subprocess.run([binary, "--json", "nested"], cwd=root, env=pane_env, capture_output=True, timeout=30)
    assert result.returncode == 0 and json.loads(result.stdout)["ok"] is True
    assert b"Started:" not in result.stderr
    result = subprocess.run([binary, "dev", "--headless"], cwd=root, env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert b"Started: running nested" not in result.stdout + result.stderr
