"""Fixed Colima guest-helper entry: echo, probe, stage, verify.

Runs inside the guest as `python3 -I -B -c <this file> <operation> <nonce> ...`.
Stdlib only. The host binds a one-use nonce before launch; this process records
Linux /proc identity, runs the allowlisted operation as a contained child in the
same session and group, waits, and emits a length-prefixed envelope. A local
client exit is not this file's concern. Missing /proc, a setsid child, an
unreaped child or a remaining group member leaves the trailer unable to close.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ALLOWED = frozenset({'echo', 'probe', 'stage', 'verify'})
NONCE = re.compile(r'^[0-9a-f]{64}$')
MAGIC = b'EFFIGY-COLIMA-GUEST-1\n'
HEADER_KEYS = ('nonce', 'operation', 'boot_id', 'pid', 'start_time', 'pgid', 'session_id', 'source_sha256')
TRAILER_KEYS = ('waited_exit', 'child_pid', 'child_start_time', 'child_pgid', 'child_session_id',
                'group_empty', 'unreaped', 'stdout_sha256', 'stderr_sha256')
PROC = '/proc'


class Refusal(ValueError):
    """Contradictory or unsupported helper input. Not a closure."""


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def format_envelope(header, stdout, stderr, trailer):
    if not isinstance(stdout, (bytes, bytearray)) or not isinstance(stderr, (bytes, bytearray)):
        raise Refusal('inner streams must be bytes')
    return b''.join((
        MAGIC,
        b'HEADER\n', compact(header), b'\n',
        b'STDOUT %d\n' % len(stdout), bytes(stdout),
        b'STDERR %d\n' % len(stderr), bytes(stderr),
        b'TRAILER\n', compact(trailer), b'\n',
    ))


def _take_line(data, offset):
    end = data.find(b'\n', offset)
    if end < 0:
        return None, offset
    return data[offset:end], end + 1


def _take_json(data, offset):
    line, offset = _take_line(data, offset)
    if line is None:
        return None, offset
    try:
        value = json.loads(line.decode())
    except (ValueError, UnicodeDecodeError):
        return None, offset
    if not isinstance(value, dict):
        return None, offset
    return value, offset


def _take_counted(data, offset, label):
    line, offset = _take_line(data, offset)
    prefix = label + b' '
    if line is None or not line.startswith(prefix):
        return None, offset
    count = line[len(prefix):]
    if not count.isdigit():
        return None, offset
    n = int(count)
    if n < 0 or offset + n > len(data):
        return None, offset
    return data[offset:offset + n], offset + n


def parse_envelope(data):
    """Return {header, stdout, stderr, trailer} or None when the envelope is incomplete."""
    if not isinstance(data, (bytes, bytearray)):
        return None
    data = bytes(data)
    if not data.startswith(MAGIC):
        return None
    offset = len(MAGIC)
    line, offset = _take_line(data, offset)
    if line != b'HEADER':
        return None
    header, offset = _take_json(data, offset)
    stdout, offset = _take_counted(data, offset, b'STDOUT')
    stderr, offset = _take_counted(data, offset, b'STDERR')
    line, offset = _take_line(data, offset)
    if line != b'TRAILER' or header is None or stdout is None or stderr is None:
        return None
    trailer, offset = _take_json(data, offset)
    if trailer is None or offset != len(data):
        return None
    return {'header': header, 'stdout': stdout, 'stderr': stderr, 'trailer': trailer}


def linux_identity(pid, proc=PROC):
    """boot_id plus /proc/<pid>/stat start_time, pgrp and session. Missing /proc refuses."""
    boot_path = Path(proc) / 'sys/kernel/random/boot_id'
    stat_path = Path(proc) / str(pid) / 'stat'
    try:
        boot = boot_path.read_text().strip()
        text = stat_path.read_text()
    except OSError as exc:
        raise Refusal('guest boot or process identity unavailable') from exc
    fields = text[text.rfind(')') + 2:].split()
    if not boot or len(fields) < 20:
        raise Refusal('guest boot or process identity unavailable')
    return {'boot_id': boot, 'pid': int(pid), 'pgid': int(fields[2]), 'session_id': int(fields[3]),
            'start_time': int(fields[19])}


def group_members(pgid, exclude, proc=PROC):
    """Pids in this process group. None means the walk was incomplete."""
    root = Path(proc)
    try:
        entries = list(root.iterdir())
    except OSError:
        return None
    found = []
    for entry in entries:
        if not entry.name.isdigit():
            continue
        pid = int(entry.name)
        if pid in exclude:
            continue
        try:
            text = (entry / 'stat').read_text()
        except OSError:
            return None
        fields = text[text.rfind(')') + 2:].split()
        if len(fields) < 4:
            return None
        if int(fields[2]) == pgid:
            found.append(pid)
    return found


def own_source_sha256():
    """Bytes of this helper as launched: the -c program via linecache, else the file."""
    import linecache
    lines = linecache.getlines('<string>')
    if lines:
        return hashlib.sha256(''.join(lines).encode()).hexdigest()
    path = sys.argv[0] if sys.argv else ''
    if path not in ('-c', '', '<stdin>') and os.path.isfile(path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    raise Refusal('helper source bytes unavailable')


def evaluate_guest_closure(intent, header, trailer, inner_stdout, inner_stderr):
    """Closed only with identity-bound waited child exit 0, same session/group, empty group.

    Contradictory nonce, operation, source hash or inner stream hash raises. Missing,
    unreaped, setsid, leftover group or non-zero wait stays unknown.
    """
    if header is None or trailer is None:
        return 'unknown'
    if any(k not in header for k in HEADER_KEYS) or any(k not in trailer for k in TRAILER_KEYS):
        return 'unknown'
    if header.get('nonce') != intent.get('nonce'):
        raise Refusal('guest nonce mismatch')
    if header.get('operation') != intent.get('operation'):
        raise Refusal('guest operation mismatch')
    if header.get('source_sha256') != intent.get('source_sha256'):
        raise Refusal('guest helper source hash mismatch')
    if intent.get('boot_id') and header.get('boot_id') != intent['boot_id']:
        raise Refusal('guest boot identity drift')
    for key in ('pid', 'start_time', 'pgid', 'session_id'):
        if type(header.get(key)) is not int or isinstance(header.get(key), bool) or header[key] <= 0:
            raise Refusal('guest process identity invalid')
    if type(trailer.get('waited_exit')) is not int or isinstance(trailer.get('waited_exit'), bool):
        return 'unknown'
    if trailer.get('unreaped') is not False:
        return 'unknown'
    if trailer.get('child_pgid') != header.get('pgid') or trailer.get('child_session_id') != header.get('session_id'):
        return 'unknown'
    if trailer.get('group_empty') is not True:
        return 'unknown'
    if hashlib.sha256(inner_stdout).hexdigest() != trailer.get('stdout_sha256'):
        raise Refusal('inner stdout hash mismatch')
    if hashlib.sha256(inner_stderr).hexdigest() != trailer.get('stderr_sha256'):
        raise Refusal('inner stderr hash mismatch')
    if trailer.get('waited_exit') != 0:
        return 'unknown'
    return 'closed'


def inner_argv(operation, rest):
    """Exact child argv for one allowlisted operation. rest is the helper argv after nonce."""
    if operation == 'echo':
        if len(rest) != 1:
            raise Refusal('echo takes the route-echo payload only')
        return [sys.executable, '-I', '-B', '-c',
                'import sys; sys.stdout.buffer.write(sys.argv[1].encode()); sys.stdout.buffer.flush()',
                rest[0]]
    if len(rest) < 1:
        raise Refusal(operation + ' requires the pinned inner source')
    source = rest[0]
    if operation == 'probe':
        if len(rest) != 1:
            raise Refusal('probe takes the pinned source only')
        return [sys.executable, '-I', '-B', '-c', source, 'probe']
    if operation == 'stage':
        if len(rest) != 2:
            raise Refusal('stage takes the pinned source and the guest spec')
        return [sys.executable, '-I', '-B', '-c', source, 'stage', rest[1]]
    if operation == 'verify':
        if len(rest) != 2:
            raise Refusal('verify takes the pinned source and the verify spec')
        return [sys.executable, '-I', '-B', '-c', source, 'verify', rest[1]]
    raise Refusal('unknown guest helper operation')


def run_contained(argv, stdin_bytes=None, proc=PROC):
    """Spawn the payload in this session, wait, and report identity plus wait facts."""
    parent = linux_identity(os.getpid(), proc=proc)
    child = subprocess.Popen(argv, stdin=subprocess.PIPE if stdin_bytes is not None else subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=False)
    try:
        child_id = linux_identity(child.pid, proc=proc)
    except Refusal:
        child_id = {'pid': child.pid, 'start_time': None, 'pgid': None, 'session_id': None, 'boot_id': None}
    out, err = child.communicate(stdin_bytes)
    waited = child.returncode
    remaining = group_members(parent['pgid'], exclude={os.getpid()}, proc=proc)
    trailer = {
        'waited_exit': waited,
        'child_pid': child_id['pid'],
        'child_start_time': child_id['start_time'],
        'child_pgid': child_id['pgid'],
        'child_session_id': child_id['session_id'],
        'group_empty': remaining == [],
        'unreaped': waited is None,
        'stdout_sha256': hashlib.sha256(out).hexdigest(),
        'stderr_sha256': hashlib.sha256(err).hexdigest(),
    }
    return parent, out, err, trailer


def operation_argv(argv):
    """Strip the launcher. `python3 -c SOURCE op ...` leaves argv[0] == '-c'."""
    if argv and argv[0] == '-c':
        return argv[1:]
    return argv[1:]


def main(argv=None):
    args = operation_argv(list(argv if argv is not None else sys.argv))
    if len(args) < 2:
        raise Refusal('fixed guest helper requires operation and nonce')
    operation, nonce, rest = args[0], args[1], args[2:]
    if operation not in ALLOWED:
        raise Refusal('step is not a fixed Colima guest helper')
    if not isinstance(nonce, str) or not NONCE.fullmatch(nonce):
        raise Refusal('nonce must be 256-bit lowercase hexadecimal')
    source_sha = own_source_sha256()
    stdin_bytes = None
    if operation == 'stage':
        stdin_bytes = sys.stdin.buffer.read(8193)
    parent, out, err, trailer = run_contained(inner_argv(operation, rest), stdin_bytes=stdin_bytes)
    header = dict(nonce=nonce, operation=operation, source_sha256=source_sha,
                  boot_id=parent['boot_id'], pid=parent['pid'], start_time=parent['start_time'],
                  pgid=parent['pgid'], session_id=parent['session_id'])
    sys.stdout.buffer.write(format_envelope(header, out, err, trailer))
    sys.stdout.buffer.flush()
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except Exception as exc:
        sys.stderr.write('refused: ' + str(exc) + '\n')
        sys.exit(2)
