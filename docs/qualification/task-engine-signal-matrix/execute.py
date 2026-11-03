#!/usr/bin/env python3
"""One source-immutable bounded execute pass for the signal-matrix packet.

Queue task effigy#582. Admission (GO, hashes, identities) happens before this
module runs. A plan is never execution. CLI `--execute` uses HostCliDriver
only: no environment or config switch can turn a labeled double into a native
receipt. Tests inject LabeledDouble as a Python argument to prove argv,
receipt, refusal and fault ordering without contacting a daemon.

Failure and unknown after the invocation fence are retained evidence, never a
retry token. Duplicate or lost-response resend is refused.
"""
import json
import os
import subprocess
from pathlib import Path

from common import (
    FORBIDDEN_TEXT, HELPER_NAME, SCHEMA, SIGNAL_CELLS, Refused, helper_tail,
    production_helper, receipt_exclusive, refuse_prior_attempt, runtime_argv,
)

CALL_S = 20
ACK_S = 60
GRACE_S = 3
SIGNAL_EXIT_S = 10


class LostResponse(Exception):
    """The transport returned no usable result. Typed unknown, not a retry."""


class HostCliDriver:
    """Production HostCli seam: one bounded backend CLI call. CLI only."""

    label = 'HostCli'

    def call(self, call):
        program = call['program']
        argv = list(call['argv'])
        blob = ' '.join([program] + [str(a) for a in argv]).lower()
        for fragment in FORBIDDEN_TEXT:
            if fragment.lower() in blob:
                raise Refused('plan names a forbidden action: %s' % fragment)
        deadline = int(call.get('deadline_s') or CALL_S)
        try:
            completed = subprocess.run(  # noqa: S603 — HostCli is the real driver
                [program] + argv,
                input=call.get('input') or b'',
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=deadline,
                check=False,
            )
        except subprocess.TimeoutExpired as exc:
            raise LostResponse('HostCli deadline exceeded') from exc
        except OSError as exc:
            raise LostResponse('HostCli unavailable: %s' % exc) from exc
        return {
            'kind': 'verified' if completed.returncode == 0 else 'failed',
            'code': completed.returncode,
            'stdout': completed.stdout,
            'stderr': completed.stderr,
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


def default_transport():
    return HostCliDriver()


def _decode(value):
    if value is None:
        return ''
    if isinstance(value, bytes):
        return value.decode('utf-8', 'replace')
    return str(value)


def _snap_members(result):
    text = _decode(result.get('stdout'))
    members = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 8 and parts[0] == 'p':
            members.append({
                'pid': parts[1],
                'state': parts[2],
                'ppid': parts[3],
                'pgrp': parts[4],
                'sid': parts[5],
                'start': parts[6],
                'nonce': parts[7],
            })
    return members


def _owned(members):
    return [m for m in members if m.get('nonce') == 'y' and m.get('state') not in ('Z', 'X')]


def _foreign(members):
    return [m for m in members if m.get('nonce') != 'y']


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
        calls.append({'op': op, 'program': program, 'argv_head': argv[:8], 'cell': cell})
        return transport.call(call)

    try:
        inspect = drive('inspect', ['inspect', full_id], extra={'image_digest': plan['image_digest']})
    except LostResponse as exc:
        return write_error('unknown', 'presence inspect lost: %s' % exc)
    except Refused as exc:
        return write_error('failed', str(exc))

    if inspect.get('kind') not in ('verified',) or inspect.get('code') not in (0, None):
        return write_error('failed', 'image/runtime is a preparation prerequisite; inspect did not verify')

    try:
        pre = drive('helper-snap', helper_tail(full_id, ['snap'], helper_script),
                    input_bytes=b'\n', extra={'helper': HELPER_NAME})
    except LostResponse as exc:
        return write_error('unknown', 'inventory_pre lost: %s' % exc)
    receipt_exclusive(scratch / receipts['inventory_pre'], {
        'schema': SCHEMA,
        'phase': 'pre',
        'identity': identity,
        'transport': transport.label,
        'members': _snap_members(pre) if pre.get('kind') == 'verified' else [],
        'qualification': False,
    })

    nonce = 'n' * 32
    for cell_id, signal in SIGNAL_CELLS:
        cell = {'id': cell_id, 'signal': signal, 'closure': None}
        try:
            workload = transport.call({
                'op': 'workload',
                'program': plan['binary'],
                'argv': list(plan['argv']),
                'input': b'',
                'deadline_s': ACK_S,
                'identity': identity,
                'cell': cell_id,
            })
            calls.append({'op': 'workload', 'program': plan['binary'],
                          'argv_head': list(plan['argv']), 'cell': cell_id})
        except LostResponse as exc:
            return write_error('unknown', '%s workload lost: %s' % (cell_id, exc))
        if workload.get('kind') not in ('verified', 'labeled-double', 'running'):
            return write_error('failed', '%s workload did not admit' % cell_id)

        try:
            snap = drive('helper-snap', helper_tail(full_id, ['snap'], helper_script),
                         input_bytes=(nonce + '\n').encode(), cell=cell_id,
                         extra={'helper': HELPER_NAME, 'phase': 'registered'})
        except LostResponse as exc:
            return write_error('unknown', '%s registration snap lost: %s' % (cell_id, exc))
        owned = _owned(_snap_members(snap))
        if not owned:
            return write_error('unknown', '%s registration omitted owned descendants' % cell_id)

        try:
            transport.call({
                'op': 'signal-owner',
                'program': None,
                'argv': [signal],
                'input': b'',
                'deadline_s': GRACE_S,
                'identity': identity,
                'cell': cell_id,
            })
            calls.append({'op': 'signal-owner', 'program': None,
                          'argv_head': [signal], 'cell': cell_id})
        except LostResponse as exc:
            return write_error('unknown', '%s owner signal lost: %s' % (cell_id, exc))

        # Closure is the helper identity recheck, never owner exit.
        try:
            after = drive('helper-snap', helper_tail(full_id, ['snap'], helper_script),
                          input_bytes=(nonce + '\n').encode(), cell=cell_id,
                          extra={'helper': HELPER_NAME, 'phase': 'after-signal'})
        except LostResponse as exc:
            return write_error('unknown', '%s closure snap lost: %s' % (cell_id, exc))
        leftover = _owned(_snap_members(after))
        if leftover:
            cell['closure'] = 'missing'
            outcome['cells'].append(cell)
            return write_error('unknown', '%s owned descendants still present; parent exit is not closure' % cell_id)
        cell['closure'] = 'helper-empty-owned'
        try:
            status = transport.call({
                'op': 'status',
                'program': plan['binary'],
                'argv': [plan['binary'], 'tasks', 'status', 'hold', '--json'],
                'input': b'',
                'deadline_s': CALL_S,
                'identity': identity,
                'cell': cell_id,
            })
            calls.append({'op': 'status', 'program': plan['binary'],
                          'argv_head': ['tasks', 'status', 'hold'], 'cell': cell_id})
        except LostResponse as exc:
            return write_error('unknown', '%s gate status lost: %s' % (cell_id, exc))
        gate = _decode(status.get('stdout'))
        if 'not-recorded' not in gate and status.get('gate') != 'not-recorded':
            return write_error('unknown', '%s foreground gate was not released' % cell_id)
        cell['gate'] = 'not-recorded'
        outcome['cells'].append(cell)

    try:
        foreign_snap = drive('helper-snap', helper_tail(full_id, ['snap'], helper_script),
                             input_bytes=(nonce + '\n').encode(), cell='container-foreign-unknown',
                             extra={'helper': HELPER_NAME, 'phase': 'foreign'})
    except LostResponse as exc:
        return write_error('unknown', 'foreign snap lost: %s' % exc)
    foreign = _foreign(_snap_members(foreign_snap))
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
        post = drive('helper-snap', helper_tail(full_id, ['snap'], helper_script),
                     input_bytes=(nonce + '\n').encode(), extra={'helper': HELPER_NAME, 'phase': 'post'})
    except LostResponse as exc:
        return write_error('unknown', 'inventory_post lost: %s' % exc)
    leftover_owned = _owned(_snap_members(post))
    for member in leftover_owned:
        try:
            killed = drive(
                'helper-kill',
                helper_tail(
                    full_id,
                    ['kill', 'boot', 'ns', 'TERM', 'member', member['pid'], member['start']],
                    helper_script,
                ),
                input_bytes=(nonce + '\n').encode(),
                extra={'helper': HELPER_NAME, 'owned_pid': member['pid']},
            )
        except LostResponse as exc:
            cleanup_failed.append({'pid': member['pid'], 'reason': 'lost: %s' % exc})
            continue
        text = _decode(killed.get('stdout')).strip()
        if killed.get('kind') == 'failed' or text not in ('sent', 'gone') or killed.get('partial'):
            cleanup_failed.append({'pid': member['pid'], 'reason': text or 'partial'})
    receipt_exclusive(scratch / receipts['inventory_post'], {
        'schema': SCHEMA,
        'phase': 'post',
        'identity': identity,
        'transport': transport.label,
        'members': _snap_members(post) if post.get('kind') == 'verified' else [],
        'qualification': False,
    })
    receipt_exclusive(scratch / receipts['cleanup'], {
        'schema': SCHEMA,
        'identity': identity,
        'transport': transport.label,
        'owned_only': True,
        'failed': cleanup_failed,
        'retry': False,
        'qualification': False,
    })
    if cleanup_failed:
        return write_error('failed', 'cleanup partial failure of owned members')

    if transport.label == 'labeled-double':
        return record('labeled-double', retry=False)
    if transport.label == 'HostCli':
        return record('native-pass', retry=False)
    return write_error('failed', 'unknown transport label %r' % transport.label)
