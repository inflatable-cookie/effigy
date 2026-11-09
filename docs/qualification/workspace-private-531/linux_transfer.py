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
import hashlib
import json
import os
import re
import select
import signal
import stat
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_resources as resources
from linux_supervisor import Refusal

MAX_DEADLINE = 1200
WRITER_TOOLS = ('/usr/bin/env', '/bin/sh', '/bin/sleep', '/bin/cat')
WRITER_TOOL_MAX_BYTES = 32 * 1024 * 1024
# Remote side of the writer. Polls for the FIFO, then streams stdin into it.
WAIT_AND_CAT = ('n=0; while [ ! -p "$1" ]; do n=$((n+1)); [ "$n" -gt "$2" ] && exit 9; /bin/sleep 0.1; done; '
                '/bin/cat > "$1"')


def writer_remote_argv(fifo, attempts=1200):
    """Remote argv for the writer. `attempts` bounds the FIFO poll (0.1 s each)."""
    if not isinstance(attempts, int) or isinstance(attempts, bool) or not 0 < attempts <= 1200:
        raise Refusal('writer poll attempts out of range')
    return ['/usr/bin/env', '-i', '/bin/sh', '-c', WAIT_AND_CAT, 'sh', fifo, str(attempts)]


def _read_writer_tool_identity(path, owner, boundary):
    """Read one tool under a custody boundary. Private fixture seam; never authority."""
    boundary = os.path.realpath(boundary)
    if not os.path.isabs(path) or os.path.commonpath((boundary, path)) != boundary:
        raise Refusal('writer tool is outside its custody boundary')
    canonical = os.path.realpath(path)
    if os.path.commonpath((boundary, canonical)) != boundary:
        raise Refusal('writer tool resolves outside its custody boundary')

    def check_chain(parent):
        current = parent
        while True:
            info = os.lstat(current)
            if info.st_uid != owner or (not stat.S_ISDIR(info.st_mode) and not stat.S_ISLNK(info.st_mode)):
                raise Refusal('writer tool directory custody differs')
            if stat.S_ISDIR(info.st_mode) and info.st_mode & 0o022:
                raise Refusal('writer tool directory is writable by others')
            if current == boundary:
                break
            next_path = os.path.dirname(current)
            if next_path == current or os.path.commonpath((boundary, next_path)) != boundary:
                raise Refusal('writer tool directory escaped its boundary')
            current = next_path

    check_chain(os.path.dirname(path))
    check_chain(os.path.dirname(canonical))
    original = os.lstat(path)
    if original.st_uid != owner or not (stat.S_ISREG(original.st_mode) or stat.S_ISLNK(original.st_mode)):
        raise Refusal('writer tool path custody differs')
    fd = os.open(canonical, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != owner or before.st_mode & 0o022 or \
                not before.st_mode & 0o111 or before.st_nlink != 1 or before.st_size > WRITER_TOOL_MAX_BYTES:
            raise Refusal('writer executable custody or size differs')
        digest = hashlib.sha256()
        total = 0
        with os.fdopen(fd, 'rb', closefd=False) as source:
            while True:
                chunk = source.read(min(65536, WRITER_TOOL_MAX_BYTES + 1 - total))
                if not chunk:
                    break
                total += len(chunk)
                if total > WRITER_TOOL_MAX_BYTES:
                    raise Refusal('writer executable grew beyond the size ceiling')
                digest.update(chunk)
        after = os.fstat(fd)
        named = os.lstat(canonical)
        requested_after = os.lstat(path)
        identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns,
                                 item.st_mode, item.st_uid, item.st_nlink)
        if identity(before) != identity(after) or identity(after) != identity(named) or \
                identity(original) != identity(requested_after) or os.path.realpath(path) != canonical:
            raise Refusal('writer executable changed while observed')
        return {'canonical': canonical, 'dev': before.st_dev, 'ino': before.st_ino,
                'uid': before.st_uid, 'mode': stat.S_IMODE(before.st_mode), 'nlink': before.st_nlink,
                'size': before.st_size,
                'sha256': digest.hexdigest()}
    finally:
        os.close(fd)


def _writer_tool_identity(path, owner, boundary):
    try:
        return _read_writer_tool_identity(path, owner, boundary)
    except OSError as exc:
        raise Refusal('writer tool custody or content is unavailable') from exc


def fixed_writer_tool_observations():
    """Read only the four fixed guest paths; originals alone do not admit a writer."""
    return {path: _writer_tool_identity(path, 0, '/') for path in WRITER_TOOLS}


def _compare_writer_tool_observations(before, after, owner):
    if not isinstance(before, dict) or not isinstance(after, dict) or \
            set(before) != set(WRITER_TOOLS) or set(after) != set(WRITER_TOOLS) or before != after:
        raise Refusal('writer tool observation set or identity changed')
    fields = {'canonical', 'dev', 'ino', 'uid', 'mode', 'nlink', 'size', 'sha256'}
    for record in before.values():
        if not isinstance(record, dict) or set(record) != fields:
            raise Refusal('writer tool observation is incomplete')
        path, digest = record['canonical'], record['sha256']
        if not isinstance(path, str) or not os.path.isabs(path) or os.path.normpath(path) != path or \
                not isinstance(digest, str) or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
            raise Refusal('writer tool canonical path or digest is invalid')
        if any(type(record[key]) is not int for key in ('dev', 'ino', 'uid', 'mode', 'nlink', 'size')) or \
                record['uid'] != owner or record['dev'] < 0 or record['ino'] <= 0 or record['nlink'] != 1 or \
                not 0 <= record['size'] <= WRITER_TOOL_MAX_BYTES or not 0 <= record['mode'] <= 0o7777 or \
                record['mode'] & 0o022 or \
                not record['mode'] & 0o111:
            raise Refusal('writer tool observation has invalid custody')


def compare_writer_tool_observations(before, after):
    """Compare exact independent root-owned observations; this grants no authority."""
    _compare_writer_tool_observations(before, after, 0)


def writer_tool_main(argv):
    """One fixed read-only entry; its output is evidence, never permission."""
    if argv != ['--observe-writer-tools']:
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    try:
        boot = Path('/proc/sys/kernel/random/boot_id').read_text(encoding='ascii').strip()
        if not re.fullmatch(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', boot):
            raise Refusal('boot identity is malformed')
        tools = fixed_writer_tool_observations()
    except (OSError, UnicodeError, Refusal) as exc:
        print('status=refused phase=writer-tools reason=' + str(exc), file=sys.stderr)
        return 3
    print(json.dumps({'boot_id': boot, 'tools': tools}, sort_keys=True))
    return 0


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
    result = {'supervisor_exit': sup_codes, 'writer_exit': writer_code, 'writer_stderr': writer_out['stderr'], 'writer_stdout': writer_out['stdout'],
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


if __name__ == '__main__':
    sys.exit(writer_tool_main(sys.argv[1:]))
