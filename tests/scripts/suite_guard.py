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
import ctypes
import os
import secrets
import signal
import subprocess
import sys
import time

TERM_GRACE = 2.0
KILL_GRACE = 2.0
PROCESS_WATCH_INTERVAL = 0.05
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


def _process_table():
    """One process snapshot with parent, birth time, and arguments."""
    try:
        listing = subprocess.run(
            ["ps", "-axo", "pid=,ppid=,lstart=,args="],
            capture_output=True, text=True, timeout=10,
        )
    except (OSError, subprocess.SubprocessError):
        return {}
    table = {}
    for line in listing.stdout.splitlines():
        fields = line.split(None, 7)
        if len(fields) != 8:
            continue
        try:
            pid, parent = int(fields[0]), int(fields[1])
        except ValueError:
            continue
        table[pid] = (parent, " ".join(fields[2:7]), fields[7])
    return table


def _test_filter(command):
    """Name one exact Rust test process from its test-harness argv."""
    arguments = command.split()
    if not arguments or "/deps/" not in arguments[0] or "--exact" not in arguments:
        return None
    return next(
        (argument for argument in arguments[1:] if not argument.startswith("-")),
        None,
    )


def _test_marker(command):
    marker = "--effigy-suite-test="
    if marker not in command:
        return None
    return command.split(marker, 1)[1].split()[0]


def _record_test_owners():
    """Remember each live test process's name for its current descendants."""
    table = _process_table()
    roots = {
        pid: name
        for pid, (_, _, command) in table.items()
        if (name := (_test_marker(command) or _test_filter(command))) is not None
    }
    owners = {}
    for pid, (_, start, _) in table.items():
        current = pid
        seen = set()
        while current in table and current not in seen:
            seen.add(current)
            if current in roots:
                owners[pid] = (start, roots[current])
                break
            current = table[current][0]
    return owners


def _test_name(pid, test_owners, process_table):
    """Read the test marker recorded in a child argv or environment."""
    try:
        if sys.platform == "darwin":
            libc = ctypes.CDLL(None, use_errno=True)
            mib = (ctypes.c_int * 3)(1, 49, pid)  # CTL_KERN, KERN_PROCARGS2
            size = ctypes.c_size_t()
            if libc.sysctl(mib, 3, None, ctypes.byref(size), None, 0) != 0:
                return "unknown test"
            arguments = ctypes.create_string_buffer(size.value)
            if libc.sysctl(
                mib, 3, arguments, ctypes.byref(size), None, 0
            ) != 0:
                return "unknown test"
            entries = arguments.raw[:size.value].split(b"\0")
        else:
            with open(f"/proc/{pid}/cmdline", "rb") as command_line:
                entries = command_line.read().split(b"\0")
        marker = b"--effigy-suite-test="
        for entry in entries:
            if marker in entry:
                return entry.split(marker, 1)[1].decode(errors="replace").split()[0]
        if sys.platform == "darwin":
            for prefix in (b"NEXTEST_TEST_NAME=", b"EFFIGY_TEST_NAME="):
                match = next(
                    (entry for entry in entries if entry.startswith(prefix)),
                    None,
                )
                if match is not None:
                    return match[len(prefix):].decode(errors="replace")
        else:
            with open(f"/proc/{pid}/environ", "rb") as environ:
                entries = environ.read().split(b"\0")
            for prefix in (b"EFFIGY_TEST_NAME=", b"NEXTEST_TEST_NAME="):
                match = next(
                    (entry for entry in entries if entry.startswith(prefix)),
                    None,
                )
                if match is not None:
                    return match[len(prefix):].decode(errors="replace")
    except (OSError, subprocess.SubprocessError):
        pass
    recorded = test_owners.get(pid)
    current = process_table.get(pid)
    if recorded is not None and current is not None and recorded[0] == current[1]:
        return recorded[1]
    return "unknown test"


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


def sweep(marker, marker_path, test_owners):
    """Kill every process still carrying the suite, and report what was found."""
    process_table = _process_table()
    found = {
        pid: (_test_name(pid, test_owners, process_table), _describe(pid))
        for pid in holders(marker, marker_path)
    }
    if not found:
        return {}
    for pid in found:
        _signal(pid, signal.SIGTERM)
    remaining = _wait_until_gone(found, TERM_GRACE)
    for pid in remaining:
        _signal(pid, signal.SIGKILL)
    survivors = _wait_until_gone(remaining, KILL_GRACE)
    for pid, (test_name, command) in sorted(found.items()):
        print(
            f"leaked test process: {pid} (test {test_name}; {command})",
            file=sys.stderr,
        )
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

    test_owners = _record_test_owners()
    deadline = None if args.timeout is None else time.monotonic() + args.timeout
    try:
        while True:
            wait_for = PROCESS_WATCH_INTERVAL
            if deadline is not None:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise subprocess.TimeoutExpired(command, args.timeout)
                wait_for = min(wait_for, remaining)
            try:
                code = child.wait(timeout=wait_for)
                break
            except subprocess.TimeoutExpired:
                test_owners.update(_record_test_owners())
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

    test_owners.update(_record_test_owners())
    leaked = sweep(marker, marker_path, test_owners)
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
