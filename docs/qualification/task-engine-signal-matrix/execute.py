#!/usr/bin/env python3
"""One source-immutable bounded execute pass for the signal-matrix packet.

Queue tasks effigy#582 and effigy#585. Admission (GO, hashes, identities)
happens before this module runs. A plan is never execution. CLI `--execute`
uses HostCliDriver only: no environment or config switch can turn a labeled
double into a native receipt. Tests inject LabeledDouble as a Python
argument for in-process control-flow, and call HostCliDriver against owned
local fake executables for argv/stdin/nonce/generation, async owner
signaling, wait/timeout and status schema. Those fixtures are not a
container engine and mint no qualification credit.

Failure and unknown after the invocation fence are retained evidence, never
a retry token. Duplicate or lost-response resend is refused. Cleanup never
blankets-kills or refunds the one-shot fence.
"""
import json
import os
import signal as os_signal
import subprocess
import time
from pathlib import Path

from common import (
    FORBIDDEN_TEXT, HELPER_NAME, SCHEMA, SIGNAL_CELLS, Refused,
    decode_output, foreign_members, generate_nonce, helper_kill_tail,
    helper_tail, observe_snapshot, owned_members, parse_inspect,
    parse_status, production_helper, receipt_exclusive, refuse_prior_attempt,
    runtime_argv,
)

CALL_S = 20
ACK_S = 60
GRACE_S = 3
SIGNAL_EXIT_S = 10
SIGNAL_NUMBERS = {
    'INT': os_signal.SIGINT,
    'TERM': os_signal.SIGTERM,
    'HUP': os_signal.SIGHUP,
    'KILL': os_signal.SIGKILL,
}


class LostResponse(Exception):
    """The transport returned no usable result. Typed unknown, not a retry."""


def process_start_identity(pid):
    """Kernel start ticks on Linux; `ps` lstart on this host. None if unreadable."""
    procfs = Path('/proc/%d/stat' % pid)
    if procfs.is_file():
        try:
            text = procfs.read_text()
            rest = text[text.rfind(')') + 2:].split()
            return rest[19]
        except (OSError, IndexError):
            return None
    try:
        completed = subprocess.run(  # noqa: S603 — identity read only
            ['ps', '-p', str(pid), '-o', 'lstart='],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=2,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if completed.returncode != 0:
        return None
    ident = completed.stdout.decode('utf-8', 'replace').strip()
    return ident or None


class HostCliDriver:
    """Production HostCli seam: bounded CLI call, async owner, exact-pid signal."""

    label = 'HostCli'

    def __init__(self, env=None, workdir=None):
        self.env = env if env is not None else os.environ.copy()
        self.workdir = workdir
        self.owners = {}

    def _require_program(self, call):
        program = call.get('program')
        if not isinstance(program, str) or not program:
            raise Refused('HostCli program is missing')
        return program

    def _scan_forbidden(self, program, argv):
        blob = ' '.join([program] + [str(a) for a in argv]).lower()
        for fragment in FORBIDDEN_TEXT:
            if fragment.lower() in blob:
                raise Refused('plan names a forbidden action: %s' % fragment)

    def call(self, call):
        program = self._require_program(call)
        argv = list(call.get('argv') or [])
        self._scan_forbidden(program, argv)
        deadline = call.get('deadline_s', CALL_S)
        try:
            completed = subprocess.run(  # noqa: S603 — HostCli is the real driver
                [program] + argv,
                input=call.get('input') or b'',
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=deadline,
                check=False,
                env=self.env,
                cwd=self.workdir,
            )
        except subprocess.TimeoutExpired as exc:
            raise LostResponse('HostCli deadline exceeded') from exc
        except OSError as exc:
            raise LostResponse('HostCli unavailable: %s' % exc) from exc
        except TypeError as exc:
            raise Refused('HostCli program is missing') from exc
        return {
            'kind': 'verified' if completed.returncode == 0 else 'failed',
            'code': completed.returncode,
            'stdout': completed.stdout,
            'stderr': completed.stderr,
        }

    def start(self, call):
        program = self._require_program(call)
        argv = list(call.get('argv') or [])
        self._scan_forbidden(program, argv)
        env = dict(self.env)
        extra = call.get('env') or {}
        for key, value in extra.items():
            env[str(key)] = str(value)
        try:
            proc = subprocess.Popen(  # noqa: S603 — async owner, not a wait
                [program] + argv,
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=env,
                cwd=self.workdir,
                start_new_session=True,
            )
        except OSError as exc:
            raise LostResponse('HostCli owner start unavailable: %s' % exc) from exc
        except TypeError as exc:
            raise Refused('HostCli program is missing') from exc
        payload = call.get('input') or b''
        if proc.stdin is not None:
            if payload:
                try:
                    proc.stdin.write(payload)
                except (BrokenPipeError, OSError):
                    pass
            try:
                proc.stdin.close()
            except OSError:
                pass
        start = process_start_identity(proc.pid)
        self.owners[proc.pid] = {'proc': proc, 'start': start, 'program': program}
        return {
            'kind': 'running',
            'code': None,
            'pid': proc.pid,
            'start': start,
            'stdout': b'',
            'stderr': b'',
        }

    def signal(self, call):
        program = self._require_program(call)
        pid = call.get('pid')
        if not isinstance(pid, int) or pid <= 0:
            raise Refused('HostCli owner pid is missing')
        recorded = self.owners.get(pid)
        if recorded is None:
            raise Refused('HostCli owner is not a live recorded owner')
        expected_start = call.get('start')
        if not expected_start or recorded.get('start') != expected_start:
            raise Refused('HostCli owner start identity does not match')
        live = process_start_identity(pid)
        if live != expected_start:
            raise Refused('HostCli owner start identity unavailable or moved')
        argv = list(call.get('argv') or [])
        sig_name = argv[0] if argv else None
        sig = SIGNAL_NUMBERS.get(sig_name)
        if sig is None:
            raise Refused('HostCli signal is not INT/TERM/HUP/KILL')
        self._scan_forbidden(program, argv)
        try:
            os.kill(pid, sig)
        except OSError as exc:
            raise LostResponse('HostCli owner signal failed: %s' % exc) from exc
        return {
            'kind': 'verified',
            'code': 0,
            'pid': pid,
            'start': recorded.get('start'),
            'stdout': b'',
            'stderr': b'',
        }

    def wait(self, call):
        pid = call.get('pid')
        if not isinstance(pid, int) or pid <= 0:
            raise Refused('HostCli owner pid is missing')
        recorded = self.owners.get(pid)
        if recorded is None or recorded.get('proc') is None:
            raise Refused('HostCli owner is not a live recorded owner')
        deadline = call.get('deadline_s', SIGNAL_EXIT_S)
        proc = recorded['proc']
        try:
            code = proc.wait(timeout=deadline)
        except subprocess.TimeoutExpired as exc:
            raise LostResponse('HostCli owner wait exceeded') from exc
        return {
            'kind': 'verified',
            'code': code,
            'pid': pid,
            'stdout': b'',
            'stderr': b'',
        }


class LabeledDouble:
    """In-process transport. Results are never native qualification."""

    label = 'labeled-double'

    def __init__(self, handler):
        self.handler = handler
        self.calls = []

    def call(self, call):
        self.calls.append(call)
        result = self.handler(call)
        if result is None:
            raise LostResponse('labeled double returned no result')
        if result.get('lost'):
            raise LostResponse(result.get('reason') or 'labeled double lost the response')
        return result

    def start(self, call):
        payload = dict(call)
        payload['async'] = True
        return self.call(payload)

    def signal(self, call):
        return self.call(call)

    def wait(self, call):
        payload = dict(call)
        payload['op'] = 'wait-owner'
        return self.call(payload)


def default_transport():
    return HostCliDriver()


def _inventory_payload(phase, identity, transport, observation):
    payload = {
        'schema': SCHEMA,
        'phase': phase,
        'identity': identity,
        'transport': transport,
        'qualification': False,
        'observe': observation['observe'],
        'code': observation.get('code'),
        'stdout': observation.get('stdout') or '',
    }
    if observation['observe'] == 'verified':
        payload['members'] = list(observation.get('members') or [])
        payload['generation'] = {
            'boot': observation['boot'],
            'ns': observation['ns'],
        }
    else:
        payload['reason'] = observation.get('reason') or 'failed-observe'
    return payload


def execute_pass(manifest, go, plan, scratch, transport=None, allow_native=False):
    """Drive one bounded pass. Returns a typed result; never a retry token."""
    scratch = Path(scratch)
    refuse_prior_attempt(scratch, manifest)
    if transport is None:
        if not allow_native:
            raise Refused('native HostCli driver requires CLI --execute after admission')
        transport = default_transport()
    if getattr(transport, 'label', None) == 'labeled-double' and allow_native:
        raise Refused('CLI --execute cannot take a labeled double')

    helper_script = production_helper()
    kind = plan['engine_kind']
    context = plan['context']
    full_id = plan['full_id']
    identity = plan['identity']
    receipts = manifest.get('receipts') or {}
    invocation_path = scratch / receipts['invocation']
    error_path = scratch / receipts['error']

    invocation = {
        'schema': SCHEMA,
        'execute': True,
        'qualification': False,
        'transport': transport.label,
        'identity': identity,
        'engine_kind': kind,
        'source': plan['source'],
        'binary_sha256': plan['binary_sha256'],
        'image_digest': plan['image_digest'],
        'window': go['window'],
        'deadlines_s': dict(plan['deadlines_s']),
        'cells': [c['id'] for c in plan['cells']],
    }
    receipt_exclusive(invocation_path, invocation)

    calls = []
    outcome = {
        'schema': SCHEMA,
        'execute': True,
        'qualification': False,
        'kind': None,
        'transport': transport.label,
        'identity': identity,
        'calls': calls,
        'cells': [],
        'credit': None,
    }

    def record(kind_name, **extra):
        outcome['kind'] = kind_name
        outcome.update(extra)
        if kind_name == 'labeled-double':
            outcome['qualification'] = False
            outcome['credit'] = 'source-control-flow'
        elif kind_name == 'native-pass':
            outcome['qualification'] = False
            outcome['credit'] = 'native-execution-not-accepted'
        else:
            outcome['qualification'] = False
            if kind_name == 'unknown':
                outcome['credit'] = 'unknown-not-a-retry'
            elif kind_name == 'failed':
                outcome['credit'] = 'failed-not-a-retry'
        return outcome

    def write_error(kind_name, reason):
        payload = {
            'schema': SCHEMA,
            'kind': kind_name,
            'qualification': False,
            'reason': reason,
            'transport': transport.label,
            'identity': identity,
            'retry': False,
        }
        if not os.path.lexists(error_path) and not error_path.is_symlink():
            receipt_exclusive(error_path, payload)
        return record(kind_name, reason=reason, retry=False)

    def drive(op, tail, deadline_s=CALL_S, input_bytes=b'', cell=None, extra=None):
        program, argv = runtime_argv(kind, context, tail)
        call = {
            'op': op,
            'program': program,
            'argv': argv,
            'input': input_bytes,
            'deadline_s': deadline_s,
            'identity': identity,
            'cell': cell,
        }
        if extra:
            call.update(extra)
        calls.append({
            'op': op,
            'program': program,
            'argv_head': argv[:8],
            'cell': cell,
            # A helper's first input line is the custody nonce, never receipt data.
            'stdin': '[redacted]' if input_bytes else '',
        })
        return transport.call(call)

    try:
        inspect = drive('inspect', ['inspect', full_id], extra={'image_digest': plan['image_digest']})
    except LostResponse as exc:
        return write_error('unknown', 'presence inspect lost: %s' % exc)
    except Refused as exc:
        return write_error('failed', str(exc))

    inspected = parse_inspect(inspect.get('stdout'), full_id, plan['image_digest'])
    if inspect.get('kind') != 'verified' or inspect.get('code') not in (0, None) or inspected is None:
        return write_error(
            'failed',
            'image/runtime is a preparation prerequisite; inspect did not verify',
        )

    try:
        pre = drive(
            'helper-snap',
            helper_tail(full_id, ['snap'], helper_script),
            input_bytes=b'\n',
            extra={'helper': HELPER_NAME, 'phase': 'pre'},
        )
    except LostResponse as exc:
        return write_error('unknown', 'inventory_pre lost: %s' % exc)
    pre_obs = observe_snapshot(pre, nonce_supplied=False)
    receipt_exclusive(scratch / receipts['inventory_pre'], _inventory_payload(
        'pre', identity, transport.label, pre_obs,
    ))
    if pre_obs['observe'] != 'verified':
        return write_error('unknown', 'inventory_pre failed-observe: %s' % pre_obs.get('reason'))

    last_nonce = None
    last_generation = None
    for cell_id, sig_name in SIGNAL_CELLS:
        nonce = generate_nonce()
        last_nonce = nonce
        cell = {
            'id': cell_id,
            'signal': sig_name,
            'closure': None,
        }
        try:
            selector = plan['fixture_task']['selector']
            workload = transport.start({
                'op': 'workload',
                'program': plan['binary'],
                'argv': [selector],
                'input': b'',
                'env': {'EFFIGY_CUSTODY_NONCE': nonce},
                'deadline_s': ACK_S,
                'identity': identity,
                'cell': cell_id,
                'nonce': nonce,
            })
            calls.append({
                'op': 'workload',
                'program': plan['binary'],
                'argv_head': [selector],
                'cell': cell_id,
                'async': True,
                'pid': workload.get('pid'),
            })
        except LostResponse as exc:
            return write_error('unknown', '%s workload lost: %s' % (cell_id, exc))
        except Refused as exc:
            return write_error('failed', str(exc))
        if workload.get('kind') not in ('verified', 'labeled-double', 'running'):
            return write_error('failed', '%s workload did not admit' % cell_id)
        owner_pid = workload.get('pid')
        owner_start = workload.get('start')
        if not isinstance(owner_pid, int) or owner_pid <= 0:
            return write_error('failed', '%s workload did not record a live owner' % cell_id)
        cell['owner'] = {'pid': owner_pid, 'start': owner_start}

        try:
            snap = drive(
                'helper-snap',
                helper_tail(full_id, ['snap'], helper_script),
                input_bytes=(nonce + '\n').encode(),
                cell=cell_id,
                extra={'helper': HELPER_NAME, 'phase': 'registered', 'nonce': nonce},
            )
        except LostResponse as exc:
            return write_error('unknown', '%s registration snap lost: %s' % (cell_id, exc))
        snap_obs = observe_snapshot(snap, nonce_supplied=True)
        if snap_obs['observe'] != 'verified':
            return write_error(
                'unknown',
                '%s registration failed-observe: %s' % (cell_id, snap_obs.get('reason')),
            )
        owned = owned_members(snap_obs['members'])
        if not owned:
            return write_error('unknown', '%s registration omitted owned descendants' % cell_id)
        generation = {'boot': snap_obs['boot'], 'ns': snap_obs['ns']}
        last_generation = generation
        cell['generation'] = generation

        settlement_deadline = time.monotonic() + SIGNAL_EXIT_S
        try:
            transport.signal({
                'op': 'signal-owner',
                'program': plan['binary'],
                'argv': [sig_name],
                'input': b'',
                'deadline_s': GRACE_S,
                'identity': identity,
                'cell': cell_id,
                'pid': owner_pid,
                'start': owner_start,
            })
            calls.append({
                'op': 'signal-owner',
                'program': plan['binary'],
                'argv_head': [sig_name],
                'cell': cell_id,
                'pid': owner_pid,
                'start': owner_start,
            })
        except LostResponse as exc:
            return write_error('unknown', '%s owner signal lost: %s' % (cell_id, exc))
        except Refused as exc:
            return write_error('failed', str(exc))

        try:
            remaining = settlement_deadline - time.monotonic()
            if remaining <= 0:
                raise LostResponse('settlement deadline exceeded')
            transport.wait({
                'op': 'wait-owner',
                'pid': owner_pid,
                'start': owner_start,
                'deadline_s': remaining,
                'identity': identity,
                'cell': cell_id,
            })
            calls.append({
                'op': 'wait-owner',
                'program': plan['binary'],
                'argv_head': [],
                'cell': cell_id,
                'pid': owner_pid,
            })
        except LostResponse as exc:
            return write_error('unknown', '%s owner wait lost: %s' % (cell_id, exc))
        except Refused as exc:
            return write_error('failed', str(exc))

        while True:
            remaining = settlement_deadline - time.monotonic()
            if remaining <= 0:
                return write_error('unknown', '%s settlement deadline exceeded' % cell_id)
            try:
                after = drive(
                    'helper-snap',
                    helper_tail(full_id, ['snap'], helper_script),
                    deadline_s=min(CALL_S, remaining),
                    input_bytes=(nonce + '\n').encode(),
                    cell=cell_id,
                    extra={'helper': HELPER_NAME, 'phase': 'after-signal', 'nonce': nonce},
                )
            except LostResponse as exc:
                return write_error('unknown', '%s closure snap lost: %s' % (cell_id, exc))
            if time.monotonic() >= settlement_deadline:
                return write_error('unknown', '%s settlement deadline exceeded' % cell_id)
            after_obs = observe_snapshot(after, nonce_supplied=True)
            if after_obs['observe'] != 'verified':
                return write_error('unknown', '%s closure failed-observe: %s' % (
                    cell_id, after_obs.get('reason')))
            if after_obs['boot'] != generation['boot'] or after_obs['ns'] != generation['ns']:
                return write_error('unknown', '%s generation moved' % cell_id)
            if not owned_members(after_obs['members']):
                cell['closure'] = 'helper-empty-owned'
                break
            time.sleep(min(0.05, max(0, settlement_deadline - time.monotonic())))

        try:
            status = transport.call({
                'op': 'status',
                'program': plan['binary'],
                'argv': ['tasks', 'status', 'hold', '--json'],
                'input': b'',
                'deadline_s': CALL_S,
                'identity': identity,
                'cell': cell_id,
            })
            calls.append({
                'op': 'status',
                'program': plan['binary'],
                'argv_head': ['tasks', 'status', 'hold', '--json'],
                'cell': cell_id,
            })
        except LostResponse as exc:
            return write_error('unknown', '%s gate status lost: %s' % (cell_id, exc))
        except Refused as exc:
            return write_error('failed', str(exc))
        parsed_status = parse_status(status.get('stdout'))
        if parsed_status is None or parsed_status['state'] != 'not-recorded':
            return write_error('unknown', '%s foreground gate was not released' % cell_id)
        cell['gate'] = parsed_status['state']
        outcome['cells'].append(cell)

    foreign_input = ((last_nonce or '') + '\n').encode()
    try:
        foreign_snap = drive(
            'helper-snap',
            helper_tail(full_id, ['snap'], helper_script),
            input_bytes=foreign_input,
            cell='container-foreign-unknown',
            extra={'helper': HELPER_NAME, 'phase': 'foreign', 'nonce': last_nonce},
        )
    except LostResponse as exc:
        return write_error('unknown', 'foreign snap lost: %s' % exc)
    foreign_obs = observe_snapshot(foreign_snap, nonce_supplied=bool(last_nonce))
    if foreign_obs['observe'] != 'verified':
        return write_error('unknown', 'foreign failed-observe: %s' % foreign_obs.get('reason'))
    foreign = foreign_members(foreign_obs['members'])
    kill_calls_before = sum(1 for c in calls if c.get('op') == 'helper-kill')
    # Strict refuse: never issue helper-kill for a foreign/unknown member.
    if foreign:
        outcome['cells'].append({
            'id': 'container-foreign-unknown',
            'signal': None,
            'obligation': 'strict foreign/unknown refusal',
            'refused_pids': [m['pid'] for m in foreign],
        })
    else:
        outcome['cells'].append({
            'id': 'container-foreign-unknown',
            'signal': None,
            'obligation': 'strict foreign/unknown refusal',
            'refused_pids': [],
            'note': 'no foreign members in the labeled observation',
        })
    kill_calls_after = sum(1 for c in calls if c.get('op') == 'helper-kill')
    if kill_calls_after != kill_calls_before:
        return write_error('failed', 'foreign/unknown observation issued helper-kill')

    cleanup_failed = []
    try:
        post = drive(
            'helper-snap',
            helper_tail(full_id, ['snap'], helper_script),
            input_bytes=foreign_input,
            extra={'helper': HELPER_NAME, 'phase': 'post', 'nonce': last_nonce},
        )
    except LostResponse as exc:
        return write_error('unknown', 'inventory_post lost: %s' % exc)
    post_obs = observe_snapshot(post, nonce_supplied=bool(last_nonce))
    if post_obs['observe'] != 'verified':
        receipt_exclusive(scratch / receipts['inventory_post'], _inventory_payload(
            'post', identity, transport.label, post_obs,
        ))
        return write_error('unknown', 'inventory_post failed-observe: %s' % post_obs.get('reason'))
    leftover_owned = owned_members(post_obs['members'])
    generation = last_generation or {'boot': post_obs['boot'], 'ns': post_obs['ns']}
    for member in leftover_owned:
        try:
            killed = drive(
                'helper-kill',
                helper_kill_tail(
                    full_id,
                    post_obs['boot'],
                    post_obs['ns'],
                    'TERM',
                    'member',
                    member['pid'],
                    member['start'],
                    helper_script,
                ),
                input_bytes=foreign_input,
                extra={
                    'helper': HELPER_NAME,
                    'owned_pid': member['pid'],
                    'generation': {'boot': post_obs['boot'], 'ns': post_obs['ns']},
                },
            )
        except LostResponse as exc:
            cleanup_failed.append({'pid': member['pid'], 'reason': 'lost: %s' % exc})
            continue
        except Refused as exc:
            cleanup_failed.append({'pid': member['pid'], 'reason': str(exc)})
            continue
        text = decode_output(killed.get('stdout')).strip()
        if killed.get('kind') == 'failed' or text not in ('sent', 'gone') or killed.get('partial'):
            cleanup_failed.append({'pid': member['pid'], 'reason': text or 'partial'})
    receipt_exclusive(scratch / receipts['inventory_post'], _inventory_payload(
        'post', identity, transport.label, post_obs,
    ))
    receipt_exclusive(scratch / receipts['cleanup'], {
        'schema': SCHEMA,
        'identity': identity,
        'transport': transport.label,
        'owned_only': True,
        'failed': cleanup_failed,
        'retry': False,
        'qualification': False,
        'generation': generation,
    })
    if cleanup_failed:
        return write_error('failed', 'cleanup partial failure of owned members')

    if transport.label == 'labeled-double':
        return record('labeled-double', retry=False)
    if transport.label == 'HostCli':
        return record('native-pass', retry=False)
    return write_error('failed', 'unknown transport label %r' % transport.label)
