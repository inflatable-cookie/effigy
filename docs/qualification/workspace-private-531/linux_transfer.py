"""Bounded local processes for setup rows, including the contained stdin transfer.

A contained transfer is two local processes that run at once:
- the supervisor start (prlctl exec of the root supervisor), whose payload reads a FIFO
  in the guest stage directory and writes the file, and
- the SSH writer, which waits for the FIFO and streams the bytes into it.

SSH exit is only the writer's result. Closure comes from the supervisor candidate and
the host's VM-route observation (linux_setup.host_credit), never from either process
exiting. Every process runs in its own session; only its group is killed. Output past
the cap, a counter refusal, a deadline or a failed writer marks the row failed.
"""
import json
import os
import select
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_resources as resources
from linux_supervisor import Refusal

MAX_DEADLINE = 1200
# Remote side of the writer. Polls for the FIFO, then streams stdin into it.
WAIT_AND_CAT = ('n=0; while [ ! -p "$1" ]; do n=$((n+1)); [ "$n" -gt "$2" ] && exit 9; sleep 0.1; done; '
                'cat > "$1"')


def writer_remote_argv(fifo, attempts=1200):
    """Remote argv for the writer. `attempts` bounds the FIFO poll (0.1 s each)."""
    if not isinstance(attempts, int) or isinstance(attempts, bool) or not 0 < attempts <= 1200:
        raise Refusal('writer poll attempts out of range')
    return ['/bin/sh', '-c', WAIT_AND_CAT, 'sh', fifo, str(attempts)]


def _kill_group(proc):
    """Kill the process group. An already-exited group reports EPERM or ESRCH on macOS; both mean nothing is left."""
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def drain(procs, timeout, output_cap, monitor=None, poll=0.05):
    """Read the streams of every process until all exit or the deadline or a refusal.

    Returns (failure, exit_codes, streams). failure is None or (disposition, reason).
    """
    if not isinstance(output_cap, int) or output_cap <= 0 or not 0 < timeout <= MAX_DEADLINE:
        raise Refusal('invalid output cap or deadline')
    streams = {proc: {'stdout': bytearray(), 'stderr': bytearray()} for proc in procs}
    owners = {}
    for proc in procs:
        owners[proc.stdout] = (proc, 'stdout')
        owners[proc.stderr] = (proc, 'stderr')
    open_pipes = set(owners)
    failure = None
    started = time.monotonic()
    try:
        while open_pipes and failure is None:
            if time.monotonic() - started > timeout:
                failure = ('timeout', 'deadline reached')
                break
            if monitor is not None:
                try:
                    monitor.check()
                except resources.Refusal as exc:
                    failure = ('resource-refusal', str(exc))
                    break
            ready, _, _ = select.select(list(open_pipes), [], [], poll)
            for pipe in ready:
                chunk = os.read(pipe.fileno(), 65536)
                if not chunk:
                    open_pipes.discard(pipe)
                    continue
                proc, name = owners[pipe]
                streams[proc][name].extend(chunk)
                if len(streams[proc][name]) > output_cap:
                    failure = ('output-cap', 'stream exceeded the cap; partial output is not accepted')
                    break
    finally:
        if failure is not None:
            for proc in procs:
                _kill_group(proc)
        codes = []
        for proc in procs:
            remaining = max(0.1, timeout - (time.monotonic() - started))
            try:
                codes.append(proc.wait(timeout=remaining))
            except subprocess.TimeoutExpired:
                _kill_group(proc)
                codes.append(proc.wait(timeout=10))
                failure = failure or ('timeout', 'process outlived the deadline')
        for proc in procs:
            proc.stdout.close()
            proc.stderr.close()
    if failure is None and monitor is not None:
        try:
            monitor.check()
        except resources.Refusal as exc:
            failure = ('resource-refusal', str(exc))
    return failure, codes, {proc: {k: bytes(v) for k, v in streams[proc].items()} for proc in procs}


def run_transfer(argv, stdin_path, timeout, output_cap, monitor=None, popen=subprocess.Popen):
    """One bounded local process, stdin from a file. Exit status is the waited result, never closure."""
    stdin = open(stdin_path, 'rb') if stdin_path else subprocess.DEVNULL
    try:
        proc = popen(argv, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    finally:
        if stdin_path:
            stdin.close()
    failure, codes, streams = drain([proc], timeout, output_cap, monitor)
    out = streams[proc]
    if failure is not None:
        return {'disposition': failure[0], 'reason': failure[1], 'waited_exit': codes[0],
                'stdout': out['stdout'], 'stderr': out['stderr']}
    disposition = 'exited' if codes[0] == 0 else 'failed'
    return {'disposition': disposition, 'reason': None, 'waited_exit': codes[0],
            'stdout': out['stdout'], 'stderr': out['stderr']}


def run_contained_transfer(supervisor_argv, writer_argv, stdin_path, timeout, output_cap, monitor=None,
                           popen=subprocess.Popen):
    """Supervisor start and SSH writer at once. Returns the supervisor's result line and both exits."""
    stdin = open(stdin_path, 'rb')
    try:
        writer = popen(writer_argv, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    finally:
        stdin.close()
    supervisor = popen(supervisor_argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       start_new_session=True)
    failure, codes, streams = drain([supervisor, writer], timeout, output_cap, monitor)
    sup_codes, writer_code = codes[0], codes[1]
    sup_out = streams[supervisor]
    writer_out = streams[writer]
    result = {'supervisor_exit': sup_codes, 'writer_exit': writer_code, 'writer_stderr': writer_out['stderr'],
              'supervisor_stdout': sup_out['stdout'], 'supervisor_stderr': sup_out['stderr'],
              'candidate': None, 'payload_stdout': '', 'supervisor_result': None}
    if failure is not None:
        result.update(disposition=failure[0], reason=failure[1])
        return result
    parsed = parse_supervisor_result(sup_out['stdout'])
    if parsed is not None:
        result['supervisor_result'] = parsed
        result['candidate'] = parsed.get('candidate')
        result['payload_stdout'] = parsed.get('payload_stdout', '')
    if sup_codes != 0:
        result.update(disposition='failed', reason='supervisor exited nonzero')
    elif writer_code != 0:
        result.update(disposition='writer-failed', reason='SSH writer exited nonzero; not closure evidence')
    else:
        result.update(disposition='exited', reason=None)
    return result


def parse_supervisor_result(stdout):
    """The supervisor prints one JSON object as its last line. Anything else is no result."""
    lines = [line for line in stdout.decode('utf-8', 'replace').splitlines() if line.strip()]
    if not lines:
        return None
    try:
        value = json.loads(lines[-1])
    except ValueError:
        return None
    if not isinstance(value, dict):
        return None
    if 'payload_stdout_b64' in value:
        import base64
        value['payload_stdout'] = base64.b64decode(value.pop('payload_stdout_b64')).decode('utf-8', 'replace')
    return value
