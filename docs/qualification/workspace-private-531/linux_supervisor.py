"""Guest supervisor adapter: transient systemd SERVICE with cgroup containment.

Source-control only. Nothing here installs a supervisor, starts a unit or talks
to a guest. Callers inject `ops`, the only route to systemd, cgroupfs and the
payload. Upstream basis (systemd v259 manuals, fetched 2026-11-07):

- `systemd-run --pipe` waits like `--wait`; `--wait` cannot combine with `--scope`.
- `--collect` unloads the unit after it finishes, so it is never used.
- `ExitType=cgroup` (v250) keeps a service active while any process in its
  cgroup has not exited; `RuntimeMaxSec=` fails the unit at its deadline.

The unit's InvocationID is required as observed state. It is not confirmed as a
documented unit property in the manuals read, so a missing value fails closed.
"""
import hashlib
import json
import os
import re
import stat
import sys
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
CGROUP_ROOT = '/sys/fs/cgroup'


class Disconnect(Exception):
    """Raised by an injected transport when the channel drops before a terminal result."""


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
    """Parse `systemctl show` key=value lines. A missing unit is unknown, not a pass."""
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
    """cgroup.events `populated` (0/1). Unreadable or absent means unknown."""
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


def sample(show_text, events_text, unit):
    """One outside-monitor observation of the unit while it is loaded.

    ExitType=cgroup removes the cgroup as the unit stops, so the only positive
    empty fact is a sample taken while InvocationID and ControlGroup still hold.
    A vanished unit or cgroup gives None, which is unknown, never a pass.
    """
    show = parse_show(show_text, unit)
    if show is None or not show['control_group']:
        return None
    populated = parse_populated(events_text)
    if populated is None:
        return None
    return {'invocation_id': show['invocation_id'], 'control_group': show['control_group'],
            'populated': populated, 'active_state': show['active_state']}


def closure_receipt(boot_id, nonce, show, positive_sample, waited_exit):
    """Receipt shape for observations.verify_contained_closure.

    Credit needs a positive sample (populated 0) for the same InvocationID and
    ControlGroup, plus the post-run terminal state of that same unit.
    """
    return {'boot_id': boot_id, 'nonce': nonce, 'unit': unit_name(nonce),
            'invocation_id': show['invocation_id'], 'control_group': positive_sample['control_group'],
            'supervisor': SUPERVISOR, 'populated': positive_sample['populated'],
            'active_state': show['active_state'], 'result': show['result'],
            'waited_exit': waited_exit}


def run_supervised(stage_dir, request, ops, clock, boot_id, seen_invocations, authenticate, samples):
    """One-use supervised payload. Intent is spent before any injected effect.

    `ops` provides: version() -> str, cgroup2() -> str, run(argv, timeout) ->
    dict(returncode, stdout, stderr, timed_out) or raises Disconnect, stop(argv),
    show(argv) -> text or None. `samples` is the list the caller's outside
    monitor filled with sample() results while the unit ran; it is read here,
    never refilled. Returns (disposition, closure), closure 'closed' or 'unknown'.
    """
    stage = Path(stage_dir)
    info = stage.lstat()
    if (not stat.S_ISDIR(info.st_mode) or stage.is_symlink() or info.st_uid != os.geteuid()
            or stat.S_IMODE(info.st_mode) != 0o700):
        raise Refusal('stage directory must be owned and mode 0700')
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
    start = clock()
    deadline = start + request['timeout_seconds'] + CLEANUP_RESERVE_SECONDS
    try:
        outcome = ops.run(argv, request['timeout_seconds'])
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
    disposition = 'timeout' if timed_out else ('disconnect' if disconnected else (
        'exited' if outcome['returncode'] == 0 else 'failed'))
    positive = None
    if show is not None and disposition == 'exited' and show['result'] == 'success' and show['exec_main_status'] == '0':
        positive = [s for s in samples if s is not None and s['invocation_id'] == show['invocation_id']
                    and s['populated'] == 0 and s['control_group']]
    if positive:
        receipt = closure_receipt(boot_id, nonce, show, positive[-1], outcome['returncode'])
        expected = {'boot_id': boot_id, 'nonce': nonce, 'unit': unit,
                    'invocation_id': show['invocation_id'], 'control_group': positive[-1]['control_group']}
        try:
            closure = observations.verify_contained_closure(expected, receipt, authenticate)
        except observations.Refusal as exc:
            # One refusal type for callers; the verifier's own class is not public API here.
            raise Refusal(str(exc)) from exc
        (stage / 'closure.json').write_bytes((json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode())
    else:
        closure = 'unknown'
    record = {'disposition': disposition, 'closure': closure, 'elapsed_seconds': clock() - start,
              'show': show, 'samples': len(samples),
              'streams': {n: {'bytes': len(outcome[n]), 'sha256': hashlib.sha256(outcome[n]).hexdigest()}
                          for n in ('stdout', 'stderr')}}
    exclusive(stage / 'supervisor.json', (json.dumps(record, sort_keys=True, indent=2) + '\n').encode())
    return disposition, closure
