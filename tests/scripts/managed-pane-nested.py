#!/usr/bin/env python3
"""A Services-style pane, nested Effigy child, through the real PTY.

The consumer shape this reproduces (effigy#729): a managed TUI tab runs a
program that starts Effigy as its own child with an environment scrubbed of
`EFFIGY_INTERNAL_*` (the shipped bundle's `profile_env.py` does exactly that).
That nested Effigy is inside the pane, so its output belongs to the pane's
canonical plain log: bounded start/finish lines, no CLI banner, no spinner
redraw frames.

Usage: managed-pane-nested.py <effigy-binary> [<evidence-dir>]

With an evidence directory the fixture also retains the raw PTY stream, the
canonical pane logs and a summary, and it reports the pre-fix shape instead of
failing on it, so the same fixture can record a before/after pair.
"""
import errno
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

binary = str(Path(sys.argv[1]).resolve())
evidence = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None

# The tab command: a wrapper that starts Effigy with the internal pane marker
# scrubbed, exactly like the bundle's `nested_environment()`.
WRAPPER = '''#!/usr/bin/env python3
import os
import subprocess
import sys

env = {
    key: value
    for key, value in os.environ.items()
    if not key.startswith(("EFFIGY_INTERNAL_", "EFFIGY_MANAGED_HOST_"))
}
print("[fixture] services-tab", flush=True)
sys.exit(subprocess.run([sys.argv[1], *sys.argv[2:]], env=env).returncode)
'''

# The adjacent tab: a non-Effigy workload beside the Services tab, so the
# fixture proves concurrent output and colour survive.
ADJACENT = '''#!/usr/bin/env python3
import time
print("[fixture] adjacent-tab", flush=True)
print("adjacent-before", flush=True)
time.sleep(1)
print("\\033[31mVite error\\033[0m", flush=True)
print("adjacent-after", flush=True)
time.sleep(120)
'''

MANIFEST = '''[tasks.dev]
mode = "tui"
concurrent = [
  { name = "services", run = "python3 __WRAPPER__ __BINARY__ nested:steps", start = 1 },
  { name = "adjacent", run = "python3 __ADJACENT__", start = 1 },
]
[tasks."nested:steps"]
run = [
  { task = "nested:wait" },
  "printf 'nested-middle\\n'",
  { task = "nested:fail" },
]
[tasks."nested:wait"]
run_in = "host"
run = "sleep 4; printf 'nested-waited\\n'"
[tasks."nested:fail"]
run_in = "host"
run = "printf 'nested-error-line\\n'; exit 3"
'''


def start(args, cwd, env):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))

    def own_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    child = subprocess.Popen(
        [binary, *args],
        cwd=cwd,
        env=env,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=own_terminal,
    )
    os.close(slave)
    return child, master


def drain(master, output):
    if select.select([master], [], [], 0.05)[0]:
        try:
            output.extend(os.read(master, 65536))
            while output.count(b"\x1b[6n") > output.count(b"\x1b[1;1R"):
                os.write(master, b"\x1b[1;1R")
                output.extend(b"\x1b[1;1R")
        except OSError as error:
            if error.errno != errno.EIO:
                raise


def pane_log(logs, needle):
    """The tab's canonical log: the one that carries this tab's own output."""
    found = b""
    for log in logs:
        data = log.read_bytes()
        if needle in data:
            found += data
    return found


def retry(fn, deadline):
    while time.monotonic() < deadline:
        found = fn()
        if found:
            return found
        time.sleep(0.1)
    return None


def run_standalone(env, root):
    """A nested child is bounded only inside a pane: standalone still animates."""
    child, master = start(["nested:steps"], root, env)
    output = bytearray()
    try:
        deadline = time.monotonic() + 60
        while child.poll() is None and time.monotonic() < deadline:
            drain(master, output)
        for _ in range(5):
            drain(master, output)
        assert child.poll() == 3, (child.poll(), bytes(output[-4000:]))
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=15)
        os.close(master)
    text = bytes(output)
    frames = text.count(b"elapsed")
    assert frames >= 5, ("standalone progress stopped", frames, text[-4000:])
    assert b"EFFIGY" in text, text[-4000:]
    return text


def run_piped(env, root):
    result = subprocess.run(
        [binary, "--json", "nested:steps"], cwd=root, env=env, capture_output=True, timeout=180
    )
    assert result.returncode == 3, (result.returncode, result.stderr[-4000:])
    assert json.loads(result.stdout)["ok"] is False
    assert b"Started:" not in result.stderr and b"elapsed" not in result.stderr, result.stderr
    result = subprocess.run(
        [binary, "nested:steps"], cwd=root, env=env, capture_output=True, timeout=180
    )
    assert b"Started:" not in result.stdout + result.stderr, (result.stdout, result.stderr)
    assert b"elapsed" not in result.stdout + result.stderr, (result.stdout, result.stderr)


with tempfile.TemporaryDirectory(prefix="enested.") as directory:
    base = Path(directory)
    home = base / "home"
    root = base / "repo"
    home.mkdir()
    root.mkdir()
    wrapper = root / "scrub-wrapper.py"
    wrapper.write_text(WRAPPER)
    adjacent = root / "adjacent.py"
    adjacent.write_text(ADJACENT)
    env = {key: value for key, value in os.environ.items() if not key.startswith("EFFIGY_")}
    env.update(HOME=str(home), TERM="xterm-256color", NO_COLOR="1")
    env.pop("CI", None)
    (root / "effigy.toml").write_text(
        MANIFEST.replace("__WRAPPER__", str(wrapper))
        .replace("__ADJACENT__", str(adjacent))
        .replace("__BINARY__", binary)
    )

    child, master = start(["dev"], root, env)
    raw = bytearray()
    services = b""
    adjacent = b""
    try:
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            drain(master, raw)
            logs = list(base.rglob("*.stdout.*.log"))
            services = pane_log(logs, b"[fixture] services-tab")
            adjacent = pane_log(logs, b"[fixture] adjacent-tab")
            if b"[error]" in services and b"adjacent-after" in adjacent:
                break
            if child.poll() is not None:
                # The failed workload ends the session; its pane log is already
                # canonical, so read the settled bytes and judge those.
                time.sleep(1.5)
                drain(master, raw)
                logs = list(base.rglob("*.stdout.*.log"))
                services = pane_log(logs, b"[fixture] services-tab")
                adjacent = pane_log(logs, b"[fixture] adjacent-tab")
                break
            time.sleep(0.05)
        assert b"nested-error-line" in services, (bytes(raw[-6000:]), services, logs)
        assert b"[error]" in services, (bytes(raw[-6000:]), services, logs)
        assert b"adjacent-before\n" in adjacent and b"adjacent-after\n" in adjacent, adjacent
        # The canonical host log stays writer-sanitized plain text; the pane
        # half of the fixture proves the rich stream separately, as the TUI's
        # own screen stream carries the escapes it drew.
        assert b"Vite error" in adjacent and b"\x1b" not in adjacent, adjacent
        assert b"\x1b" in bytes(raw), bytes(raw[-6000:])
        assert b"\r" not in services, services

        frames = services.count(b"elapsed")
        summary = {
            "nested_header": b"EFFIGY" in services,
            "frame_lines": frames,
            "started_lines": services.count(b"Started: "),
            "wait_step_lines": services.count(b"waiting for step `"),
            "longest_line": max((len(line) for line in services.splitlines()), default=0),
            "error_line": b"nested-error-line" in services,
            "duplicate_lines": len(services.splitlines()) - len(set(services.splitlines())),
            "repeated_lines": max(
                (services.splitlines().count(line) for line in services.splitlines()),
                default=0,
            ),
            "indented_lines": sum(
                1 for line in services.splitlines() if line[:1] == b" "
            ),
        }
        # Retain the evidence before judging it, so a pre-fix run keeps its
        # raw stream and canonical pane log as well as its failure.
        if evidence:
            (evidence / "raw-pty.bin").write_bytes(bytes(raw))
            (evidence / "services-pane.log").write_bytes(services)
            (evidence / "adjacent-pane.log").write_bytes(adjacent)
            (evidence / "pane-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        # The same binary keeps live progress on a real terminal, and keeps
        # the piped and `--json` contracts, beside the pane that is bounded.
        standalone = run_standalone(env, root)
        run_piped(env, root)
        if evidence:
            (evidence / "standalone-pty.bin").write_bytes(standalone)
    finally:
        if child.poll() is None:
            child.send_signal(signal.SIGTERM)
            deadline = time.monotonic() + 30
            while child.poll() is None and time.monotonic() < deadline:
                drain(master, raw)
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=15)
            exited = child.returncode
            os.close(master)
            if evidence:
                (evidence / "cancellation.json").write_text(
                    json.dumps({"exit": exited, "raw_tail": bytes(raw[-2000:]).decode("utf-8", "replace")}) + "\n"
                )

    # Cancellation and fixture hygiene: the client stopped, and nothing it
    # started outlives it (observed, never signalled by pattern).
    assert child.poll() is not None, "the client did not stop"
    listing = subprocess.run(
        ["ps", "-eo", "pid=,command="], capture_output=True, text=True, check=False
    ).stdout
    leaked = [line for line in listing.splitlines() if str(base) in line]
    assert not leaked, leaked

    # Bounded start/finish history, one meaningful line per step: no staircase
    # of live frames, no nested CLI banner, no stale frame history. The child's
    # own indented error block is not a frame and stays untouched.
    assert b"EFFIGY" not in services, (summary, services)
    assert frames <= 8, (summary, services)
    lines = services.splitlines()
    frame_lines = [line for line in lines if b"elapsed" in line]
    assert frame_lines and all(not line.startswith(b" ") for line in frame_lines), (
        summary,
        services,
    )
    assert summary["started_lines"] <= 8, (summary, services)
    assert summary["wait_step_lines"] <= 4, (summary, services)
    assert summary["repeated_lines"] <= 2, (summary, services)
    assert summary["longest_line"] <= 400, (summary, services)
    # The failure stays readable: the child's own error line and a bounded
    # failure ending that names the exit code.
    assert b"[error]" in services, services
    assert b"exit code 3" in services, services
    if evidence:
        (evidence / "ok.txt").write_text("ok\n")
