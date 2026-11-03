"""Future owner-invoked, GO-gated ONE administrative SIGINT. No CLI/env toggle.

Trusted hash arguments come from the authenticated Queue owner dispatch, just as
launch.py's input/GO-index pins do. Matching self-authored files are not authority.
No such native dispatch or operator residual-risk acceptance currently exists.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import stat
import time

from common import (PACKET, SCRATCH, artifact, digest, exclusive, load,
                    open_qualification_root, source_head, validate_go, verify_qualification_root, run)
import recovery as r

EFFECTS = 'workspace-private-531-admin-sigint-r1'
ACTION = 'one-administrative-sigint'
OWNER_SHA = '82de7f5165bde56f0411252972d670a5e7481fbffcb9deadadec15c00ce516ab'


def bound_file(path, expected_hash):
    path = Path(path)
    r.require(path.is_absolute() and path.is_relative_to(SCRATCH)
              and path.resolve() == path, 'Queue-bound scratch artifact required')
    return artifact({'path': str(path), 'sha256': expected_hash})


def authorize(inputs, inputs_hash, go_index, index_hash, deadline):
    """No target/root/process observation occurs before this complete gate."""
    deadline.remaining()
    value = load(bound_file(inputs, inputs_hash))
    head = source_head(PACKET.parents[2])
    r.require(value['source_head'] == head, 'owner source head differs')
    r.admit_recorded(value, value['recorded'], deadline)
    for name in ('common.py', 'recovery.py', 'administrative_recovery.py'):
        r.require(value['programs'][name] == digest(PACKET / name), 'reviewed program differs')
    index = load(bound_file(go_index, index_hash))
    go = load(bound_file(index['colima']['path'], index['colima']['sha256']))
    validate_go(go, inputs_hash, dict(backend='colima', budget_seconds=value['budget_seconds'],
                                    engine_identity=r.PROFILE), head, effects=EFFECTS)
    r.require(go['operator_authority'] == 'operator (board)' and go['action'] == ACTION
              and go['count'] == 1 and type(go['count']) is int
              and go['target'] == value['agent']
              and go['residual_pid_reuse_risk_accepted'] is True
              and go['allowance'] == 'new-native-recovery', 'specific operator risk/action acceptance required')
    now = datetime.datetime.now(datetime.timezone.utc)
    start = datetime.datetime.fromisoformat(go['started_at'])
    end = datetime.datetime.fromisoformat(go['expires_at'])
    r.require(start.tzinfo is not None and end.tzinfo is not None
              and start <= now < end and 0 < (end-start).total_seconds() <= value['budget_seconds']
              and deadline.utc <= end, 'fresh bounded allowance/deadline required')
    argv = shlex.split(value['agent']['argv_rendering'])
    r.require(argv and Path(argv[0]).name == 'limactl' and 'hostagent' in argv
              and 'colima-' + r.PROFILE in argv, 'exact bound host-agent argv required')
    return value, go


def binding(value):
    return {key: value[key] for key in ('path', 'device', 'inode', 'uid')}


class NativeTransport:
    """Fixed target implementation, constructed only AFTER bound authorization.

    ps command rendering is compared exactly, not represented as byte-delimited
    kernel argv. Snapshot+watch+last recheck retain a numeric-PID race.
    """
    fixture_only = False

    def __init__(self, plan):
        self.plan = plan
        self.fds = {}
        self.attempted = False

    def inspect(self, deadline):
        deadline.remaining()
        for name in ('control', 'shared'):
            pin = binding(self.plan[name])
            if name not in self.fds:
                _, self.fds[name] = open_qualification_root(pin)
            verify_qualification_root(pin, self.fds[name])
        r.require(not os.listdir(self.fds['shared']), 'shared root changed')
        owner = Path(r.CONTROL) / 'owner.json'
        r.require(digest(owner) == OWNER_SHA and owner.stat().st_uid == 501
                  and stat.S_IMODE(owner.stat().st_mode) == 0o600, 'owner receipt changed')
        for key in ('root_disk', 'data_disk'):
            pin = self.plan[key]
            path = Path(pin['path'])
            found = path.lstat()
            r.require(path.resolve() == path and stat.S_ISREG(found.st_mode)
                      and (found.st_dev, found.st_ino, found.st_uid, found.st_size)
                      == (pin['device'], pin['inode'], pin['uid'], pin['logical_bytes']), 'disk identity changed')
        for pin in self.plan['executables'].values():
            artifact(pin)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'TZ': 'UTC', 'LC_ALL': 'C'}
        def ps(fields):
            deadline.remaining()
            return run(['/bin/ps', '-ww', '-p', '94093', '-o', fields], env=env,
                       timeout=min(2, deadline.remaining())).stdout.decode().strip()
        row = ps('pid=,ppid=,uid=,lstart=').split(maxsplit=3)
        r.require(len(row) == 4, 'target unavailable')
        started = datetime.datetime.strptime(row[3], '%a %b %d %H:%M:%S %Y').strftime('%Y-%m-%dT%H:%M:%SZ')
        executable = str(Path(ps('comm=')).resolve(strict=True))
        value = dict(pid=int(row[0]), ppid=int(row[1]), uid=int(row[2]), started_utc=started,
                     executable=executable, profile=r.PROFILE, argv_rendering=ps('command='))
        deadline.remaining()
        return value

    def claim(self, token, value):
        verify_qualification_root(binding(self.plan['control']), self.fds['control'])
        exclusive('admin-sigint-' + token + '.json', value, dir_fd=self.fds['control'])
        os.fsync(self.fds['control'])

    def watch(self, deadline):
        return r.ProcessExitWatch(94093, deadline)

    def interrupt(self):
        r.require(not self.attempted, 'single SIGINT already consumed')
        self.attempted = True  # No automatic retry even when kill returns an error.
        os.kill(94093, signal.SIGINT)

    def close(self):
        for fd in self.fds.values():
            os.close(fd)
        self.fds.clear()


def administrative_sigint(inputs, inputs_hash, go_index, index_hash, deadline, *, transport=None):
    """Authenticated owner pins are mandatory. No caller exit dict/GO string API.

    transport injection is for labeled owned-child tests, never an input/env
    choice of the native launcher. Production always uses NativeTransport.
    """
    def gate():
        return authorize(inputs, inputs_hash, go_index, index_hash, deadline)
    plan, go = gate()  # Before constructing even the observer.
    io = NativeTransport(plan) if transport is None else transport
    receipt = dict(status='retained-unknown', signal_attempts=0, resumes=0, race_free=False,
                   residual_pid_reuse_risk=True, fixture_only=io.fixture_only,
                   source_head=plan['source_head'], inputs_sha256=inputs_hash, decision_id=go['decision_id'],
                   go_index_sha256=index_hash, residual_risk_explicitly_accepted=True)
    watch = None
    try:
        first = io.inspect(deadline)
        r.require(first == plan['agent'], 'initial target identity changed')
        receipt['initial_identity'] = first
        gate()
        token = hashlib.sha256((go['decision_id'] + ACTION).encode()).hexdigest()
        io.claim(token, dict(action=ACTION, count=1, source_head=plan['source_head'],
                             decision_id=go['decision_id'], race_free=False))
        # Durable one-shot consumed BEFORE registration or uncertain delivery.
        receipt['one_shot_consumed'] = True
        watch = io.watch(deadline)
        gate()
        last = io.inspect(deadline)
        r.require(last == first == plan['agent'], 'final target identity changed')
        receipt['final_identity'] = last
        receipt['watch_registered_before_signal'] = True
        deadline.remaining()
        receipt['signal_attempts'] = 1
        receipt['signal_intent_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        receipt['signal_intent_monotonic'] = time.monotonic()
        io.interrupt()  # Exactly one numeric PID syscall, with acknowledged race.
        receipt['signal_syscall_returned'] = True
        receipt['exit_observation'] = watch.wait()
        receipt['status'] = 'exit-observed-closure-unverified'
        # This never means handler success, VZ closure, healthy disks or resume.
    except Exception as error:
        receipt['failure_type'] = type(error).__name__
    finally:
        if watch is not None:
            watch.close()
        io.close()
        receipt['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        receipt['finished_monotonic'] = time.monotonic()
    return receipt


def resume_intent(plan, receipt, closure_path, closure_hash, deadline, ledger):
    """Only an independent Queue-pinned closure artifact can yield an intent.

    This is NOT a restart executor. A later owner-approved restart actuator must
    have its own exact scope; the SIGINT GO does not authorize a resume.
    """
    deadline.remaining()
    r.require(receipt['signal_attempts'] == 1 and receipt.get('signal_syscall_returned') is True,
              'no established administrative attempt')
    closure = load(bound_file(closure_path, closure_hash))
    r.require(closure['kind'] == 'independent-owned-backend-closure'
              and closure['source_head'] == receipt['source_head']
              and closure['inputs_sha256'] == receipt['inputs_sha256']
              and closure['decision_id'] == receipt['decision_id']
              and closure['fixture_only'] == receipt['fixture_only'], 'closure binding differs')
    r.admit_recorded(plan, closure['admission'], deadline, agent_alive=False)
    r.require(closure['evidence'] == dict(agent_terminated=True, driver_terminated=True,
        handler_shutdown_verified=True, profile_stopped=True, holders_settled=True,
        disk_identity_unchanged=True, independent_of_exit_event=True), 'independent closure unknown')
    ledger.claim('resume-intent', {'execute': False})  # Durable one-shot intent only.
    return dict(argv=r.RESUME, execute=False, race_free=False, fixture_only=receipt['fixture_only'])
