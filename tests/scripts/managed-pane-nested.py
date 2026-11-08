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
worse there. A concurrent non-Effigy tab proves unrelated output survives: its
plain lines reach the canonical log and, once that tab is selected, its red
line renders as red SGR on the screen stream. The fixture also stops a running
client on purpose (SIGTERM, clean exit, no forced kill), and runs the same
binary standalone (live progress must still animate) and piped/`--json`
(contracts unchanged).

Usage: managed-pane-nested.py <effigy-binary> [<evidence-dir>]

With an evidence directory the fixture also retains, per width, the raw PTY
stream and the canonical pane logs, plus the standalone stream and a summary.
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

# A long single line, wider than either terminal, printed by one step: the pane
# log is plain text, so it must arrive whole and exactly once.
LONG_LINE = '''#!/usr/bin/env python3
import subprocess

print("nested-middle", flush=True)
print("nested-long:" + "x" * 300, flush=True)
size = subprocess.run(["stty", "size"], capture_output=True, text=True)
print("nested-size:" + size.stdout.strip(), flush=True)
'''

CANCEL_MANIFEST = '''[tasks.dev]
mode = "tui"
concurrent = [
  { name = "hold", run = "printf 'cancel-ready\\n'; sleep 120", start = 1 },
]
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


# A red foreground in any of the forms a renderer may choose: basic, bright or
# an indexed 256-colour red.
RED_SGR = re.compile(rb"\x1b\[(?:31|91|38;5;(?:1|9))(?:;[0-9]+)*m")


def adjacent_colour(raw):
    """Whether the adjacent tab's red line renders as red SGR on screen.

    The TUI draws only the selected tab, so the fixture selects the adjacent
    tab before reading this. The canonical log stays plain beside it.
    """
    at = raw.find(b"Vite error")
    if at < 0:
        return False, -1
    return bool(RED_SGR.search(raw[max(0, at - 48) : at])), at


def measure(services, adjacent, raw, rows, cols):
    coloured, at = adjacent_colour(raw)
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
        "adjacent_rendered": at >= 0,
        "adjacent_coloured": coloured,
    }


def stop(client, master, raw):
    """Stop the client this fixture started, by its own PID.

    Returns `(exit, signalled, escalated)`: whether this fixture asked the
    client to stop, and whether the client needed a forced kill after that.
    """
    if client.poll() is not None:
        return client.returncode, False, False
    client.send_signal(signal.SIGTERM)
    deadline = time.monotonic() + 30
    while client.poll() is None and time.monotonic() < deadline:
        drain(master, raw)
    escalated = False
    if client.poll() is None:
        escalated = True
        os.killpg(client.pid, signal.SIGKILL)
        client.wait(timeout=15)
    return client.returncode, True, escalated


def scenario_base(temp, label, manifest=MANIFEST):
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
    # Colour stays live: the pane renders the adjacent producer's red line as
    # SGR while the canonical log keeps the plain text beside it.
    env = {key: value for key, value in os.environ.items() if not key.startswith("EFFIGY_")}
    env.update(HOME=str(home), TERM="xterm-256color")
    env.pop("CI", None)
    env.pop("NO_COLOR", None)
    (root / "effigy.toml").write_text(
        manifest.replace("__WRAPPER__", str(root / "scrub-wrapper.py"))
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
        # The TUI draws only the selected tab, so select the adjacent one: its
        # red SGR line must reach the rendered screen beside the plain log.
        os.write(master, b"\x1b[C")
        settle = time.monotonic() + 3
        while time.monotonic() < settle and b"Vite error" not in raw:
            drain(master, raw)
        time.sleep(0.3)
        drain(master, raw)
    finally:
        exited, signalled, escalated = stop(client, master, raw)
        os.close(master)
    summary = measure(services, adjacent, bytes(raw), rows, cols)
    summary["terminal"] = {"cols": cols, "rows": rows}
    summary["client_exit"] = exited
    summary["client_signalled"] = signalled
    summary["kill_escalated"] = escalated
    if evidence_dir:
        (evidence_dir).mkdir(parents=True, exist_ok=True)
        (evidence_dir / "raw-pty.bin").write_bytes(bytes(raw))
        (evidence_dir / "services-pane.log").write_bytes(services)
        (evidence_dir / "adjacent-pane.log").write_bytes(adjacent)
        (evidence_dir / "pane-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary, services, adjacent


def cancel_scenario(temp, evidence_dir):
    """A client this fixture stops on purpose: SIGTERM, a clean exit, no kill.

    The failure scenarios cannot assert this deterministically (the failed
    workload may end the session first), so this holds one workload and stops
    the client while it is running.
    """
    base, root, env = scenario_base(temp, "cancel", CANCEL_MANIFEST)
    client, master = start(["dev"], root, env, 120, 40)
    raw = bytearray()
    started = False
    try:
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            drain(master, raw)
            if pane_log(list(base.rglob("*.stdout.*.log")), b"cancel-ready"):
                started = True
                break
            assert client.poll() is None, (client.returncode, bytes(raw[-4000:]))
            time.sleep(0.05)
        assert started, bytes(raw[-4000:])
        exit_code, signalled, escalated = stop(client, master, raw)
    finally:
        if client.poll() is None:
            os.killpg(client.pid, signal.SIGKILL)
            client.wait(timeout=15)
        os.close(master)
    summary = {
        "client_exit": exit_code,
        "client_signalled": signalled,
        "kill_escalated": escalated,
        "workload_started": started,
    }
    if evidence_dir:
        evidence_dir.mkdir(parents=True, exist_ok=True)
        (evidence_dir / "raw-pty.bin").write_bytes(bytes(raw))
        (evidence_dir / "pane-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    assert summary["workload_started"], summary
    assert summary["client_signalled"], summary
    assert not summary["kill_escalated"], summary
    assert summary["client_exit"] == 0, summary
    return summary


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
    # The pane's wait lines are its start lines here, so none may carry the
    # stale indentation a redrawn frame pile leaves behind.
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
    # The concurrent non-Effigy tab is untouched and still rich: its ordered
    # lines reach the canonical log as plain text, and selecting it renders the
    # red line as red SGR on the screen stream.
    assert summary["adjacent_ordered"], (label, adjacent)
    assert summary["adjacent_plain"], (label, adjacent)
    assert summary["adjacent_rendered"], (label, summary)
    assert summary["adjacent_coloured"], (label, summary)
    # The failed workload may end the session before this fixture asks it to
    # stop, so only the forced kill is forbidden here; the dedicated cancel
    # scenario asserts the clean stop itself.
    assert not summary["kill_escalated"], (label, summary)


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
removed = False
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

    # Cancellation: a running client stops cleanly on this fixture's SIGTERM.
    results["cancel"] = cancel_scenario(
        temp, (evidence / "cancel") if evidence else None
    )

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
    # Remove this fixture's own directory only once nothing writes into it; a
    # leaked process keeps its directory so the next reader can inspect it.
    if not wait_until_quiet(temp, time.monotonic() + 10):
        remove_tree(temp)
        removed = True
assert removed, temp
print(json.dumps(results, indent=2))
