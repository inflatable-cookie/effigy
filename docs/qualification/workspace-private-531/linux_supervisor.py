#!/usr/bin/python3 -I
"""Guest supervisor: transient systemd SERVICE with a gate and an outside-payload monitor.

Runs as root through the VM route (`prlctl exec`), because the system manager cannot
start a unit as another account without root. The payload runs as the named non-root
account. Nothing here is installed by source checks; tests inject `ops`, the only route
to systemctl, systemd-run and cgroupfs.

Lifecycle. The payload is wrapped so that it blocks on a gate FIFO in the stage
directory before running. The outside monitor samples the unit while it is loaded
and releases the gate on the first sample that shows the unit active in its cgroup.
So the unit is provably active, with a control group, before the payload does any
work. A payload that is never observed active is never released and ends at its
RuntimeMaxSec deadline, which is unknown.

The emitted argv sets RemainAfterExit=yes. systemd-run --pipe waits until the
unit is inactive or failed (v259 run.c run_context_check_done); it does not return
at active(exited). The outside monitor stops the already empty successful
active(exited) unit, then the original systemd-run caller returns its actual
waited result. The final loaded inactive/dead sample must keep InvocationID and
report success, status zero and no cgroup. Collection or read races are unknown.

Gate admission has a separate 30-second reserve. The full payload deadline begins
at release; RuntimeMaxSec includes that gate reserve, and the monitor enforces
the original payload deadline. Cleanup has a separate 30-second reserve. These
reserves consume existing admission/report headroom, never extend payload bounds.

The root entry consumes a private exact request provisioned through the separately
admitted authenticated root bootstrap. Caller arguments cannot authorize a request.
Root owns the row directory and holds its descriptor through every root-side file
access. Nothing here is installed or executed by source controls.

Credit remains host-owned: the candidate needs a separate authenticated terminal
observation. SSH exit alone never establishes closure.
"""
import base64
import hashlib
import json
import os
import pwd
import re
import stat
import subprocess
import sys
import threading
import time
from pathlib import Path

# This file is installed alone as an executable (linux_setup.supervisor_install_argv), so it
# imports only the standard library. Refusal and exclusive mirror preparation_stage; the
# constant mirrors preparation_observations.UNIT_INACTIVE. A test pins both.
UNIT_INACTIVE = 'unit-inactive-exittype-cgroup'
UNIT_EXITED = 'unit-exited-exittype-cgroup'


class Refusal(ValueError):
    """A refused request. Callers treat it as no evidence, never as a pass."""


def exclusive(path, data):
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())

NONCE = re.compile(r'^[0-9a-f]{64}$')
INVOCATION = re.compile(r'^[0-9a-f]{32}$')
MIN_SYSTEMD = 250          # ExitType=cgroup
MAX_TIMEOUT = 900          # per-stage ceiling in the setup manifest
CLEANUP_RESERVE_SECONDS = 30
GATE_RESERVE_SECONDS = 30
FORBIDDEN_RUN_FLAGS = ('--scope', '--collect', '--user', '--no-block', '--wait')
SUPERVISOR = 'systemd-service-cgroup-v2'
BOOT_ID_PATH = '/proc/sys/kernel/random/boot_id'
CGROUP_ROOT = '/sys/fs/cgroup'
PROPERTIES = 'LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup'
GATE_NAME = 'gate.fifo'
TRANSFER_NAME = 'transfer.fifo'
PAYLOAD_STDOUT_CAP = 65536
PAYLOAD_ACCOUNT = 'effigy531'     # the one account a request may name; a request for any other account is refused
STAGE_OWNER = 0                   # the stage root is root-owned in production
# The only directory the installed entry creates row stages under. It is root-owned and mode 0711,
# so the payload account can traverse to FIFO endpoints without owning any directory.
STAGE_ROOT = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/stage'
STAGE_ROOT_MODE = 0o711
AUTH_ROOT = '/var/lib/effigy531-approved'
ROW_NAME = re.compile(r'^[A-Za-z0-9._-]{1,32}$')


class Disconnect(Exception):
    """The injected transport dropped before a terminal result."""


def unit_name(nonce):
    if not isinstance(nonce, str) or not NONCE.fullmatch(nonce):
        raise Refusal('nonce must be 256-bit lowercase hexadecimal')
    return 'effigy-stage-' + nonce


def normalize_invocation(value):
    """systemctl prints contiguous hex. Hyphenated UUID text is accepted and folded to 32 hex."""
    folded = (value or '').replace('-', '').lower()
    if not INVOCATION.fullmatch(folded):
        raise Refusal('InvocationID is not 32 hexadecimal characters')
    return folded


def gate_argv(gate, payload_argv):
    """Block on the gate FIFO, then exec the payload. The payload runs only after release."""
    return ['/bin/sh', '-c', 'read _effigy_gate < "$1"; shift; exec "$@"', 'sh', gate] + list(payload_argv)


def run_argv(nonce, payload_user, cwd, timeout, argv):
    """Transient SERVICE argv. Scope, collect, wait and user-manager flags are refused."""
    if not isinstance(argv, list) or not argv or any(not isinstance(x, str) or not x or '\x00' in x for x in argv):
        raise Refusal('payload argv must be a non-empty list of strings')
    if not isinstance(timeout, int) or isinstance(timeout, bool) or not 0 < timeout <= MAX_TIMEOUT:
        raise Refusal('timeout outside the stage ceiling')
    if not isinstance(payload_user, str) or not re.fullmatch(r'[a-z_][a-z0-9_-]{0,31}', payload_user) or payload_user == 'root':
        raise Refusal('payload must run as a named non-root account')
    if not cwd.startswith('/') or '..' in Path(cwd).parts:
        raise Refusal('payload cwd must be an absolute canonical path')
    flags = [
        '--unit=' + unit_name(nonce),
        '--uid=' + payload_user,
        '--service-type=exec',
        '--property=ExitType=cgroup',
        '--property=KillMode=control-group',
        '--property=RuntimeMaxSec=%d' % (timeout + GATE_RESERVE_SECONDS),
        '--property=WorkingDirectory=' + cwd,
        '--remain-after-exit',
        '--pipe',
    ]
    if any(f.split('=')[0] in FORBIDDEN_RUN_FLAGS for f in flags):
        raise Refusal('forbidden systemd-run flag')
    return ['systemd-run'] + flags + ['--'] + list(argv)


def show_argv(unit):
    return ['systemctl', 'show', unit, '--property=' + PROPERTIES]


def stop_argv(unit):
    return ['systemctl', 'kill', '--kill-whom=all', '--signal=SIGKILL', unit]


def parse_systemd_version(text):
    match = re.match(r'^systemd (\d+)', text or '')
    if not match:
        raise Refusal('systemd version unavailable')
    return int(match.group(1))


def parse_show(text, unit):
    """Parse `systemctl show` output. None means the unit is not loaded (absent or collected)."""
    if text is None:
        return None
    values = {}
    for line in text.splitlines():
        if '=' in line:
            key, value = line.split('=', 1)
            values[key] = value
    if values.get('LoadState') != 'loaded':
        return None
    group = values.get('ControlGroup', '')
    if group and (not group.startswith('/') or '..' in Path(group).parts):
        raise Refusal('ControlGroup not canonical')
    return {'unit': unit, 'invocation_id': normalize_invocation(values.get('InvocationID', '')),
            'control_group': group, 'active_state': values.get('ActiveState', ''),
            'sub_state': values.get('SubState', ''), 'result': values.get('Result', ''),
            'exec_main_status': values.get('ExecMainStatus', '')}


def parse_populated(text):
    """cgroup.events `populated`. None means the file vanished."""
    if text is None:
        return None
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] == 'populated' and parts[1] in ('0', '1'):
            return int(parts[1])
    raise Refusal('cgroup.events has no populated field')


def events_path(control_group):
    if not control_group.startswith('/') or '..' in Path(control_group).parts:
        raise Refusal('control group not canonical')
    return CGROUP_ROOT + control_group + '/cgroup.events'


def release_gate(gate):
    """Open the gate for writing without blocking and send one line. The payload's `read` returns."""
    fd = os.open(gate, os.O_RDWR | os.O_NONBLOCK)
    try:
        os.write(fd, b'go\n')
    finally:
        os.close(fd)


class Monitor:
    """Outside-payload monitor on its own thread.

    Samples the unit while it is loaded. On the first sample that shows the unit active
    with a control group, it calls `on_active` once and records the release index. A
    failed read is recorded as a failure, never a pass. Absence before start is normal.
    """

    def __init__(self, ops, unit, interval=0.05, on_active=None, payload_timeout=None, clock=time.monotonic):
        self.ops, self.unit, self.interval = ops, unit, interval
        self.on_active = on_active
        self.payload_timeout, self.clock = payload_timeout, clock
        self.started_at = None
        self.released_at = None
        self.deadline_expired = False
        self.samples = []
        self.release_index = None
        self.finish_requested = False
        self.released_invocation = None
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._loop, name='effigy-stage-monitor', daemon=True)

    def start(self):
        self.started_at = self.clock()
        self._thread.start()

    def stop(self):
        self._stop.set()
        self._thread.join(timeout=10)
        if self._thread.is_alive():
            raise Refusal('monitor thread did not stop')

    def poll_once(self):
        try:
            show = parse_show(self.ops.show(show_argv(self.unit)), self.unit)
        except Refusal as exc:
            return {'kind': 'read-failed', 'reason': str(exc)}
        if show is None:
            return {'kind': 'absent'}
        record = {'kind': 'loaded', 'invocation_id': show['invocation_id'], 'control_group': show['control_group'],
                  'active_state': show['active_state'], 'sub_state': show['sub_state'],
                  'result': show['result'], 'exec_main_status': show['exec_main_status'], 'populated': None}
        if show['control_group']:
            try:
                record['populated'] = parse_populated(self.ops.events(events_path(show['control_group'])))
            except Refusal as exc:
                record['populated_error'] = str(exc)
        return record

    def _loop(self):
        while not self._stop.is_set():
            record = self.poll_once()
            self.samples.append(record)
            if (self.on_active and self.release_index is None and record['kind'] == 'loaded'
                    and record['active_state'] == 'active' and record['control_group']):
                self.release_index = len(self.samples)
                self.released_invocation = record['invocation_id']
                try:
                    self.on_active(record)
                    self.released_at = self.clock()
                except (Refusal, OSError) as exc:
                    self.samples.append({'kind': 'release-failed', 'reason': str(exc)})
            # --pipe waits for inactive/failed, including with RemainAfterExit. Stop the
            # already empty successful unit from outside the payload before waiting for
            # systemd-run. No payload deadline is shortened and failed/missing reads fail closed.
            if (self.release_index is not None and not self.finish_requested
                    and record.get('kind') == 'loaded'
                    and record.get('invocation_id') == self.released_invocation and terminal_exited(record)):
                self.finish_requested = True
                try:
                    self.ops.finish(['systemctl', 'stop', self.unit])
                except (Refusal, OSError) as exc:
                    self.samples.append({'kind': 'read-failed', 'reason': str(exc)})
            now = self.clock()
            if self.payload_timeout is not None and not self.deadline_expired:
                end = (self.released_at + self.payload_timeout if self.released_at is not None
                       else self.started_at + GATE_RESERVE_SECONDS)
                if now > end:
                    self.deadline_expired = True
                    try:
                        self.ops.stop(stop_argv(self.unit))
                    except (Refusal, OSError) as exc:
                        self.samples.append({'kind': 'read-failed', 'reason': str(exc)})
            self._stop.wait(self.interval)


def closure_receipt(boot_id, nonce, show, observed_group, waited_exit):
    """Candidate for the host: terminal facts plus the cgroup the monitor saw while the gate was closed."""
    return {'boot_id': boot_id, 'nonce': nonce, 'unit': unit_name(nonce),
            'invocation_id': show['invocation_id'], 'control_group': '',
            'observed_control_group': observed_group,
            'supervisor': SUPERVISOR, 'terminal_basis': UNIT_INACTIVE,
            'populated': 0, 'active_state': show['active_state'], 'sub_state': show['sub_state'],
            'result': show['result'], 'exec_main_status': show['exec_main_status'], 'waited_exit': waited_exit}


def terminal_exited(show):
    """RemainAfterExit leaves a finished service active(exited): main exited 0, the cgroup is gone."""
    return (show['active_state'] == 'active' and show['sub_state'] == 'exited' and show['result'] == 'success'
            and show['exec_main_status'] == '0' and show['control_group'] == '')


def _eligible(boot_id, nonce, show, samples, release_index, waited_exit):
    if release_index is None:
        return None, 'gate never released: the monitor never saw the unit active; closure is unknown'
    if any(s.get('kind') in ('read-failed', 'release-failed') or s.get('populated_error') for s in samples):
        return None, 'a failed read or release occurred; closure is unknown'
    after = samples[release_index:]
    if any(s.get('kind') == 'absent' for s in after):
        return None, 'the unit was collected after release; closure is unknown'
    invocation = show['invocation_id']
    if any(s.get('kind') == 'loaded' and s.get('invocation_id') != invocation for s in after):
        return None, 'InvocationID changed after release; closure is unknown'
    before = [s for s in samples[:release_index] if s.get('kind') == 'loaded' and s['invocation_id'] == invocation]
    active = [s for s in before if s['active_state'] == 'active' and s['control_group']]
    if not active:
        return None, 'no pre-release sample shows this InvocationID active in a cgroup'
    if len({s['control_group'] for s in active}) != 1:
        return None, 'monitor saw more than one control group for this InvocationID'
    if not (show['active_state'] == 'inactive' and show['sub_state'] == 'dead'
            and show['result'] == 'success' and show['exec_main_status'] == '0' and show['control_group'] == ''):
        return None, 'post-run terminal state is not inactive/dead, Result=success, status 0, empty cgroup'
    return closure_receipt(boot_id, nonce, show, active[-1]['control_group'], waited_exit), None


def hold_stage_parent(parent, owner):
    """Open a stage parent and check it. Returns the held descriptor; the caller closes it.

    The parent must be canonical (no symlink or alias in its path), a real directory owned by
    `owner`, and not writable by group or others. The check runs on the held descriptor, so a
    row created through it cannot be redirected by a path swapped after the check.
    """
    text = str(parent)
    if os.path.realpath(text) != text:
        raise Refusal('stage parent is not canonical: a symlink or alias is in its path')
    info = os.lstat(text)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != owner or info.st_mode & 0o022:
        raise Refusal('stage parent must be a directory owned by the reviewed owner, not writable by others')
    fd = os.open(text, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    held = os.fstat(fd)
    if (held.st_dev, held.st_ino) != (info.st_dev, info.st_ino):
        os.close(fd)
        raise Refusal('stage parent changed between the check and the open')
    return fd


def create_row(parent_fd, name, uid):
    """Root retains row ownership; the payload owns only FIFO endpoints, never the directory."""
    try:
        os.mkdir(name, 0o711, dir_fd=parent_fd)
    except FileExistsError as exc:
        raise Refusal('row stage already exists; a stage is never reused') from exc
    fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
    info = os.fstat(fd)
    if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o711:
        os.close(fd)
        raise Refusal('row stage must remain supervisor-owned, mode 0711')
    return fd


def _row_write(row_fd, name, data):
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=row_fd)
    with os.fdopen(fd, 'wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def _make_fifo(row_fd, name, uid):
    if os.geteuid() != 0 and uid != os.geteuid():
        raise Refusal('unprivileged supervisor cannot assign another payload uid')
    if os.mkfifo in os.supports_dir_fd:
        os.mkfifo(name, 0o600, dir_fd=row_fd)
    else:
        # CPython on macOS omits mkfifo(dir_fd), although libc supports mkfifoat.
        # Keep fd custody in source controls too; never fall back to a pathname.
        import ctypes
        libc = ctypes.CDLL(None, use_errno=True)
        make = libc.mkfifoat
        make.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        make.restype = ctypes.c_int
        if make(row_fd, os.fsencode(name), 0o600) != 0:
            raise OSError(ctypes.get_errno(), 'mkfifoat failed')
    fd = os.open(name, os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=row_fd)
    try:
        if not stat.S_ISFIFO(os.fstat(fd).st_mode):
            raise Refusal('payload endpoint is not a FIFO')
        os.fchown(fd, uid, -1)
    finally:
        os.close(fd)


def _release_row_gate(row_fd):
    fd = os.open(GATE_NAME, os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=row_fd)
    try:
        if not stat.S_ISFIFO(os.fstat(fd).st_mode):
            raise Refusal('gate endpoint is not a FIFO')
        os.write(fd, b'go\n')
    finally:
        os.close(fd)


def run_supervised(stage_dir, request, ops, clock, boot_id, seen_invocations, monitor=None, stage_owner=STAGE_OWNER):
    """One-use supervised payload. Intent is spent before any effect.

    Candidate eligibility, all required: the unit was absent before start; the monitor
    saw this InvocationID active in one cgroup before it released the gate; any live
    descendant keeps the unit active/running; the post-run unit is loaded with the same
    InvocationID, inactive, Result=success, ExecMainStatus=0 and an empty ControlGroup.
    A gate that is never released ends at the deadline and is unknown. Returns
    {'disposition', 'candidate', 'reason', 'show', 'samples', 'payload_stdout'}.
    """
    stage = Path(stage_dir)
    if not isinstance(boot_id, str) or not boot_id:
        raise Refusal('boot identity required before supervised start')
    nonce = request['nonce']
    unit = unit_name(nonce)
    if nonce in seen_invocations.get('nonces', ()):
        raise Refusal('nonce reused')
    if parse_systemd_version(ops.version()) < MIN_SYSTEMD:
        raise Refusal('installed systemd predates ExitType=cgroup')
    if ops.cgroup2() != 'cgroup2fs':
        raise Refusal('cgroup v2 not available')
    gate = stage / GATE_NAME
    if request.get('transfer'):
        transfer = stage / TRANSFER_NAME
        if str(transfer) not in ' '.join(request['argv']):
            raise Refusal('transfer payload must read the stage transfer FIFO')
    payload = list(request['argv'])
    argv = run_argv(nonce, request['payload_user'], request['cwd'], request['timeout_seconds'],
                    gate_argv(str(gate), payload))
    intent = {'unit': unit, 'argv': argv, 'boot_id': boot_id, 'timeout_seconds': request['timeout_seconds'],
              'cleanup_reserve_seconds': CLEANUP_RESERVE_SECONDS, 'gate': str(gate)}
    # Custody and creation come after every refusal above, so a refused request leaves no stage behind.
    if os.path.lexists(stage):
        raise Refusal('row stage already exists; a stage is never reused')
    if not os.path.lexists(stage.parent):
        # Created once, mode 0711. Its own parent is held and checked before the mkdir.
        ancestor_fd = hold_stage_parent(stage.parent.parent, stage_owner)
        try:
            os.mkdir(stage.parent.name, STAGE_ROOT_MODE, dir_fd=ancestor_fd)
        finally:
            os.close(ancestor_fd)
    parent_fd = hold_stage_parent(stage.parent, stage_owner)
    row_fd = None
    try:
        row_fd = create_row(parent_fd, stage.name, request['payload_uid'])
        return _run_held(stage, request, ops, clock, boot_id, seen_invocations, monitor,
                         unit, gate, argv, intent, row_fd)
    finally:
        if row_fd is not None:
            os.close(row_fd)
        os.close(parent_fd)


def _run_held(stage, request, ops, clock, boot_id, seen_invocations, monitor, unit, gate, argv, intent, row_fd):
    nonce = request['nonce']
    _row_write(row_fd, 'intent.json', (json.dumps(intent, sort_keys=True, indent=2) + '\n').encode())
    seen_invocations.setdefault('nonces', set()).add(nonce)
    if parse_show(ops.show(show_argv(unit)), unit) is not None:
        raise Refusal('unit already loaded before start; nonce is not fresh')
    _make_fifo(row_fd, GATE_NAME, request['payload_uid'])
    if request.get('transfer'):
        _make_fifo(row_fd, TRANSFER_NAME, request['payload_uid'])
    monitor = monitor or Monitor(ops, unit, on_active=lambda record: _release_row_gate(row_fd),
                                 payload_timeout=request['timeout_seconds'], clock=clock)
    start = clock()
    deadline = start + request['timeout_seconds'] + GATE_RESERVE_SECONDS + CLEANUP_RESERVE_SECONDS
    monitor.start()
    try:
        outcome = ops.run(argv, request['timeout_seconds'] + GATE_RESERVE_SECONDS + CLEANUP_RESERVE_SECONDS)
        disconnected = False
    except Disconnect:
        outcome = {'returncode': None, 'stdout': b'', 'stderr': b'', 'timed_out': False}
        disconnected = True
    finally:
        monitor.stop()
    for name in ('stdout', 'stderr'):
        _row_write(row_fd, name, outcome[name])
    timed_out = bool(outcome.get('timed_out')) or getattr(monitor, 'deadline_expired', False) or clock() > deadline
    if timed_out or disconnected:
        ops.stop(stop_argv(unit))
    show = parse_show(ops.show(show_argv(unit)), unit)
    samples = list(monitor.samples)
    disposition = 'timeout' if timed_out else ('disconnect' if disconnected else (
        'exited' if outcome['returncode'] == 0 else 'failed'))
    if disposition != 'exited':
        candidate, reason = None, 'payload did not exit cleanly: ' + disposition
    elif show is None:
        candidate, reason = None, 'unit vanished before the terminal read'
    else:
        candidate, reason = _eligible(boot_id, nonce, show, samples, monitor.release_index, outcome['returncode'])
        if candidate is not None and show['invocation_id'] in seen_invocations.get('invocations', ()):
            raise Refusal('InvocationID reused')
    if candidate is not None:
        seen_invocations.setdefault('invocations', set()).add(show['invocation_id'])
    payload_stdout = outcome['stdout'][:PAYLOAD_STDOUT_CAP]
    record = {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
              'samples': samples, 'release_index': monitor.release_index, 'elapsed_seconds': clock() - start,
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    _row_write(row_fd, 'supervisor.json', (json.dumps(record, sort_keys=True, indent=2, default=str) + '\n').encode())
    return {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
            'samples': samples, 'payload_stdout': payload_stdout.decode('utf-8', 'replace')}


class SystemdOps:
    """Adapter from an injected runner to the systemd and cgroup reads used above."""

    def __init__(self, runner):
        self.runner = runner

    def _checked(self, argv, timeout=30):
        result = self.runner(argv, timeout)
        if result['returncode'] != 0 or result.get('timed_out'):
            raise Refusal('systemctl call failed: ' + ' '.join(argv[:2]))
        return result['stdout'].decode('utf-8', 'replace')

    def version(self):
        return self._checked(['systemctl', '--version']).splitlines()[0]

    def cgroup2(self):
        return self._checked(['stat', '-fc', '%T', CGROUP_ROOT]).strip()

    def run(self, argv, timeout):
        result = self.runner(argv, timeout)
        return {'returncode': result['returncode'], 'stdout': result['stdout'],
                'stderr': result['stderr'], 'timed_out': bool(result.get('timed_out'))}

    def stop(self, argv):
        self.runner(argv, 30)

    def finish(self, argv):
        self._checked(argv, CLEANUP_RESERVE_SECONDS)

    def show(self, argv):
        # A failed read must refuse: reading "absent" here could start a second unit.
        return self._checked(argv)

    def events(self, path):
        try:
            return Path(path).read_text()
        except FileNotFoundError:
            return None


def _subprocess_runner(argv, timeout):
    """Real runner: own session; the deadline kills only its own process group."""
    proc = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=timeout)
        return {'returncode': proc.returncode, 'stdout': out, 'stderr': err, 'timed_out': False}
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, 9)
        out, err = proc.communicate()
        return {'returncode': None, 'stdout': out, 'stderr': err, 'timed_out': True}


def consume_approved_request(digest, root=AUTH_ROOT, owner=0):
    """The authenticated root bootstrap provisions exact bytes; the caller supplies no authority.

    Hold the approval directory for the read and the durable one-use spend. A hash
    from an unprivileged caller cannot create a root-owned approved record.
    """
    if not isinstance(digest, str) or not NONCE.fullmatch(digest):
        raise Refusal('approved request digest must be SHA256')
    parent = hold_stage_parent(Path(root), owner)
    try:
        if stat.S_IMODE(os.fstat(parent).st_mode) != 0o700:
            raise Refusal('approval directory must be mode 0700')
        fd = os.open(digest + '.json', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent)
        with os.fdopen(fd, 'rb') as source:
            info = os.fstat(source.fileno())
            if (not stat.S_ISREG(info.st_mode) or info.st_uid != owner
                    or stat.S_IMODE(info.st_mode) != 0o600 or info.st_nlink != 1):
                raise Refusal('approval must be a private supervisor-owned regular file')
            raw = source.read(1024 * 1024 + 1)
        if len(raw) > 1024 * 1024 or hashlib.sha256(raw).hexdigest() != digest:
            raise Refusal('approved request bytes differ from digest')
        try:
            _row_write(parent, digest + '.spent', digest.encode())
        except FileExistsError as exc:
            raise Refusal('approved request already spent') from exc
        os.fsync(parent)
        return raw
    finally:
        os.close(parent)


def main(argv):
    """Root entry: only an exact, provisioned, one-use approval digest is accepted."""
    if len(argv) != 2 or argv[0] != '--approved-request':
        raise Refusal('usage: --approved-request SHA256')
    approval_seam = os.environ.get('EFFIGY_APPROVAL_ROOT')
    if approval_seam is not None and os.geteuid() == 0:
        raise Refusal('test seams are refused for root runs')
    raw = consume_approved_request(argv[1], approval_seam or AUTH_ROOT,
                                   os.geteuid() if approval_seam else 0)
    request = json.loads(raw)
    required = {'nonce', 'payload_user', 'payload_uid', 'cwd', 'timeout_seconds', 'argv', 'stage_dir',
                'expected_boot_id'}
    if not required <= set(request) or set(request) - required - {'transfer', 'input_hashes', 'resources', 'ssh_binding'}:
        raise Refusal('unsupported request schema')
    stage = Path(request['stage_dir'])
    # Two test seams, EFFIGY_STAGE_ROOT and EFFIGY_PAYLOAD_ACCOUNT, exist because the reviewed root and
    # account do not exist on a development host. A root run refuses both, so they cannot widen the
    # installed entry; the host also checks every credit independently.
    seam = os.environ.get('EFFIGY_STAGE_ROOT')
    account_seam = os.environ.get('EFFIGY_PAYLOAD_ACCOUNT')
    if (seam is not None or account_seam is not None) and os.geteuid() == 0:
        raise Refusal('test seams are refused for root runs')
    root = seam if seam is not None else STAGE_ROOT
    owner = os.geteuid() if seam is not None else STAGE_OWNER
    if str(stage.parent) != root or not ROW_NAME.fullmatch(stage.name):
        raise Refusal('stage must be a direct child of the reviewed stage root')
    account = account_seam if account_seam is not None else PAYLOAD_ACCOUNT
    if request['payload_user'] != account:
        raise Refusal('payload account is not the reviewed account')
    try:
        if pwd.getpwnam(account).pw_uid != request['payload_uid']:
            raise Refusal('payload uid differs from the account on this host')
    except KeyError as exc:
        raise Refusal('payload account does not exist on this host') from exc
    boot_seam = os.environ.get('EFFIGY_BOOT_ID_PATH')
    if boot_seam is not None and os.geteuid() == 0:
        raise Refusal('test seams are refused for root runs')
    boot_id = Path(boot_seam or BOOT_ID_PATH).read_text().strip()
    if boot_id != request['expected_boot_id']:
        raise Refusal('boot identity differs from the request')
    result = run_supervised(request['stage_dir'], request, SystemdOps(_subprocess_runner), time.monotonic, boot_id, {},
                            None, stage_owner=owner)
    result['payload_stdout_b64'] = base64.b64encode(result.pop('payload_stdout').encode('utf-8')).decode('ascii')
    print(json.dumps(result, sort_keys=True, default=str))
    return 0 if result['candidate'] is not None else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
