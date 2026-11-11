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
import shlex
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
HEX64 = NONCE
INVOCATION = re.compile(r'^[0-9a-f]{32}$')
MIN_SYSTEMD = 250          # ExitType=cgroup
MAX_TIMEOUT = 900          # per-stage ceiling in the setup manifest
CLEANUP_RESERVE_SECONDS = 30
GATE_RESERVE_SECONDS = 30
FORBIDDEN_RUN_FLAGS = ('--scope', '--collect', '--user', '--no-block', '--wait')
SUPERVISOR = 'systemd-service-cgroup-v2'
BOOT_ID_PATH = '/proc/sys/kernel/random/boot_id'
CGROUP_ROOT = '/sys/fs/cgroup'
PROPERTIES = 'LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup,MainPID'
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
    main_pid = values.get('MainPID', '') or '0'
    if not main_pid.isdigit():
        raise Refusal('MainPID is not a process id')
    return {'unit': unit, 'invocation_id': normalize_invocation(values.get('InvocationID', '')),
            'control_group': group, 'active_state': values.get('ActiveState', ''),
            'sub_state': values.get('SubState', ''), 'result': values.get('Result', ''),
            'exec_main_status': values.get('ExecMainStatus', ''), 'main_pid': int(main_pid)}


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


CONFINEMENT = {'PrivateUsers': 'identity', 'NoNewPrivileges': 'yes',
               'SupplementaryGroups': '', 'CapabilityBoundingSet': ''}
CONFINEMENT_PROPERTIES = tuple(CONFINEMENT)


def property_argv(unit):
    """Fixed effective-property read for the confined payload unit (one read, no caller choice)."""
    return ['systemctl', 'show', unit, '--property=' + ','.join(CONFINEMENT_PROPERTIES)]


def parse_properties(text, unit):
    """Parse the four fixed effective properties; a missing, extra or different value refuses."""
    if not isinstance(text, str):
        raise Refusal('unit properties are unavailable')
    values = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        key, sep, value = line.partition('=')
        if not sep or key in values:
            raise Refusal('unit properties are malformed or repeat a key')
        values[key] = value
    if set(values) != set(CONFINEMENT_PROPERTIES):
        raise Refusal('unit properties are not the four fixed confinement properties')
    if values != CONFINEMENT:
        raise Refusal('unit confinement differs from the fixed confinement properties: ' +
                      ','.join(key for key in CONFINEMENT_PROPERTIES if values[key] != CONFINEMENT[key]))
    return dict(values)


def parse_proc_status(text):
    """Parse the fields the writer bound needs from /proc/<pid>/status; absent fields refuse."""
    if not isinstance(text, str):
        raise Refusal('payload process status is unavailable')
    values = {}
    for line in text.splitlines():
        key, sep, value = line.partition(':')
        if sep:
            values[key.strip()] = value.strip()
    required = ('Uid', 'Groups', 'CapEff', 'CapPrm', 'NoNewPrivs')
    missing = [key for key in required if key not in values]
    if missing:
        raise Refusal('payload process status lacks ' + ','.join(missing))
    try:
        return {'uid': int(values['Uid'].split()[0]),
                'groups': sorted(int(group) for group in values['Groups'].split()),
                'cap_eff': int(values['CapEff'], 16), 'cap_prm': int(values['CapPrm'], 16),
                'no_new_privs': int(values['NoNewPrivs'])}
    except (ValueError, IndexError) as exc:
        raise Refusal('payload process status is malformed') from exc


def namespace_inode(text):
    """`user:[4026532345]` from /proc/<pid>/ns/user; only the numeric inode is compared."""
    if not isinstance(text, str):
        raise Refusal('user namespace link is unavailable')
    match = re.fullmatch(r'user:\[(\d+)\]', text.strip())
    if not match:
        raise Refusal('user namespace link is malformed')
    return int(match.group(1))


def docker_gid(group_text):
    """The docker group's gid from the guest /etc/group, or None when the group is absent."""
    if not isinstance(group_text, str):
        raise Refusal('group database is unavailable')
    for line in group_text.splitlines():
        fields = line.split(':')
        if len(fields) >= 4 and fields[0] == 'docker' and fields[2].isdigit():
            return int(fields[2])
    return None


def confinement_from_process(status, user_ns, init_user_ns, groups_text, payload_uid):
    """Check one pre-release process observation against the required confinement.

    The private identity user namespace is the source-qualified mechanism that keeps an owning
    writer from changing project attributes (fs/file_attr.c and fs/ioctl.c reject that outside the
    initial user namespace), and the empty capability sets remove the quota-bypass authority
    (fs/quota/dquot.c checks CAP_SYS_RESOURCE). An empty supplementary group set removes the Docker
    socket channel for this setup workload only. Absence or a different value refuses.
    """
    if type(payload_uid) is not int or payload_uid <= 0:
        raise Refusal('payload uid is required for the confinement observation')
    facts = parse_proc_status(status)
    if facts['uid'] != payload_uid:
        raise Refusal('payload main process runs as another uid')
    if facts['cap_eff'] or facts['cap_prm']:
        raise Refusal('payload retains capabilities; the quota-bypass authority is not removed')
    if facts['no_new_privs'] != 1:
        raise Refusal('payload has no no-new-privileges guarantee')
    private_ns = namespace_inode(user_ns)
    host_ns = namespace_inode(init_user_ns)
    if private_ns == host_ns:
        raise Refusal('payload runs in the initial user namespace; project reassignment is possible')
    docker = docker_gid(groups_text)
    if docker is not None and docker in facts['groups']:
        raise Refusal('payload keeps the docker group; an unconfined channel defeats the writer bound')
    return {'uid': facts['uid'], 'groups': facts['groups'], 'cap_eff': 0, 'cap_prm': 0,
            'no_new_privs': 1, 'user_ns': private_ns, 'init_user_ns': host_ns, 'docker_gid': docker}


def confined_run_argv(nonce, payload_user, cwd, timeout, argv):
    """Production payload argv: the fixed confinement properties are installed before the gate.

    The service manager must install all four properties before exec. PrivateUsers=identity gives the
    payload its own identity user namespace (no project or inheritance-flag change), and the empty
    CapabilityBoundingSet with NoNewPrivileges removes the quota-bypass authority across exec and
    descendants. SupplementaryGroups is emptied only for this setup workload, so the account keeps
    its docker grant everywhere else. The installer's effective values are read back from the unit
    (parse_properties) before the gate opens; an unknown or different value refuses.
    """
    command = run_argv(nonce, payload_user, cwd, timeout, argv)
    separator = command.index('--')
    properties = ['--property=%s=%s' % (key, CONFINEMENT[key]) for key in CONFINEMENT_PROPERTIES]
    return command[:separator] + properties + command[separator:]


# Historical charged-tree path remains the fixed broker destination.
QUOTA_ROOT = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3'


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


def drop_quota_bypass(capget, capset, prctl):
    """The fixed child preparation every allocating writer runs before it opens for write.

    Sets no-new-privileges, clears ambient capabilities and removes CAP_SYS_RESOURCE from the
effective, permitted and inheritable sets, then re-reads all three. The check is repeated in the
same process that performs the write and it survives exec and descendants, so a restored or
inherited bypass refuses rather than silently re-acquiring quota authority.
    """
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


def broker_record(row_fd, name, data, expected_dev, owner,
                  capget, capset, prctl, read_project=_broker_project_attributes):
    """Bounded exclusive record write under a held, project-custodied directory descriptor.

    Checks the fixed record name, the held directory owner/device and project inheritance, drops the
    quota bypass, re-checks the directory, then creates the exclusive bounded record. The payload
    broker uses the same custody and drop for its transfer file (`broker_write`).
    """
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
    drop_quota_bypass(capget, capset, prctl)
    after, post_project = observe_stat(), observe_project()
    if (before.st_dev, before.st_ino, before.st_uid, before.st_mode) != \
            (after.st_dev, after.st_ino, after.st_uid, after.st_mode) or post_project != project:
        raise Refusal('broker directory changed after capability drop')
    _row_write(row_fd, name, data)


def libc_capability_ops():
    """The reviewed capability syscall handles, or a refusal when libc does not expose them.

    One resolver so the broker child and the live broker writer cannot disagree about the ABI.
    """
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
    return capget, capset, prctl


def default_bypass_drop():
    """The live drop: resolve the capability syscalls, then re-read the result before any write."""
    drop_quota_bypass(*libc_capability_ops())


def launch_quota_record(row_fd, name, data, expected_dev, deadline, cleanup_deadline,
                        test_capops=None, test_project=None):
    """Launch one bounded broker child; a timeout kills only that child and never grants closure."""
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
        capget, capset, prctl = libc_capability_ops()
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
            broker_record(row_fd, name, data, expected_dev, owner,
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


TRANSFER_SUBCOMMAND = '--transfer-writer'
BROKER_SUBCOMMAND = '--broker-write'
SUPERVISOR_PATH = '/usr/local/libexec/effigy-stage-supervisor'
MAX_TRANSFER_BYTES = 512 << 20
BROKER_DEST_PREFIX = QUOTA_ROOT + '/'


def transfer_writer_argv(fifo):
    """The reviewed remote command; the forced command re-parses exactly this shape."""
    if not isinstance(fifo, str) or not fifo.startswith(STAGE_ROOT + '/'):
        raise Refusal('transfer writer endpoint is outside the stage root')
    return ['/usr/bin/python3', '-I', '-B', SUPERVISOR_PATH, TRANSFER_SUBCOMMAND, fifo]


def forced_command():
    """The exact key-restricted entry; sshd runs this and passes the requested argv as env."""
    return '/usr/bin/python3 -I -B ' + SUPERVISOR_PATH + ' ' + TRANSFER_SUBCOMMAND


def transfer_endpoint_reasons(original, stage_root=STAGE_ROOT):
    """Reasons the SSH requested command is not the one fixed writer form; empty means accepted.

    The qualification key is restricted to one forced command, so a session never gets a shell.
    The requested command is not executed; it only names the row's transfer FIFO, and it must be
    exactly the reviewed argv for a stage row, never a general command or an extra argument.
    """
    if not isinstance(original, str):
        return ['SSH_ORIGINAL_COMMAND is absent']
    try:
        tokens = shlex.split(original)
    except ValueError:
        return ['SSH_ORIGINAL_COMMAND is not a quoted argv']
    if len(tokens) != 6 or tokens[:5] != ['/usr/bin/python3', '-I', '-B', SUPERVISOR_PATH, TRANSFER_SUBCOMMAND]:
        return ['SSH_ORIGINAL_COMMAND is not the fixed transfer writer argv']
    fifo = tokens[5]
    prefix = stage_root + '/'
    if not fifo.startswith(prefix):
        return ['transfer endpoint is outside the reviewed stage root']
    parts = fifo[len(prefix):].split('/')
    if len(parts) != 2 or not ROW_NAME.fullmatch(parts[0]) or parts[1] != TRANSFER_NAME:
        return ['transfer endpoint is not a reviewed row FIFO']
    return []


def transfer_endpoint(original, stage_root=STAGE_ROOT):
    """The exact FIFO for one accepted request, or a refusal."""
    reasons = transfer_endpoint_reasons(original, stage_root)
    if reasons:
        raise Refusal(reasons[0])
    return shlex.split(original)[5]


def broker_write_argv(row, destination, sha256, max_bytes, fifo, uid):
    """The fixed payload that performs the row's allocating write inside the confined unit."""
    if not isinstance(row, str) or not ROW_NAME.fullmatch(row):
        raise Refusal('broker row name is invalid')
    return ['/usr/bin/python3', '-I', '-B', SUPERVISOR_PATH, BROKER_SUBCOMMAND, row, destination,
            sha256, str(max_bytes), fifo, str(uid)]


def _broker_destination(destination, row, fifo):
    """The fixed destination inside the charged root, and the row FIFO that feeds it."""
    if not isinstance(destination, str) or not destination.startswith(BROKER_DEST_PREFIX) or \
            '..' in Path(destination).parts or os.path.normpath(destination) != destination:
        raise Refusal('broker destination is outside the charged stage root')
    if not isinstance(row, str) or not ROW_NAME.fullmatch(row):
        raise Refusal('broker row name is invalid')
    expected = STAGE_ROOT + '/' + row + '/' + TRANSFER_NAME
    if fifo != expected:
        raise Refusal('broker source is not this row transfer FIFO')
    return destination


def _broker_parent_fd(destination):
    parent = os.path.dirname(destination)
    if not parent:
        raise Refusal('broker destination has no parent')
    fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        project, flags = _broker_project_attributes(fd)
        if project != BROKER_PROJECT_ID or not flags & BROKER_PROJINHERIT:
            raise Refusal('broker destination parent is not the charged inheriting project')
        if info.st_uid not in (0, os.geteuid()) or info.st_mode & 0o022:
            raise Refusal('broker destination parent custody differs')
        return fd
    except BaseException:
        os.close(fd)
        raise


def broker_write(destination, expected_sha256, max_bytes, fifo, row, uid,
                 drop=None, parent_fd=None, clock=time.monotonic, deadline=900):
    """Stream one bounded file into the charged project as the payload, with custody checked first.

    The allocating open happens only after the parent's project/inheritance, custody and the
    process's own confinement are re-read with every capability set emptied. The live path resolves
    the capability syscalls itself (`default_bypass_drop`); `drop` is a control seam and is the only
    way to replace that step. The write is exclusive, never follows a link, stops at `max_bytes` and
    must hash to the reviewed bytes; a short, long, mismatched or partially written file refuses and
    leaves its charge visible.
    """
    if not isinstance(expected_sha256, str) or not HEX64.fullmatch(expected_sha256):
        raise Refusal('broker expected digest is invalid')
    if type(max_bytes) is not int or not 0 < max_bytes <= MAX_TRANSFER_BYTES:
        raise Refusal('broker byte ceiling is outside the fixed transfer bound')
    if type(uid) is not int or uid <= 0:
        raise Refusal('broker payload uid is invalid')
    _broker_destination(destination, row, fifo)
    parent = parent_fd if parent_fd is not None else _broker_parent_fd(destination)
    started = clock()
    own = parent_fd is None
    try:
        before = os.fstat(parent)
        (drop or default_bypass_drop)()
        with open(fifo, 'rb') as stream:
            try:
                fd = os.open(os.path.basename(destination),
                             os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
            except OSError as exc:
                raise Refusal('broker destination already exists; a placement is never reused') from exc
            digest, total = hashlib.sha256(), 0
            with os.fdopen(fd, 'wb') as output:
                while True:
                    chunk = stream.read(65536)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_bytes:
                        raise Refusal('broker transfer exceeded the row ceiling')
                    digest.update(chunk)
                    output.write(chunk)
                output.flush()
                os.fsync(output.fileno())
            if clock() - started > deadline:
                raise Refusal('broker transfer exceeded its deadline')
        if total == 0 or digest.hexdigest() != expected_sha256:
            raise Refusal('broker transfer bytes differ from the reviewed digest')
        after = os.fstat(parent)
        if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
            raise Refusal('broker destination parent changed during the write')
        return {'destination': destination, 'bytes': total, 'sha256': digest.hexdigest(), 'row': row}
    finally:
        if own:
            os.close(parent)


def broker_write_main(argv, parent_fd=None, drop=None):
    """Fixed payload entry: `--broker-write ROW DEST SHA256 MAX FIFO UID`. Nothing is caller-chosen."""
    if len(argv) != 6:
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    row, destination, digest, raw_max, fifo, raw_uid = argv
    try:
        if not raw_max.isdigit() or not raw_uid.isdigit():
            raise Refusal('broker byte ceiling or uid is malformed')
        if os.geteuid() == 0:
            raise Refusal('broker writer must not run as root')
        result = broker_write(destination, digest, int(raw_max), fifo, row, int(raw_uid),
                              drop=drop, parent_fd=parent_fd)
    except (Refusal, OSError) as exc:
        print('status=refused phase=broker-write reason=' + str(exc), file=sys.stderr)
        return 3
    print('%s  %s' % (result['sha256'], result['destination']))
    return 0


def transfer_writer_main(argv, environ=None, stdin=None, stage_root=None, deadline=900):
    """The forced-command writer: only the row FIFO named by the reviewed request is writable.

    The session cannot obtain a shell, change directories, create a file or name another path: the
    forced command ignores everything except the exact reviewed argv, checks the row stage is a
    root-owned non-writable directory and the endpoint is its transfer FIFO, then copies stdin into
    that FIFO. The allocating write stays in the confined unit's payload.
    """
    environ = os.environ if environ is None else environ
    if argv != [TRANSFER_SUBCOMMAND]:
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    seam = environ.get('EFFIGY_STAGE_ROOT')
    if seam is not None and os.geteuid() == 0:
        print('status=refused phase=seam', file=sys.stderr)
        return 2
    stage_root = stage_root or seam or STAGE_ROOT
    try:
        fifo = transfer_endpoint(environ.get('SSH_ORIGINAL_COMMAND'), stage_root)
        # The root supervisor creates the row stage before it starts the payload; this session may
        # arrive first. Wait for the reviewed endpoint under the same absolute deadline, then check
        # its custody. A stage that never appears ends at the deadline and reports writer failure.
        started = time.monotonic()
        while not os.path.lexists(fifo):
            if time.monotonic() - started > deadline:
                raise Refusal('row transfer endpoint never appeared before the deadline')
            time.sleep(0.1)
        parent = os.path.dirname(fifo)
        info = os.lstat(parent)
        if not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.geteuid()) or \
                stat.S_IMODE(info.st_mode) != 0o711:
            # A root run requires the root-owned 0711 stage the supervisor creates; a non-root
            # process can only see its own fixture, so the check admits only that same owner.
            raise Refusal('row stage custody differs')
        endpoint = os.lstat(fifo)
        if not stat.S_ISFIFO(endpoint.st_mode):
            raise Refusal('row transfer endpoint is not a FIFO')
        source = sys.stdin.buffer if stdin is None else stdin
        total, started = 0, time.monotonic()
        with open(fifo, 'wb') as output:
            while True:
                chunk = source.read(65536)
                if not chunk:
                    break
                total += len(chunk)
                if total > MAX_TRANSFER_BYTES:
                    raise Refusal('forced-command writer exceeded the transfer ceiling')
                if time.monotonic() - started > deadline:
                    raise Refusal('forced-command writer exceeded its deadline')
                output.write(chunk)
        return 0
    except (Refusal, OSError) as exc:
        print('status=refused phase=transfer-writer reason=' + str(exc), file=sys.stderr)
        return 3


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
    confined = request.get('confinement')
    if confined is not None and confined != CONFINEMENT:
        raise Refusal('requested confinement is not the fixed confinement property set')
    build = confined_run_argv if confined is not None else run_argv
    argv = build(nonce, request['payload_user'], request['cwd'], request['timeout_seconds'],
                 gate_argv(str(gate), payload))
    intent = {'unit': unit, 'argv': argv, 'boot_id': boot_id, 'timeout_seconds': request['timeout_seconds'],
              'cleanup_reserve_seconds': CLEANUP_RESERVE_SECONDS, 'gate': str(gate),
              'confinement': dict(confined) if confined is not None else None}
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
    confined = request.get('confinement') is not None
    observed = {}

    def observe(record):
        if confined:
            observed['properties'] = _confined_properties(ops, unit)
            observed['process'] = _confined_process(ops, unit, request['payload_uid'])
        _release_row_gate(gate_fd)

    monitor = monitor or Monitor(ops, unit, on_active=observe,
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
    if candidate is not None and confined:
        try:
            observed['properties'] = _confined_properties(ops, unit)
            observed['process'] = observed.get('process') or _confined_process(
                ops, unit, request['payload_uid'])
        except Refusal as exc:
            candidate, reason = None, 'confinement is unproven: ' + str(exc)
    if candidate is not None:
        seen_invocations.setdefault('invocations', set()).add(show['invocation_id'])
    if any(len(outcome[name]) > PAYLOAD_STDOUT_CAP for name in ('stdout', 'stderr')):
        candidate, reason, disposition = None, 'partial output exceeds retained transport cap', 'output-cap'
    payload_stdout = outcome['stdout'][:PAYLOAD_STDOUT_CAP]
    record = {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
              'samples': samples, 'release_index': monitor.release_index, 'elapsed_seconds': clock() - start,
              'confinement': observed or None,
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    _record_after_start(row_fd, 'supervisor.json',
                        (json.dumps(record, sort_keys=True, indent=2, default=str) + '\n').encode(), ops, unit)
    return {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
            'samples': samples, 'payload_stdout': payload_stdout.decode('utf-8', 'replace'),
            'payload_stdout_b64': base64.b64encode(payload_stdout).decode('ascii'),
            'payload_stderr_b64': base64.b64encode(outcome['stderr'][:PAYLOAD_STDOUT_CAP]).decode('ascii'),
            'confinement': observed or None}


def _confined_properties(ops, unit):
    """Read the installed effective properties from the unit itself and require the fixed values."""
    return parse_properties(ops.properties(property_argv(unit)), unit)


def _confined_process(ops, unit, payload_uid):
    """Observe the payload main process from outside it, while it is held at the gate.

    The unit must be loaded with a MainPID. The observation reads the guest /proc status, the
    process's user-namespace link, the init user-namespace link and the group database, and requires
    the private identity namespace, empty capability sets, no-new-privileges and no docker
    supplementary group. A missing or unreadable fact refuses.
    """
    show = parse_show(ops.show(show_argv(unit)), unit)
    if show is None or show['main_pid'] <= 0:
        raise Refusal('payload unit has no main process to observe')
    pid = show['main_pid']
    return confinement_from_process(ops.proc('/proc/%d/status' % pid),
                                    ops.proc_ns('/proc/%d/ns/user' % pid),
                                    ops.proc_ns('/proc/1/ns/user'),
                                    ops.proc('/etc/group'), payload_uid)


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

    def properties(self, argv):
        return self._checked(argv, 30)

    def proc(self, path):
        return Path(path).read_text(encoding='utf-8', errors='surrogateescape')

    def proc_ns(self, path):
        return os.readlink(path)


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
    """Installed entry: supervised payload, bounded broker and forced transfer writer."""
    if argv[:1] == ['--broker-write']:
        return broker_write_main(argv[1:])
    if argv[:1] == ['--transfer-writer']:
        return transfer_writer_main(argv)
    if len(argv) != 2 or argv[0] != '--approved-request':
        raise Refusal('usage: --approved-request SHA256 | --broker-write ... | --transfer-writer')
    approval_seam = os.environ.get('EFFIGY_APPROVAL_ROOT')
    if approval_seam is not None and os.geteuid() == 0:
        raise Refusal('test seams are refused for root runs')
    raw = consume_approved_request(argv[1], approval_seam or AUTH_ROOT,
                                   os.geteuid() if approval_seam else 0)
    request = json.loads(raw)
    required = {'nonce', 'payload_user', 'payload_uid', 'cwd', 'timeout_seconds', 'argv', 'stage_dir',
                'expected_boot_id'}
    if not required <= set(request) or set(request) - required - {'transfer', 'input_hashes', 'resources',
                                                                   'ssh_binding', 'confinement', 'quota'}:
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
