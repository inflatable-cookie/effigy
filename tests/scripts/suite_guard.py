#!/usr/bin/env python3
"""Run one test selector under a suite kill sweep.

The guard owns one suite identifier. It exports it as `EFFIGY_TEST_SUITE` and
holds a marker descriptor that every descendant inherits, including a process
that starts its own session. When the runner ends - passing, failing, killed by
this guard's timeout, or killed with this guard - the guard kills whatever still
carries the identifier and fails the run. Nothing is recorded and nothing is
repaired: the sweep reads live kernel state once.
"""
import argparse
import os
import secrets
import signal
import subprocess
import sys
import time

TERM_GRACE = 2.0
KILL_GRACE = 2.0
LSOF = "/usr/sbin/lsof"


class Interrupted(Exception):
    def __init__(self, signum):
        self.signum = signum


def _describe(pid):
    """A PID's parent and command line, for the failure report."""
    try:
        listing = subprocess.run(
            ["ps", "-o", "pid=,ppid=,command=", "-p", str(pid)],
            capture_output=True, text=True, timeout=10,
        )
    except (OSError, subprocess.SubprocessError):
        return "unknown command"
    return " ".join(listing.stdout.split()) or "exited"


def _installed_effigy(command):
    """A machine-wide Effigy install is never a suite descendant."""
    return ".local-install" in command


def _holders_macos(marker_path):
    try:
        listing = subprocess.run(
            [LSOF, "-n", "-w", "-Fp", "--", marker_path],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.SubprocessError) as error:
        print(f"suite guard: cannot list marker holders: {error}", file=sys.stderr)
        return set()
    return {
        int(line[1:]) for line in listing.stdout.splitlines()
        if line.startswith("p") and line[1:].isdigit()
    }


def _holders_linux(marker):
    want = f"EFFIGY_TEST_SUITE={marker}".encode()
    pids = set()
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open(f"/proc/{entry}/environ", "rb") as environ:
                if want in environ.read().split(b"\0"):
                    pids.add(int(entry))
        except OSError:
            continue
    return pids


def holders(marker, marker_path):
    """Live PIDs of the current user carrying this suite's identifier."""
    pids = _holders_macos(marker_path) if sys.platform == "darwin" else _holders_linux(marker)
    pids.discard(os.getpid())
    return {pid for pid in pids if not _installed_effigy(_describe(pid))}


def _alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _signal(pid, signum):
    try:
        os.kill(pid, signum)
    except ProcessLookupError:
        pass
    except PermissionError as error:
        print(f"suite guard: cannot signal {pid}: {error}", file=sys.stderr)


def _wait_until_gone(pids, seconds):
    deadline = time.monotonic() + seconds
    remaining = set(pids)
    while remaining and time.monotonic() < deadline:
        time.sleep(0.05)
        remaining = {pid for pid in remaining if _alive(pid)}
    return remaining


def sweep(marker, marker_path):
    """Kill every process still carrying the suite, and report what was found."""
    found = {pid: _describe(pid) for pid in holders(marker, marker_path)}
    if not found:
        return {}
    for pid in found:
        _signal(pid, signal.SIGTERM)
    remaining = _wait_until_gone(found, TERM_GRACE)
    for pid in remaining:
        _signal(pid, signal.SIGKILL)
    survivors = _wait_until_gone(remaining, KILL_GRACE)
    for pid, command in sorted(found.items()):
        print(f"leaked test process: {pid} ({command})", file=sys.stderr)
    for pid in sorted(survivors):
        print(f"unreaped test process: {pid} survived SIGKILL", file=sys.stderr)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=float, default=None)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    # Strip the guard's own `--`; a runner that needs one keeps its own copy.
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a test runner command is required")

    marker = secrets.token_hex(8)
    marker_path = os.path.join(
        os.environ.get("TMPDIR") or "/tmp", f"effigy-suite-{marker}.marker"
    )
    descriptor = os.open(marker_path, os.O_RDWR | os.O_CREAT, 0o600)
    env = dict(os.environ, EFFIGY_TEST_SUITE=marker)
    # Fixture launches must not inherit an outer command's custody nonce.
    for key in ("EFFIGY_CUSTODY_NONCE", "EFFIGY_PROBE_NONCE"):
        env.pop(key, None)

    def interrupted(signum, _frame):
        # A second signal must not stop the sweep that is already running.
        for handled in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
            signal.signal(handled, signal.SIG_IGN)
        raise Interrupted(signum)

    for handled in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(handled, interrupted)

    try:
        child = subprocess.Popen(
            command, env=env, pass_fds=(descriptor,), start_new_session=True
        )
    except OSError as error:
        os.close(descriptor)
        os.unlink(marker_path)
        print(f"suite guard: {error}", file=sys.stderr)
        return 127

    try:
        code = child.wait(timeout=args.timeout)
    except subprocess.TimeoutExpired:
        code = 124
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
    except Interrupted as stop:
        code = 128 + stop.signum
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=KILL_GRACE)
        except subprocess.TimeoutExpired:
            pass

    leaked = sweep(marker, marker_path)
    os.close(descriptor)
    os.unlink(marker_path)
    if leaked:
        print(
            f"suite guard: {len(leaked)} process(es) outlived the test suite",
            file=sys.stderr,
        )
        # A leak always fails the selector, but a timeout keeps its own code.
        return code or 1
    return code


if __name__ == "__main__":
    sys.exit(main())
