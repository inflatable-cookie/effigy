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
import ctypes
import fcntl
import hashlib
import json
import math
import os
import pwd
import re
import signal
import stat
import struct
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
        _set_mode(output.fileno(), 0o600, stat.S_ISREG)
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
ROOT_RECORD_CAP = 1024 * 1024
CAP_SYS_RESOURCE_BIT = 1 << 24
CAPABILITY_VERSION_3 = 0x20080522
PR_SET_NO_NEW_PRIVS, PR_GET_NO_NEW_PRIVS, PR_CAP_AMBIENT = 38, 39, 47
PR_CAP_AMBIENT_IS_SET, PR_CAP_AMBIENT_CLEAR_ALL = 1, 4
BROKER_PROJECT_ID, BROKER_PROJINHERIT = 531, 0x200
BROKER_RECORD_NAMES = frozenset({'intent.json', 'stdout', 'stderr', 'supervisor.json'})
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
    return ['/bin/sh', '-c', 'IFS= read -r _effigy_gate < "$1" && [ "$_effigy_gate" = go ] || exit 125; shift; exec "$@"', 'sh', gate] + list(payload_argv)


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


def _candidate_confined_run_argv(nonce, payload_user, cwd, timeout, argv):
    """Fixed prospective child configuration, not an admission or production entry.

    The service manager must install both properties before exec. A future caller
    must also prove the supported systemd/kernel versions, observe that setup and
    exclude outside writers. Merely constructing this argv proves none of those.
    """
    command = run_argv(nonce, payload_user, cwd, timeout, argv)
    separator = command.index('--')
    return command[:separator] + ['--property=PrivateUsers=identity',
                                  '--property=NoNewPrivileges=yes'] + command[separator:]


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


def _set_mode(fd, mode, kind):
    """Set only newly created objects through custody, then verify the held inode."""
    info = os.fstat(fd)
    if info.st_uid != os.geteuid() or not kind(info.st_mode):
        raise Refusal('new object must remain supervisor-owned with its intended type')
    os.fchmod(fd, mode)
    info = os.fstat(fd)
    if info.st_uid != os.geteuid() or not kind(info.st_mode) or stat.S_IMODE(info.st_mode) != mode:
        raise Refusal('held object mode or ownership differs')


def create_row(parent_fd, name, uid):
    """Root retains row and gate ownership; the payload owns only the transfer FIFO."""
    try:
        os.mkdir(name, 0o711, dir_fd=parent_fd)
    except FileExistsError as exc:
        raise Refusal('row stage already exists; a stage is never reused') from exc
    fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
    info = os.fstat(fd)
    if info.st_uid != os.geteuid() or not stat.S_ISDIR(info.st_mode):
        os.close(fd)
        raise Refusal('row stage must remain supervisor-owned')
    try:
        _set_mode(fd, 0o711, stat.S_ISDIR)
        return fd
    except BaseException:
        os.close(fd)
        raise


def _row_write(row_fd, name, data):
    if not isinstance(data, bytes) or len(data) > ROOT_RECORD_CAP:
        raise Refusal('root record exceeds its fixed byte ceiling')
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=row_fd)
    with os.fdopen(fd, 'wb') as output:
        _set_mode(output.fileno(), 0o600, stat.S_ISREG)
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


class _CapHeader(ctypes.Structure):
    _fields_ = [('version', ctypes.c_uint32), ('pid', ctypes.c_int32)]


class _CapData(ctypes.Structure):
    _fields_ = [('effective', ctypes.c_uint32), ('permitted', ctypes.c_uint32),
                ('inheritable', ctypes.c_uint32)]


def _candidate_drop_quota_bypass(capget, capset, prctl):
    """Source-only child preparation; no production entry invokes this candidate."""
    if ctypes.sizeof(_CapHeader) != 8 or ctypes.sizeof(_CapData) != 12:
        raise Refusal('unsupported capability ABI')
    def expect(value, expected, phase):
        if type(value) is not int or value != expected:
            raise Refusal('quota-bypass capability ' + phase + ' failed or is unknown')
    expect(prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0, 'no-new-privileges setup')
    expect(prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_CLEAR_ALL, 0, 0, 0), 0, 'ambient clear')
    header, data = _CapHeader(CAPABILITY_VERSION_3, 0), (_CapData * 2)()
    expect(capget(ctypes.byref(header), data), 0, 'read')
    if header.version != CAPABILITY_VERSION_3:
        raise Refusal('unsupported capability version')
    for field in ('effective', 'permitted', 'inheritable'):
        setattr(data[0], field, getattr(data[0], field) & ~CAP_SYS_RESOURCE_BIT)
    expect(capset(ctypes.byref(header), data), 0, 'drop')
    after = (_CapData * 2)()
    expect(capget(ctypes.byref(header), after), 0, 'verification')
    if header.version != CAPABILITY_VERSION_3 or any(
            getattr(after[0], field) & CAP_SYS_RESOURCE_BIT for field in ('effective', 'permitted', 'inheritable')):
        raise Refusal('quota-bypass capability remains present')
    expect(prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0), 1, 'no-new-privileges verification')
    expect(prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_IS_SET, 24, 0, 0), 0, 'ambient verification')


def _broker_project_attributes(fd):
    data = bytearray(28)
    fcntl.ioctl(fd, 0x801c581f, data)
    flags, _, _, project, _, _ = struct.unpack('=5I8s', data)
    return project, flags


def _candidate_quota_broker_record(row_fd, name, data, expected_dev, owner,
                                   capget, capset, prctl, read_project=_broker_project_attributes):
    """Source-only child body; no production entry or caller authority invokes it."""
    if not isinstance(name, str) or name not in BROKER_RECORD_NAMES or \
            not isinstance(data, bytes) or len(data) > ROOT_RECORD_CAP:
        raise Refusal('broker record is not fixed or exceeds its byte ceiling')
    if type(row_fd) is not int or type(expected_dev) is not int or type(owner) is not int:
        raise Refusal('broker device or owner is invalid')
    def observe_stat():
        try:
            return os.fstat(row_fd)
        except OSError as exc:
            raise Refusal('broker directory observation is unavailable') from exc
    def observe_project():
        try:
            return read_project(row_fd)
        except OSError as exc:
            raise Refusal('broker project observation is unavailable') from exc
    before = observe_stat()
    if not stat.S_ISDIR(before.st_mode) or before.st_uid != owner or before.st_dev != expected_dev or \
            before.st_mode & 0o022:
        raise Refusal('broker directory custody differs')
    project = observe_project()
    if not isinstance(project, tuple) or len(project) != 2 or type(project[0]) is not int or \
            type(project[1]) is not int or project[0] != BROKER_PROJECT_ID or \
            not 0 <= project[1] <= 0xffffffff or not project[1] & BROKER_PROJINHERIT:
        raise Refusal('broker directory project or inheritance differs')
    _candidate_drop_quota_bypass(capget, capset, prctl)
    after, post_project = observe_stat(), observe_project()
    if (before.st_dev, before.st_ino, before.st_uid, before.st_mode) != \
            (after.st_dev, after.st_ino, after.st_uid, after.st_mode) or post_project != project:
        raise Refusal('broker directory changed after capability drop')
    _row_write(row_fd, name, data)


def _launch_candidate_quota_record(row_fd, name, data, expected_dev, deadline, cleanup_deadline,
                                   test_capops=None, test_project=None):
    """Launch one bounded candidate child; this is not wired to a production entry."""
    testing = test_capops is not None or test_project is not None
    if testing and os.geteuid() == 0:
        raise Refusal('broker child test seams are refused for root runs')
    if not testing and (os.geteuid() != 0 or sys.platform != 'linux'):
        raise Refusal('broker child requires the supported root Linux route')
    if type(deadline) not in (int, float) or type(cleanup_deadline) not in (int, float) or \
            not math.isfinite(deadline) or not math.isfinite(cleanup_deadline):
        raise Refusal('broker child deadline is invalid')
    now = time.monotonic()
    remaining = deadline - now
    if not 0 < remaining or cleanup_deadline < deadline or \
            cleanup_deadline - now > MAX_TIMEOUT + GATE_RESERVE_SECONDS + CLEANUP_RESERVE_SECONDS:
        raise Refusal('broker child has no bounded remaining deadline')
    if threading.active_count() != 1:
        raise Refusal('broker child cannot fork with other threads running')
    if testing:
        if not isinstance(test_capops, tuple) or len(test_capops) != 3 or \
                any(not callable(operation) for operation in test_capops) or not callable(test_project):
            raise Refusal('broker child test seams are incomplete')
        capget, capset, prctl = test_capops
        read_project = test_project
        owner = os.geteuid()
    else:
        libc = ctypes.CDLL(None, use_errno=True)
        try:
            capget, capset, prctl = libc.capget, libc.capset, libc.prctl
        except AttributeError as exc:
            raise Refusal('broker capability calls are unavailable') from exc
        capget.argtypes = [ctypes.POINTER(_CapHeader), ctypes.POINTER(_CapData)]
        capset.argtypes = [ctypes.POINTER(_CapHeader), ctypes.POINTER(_CapData)]
        prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
        for operation in (capget, capset, prctl):
            operation.restype = ctypes.c_int
        read_project, owner = _broker_project_attributes, 0

    read_fd, write_fd = os.pipe()
    try:
        pid = os.fork()
    except BaseException:
        os.close(read_fd)
        os.close(write_fd)
        raise
    if pid == 0:
        os.close(read_fd)
        try:
            _candidate_quota_broker_record(row_fd, name, data, expected_dev, owner,
                                           capget, capset, prctl, read_project)
        except BaseException as exc:
            try:
                os.write(write_fd, str(exc).encode('utf-8', 'replace')[:512])
            finally:
                os._exit(3)
        os._exit(0)

    os.close(write_fd)
    reaped = False
    try:
        while True:
            try:
                waited, status = os.waitpid(pid, os.WNOHANG)
            except ChildProcessError as exc:
                reaped = True  # The PID may be reusable; never signal it after losing the wait.
                raise Refusal('broker child wait status is unavailable') from exc
            if waited == pid:
                reaped = True
                if time.monotonic() > deadline:
                    raise Refusal('broker child completed after its deadline; outcome is unknown')
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise Refusal('broker child deadline expired; outcome is unknown')
            time.sleep(min(0.01, remaining))
        detail = os.read(read_fd, 512).decode('utf-8', 'replace')
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0 or detail:
            raise Refusal('broker child failed or its outcome is unknown: ' + detail)
        return {'waited_exit': 0}
    finally:
        os.close(read_fd)
        if not reaped:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            while time.monotonic() < cleanup_deadline:
                try:
                    waited, _ = os.waitpid(pid, os.WNOHANG)
                except ChildProcessError as exc:
                    reaped = True
                    raise Refusal('broker child cleanup wait status is unavailable') from exc
                if waited == pid:
                    reaped = True
                    break
                time.sleep(min(0.01, max(0, cleanup_deadline - time.monotonic())))
            if not reaped:
                raise Refusal('broker child %d was not reaped within the cleanup reserve' % pid)


def _record_after_start(row_fd, name, data, ops, unit):
    try:
        _row_write(row_fd, name, data)
    except (Refusal, OSError):
        # The unit may still be loaded. A failed bounded record never grants
        # closure; attempt cleanup of this exact unit before propagating it.
        ops.stop(stop_argv(unit))
        raise


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
        _set_mode(fd, 0o600, stat.S_ISFIFO)
        os.fchown(fd, uid, -1)
        if os.fstat(fd).st_uid != uid or stat.S_IMODE(os.fstat(fd).st_mode) != 0o600:
            raise Refusal('transfer FIFO owner or mode differs')
    finally:
        os.close(fd)


def _make_gate(row_fd):
    # Never chown the gate to the payload. Open the sole writer capability while
    # private, then remove every pathname write bit. Popen closes this fd in children.
    _make_fifo(row_fd, GATE_NAME, os.geteuid())
    fd = os.open(GATE_NAME, os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=row_fd)
    try:
        _set_mode(fd, 0o444, stat.S_ISFIFO)
        os.set_inheritable(fd, False)
        return fd
    except BaseException:
        os.close(fd)
        raise


def _release_row_gate(fd):
    info = os.fstat(fd)
    if not stat.S_ISFIFO(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o444:
        raise Refusal('gate custody or mode differs')
    os.write(fd, b'go\n')


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
    if os.geteuid() != 0 and request['payload_uid'] != os.geteuid():
        raise Refusal('unprivileged supervisor cannot assign another payload uid')
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
            created_fd = os.open(stage.parent.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=ancestor_fd)
            try:
                _set_mode(created_fd, STAGE_ROOT_MODE, stat.S_ISDIR)
            finally:
                os.close(created_fd)
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
    gate_fd = _make_gate(row_fd)
    try:
        return _run_gated(stage, request, ops, clock, boot_id, seen_invocations, monitor, unit, gate, argv, intent, row_fd, gate_fd)
    finally:
        os.close(gate_fd)


def _run_gated(stage, request, ops, clock, boot_id, seen_invocations, monitor, unit, gate, argv, intent, row_fd, gate_fd):
    nonce = request['nonce']
    if request.get('transfer'):
        _make_fifo(row_fd, TRANSFER_NAME, request['payload_uid'])
    monitor = monitor or Monitor(ops, unit, on_active=lambda record: _release_row_gate(gate_fd),
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
        _record_after_start(row_fd, name, outcome[name], ops, unit)
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
    if any(len(outcome[name]) > PAYLOAD_STDOUT_CAP for name in ('stdout', 'stderr')):
        candidate, reason, disposition = None, 'partial output exceeds retained transport cap', 'output-cap'
    payload_stdout = outcome['stdout'][:PAYLOAD_STDOUT_CAP]
    record = {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
              'samples': samples, 'release_index': monitor.release_index, 'elapsed_seconds': clock() - start,
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    _record_after_start(row_fd, 'supervisor.json',
                        (json.dumps(record, sort_keys=True, indent=2, default=str) + '\n').encode(), ops, unit)
    return {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
            'samples': samples, 'payload_stdout': payload_stdout.decode('utf-8', 'replace'),
            'payload_stdout_b64': base64.b64encode(payload_stdout).decode('ascii'),
            'payload_stderr_b64': base64.b64encode(outcome['stderr'][:PAYLOAD_STDOUT_CAP]).decode('ascii')}


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
            approved_nonce = json.loads(raw).get('nonce')
        except (ValueError, AttributeError) as exc:
            raise Refusal('approved request is not a nonce-bound object') from exc
        if not isinstance(approved_nonce, str) or not NONCE.fullmatch(approved_nonce):
            raise Refusal('approved request nonce must be 64 hex')
        try:
            _row_write(parent, digest + '.spent', digest.encode())
            _row_write(parent, 'nonce-' + approved_nonce + '.spent', digest.encode())
        except FileExistsError as exc:
            raise Refusal('approved request or nonce already spent') from exc
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
    result.pop('payload_stdout')  # exact bytes already encoded; no Unicode round trip
    print(json.dumps(result, sort_keys=True, default=str))
    return 0 if result['candidate'] is not None else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
