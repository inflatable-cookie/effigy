#!/usr/bin/env python3
"""Real terminal input and owner-death proofs in one disposable checkout."""
from contextlib import contextmanager
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shlex
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="q010-test-tui-") as temporary:
    base = Path(temporary).resolve()
    home = base / "home"
    project = base / "checkout"
    home.mkdir()
    project.mkdir()
    (project / ".git").mkdir()
    env = dict(os.environ, HOME=str(home), TERM="xterm-256color", NO_COLOR="1")
    for key in list(env):
        if key.startswith("EFFIGY_"):
            env.pop(key)
    env["EFFIGY_TEST_BIN"] = str(binary)
    interactive = "stty size > geometry; printf READY; touch ready; read answer; printf '%s' \"$answer\" > answer"
    death_program = "import os,signal,stat,time; from pathlib import Path; assert stat.S_ISFIFO(os.fstat(10).st_mode); signal.signal(signal.SIGTERM, signal.SIG_IGN); signal.signal(signal.SIGHUP, signal.SIG_IGN); signal.signal(signal.SIGINT, signal.SIG_IGN); Path('suite-pid').write_text(str(os.getpid())); time.sleep(15)"
    death = "exec " + shlex.quote(sys.executable) + " -c " + shlex.quote(death_program)
    (project / "effigy.toml").write_text(
        '[tasks.nested]\nrun = ' + json.dumps('"$EFFIGY_TEST_BIN" test death --tui') +
        '\n[test.suites.local]\nrun = ' + json.dumps(interactive) +
        '\n[test.suites.death]\ndefault = false\nrun = ' + json.dumps(death) + '\n')

    def gates_free():
        for path in project.glob(".effigy/state/gates/*/*.gate"):
            with path.open("rb") as gate:
                try:
                    fcntl.flock(gate, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    return False
                finally:
                    fcntl.flock(gate, fcntl.LOCK_UN)
        return True

    @contextmanager
    def terminal_session(args):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0))
        def own_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        child = subprocess.Popen([str(binary), *args], cwd=project, env=env,
                                 stdin=slave, stdout=slave, stderr=slave, preexec_fn=own_terminal)
        os.close(slave)
        transcript = bytearray()
        cursor_answers = [0]
        def drain():
            if select.select([master], [], [], .02)[0]:
                try:
                    transcript.extend(os.read(master, 65536))
                    queries = transcript.count(b"\x1b[6n")
                    while cursor_answers[0] < queries:
                        os.write(master, b"\x1b[1;1R")
                        cursor_answers[0] += 1
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
        def until(predicate, label):
            deadline = time.monotonic() + 15
            while not predicate():
                drain()
                assert child.poll() is None, (label, child.returncode, bytes(transcript[-6000:]))
                assert time.monotonic() < deadline, (label, bytes(transcript[-6000:]))
        try:
            yield child, master, transcript, drain, until
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=12)
            deadline = time.monotonic() + 18
            while not gates_free() and time.monotonic() < deadline:
                drain()
            os.close(master)
            assert gates_free(), "custody gates remain held after fixture cleanup"

    with terminal_session(["test", "local", "--tui"]) as (child, master, transcript, drain, until):
        until(lambda: (project / "ready").exists(), "suite readiness")
        assert (project / "geometry").read_text().strip() == "32 100", "suite PTY geometry"
        os.write(master, b"ihello\r")
        until(lambda: (project / "answer").exists(), "suite input")
        assert (project / "answer").read_text() == "hello"
        deadline = time.monotonic() + 15
        while child.poll() is None:
            drain()
            try:
                os.write(master, b"\x1b")
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                child.wait(timeout=10)
                break
            time.sleep(.1)
            assert time.monotonic() < deadline, bytes(transcript[-6000:])
        assert child.returncode == 0, bytes(transcript[-6000:])

    # An operator stop is a cancellation, not a failure and never a pass: the
    # suite the session ended is reported `cancelled` and the run exits with
    # the interrupted code.
    (project / "suite-pid").unlink(missing_ok=True)
    with terminal_session(["test", "death", "--tui"]) as (child, master, transcript, drain, until):
        until(lambda: (project / "suite-pid").exists(), "cancellable suite readiness")
        deadline = time.monotonic() + 20
        while child.poll() is None:
            drain()
            try:
                os.write(master, b"\x03")
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                child.wait(timeout=10)
                break
            time.sleep(.1)
            assert time.monotonic() < deadline, bytes(transcript[-6000:])
        stopped = bytes(transcript)
        assert child.returncode == 130, (child.returncode, stopped[-6000:])
        assert b"cancelled" in stopped, stopped[-6000:]
        # The dead suite is named by the result it produced, never as a pass.
        assert b"death" in stopped, stopped[-6000:]
    print(json.dumps({"proof": "interactive-test-stop", "exit": 130, "outcome": "cancelled"}))

    def alive(pid):
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False

    for args in [["test", "death", "--tui"], ["nested"]]:
        (project / "suite-pid").unlink(missing_ok=True)
        with terminal_session(args) as (child, master, transcript, drain, until):
            until(lambda: (project / "suite-pid").exists(), "death suite readiness")
            pid = int((project / "suite-pid").read_text())
            outer_gate = None
            if args == ["nested"]:
                for path in project.glob(".effigy/state/gates/*/*.gate"):
                    try:
                        body = json.loads(path.read_bytes())
                    except ValueError:
                        continue
                    if body.get("foreground", {}).get("task", {}).get("task") == "nested":
                        outer_gate = path.open("rb")
                        break
                assert outer_gate is not None, "nested task custody gate"
            started = time.monotonic()
            child.kill()  # Only this proof's recorded owner PID.
            child.wait(timeout=10)
            try:
                while alive(pid):
                    if outer_gate is not None:
                        try:
                            fcntl.flock(outer_gate, fcntl.LOCK_SH | fcntl.LOCK_NB)
                        except BlockingIOError:
                            pass
                        else:
                            fcntl.flock(outer_gate, fcntl.LOCK_UN)
                            assert not alive(pid), "ancestor gate released before nested suite cleanup"
                    drain()
                    assert time.monotonic() - started < 10, "owner death left suite residue"
            finally:
                if outer_gate is not None:
                    outer_gate.close()
            print(json.dumps({"proof": "test-tui-owner-death", "nested": args == ["nested"],
                              "cleanup_seconds": round(time.monotonic() - started, 3)}))
    assert not list(base.rglob("*.db")), "local TUI wrote a database"
    print(json.dumps({"proof": "interactive-local-test", "exit": 0, "database_files": 0,
                      "rows": 32, "columns": 100, "input": "hello"}))
