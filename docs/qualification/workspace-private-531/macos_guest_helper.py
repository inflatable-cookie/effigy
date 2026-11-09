"""Fixed Colima guest-helper entry: echo, probe, stage, verify, transfer, check-inputs and the four C4
remote commands (build-help, build, read-iidfile, image-inspect).

The host launches this file as
`python3 -I -B -c LAUNCHER <hex(this file)> <operation> <nonce> ...`.
LAUNCHER is a one-argv exec of the hex payload; the helper bytes that cross the
Colima route are [0-9a-f] only. Stdlib only. The host binds a one-use nonce
before launch; this process records Linux /proc identity, runs the allowlisted
operation as a contained child in the same session and group, waits, and emits a
length-prefixed envelope. A local client exit is not this file's concern.
Missing /proc, a failed child-subreaper call, a setsid child, an unreaped child
or a remaining contained process (a live or zombie descendant whose ppid chain
reaches this helper after wait) leaves the trailer unable to close.

The helper sets PR_SET_CHILD_SUBREAPER before spawn so the child's orphans
reparent here instead of init. Tests relocate /proc with an explicit proc=
argument; production reads /proc.

What each operation runs is fixed here, never taken from the caller:
- probe/stage/verify run the pinned guest-context source with the chain spec.
- transfer/check-inputs run the pinned guest-inputs source with the reviewed hex
  spec; transfer inherits this helper's stdin so the framed runtime-input
  stream reaches the child directly and is never buffered here.
- build-help/build/read-iidfile/image-inspect run one exact reviewed command,
  rebuilt from a bounded hex spec and compared before spawn. Their header
  records the command that actually ran, so the host can re-derive it. Daemon
  work behind a command is not this helper's work and is not claimed here.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

PR_SET_CHILD_SUBREAPER = 36

ALLOWED = frozenset({'echo', 'probe', 'stage', 'verify', 'transfer', 'check-inputs', 'build-help', 'build',
                     'read-iidfile', 'image-inspect', 'disposable-check', 'read-check-cid', 'container-list',
                     'nerdctl-version', 'container-inspect', 'container-rm'})
# Operations whose child is the pinned guest-inputs source and one hex spec.
INPUT_MODES = frozenset({'transfer', 'check-inputs'})
# Operations whose child is one exact reviewed remote command, rebuilt from a hex spec.
REMOTE = frozenset({'build-help', 'build', 'read-iidfile', 'image-inspect', 'disposable-check', 'read-check-cid',
                    'container-list', 'nerdctl-version', 'container-inspect', 'container-rm'})
# C5's disposable check: the pinned check source's sha256, the one name shape and the one network-none form.
CHECK_SOURCE_SHA256 = '5fcd965f2339afd885895d4e75684503d6077371089e3025f6c434e2536bdb28'
CHECK_NAME = re.compile(r'^effigy-4f6a-check-[0-9a-f]{8}$')
CONTAINER_ID = re.compile(r'^[0-9a-f]{64}$')
# The reviewed hex spec is bounded: it is the layout, the caps and one fixed argv, never a payload.
SPEC_CAP_BYTES = 64 * 1024
IMAGE_ID = re.compile(r'^sha256:[0-9a-f]{64}$')
CONTEXT = re.compile(r'^/[A-Za-z0-9_.+-]+(?:/[A-Za-z0-9_.+-]+)*/effigy-531-image-[0-9a-f]{32}$')
BUILD_TAG = 'effigy-4f6a-runtime:r1'
NONCE = re.compile(r'^[0-9a-f]{64}$')
HEX = re.compile(r'^[0-9a-f]+$')
# One argv, no $, backticks, newlines or double quotes. Helper bytes travel as hex.
LAUNCHER = "exec(__import__('binascii').unhexlify(__import__('sys').argv[1]))"
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


def resolved_proc(proc=None):
    """Production reads /proc. Tests pass proc=; the environment is not a seam."""
    if proc:
        return proc
    return PROC


def _stat_fields(text):
    fields = text[text.rfind(')') + 2:].split()
    if len(fields) < 20:
        return None
    return {'ppid': int(fields[1]), 'pgid': int(fields[2]), 'session_id': int(fields[3]),
            'start_time': int(fields[19])}


def linux_identity(pid, proc=PROC):
    """boot_id plus /proc/<pid>/stat start_time, pgrp and session. Missing /proc refuses."""
    root = Path(proc)
    boot_path = root / 'sys/kernel/random/boot_id'
    stat_path = root / str(pid) / 'stat'
    if not stat_path.is_file() and int(pid) == os.getpid():
        stat_path = root / 'self' / 'stat'
    try:
        boot = boot_path.read_text().strip()
        text = stat_path.read_text()
    except OSError as exc:
        raise Refusal('guest boot or process identity unavailable') from exc
    parsed = _stat_fields(text)
    if not boot or parsed is None:
        raise Refusal('guest boot or process identity unavailable')
    return {'boot_id': boot, 'pid': int(pid), 'pgid': parsed['pgid'], 'session_id': parsed['session_id'],
            'start_time': parsed['start_time']}


def _walk_proc(proc):
    root = Path(proc)
    try:
        entries = list(root.iterdir())
    except OSError:
        return None
    found = {}
    for entry in entries:
        if not entry.name.isdigit():
            continue
        try:
            text = (entry / 'stat').read_text()
        except OSError:
            return None
        parsed = _stat_fields(text)
        if parsed is None:
            return None
        found[int(entry.name)] = parsed
    return found


def group_members(pgid, exclude, proc=PROC):
    """Pids in this process group. None means the walk was incomplete."""
    walked = _walk_proc(proc)
    if walked is None:
        return None
    return [pid for pid, info in walked.items() if pid not in exclude and info['pgid'] == pgid]


def become_child_subreaper():
    """PR_SET_CHILD_SUBREAPER via prctl. Unprivileged; refuse if the call is missing or fails."""
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        prctl = libc.prctl
        prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
        prctl.restype = ctypes.c_int
        rc = prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0)
    except (OSError, AttributeError, TypeError, ValueError) as exc:
        raise Refusal('child subreaper unavailable') from exc
    if rc != 0:
        raise Refusal('child subreaper unavailable')


def leftover_after_wait(helper_pid, child_pid, exclude, proc=PROC):
    """Leftovers after the child is reaped. None if the walk failed.

    A remaining live or zombie process whose ppid chain reaches the helper, other than the
    waited child, is leftover. Subreaper is what keeps the child's orphans on this pid;
    a descendant reparented to init is not leftover here.
    """
    walked = _walk_proc(proc)
    if walked is None:
        return None
    leftovers = []
    for pid in walked:
        if pid in exclude or pid == child_pid or pid == helper_pid:
            continue
        cur, seen = pid, set()
        while cur and cur not in seen:
            seen.add(cur)
            if cur == helper_pid:
                leftovers.append(pid)
                break
            nxt = walked.get(cur)
            if nxt is None:
                break
            cur = nxt['ppid']
    return leftovers


def _orig_argv_parts(proc=PROC):
    orig = getattr(sys, 'orig_argv', None)
    if orig:
        return [p.encode() if isinstance(p, str) else p for p in orig]
    path = Path(proc) / 'self' / 'cmdline'
    try:
        raw = path.read_bytes()
    except OSError:
        return None
    parts = raw.split(b'\0')
    if parts and parts[-1] == b'':
        parts = parts[:-1]
    return parts or None


def _hex_payload(value):
    if isinstance(value, bytes):
        try:
            value = value.decode('ascii')
        except UnicodeDecodeError:
            return None
    if not isinstance(value, str) or not HEX.fullmatch(value):
        return None
    try:
        return bytes.fromhex(value)
    except ValueError:
        return None


def own_source_bytes(proc=PROC):
    """Bytes of this helper as launched: hex payload after LAUNCHER, else the -c program, else the file.

    `python3 -c LAUNCHER <hex> …` leaves the hex in sys.argv[1] even when orig_argv
    (3.10+) and /proc/self/cmdline are absent, which is the Darwin 3.9 host.
    """
    parts = _orig_argv_parts(proc)
    if parts:
        try:
            i = parts.index(b'-c')
        except ValueError:
            i = -1
        if 0 <= i < len(parts) - 1:
            program = parts[i + 1]
            if program == LAUNCHER.encode():
                if i + 2 >= len(parts):
                    raise Refusal('helper source bytes unavailable')
                decoded = _hex_payload(parts[i + 2])
                if decoded is None:
                    raise Refusal('helper source bytes unavailable')
                return decoded
            return program
    path = globals().get('__file__')
    if path and path not in ('<string>', '-c') and os.path.isfile(path):
        return Path(path).read_bytes()
    if sys.argv and sys.argv[0] in ('-c', '') and len(sys.argv) > 1:
        decoded = _hex_payload(sys.argv[1])
        if decoded is not None:
            return decoded
    raise Refusal('helper source bytes unavailable')


def own_source_sha256(proc=PROC):
    return hashlib.sha256(own_source_bytes(proc=proc)).hexdigest()


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
    if operation in INPUT_MODES:
        if len(rest) != 2:
            raise Refusal(operation + ' takes the pinned source and the hex spec')
        if not isinstance(rest[0], str) or not rest[0] or len(rest[0]) > SPEC_CAP_BYTES:
            raise Refusal(operation + ' needs the pinned guest-inputs source')
        input_spec(rest[1])
        return [sys.executable, '-I', '-B', '-c', rest[0], operation, rest[1]]
    if operation in REMOTE:
        if len(rest) != 1:
            raise Refusal(operation + ' takes one hex command spec')
        return checked_remote(operation, remote_spec(rest[0]))
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


def hex_json(value, label):
    """Decode one bounded, canonical hex-encoded JSON object. Hex keeps the route to [0-9a-f] only."""
    if not isinstance(value, str) or not value or len(value) > 2 * SPEC_CAP_BYTES or not HEX.fullmatch(value):
        raise Refusal(label + ' must be bounded lowercase hex')
    try:
        raw = bytes.fromhex(value)
        parsed = json.loads(raw.decode())
    except (ValueError, UnicodeDecodeError) as exc:
        raise Refusal(label + ' is not hex-encoded JSON') from exc
    if compact(parsed) != raw:
        raise Refusal(label + ' is not the canonical compact form')
    return parsed


def input_spec(value):
    """The guest-inputs spec: parent, name, layout and limits only."""
    spec = hex_json(value, 'guest input spec')
    if not isinstance(spec, dict) or set(spec) != {'parent', 'name', 'layout', 'limits'}:
        raise Refusal('guest input spec must state parent, name, layout and limits only')
    return spec


def remote_spec(value):
    spec = hex_json(value, 'remote command spec')
    if not isinstance(spec, dict) or set(spec) != {'argv'}:
        raise Refusal('remote command spec must state one argv')
    return spec


def build_command(context):
    """The one reviewed runtime build: local oci-layout and deb contexts, no registry, immutable iidfile."""
    return ['sudo', '-n', 'env', 'DOCKER_CONFIG=' + context + '/docker-config', 'nerdctl', 'build',
            '--platform', 'linux/arm64', '--provenance=mode=max', '--iidfile', context + '.iid',
            '--tag', BUILD_TAG, '--network', 'none',
            '--build-context', 'ubuntu=oci-layout://' + context + '.oci',
            '--build-context', 'debs=' + context + '.debs', context]


def check_run_command(context, name, image, source):
    """C5's one disposable check: a synchronous `nerdctl run`, network none, no pull, the cidfile beside the context.
    No --rm: the container stays for its exact-ID inspection, then the exact-ID removal. nerdctl writes the cidfile
    during create and refuses an existing one, so the ID is this run's own."""
    return ['sudo', '-n', 'nerdctl', 'run', '--pull=never', '--network', 'none',
            '--cidfile', context + '.check.cid', '--name', name, image, 'python3', '-I', '-c', source]


CONTAINER_LIST = ['sudo', '-n', 'nerdctl', 'container', 'ls', '--all', '--no-trunc', '--quiet']
NERDCTL_VERSION = ['nerdctl', '--version']
CHECK_OPERATIONS = frozenset({'disposable-check', 'read-check-cid', 'container-list', 'nerdctl-version',
                              'container-inspect', 'container-rm'})


def checked_check_remote(operation, argv):
    """C5's three fixed forms. The run's own name, image and source are read from the spec and must rebuild to
    exactly the reviewed command; the check source is pinned by sha256 before it is accepted."""
    if operation == 'container-list':
        if argv != CONTAINER_LIST:
            raise Refusal('container list is not the exact reviewed form')
        return argv
    if operation == 'nerdctl-version':
        if argv != NERDCTL_VERSION:
            raise Refusal('nerdctl version is not the exact reviewed form')
        return argv
    if operation in ('container-inspect', 'container-rm'):
        verb = ['container', 'inspect'] if operation == 'container-inspect' else ['container', 'rm']
        if len(argv) != 6 or argv[:3] != ['sudo', '-n', 'nerdctl'] or argv[3:5] != verb or not CONTAINER_ID.fullmatch(argv[5]):
            raise Refusal(f'{operation} is not the exact reviewed form for one full container ID')
        return argv
    if operation == 'read-check-cid':
        if len(argv) != 4 or argv[:3] != ['sudo', '-n', 'cat'] or not argv[3].endswith('.check.cid'):
            raise Refusal('check ID read is not the exact reviewed form')
        if not CONTEXT.fullmatch(argv[3][:-len('.check.cid')]):
            raise Refusal('check ID read is not the exact reviewed form')
        return argv
    # disposable-check: the cidfile, name, image and source are the only variables; each is checked first.
    if len(argv) < 2 or '--cidfile' not in argv or '--name' not in argv:
        raise Refusal('disposable check is not the exact reviewed form')
    cidfile = argv[argv.index('--cidfile') + 1] if argv.index('--cidfile') + 1 < len(argv) else ''
    if not cidfile.endswith('.check.cid') or not CONTEXT.fullmatch(cidfile[:-len('.check.cid')]):
        raise Refusal('disposable check cidfile is not beside the reviewed context')
    context = cidfile[:-len('.check.cid')]
    name_at = argv.index('--name') + 1
    if name_at >= len(argv) or not CHECK_NAME.fullmatch(argv[name_at]) or name_at + 1 >= len(argv):
        raise Refusal('disposable check name is not the reviewed shape')
    image = argv[name_at + 1]
    if not IMAGE_ID.fullmatch(image) or len(argv) < 5:
        raise Refusal('disposable check image is not an image ID')
    source = argv[-1]
    if hashlib.sha256(source.encode()).hexdigest() != CHECK_SOURCE_SHA256:
        raise Refusal('disposable check source differs from its pin')
    if argv != check_run_command(context, argv[name_at], image, source):
        raise Refusal('disposable check is not the exact reviewed command')
    return argv


def checked_remote(operation, spec):
    """Rebuild one reviewed remote command from its spec and require the spec to be exactly that command.
    Nothing caller-chosen reaches a guest child: only these fixed forms can run."""
    argv = spec['argv']
    if operation in CHECK_OPERATIONS:
        return checked_check_remote(operation, argv)
    if (not isinstance(argv, list) or not argv or len(argv) > 24
            or any(not isinstance(a, str) or not a or len(a) > 512 or '\x00' in a or '\n' in a for a in argv)):
        raise Refusal('remote command argv is malformed')
    if operation == 'build-help':
        if argv != ['nerdctl', 'build', '--help']:
            raise Refusal('build-help is not the exact reviewed form')
        return argv
    if operation == 'build':
        context = argv[-1]
        if not CONTEXT.fullmatch(context) or argv != build_command(context):
            raise Refusal('build is not the exact reviewed nerdctl invocation')
        return argv
    if operation == 'read-iidfile':
        if len(argv) != 4 or not argv[3].endswith('.iid'):
            raise Refusal('iidfile read is not the exact reviewed form')
        context = argv[3][:-len('.iid')]
        if not CONTEXT.fullmatch(context) or argv != ['sudo', '-n', 'cat', context + '.iid']:
            raise Refusal('iidfile read is not the exact reviewed form')
        return argv
    if operation == 'image-inspect':
        if (len(argv) != 6 or argv[:5] != ['sudo', '-n', 'nerdctl', 'image', 'inspect']
                or not IMAGE_ID.fullmatch(argv[5])):
            raise Refusal('image inspect is not the exact reviewed form')
        return argv
    raise Refusal('unknown remote command operation')


def run_contained(argv, stdin_bytes=None, inherit_stdin=False, proc=PROC):
    """Spawn the payload in this session, wait, and report identity plus wait facts.

    become_child_subreaper runs before Popen so the child's orphans reparent here.
    After wait, leftover_after_wait treats remaining live or zombie processes whose
    ppid chain reaches this helper (other than the waited child) as leftover.
    inherit_stdin hands this helper's own stdin to the child unchanged, so a large
    framed transfer reaches the child directly instead of through this process.
    """
    parent = linux_identity(os.getpid(), proc=proc)
    become_child_subreaper()
    if stdin_bytes is not None:
        stdin = subprocess.PIPE
    else:
        stdin = None if inherit_stdin else subprocess.DEVNULL
    child = subprocess.Popen(argv, stdin=stdin,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=False)
    try:
        child_id = linux_identity(child.pid, proc=proc)
    except Refusal:
        child_id = {'pid': child.pid, 'start_time': None, 'pgid': None, 'session_id': None, 'boot_id': None}
    out, err = child.communicate(stdin_bytes)
    waited = child.returncode
    remaining = leftover_after_wait(parent['pid'], child.pid, exclude={os.getpid()}, proc=proc)
    trailer = {
        'waited_exit': waited,
        'child_pid': child_id['pid'],
        'child_start_time': child_id['start_time'],
        'child_pgid': child_id['pgid'],
        'child_session_id': child_id['session_id'],
        'group_empty': remaining == [],
        'unreaped': remaining is None,
        'stdout_sha256': hashlib.sha256(out).hexdigest(),
        'stderr_sha256': hashlib.sha256(err).hexdigest(),
    }
    return parent, out, err, trailer


def operation_argv(argv):
    """Strip the python launcher and the hex payload. `-c HEX op ...` and `script.py op ...` both yield `op ...`."""
    args = list(argv or [])
    if args:
        args = args[1:]
    if args and args[0] not in ALLOWED and len(args) >= 2 and args[1] in ALLOWED:
        args = args[1:]
    return args


def main(argv=None, proc=None):
    proc = resolved_proc(proc)
    args = operation_argv(list(argv if argv is not None else sys.argv))
    if len(args) < 2:
        raise Refusal('fixed guest helper requires operation and nonce')
    operation, nonce, rest = args[0], args[1], args[2:]
    if operation not in ALLOWED:
        raise Refusal('step is not a fixed Colima guest helper')
    if not isinstance(nonce, str) or not NONCE.fullmatch(nonce):
        raise Refusal('nonce must be 256-bit lowercase hexadecimal')
    source_sha = own_source_sha256(proc=proc)
    stdin_bytes = None
    if operation == 'stage':
        stdin_bytes = sys.stdin.buffer.read(8193)
    inherit = operation == 'transfer'
    inner = inner_argv(operation, rest)
    parent, out, err, trailer = run_contained(inner, stdin_bytes=stdin_bytes, inherit_stdin=inherit, proc=proc)
    header = dict(nonce=nonce, operation=operation, source_sha256=source_sha,
                  boot_id=parent['boot_id'], pid=parent['pid'], start_time=parent['start_time'],
                  pgid=parent['pgid'], session_id=parent['session_id'])
    if operation in REMOTE:
        header['command'] = list(inner)
    sys.stdout.buffer.write(format_envelope(header, out, err, trailer))
    sys.stdout.buffer.flush()
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except Exception as exc:
        sys.stderr.write('refused: ' + str(exc) + '\n')
        sys.exit(2)
