"""Guest supervisor: transient systemd SERVICE with an outside-payload monitor.

Runs as root through the VM route (`prlctl exec`), because the system manager cannot
start a unit as another account without root. The payload runs as the named
non-root account. Nothing here is installed by source checks; tests inject `ops`,
the only route to systemctl, systemd-run and cgroupfs.

Upstream basis (systemd v259, fetched 2026-11-07):
- `systemd-run --pipe` waits like `--wait`; `--wait` cannot combine with `--scope`.
- `--collect` unloads the unit after it finishes, so it is never used. Without it a
  finished transient service stays loaded, inactive, with Result and ExecMainStatus.
- `ExitType=cgroup` (v250) keeps a service active while any process in its cgroup
  has not exited; `RuntimeMaxSec=` fails the unit at its deadline.
- systemctl prints a byte-array property such as InvocationID as contiguous lowercase
  hex with no hyphens (bus-print-properties.c, the "y" branch).

Credit is never decided here. The guest returns a candidate. The host credits closure
(linux_setup.host_credit) only after its own VM-route observation agrees with it.
"""
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_observations as observations
from preparation_stage import Refusal, exclusive

NONCE = re.compile(r'^[0-9a-f]{64}$')
INVOCATION = re.compile(r'^[0-9a-f]{32}$')
MIN_SYSTEMD = 250          # ExitType=cgroup
MAX_TIMEOUT = 900          # per-stage ceiling in the setup manifest
CLEANUP_RESERVE_SECONDS = 30
FORBIDDEN_RUN_FLAGS = ('--scope', '--collect', '--user', '--no-block', '--wait')
SUPERVISOR = 'systemd-service-cgroup-v2'
BOOT_ID_PATH = '/proc/sys/kernel/random/boot_id'
CGROUP_ROOT = '/sys/fs/cgroup'
PROPERTIES = 'LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup'


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
        '--property=RuntimeMaxSec=%d' % timeout,
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


class Monitor:
    """Outside-payload monitor on its own thread.

    Samples the unit while it is loaded: InvocationID, ControlGroup, ActiveState and
    the cgroup's `populated` bit. A failed read is recorded as a failure, never as a
    pass. Absence before start is normal and recorded as such.
    """

    def __init__(self, ops, unit, interval=0.05):
        self.ops, self.unit, self.interval = ops, unit, interval
        self.samples = []
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._loop, name='effigy-stage-monitor', daemon=True)

    def start(self):
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
                  'active_state': show['active_state'], 'populated': None}
        if show['control_group']:
            try:
                record['populated'] = parse_populated(self.ops.events(events_path(show['control_group'])))
            except Refusal as exc:
                record['populated_error'] = str(exc)
        return record

    def _loop(self):
        while not self._stop.is_set():
            self.samples.append(self.poll_once())
            self._stop.wait(self.interval)


def closure_receipt(boot_id, nonce, show, observed_group, waited_exit):
    """Candidate for the host: terminal facts plus the cgroup the monitor saw while active."""
    return {'boot_id': boot_id, 'nonce': nonce, 'unit': unit_name(nonce),
            'invocation_id': show['invocation_id'], 'control_group': '',
            'observed_control_group': observed_group,
            'supervisor': SUPERVISOR, 'terminal_basis': observations.UNIT_INACTIVE,
            'populated': 0, 'active_state': show['active_state'], 'result': show['result'],
            'waited_exit': waited_exit}


def _eligible(boot_id, nonce, show, samples, waited_exit):
    invocation = show['invocation_id']
    seen = [s for s in samples if s.get('kind') == 'loaded' and s['invocation_id'] == invocation]
    if any(s.get('populated') == 1 for s in seen):
        return None, 'monitor saw a live descendant for this InvocationID'
    active = [s for s in seen if s['active_state'] == 'active' and s['control_group']]
    if not active:
        return None, 'monitor never observed the unit active; closure is unknown'
    groups = {s['control_group'] for s in active}
    if len(groups) != 1:
        return None, 'monitor saw more than one control group for this InvocationID'
    if show['active_state'] != 'inactive' or show['result'] != 'success' or show['exec_main_status'] != '0':
        return None, 'post-run terminal state is not inactive success'
    if show['control_group'] != '':
        return None, 'post-run unit still has a control group'
    return closure_receipt(boot_id, nonce, show, groups.pop(), waited_exit), None


def run_supervised(stage_dir, request, ops, clock, boot_id, seen_invocations, monitor=None):
    """One-use supervised payload. Intent is spent before any effect.

    Candidate eligibility, all required:
    - the unit was absent before start (the nonce is fresh);
    - the monitor saw this InvocationID loaded and active with one ControlGroup;
    - no monitor sample for this InvocationID showed populated 1 (veto);
    - the post-run unit is loaded with the same InvocationID, inactive, Result=success,
      ExecMainStatus=0, and an empty ControlGroup.
    A failed monitor read, a vanished unit or a changed InvocationID is unknown.
    Returns {'disposition', 'candidate', 'reason', 'show', 'samples'}.
    """
    stage = Path(stage_dir)
    info = stage.lstat()
    if (not stat.S_ISDIR(info.st_mode) or stage.is_symlink() or info.st_uid != request['payload_uid']
            or stat.S_IMODE(info.st_mode) != 0o700):
        raise Refusal('stage directory must be owned by the payload account and mode 0700')
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
    argv = run_argv(nonce, request['payload_user'], request['cwd'], request['timeout_seconds'], request['argv'])
    intent = {'unit': unit, 'argv': argv, 'boot_id': boot_id, 'timeout_seconds': request['timeout_seconds'],
              'cleanup_reserve_seconds': CLEANUP_RESERVE_SECONDS}
    exclusive(stage / 'intent.json', (json.dumps(intent, sort_keys=True, indent=2) + '\n').encode())
    seen_invocations.setdefault('nonces', set()).add(nonce)
    if parse_show(ops.show(show_argv(unit)), unit) is not None:
        raise Refusal('unit already loaded before start; nonce is not fresh')
    monitor = monitor or Monitor(ops, unit)
    start = clock()
    deadline = start + request['timeout_seconds'] + CLEANUP_RESERVE_SECONDS
    monitor.start()
    try:
        outcome = ops.run(argv, request['timeout_seconds'] + CLEANUP_RESERVE_SECONDS)
        disconnected = False
    except Disconnect:
        outcome = {'returncode': None, 'stdout': b'', 'stderr': b'', 'timed_out': False}
        disconnected = True
    finally:
        monitor.stop()
    for name in ('stdout', 'stderr'):
        exclusive(stage / name, outcome[name])
    timed_out = bool(outcome.get('timed_out')) or clock() > deadline
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
        candidate, reason = _eligible(boot_id, nonce, show, samples, outcome['returncode'])
        if candidate is not None and invocation_reused(seen_invocations, show['invocation_id']):
            raise Refusal('InvocationID reused')
    if candidate is not None:
        seen_invocations.setdefault('invocations', set()).add(show['invocation_id'])
    record = {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show,
              'samples': samples, 'elapsed_seconds': clock() - start,
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    exclusive(stage / 'supervisor.json', (json.dumps(record, sort_keys=True, indent=2, default=str) + '\n').encode())
    return {'disposition': disposition, 'candidate': candidate, 'reason': reason, 'show': show, 'samples': samples}


def invocation_reused(seen_invocations, invocation):
    return invocation in seen_invocations.get('invocations', ())


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


def main(argv):
    """Root entry point: `effigy-stage-supervisor --request PATH --sha256 HEX`.

    Prints one JSON line with the disposition and the candidate. It never credits closure.
    """
    if len(argv) != 4 or argv[0] != '--request' or argv[2] != '--sha256':
        raise Refusal('usage: --request PATH --sha256 HEX')
    raw = Path(argv[1]).read_bytes()
    if len(raw) > 1024 * 1024 or hashlib.sha256(raw).hexdigest() != argv[3]:
        raise Refusal('request hash mismatch')
    request = json.loads(raw)
    required = {'nonce', 'payload_user', 'payload_uid', 'cwd', 'timeout_seconds', 'argv', 'stage_dir', 'expected_boot_id'}
    if set(request) != required:
        raise Refusal('unsupported request schema')
    boot_id = Path(BOOT_ID_PATH).read_text().strip()
    if boot_id != request['expected_boot_id']:
        raise Refusal('boot identity differs from the request')
    result = run_supervised(request['stage_dir'], request, SystemdOps(_subprocess_runner), time.monotonic, boot_id, {})
    print(json.dumps(result, sort_keys=True, default=str))
    return 0 if result['candidate'] is not None else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
