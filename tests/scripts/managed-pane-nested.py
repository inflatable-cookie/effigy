#!/usr/bin/env python3
"""A Services-style pane, nested Effigy child, through the real PTY.

The consumer shape this reproduces (effigy#729): a managed TUI tab runs a
program that starts Effigy as its own child with an environment scrubbed of
`EFFIGY_INTERNAL_*` (the shipped bundle's `profile_env.py` does exactly that).
That nested Effigy is inside the pane, so its output belongs to the pane's
canonical plain log: one bounded start and one finish per step, no CLI banner,
no spinner redraw frames, and the failure readable.

The shape runs twice against the same binary: at a wide terminal and at a
narrow one, because a pane's width is the child's width and a frame pile is
worse there. A concurrent non-Effigy tab proves unrelated output and colour
survive, and the same binary is also run standalone (live progress must still
animate) and piped/`--json` (contracts unchanged).

Usage: managed-pane-nested.py <effigy-binary> [<evidence-dir>]

With an evidence directory the fixture also retains, per width, the raw PTY
stream and the canonical pane logs, plus the standalone stream and a summary.
"""
import errno
import fcntl
import json
import os
import pty
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

# A long single line, wider than either terminal, printed by one step: the pane
# log is plain text, so it must arrive whole and exactly once.
LONG_LINE = '''#!/usr/bin/env python3
import subprocess

print("nested-middle", flush=True)
print("nested-long:" + "x" * 300, flush=True)
size = subprocess.run(["stty", "size"], capture_output=True, text=True)
print("nested-size:" + size.stdout.strip(), flush=True)
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
  { task = "nested:long" },
  { task = "nested:fail" },
]
[tasks."nested:wait"]
run_in = "host"
run = "sleep 4; printf 'nested-waited\\n'"
[tasks."nested:long"]
run_in = "host"
run = "python3 __LONG_LINE__"
[tasks."nested:fail"]
run_in = "host"
run = "printf 'nested-error-line\\n'; exit 3"
'''

STEPS = (
    b"1/3: nested:wait",
    b"2/3: nested:long",
    b"3/3: nested:fail",
)


def start(args, cwd, env, cols, rows):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

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


def measure(services, adjacent, rows, cols):
    lines = services.splitlines()
    return {
        "nested_header": b"EFFIGY" in services,
        "frame_lines": services.count(b"elapsed"),
        "started_lines": services.count(b"Started: "),
        "finished_lines": services.count("✓ nested:steps step".encode()),
        "failed_lines": services.count("✗ nested:steps step".encode()),
        "wait_step_lines": services.count(b"waiting for step `"),
        "longest_line": max((len(line) for line in lines), default=0),
        "long_line_intact": services.count(b"nested-long:" + b"x" * 300) == 1,
        "child_terminal": (
            b"nested-size:" + ("%d %d" % (rows, cols)).encode() in services
        ),
        "error_line": b"nested-error-line" in services,
        "duplicate_lines": len(lines) - len(set(lines)),
        "repeated_lines": max((lines.count(line) for line in lines), default=0),
        "indented_lines": sum(1 for line in lines if line[:1] == b" "),
        "adjacent_ordered": b"adjacent-before\n" in adjacent
        and b"adjacent-after\n" in adjacent
        and adjacent.index(b"adjacent-before\n") < adjacent.index(b"adjacent-after\n"),
        "adjacent_plain": b"Vite error" in adjacent and b"\x1b" not in adjacent,
    }


def stop(client, master, raw):
    """Stop the client this fixture started, by its own PID."""
    if client.poll() is not None:
        return client.returncode
    client.send_signal(signal.SIGTERM)
    deadline = time.monotonic() + 30
    while client.poll() is None and time.monotonic() < deadline:
        drain(master, raw)
    if client.poll() is None:
        os.killpg(client.pid, signal.SIGKILL)
        client.wait(timeout=15)
    return client.returncode


def scenario_base(temp, label):
    """A scenario's own checkout and state home: the wide and narrow runs must
    not read or tear down each other's run records."""
    base = temp / label
    home = base / "home"
    root = base / "repo"
    home.mkdir(parents=True)
    root.mkdir()
    for script, text in (
        ("scrub-wrapper.py", WRAPPER),
        ("adjacent.py", ADJACENT),
        ("long-line.py", LONG_LINE),
    ):
        (root / script).write_text(text)
    env = {key: value for key, value in os.environ.items() if not key.startswith("EFFIGY_")}
    env.update(HOME=str(home), TERM="xterm-256color", NO_COLOR="1")
    env.pop("CI", None)
    (root / "effigy.toml").write_text(
        MANIFEST.replace("__WRAPPER__", str(root / "scrub-wrapper.py"))
        .replace("__ADJACENT__", str(root / "adjacent.py"))
        .replace("__LONG_LINE__", str(root / "long-line.py"))
        .replace("__BINARY__", binary)
    )
    return base, root, env


def pane_scenario(temp, label, cols, rows, evidence_dir):
    """One real `effigy dev` TUI whose Services tab starts a scrubbed Effigy."""
    base, root, env = scenario_base(temp, label)
    client, master = start(["dev"], root, env, cols, rows)
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
            if client.poll() is not None:
                # The failed workload ends the session; its pane log is already
                # canonical, so read the settled bytes and judge those.
                time.sleep(1.5)
                drain(master, raw)
                logs = list(base.rglob("*.stdout.*.log"))
                services = pane_log(logs, b"[fixture] services-tab")
                adjacent = pane_log(logs, b"[fixture] adjacent-tab")
                break
            time.sleep(0.05)
    finally:
        exited = stop(client, master, raw)
        os.close(master)
    summary = measure(services, adjacent, rows, cols)
    summary["terminal"] = {"cols": cols, "rows": rows}
    summary["client_exit"] = exited
    if evidence_dir:
        (evidence_dir).mkdir(parents=True, exist_ok=True)
        (evidence_dir / "raw-pty.bin").write_bytes(bytes(raw))
        (evidence_dir / "services-pane.log").write_bytes(services)
        (evidence_dir / "adjacent-pane.log").write_bytes(adjacent)
        (evidence_dir / "pane-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary, services, adjacent


def assert_bounded(label, summary, services, adjacent):
    """Bounded start/finish history, one meaningful line per step."""
    lines = services.splitlines()
    detail = (label, summary, services)
    # One start and one ending per step, named by the step it belongs to. This
    # is the defect's strongest signal: the pre-fix pane has no start lines at
    # all, only a pile of live frames.
    for step in STEPS:
        assert services.count(b"Started: running nested:steps step " + step) == 1, detail
    assert summary["started_lines"] == 3, detail
    assert summary["finished_lines"] == 2, detail
    assert summary["failed_lines"] == 1, detail
    assert b"EFFIGY" not in services, detail
    assert summary["frame_lines"] <= 8, detail
    # A frame line is a start line here, so none may carry stale indentation.
    frame_lines = [line for line in lines if b"elapsed" in line]
    assert frame_lines and all(not line.startswith(b" ") for line in frame_lines), detail
    assert summary["wait_step_lines"] <= 4, detail
    assert summary["repeated_lines"] <= 2, detail
    assert summary["longest_line"] <= 400, detail
    # A line wider than the terminal arrives whole and exactly once, and the
    # child really ran at this pane's size: the narrow case is a narrow child,
    # not the wide one twice.
    assert summary["long_line_intact"], detail
    assert summary["child_terminal"], detail
    # The failure stays readable: the child's own error line, a bounded failure
    # ending that names the exit code, and no torn transport bytes.
    assert summary["error_line"], detail
    assert b"[error]" in services and b"exit code 3" in services, detail
    assert b"\r" not in services, detail
    # The concurrent non-Effigy tab is untouched: ordered lines, plain capture,
    # with the rich stream proved on the PTY side.
    assert summary["adjacent_ordered"], (label, adjacent)
    assert summary["adjacent_plain"], (label, adjacent)


def run_standalone(root, env):
    """A child is bounded only inside a pane: standalone still animates."""
    client, master = start(["nested:steps"], root, env, 120, 40)
    output = bytearray()
    try:
        deadline = time.monotonic() + 60
        while client.poll() is None and time.monotonic() < deadline:
            drain(master, output)
        for _ in range(5):
            drain(master, output)
        assert client.poll() == 3, (client.poll(), bytes(output[-4000:]))
    finally:
        if client.poll() is None:
            os.killpg(client.pid, signal.SIGKILL)
            client.wait(timeout=15)
        os.close(master)
    text = bytes(output)
    frames = text.count(b"elapsed")
    assert frames >= 5, ("standalone progress stopped", frames, text[-4000:])
    assert b"EFFIGY" in text, text[-4000:]
    return text


def run_piped(root, env):
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


def wait_until_quiet(temp, deadline):
    """No process this fixture started may still name its own directory."""
    while time.monotonic() < deadline:
        listing = subprocess.run(
            ["ps", "-eo", "pid=,command="], capture_output=True, text=True, check=False
        ).stdout
        leaked = [line for line in listing.splitlines() if str(temp) in line]
        if not leaked:
            return []
        time.sleep(0.2)
    return leaked


def remove_tree(path):
    """Remove this fixture's own directory once nothing writes into it."""
    import shutil

    for _ in range(50):
        try:
            shutil.rmtree(path)
            return
        except FileNotFoundError:
            return
        except OSError:
            time.sleep(0.2)
    shutil.rmtree(path)


temp = Path(tempfile.mkdtemp(prefix="enested."))
results = {}
try:
    wide, wide_services, wide_adjacent = pane_scenario(
        temp, "wide", 120, 40, (evidence / "wide") if evidence else None
    )
    narrow, narrow_services, narrow_adjacent = pane_scenario(
        temp, "narrow", 40, 24, (evidence / "narrow") if evidence else None
    )
    results = {"wide": wide, "narrow": narrow}
    assert_bounded("wide", wide, wide_services, wide_adjacent)
    assert_bounded("narrow", narrow, narrow_services, narrow_adjacent)

    # The same binary keeps live progress on a real terminal, and keeps the
    # piped and `--json` contracts, beside the panes that are bounded.
    _, standalone_root, standalone_env = scenario_base(temp, "standalone")
    standalone = run_standalone(standalone_root, standalone_env)
    run_piped(standalone_root, standalone_env)
    if evidence:
        (evidence / "standalone-pty.bin").write_bytes(standalone)

    # Cancellation and fixture hygiene: every client stopped, and nothing it
    # started outlives it (observed, never signalled by pattern).
    leaked = wait_until_quiet(temp, time.monotonic() + 30)
    assert not leaked, leaked
finally:
    remove_tree(temp)
assert not temp.exists(), temp
print(json.dumps(results, indent=2))
