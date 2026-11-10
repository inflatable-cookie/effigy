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
from managed_pane_live_step import prove_animation

binary = str(Path(sys.argv[1]).resolve())
live_fixture = os.environ["EFFIGY_PANE_LIVE_STEP_FIXTURE"]
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

    def start(args, program=binary):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        def own_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        child = subprocess.Popen([program, *args], cwd=root, env=env, stdin=slave,
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
            if pane.count(b" finished") == 2:
                break
            assert child.poll() is None, (child.returncode, bytes(output[-5000:]), logs)
        assert pane and b"finished" in pane, (bytes(output[-5000:]), pane, logs)
        assert b"EFFIGY" not in pane and b"\x1b" not in pane, pane
        # Pane logs are plain LF files: Pane mode writes `Started:`/`Finished:`
        # lines with `\n` (crates/effigy-ui/src/steps.rs), never PTY CRLF/ANSI.
        # Any CR here is contamination, not a line ending.
        assert b"\r" not in pane, pane
        assert pane.count(b"Started: running nested; waiting for step `1` to exit;") == 1, pane
        assert pane.count(b" finished") == 2, pane
        assert b"child-before\n" in pane and b"child-after\n" in pane, pane
        assert pane.index(b"child-before\n") < pane.index(b"child-after\n"), pane
        def adjacent_ordered(log):
            data = log.read_bytes()
            return (
                b"adjacent-before\n" in data
                and b"adjacent-after\n" in data
                and data.index(b"adjacent-before\n") < data.index(b"adjacent-after\n")
                # The canonical host log carries writer-sanitized plain text
                # (product-guardrails: managed producer colour behind custody):
                # the child's red line arrives as plain `Vite error`, never as
                # escapes. The PTY half of this fixture still proves rich output.
                and b"Vite error" in data
                and b"\x1b" not in data
            )
        assert any(adjacent_ordered(log) for log in logs)
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

    prove_animation(
        lambda: start(["--ignored", "--exact", "non_streaming_live_step_fixture", "--nocapture", "--color", "never"], program=live_fixture),
        drain,
    )
    manifest = root / "effigy.toml"
    manifest.write_text(manifest.read_text().replace(
        "sleep 2;", "while [ ! -f release-stream ]; do sleep 0.02; done;"
    ))
    child, master = start(["nested"])
    output = bytearray()
    try:
        deadline = time.monotonic() + 30
        live = False
        while child.poll() is None and time.monotonic() < deadline:
            drain(master, output)
            if not live and b"child-before" in output:
                assert child.poll() is None, "producer completed before live output"
                live = True
                (root / "release-stream").touch()
        assert live, bytes(output)
        assert child.poll() == 0, (child.poll(), bytes(output))
        for _ in range(5):
            drain(master, output)
        assert b"EFFIGY" in output and b"running nested" in output, bytes(output)
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=10)
        os.close(master)
    result = subprocess.run([binary, "--json", "nested"], cwd=root, env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["ok"] is True
    assert b"Started:" not in result.stderr
    # `--json` is silent in a pane under either spelling of the pane fact: the
    # marker an inherited environment carries, and the first one Effigy wrote.
    for marker in ("EFFIGY_MANAGED_PANE", "EFFIGY_INTERNAL_MANAGED_PANE"):
        pane_env = dict(env, **{marker: "1"})
        result = subprocess.run([binary, "--json", "nested"], cwd=root, env=pane_env, capture_output=True, timeout=30)
        assert result.returncode == 0 and json.loads(result.stdout)["ok"] is True, (marker, result.stderr)
        assert b"Started:" not in result.stderr, (marker, result.stderr)
    result = subprocess.run([binary, "dev", "--headless"], cwd=root, env=env, capture_output=True, timeout=60)
    assert result.returncode == 0, (result.stdout, result.stderr)
    headless = result.stdout + result.stderr
    # A workload's child sits in a pane whichever front end renders the run:
    # bounded start/finish lines, never spinner frames (effigy#729).
    assert b"Started: running nested" in headless, headless
    assert headless.count(b"elapsed") <= 3, headless
