"""Guest supervisor: transient systemd SERVICE with cgroup containment.

Runs as root through the VM route (`prlctl exec`), because the system manager
cannot start a unit as another account without root. The payload runs as the
named non-root account. Nothing here is installed or run by source checks; tests
inject `runner`, the only route to systemctl, systemd-run and the payload.

Upstream basis (systemd v259 manuals, fetched 2026-11-07):
- `systemd-run --pipe` waits like `--wait`; `--wait` cannot combine with `--scope`.
- `--collect` unloads the unit after it finishes, so it is never used. Without it
  the loaded, inactive unit keeps its Result and ExecMainStatus for inspection.
- `ExitType=cgroup` (v250) keeps a service active while any process in its
  cgroup has not exited; `RuntimeMaxSec=` fails the unit at its deadline.
  Hence an inactive unit with Result=success has an empty cgroup.
"""
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_observations as observations
from preparation_stage import Refusal, exclusive

NONCE = re.compile(r'^[0-9a-f]{64}$')
UUID = re.compile(r'^[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}$')
MIN_SYSTEMD = 250          # ExitType=cgroup
MAX_TIMEOUT = 900          # per-stage ceiling in the setup manifest
CLEANUP_RESERVE_SECONDS = 30
FORBIDDEN_RUN_FLAGS = ('--scope', '--collect', '--user', '--no-block', '--wait')
SUPERVISOR = 'systemd-service-cgroup-v2'
BOOT_ID_PATH = '/proc/sys/kernel/random/boot_id'


def unit_name(nonce):
    if not isinstance(nonce, str) or not NONCE.fullmatch(nonce):
        raise Refusal('nonce must be 256-bit lowercase hexadecimal')
    return 'effigy-stage-' + nonce


def run_argv(nonce, payload_user, cwd, timeout, argv):
    """Transient SERVICE argv. Scope, collect and user-manager flags are refused."""
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
    return ['systemctl', 'show', unit,
            '--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup']


def stop_argv(unit):
    return ['systemctl', 'kill', '--kill-whom=all', '--signal=SIGKILL', unit]


def parse_systemd_version(text):
    match = re.match(r'^systemd (\d+)', text or '')
    if not match:
        raise Refusal('systemd version unavailable')
    return int(match.group(1))


def parse_show(text, unit):
    """Parse `systemctl show` key=value lines. None means the unit is not loaded."""
    if text is None:
        return None
    values = {}
    for line in text.splitlines():
        if '=' in line:
            key, value = line.split('=', 1)
            values[key] = value
    if values.get('LoadState') != 'loaded':
        return None
    invocation = values.get('InvocationID', '')
    if not UUID.fullmatch(invocation):
        raise Refusal('InvocationID missing or malformed')
    group = values.get('ControlGroup', '')
    if group and (not group.startswith('/') or '..' in Path(group).parts):
        raise Refusal('ControlGroup not canonical')
    return {'unit': unit, 'invocation_id': invocation.lower(), 'control_group': group,
            'active_state': values.get('ActiveState', ''), 'sub_state': values.get('SubState', ''),
            'result': values.get('Result', ''), 'exec_main_status': values.get('ExecMainStatus', '')}


def parse_populated(text):
    """cgroup.events `populated` (0/1). None means the read vanished."""
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
    return '/sys/fs/cgroup' + control_group + '/cgroup.events'


def sample(show_text, events_text, unit):
    """Outside-monitor observation while the unit is loaded and active.

    A sample can only veto credit: a live `populated 1` for the same InvocationID
    means a descendant survived the leader. Absence of samples grants nothing.
    """
    show = parse_show(show_text, unit)
    if show is None or not show['control_group']:
        return None
    populated = parse_populated(events_text)
    if populated is None:
        return None
    return {'invocation_id': show['invocation_id'], 'control_group': show['control_group'],
            'populated': populated, 'active_state': show['active_state']}


def closure_receipt(boot_id, nonce, show, waited_exit):
    """Receipt for observations.verify_contained_closure, inactive terminal basis."""
    return {'boot_id': boot_id, 'nonce': nonce, 'unit': unit_name(nonce),
            'invocation_id': show['invocation_id'], 'control_group': '',
            'supervisor': SUPERVISOR, 'terminal_basis': observations.UNIT_INACTIVE,
            'populated': 0, 'active_state': show['active_state'], 'result': show['result'],
            'waited_exit': waited_exit}


def run_supervised(stage_dir, request, ops, clock, boot_id, seen_invocations, authenticate, samples):
    """One-use supervised payload. Intent is spent before any injected effect.

    Credit needs: the unit absent before start (the nonce is fresh), a loaded
    post-run unit with the same new InvocationID, inactive, Result=success,
    ExecMainStatus=0, empty ControlGroup, and no live-populated sample for it.
    Anything else is unknown. Returns (disposition, closure).
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
    start = clock()
    deadline = start + request['timeout_seconds'] + CLEANUP_RESERVE_SECONDS
    try:
        outcome = ops.run(argv, request['timeout_seconds'] + CLEANUP_RESERVE_SECONDS)
        disconnected = False
    except Disconnect:
        outcome = {'returncode': None, 'stdout': b'', 'stderr': b'', 'timed_out': False}
        disconnected = True
    for name in ('stdout', 'stderr'):
        exclusive(stage / name, outcome[name])
    timed_out = bool(outcome.get('timed_out')) or clock() > deadline
    if timed_out or disconnected:
        ops.stop(stop_argv(unit))
    show = parse_show(ops.show(show_argv(unit)), unit)
    if show is not None:
        if show['invocation_id'] in seen_invocations.get('invocations', ()):
            raise Refusal('InvocationID reused')
        seen_invocations.setdefault('invocations', set()).add(show['invocation_id'])
    vetoed = show is not None and any(
        s is not None and s['invocation_id'] == show['invocation_id'] and s['populated'] == 1 for s in samples)
    disposition = 'timeout' if timed_out else ('disconnect' if disconnected else (
        'exited' if outcome['returncode'] == 0 else 'failed'))
    closure = 'unknown'
    if (show is not None and disposition == 'exited' and not vetoed and show['active_state'] == 'inactive'
            and show['result'] == 'success' and show['exec_main_status'] == '0' and show['control_group'] == ''):
        receipt = closure_receipt(boot_id, nonce, show, outcome['returncode'])
        expected = {'boot_id': boot_id, 'nonce': nonce, 'unit': unit,
                    'invocation_id': show['invocation_id'], 'control_group': ''}
        try:
            closure = observations.verify_contained_closure(expected, receipt, authenticate)
        except observations.Refusal as exc:
            # One refusal type for callers; the verifier's own class is not public API here.
            raise Refusal(str(exc)) from exc
        (stage / 'closure.json').write_bytes((json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode())
    record = {'disposition': disposition, 'closure': closure, 'elapsed_seconds': clock() - start,
              'show': show, 'vetoed': vetoed, 'samples': len(samples),
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    exclusive(stage / 'supervisor.json', (json.dumps(record, sort_keys=True, indent=2) + '\n').encode())
    return disposition, closure


class Disconnect(Exception):
    """Raised by an injected transport when the channel drops before a terminal result."""


def _subprocess_runner(argv, timeout):
    """Real runner: own session, deadline kills only its own process group."""
    proc = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=timeout)
        return {'returncode': proc.returncode, 'stdout': out, 'stderr': err, 'timed_out': False}
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, 9)
        out, err = proc.communicate()
        return {'returncode': None, 'stdout': out, 'stderr': err, 'timed_out': True}


class SystemdOps:
    """Adapter from the injected runner to the systemd calls used above."""

    def __init__(self, runner=_subprocess_runner):
        self.runner = runner

    def _checked(self, argv, timeout=30):
        result = self.runner(argv, timeout)
        if result['returncode'] != 0 or result.get('timed_out'):
            raise Refusal('systemctl call failed: ' + ' '.join(argv[:2]))
        return result['stdout'].decode('utf-8', 'replace')

    def version(self):
        return self._checked(['systemctl', '--version']).splitlines()[0]

    def cgroup2(self):
        return self._checked(['stat', '-fc', '%T', '/sys/fs/cgroup']).strip()

    def run(self, argv, timeout):
        result = self.runner(argv, timeout)
        return {'returncode': result['returncode'], 'stdout': result['stdout'],
                'stderr': result['stderr'], 'timed_out': bool(result.get('timed_out'))}

    def stop(self, argv):
        self.runner(argv, 30)

    def show(self, argv):
        # A failed read must refuse: treating it as "absent" could start a second unit.
        return self._checked(argv)


def main(argv):
    """Root entry point: `effigy-stage-supervisor --request PATH --sha256 HEX`.

    Prints one JSON line with the disposition and closure. The request hash binds
    the exact payload, and the VM route returns the stdout to the host.
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
    disposition, closure = run_supervised(request['stage_dir'], request, SystemdOps(), time.monotonic,
                                          boot_id, {}, lambda receipt: receipt.get('nonce') == request['nonce'], [])
    print(json.dumps({'disposition': disposition, 'closure': closure, 'nonce': request['nonce']}, sort_keys=True))
    return 0 if closure == 'closed' else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
