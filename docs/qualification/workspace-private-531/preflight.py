"""Owner-bound read-only preflight. No signals, watch, claim or repair interface.

Only an authenticated Queue dispatch may supply the four artifact/hash arguments.
Matching self-authored files are not authority. Importing performs no observation.
"""
import datetime
import hashlib
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import time

import administrative_recovery as a
from common import (PACKET, digest, exclusive, load, source_head, validate_go,
                    open_qualification_root, verify_qualification_root)
import recovery as r
import rendering as render

EFFECTS = 'workspace-private-531-preflight-r1'
ACTION = 'one-read-only-preflight'
PS = ('/bin/ps', '-ww', '-p', '94093', '-o', 'pid=,ppid=,uid=,lstart=,comm=,args=')
LIMIT = 16384
PROGRAMS = ('common.py', 'recovery.py', 'administrative_recovery.py', 'preflight.py', 'rendering.py')


def now():
    return dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), monotonic=time.monotonic())


def authorize(inputs, inputs_hash, go_index, index_hash, deadline):
    deadline.remaining()
    r.require(0 < (deadline.utc-datetime.datetime.now(datetime.timezone.utc)).total_seconds() <= 10
              and 0 < deadline.monotonic-time.monotonic() <= 10, 'preflight single-call cap is 10 seconds')
    plan = load(a.bound_file(inputs, inputs_hash))
    r.require(plan['source_head'] == source_head(PACKET.parents[2]), 'source differs')
    r.require(set(plan) == {'schema', 'source_head', 'profile', 'budget_seconds', 'control', 'shared',
                           'root_disk', 'data_disk', 'agent', 'executables', 'recorded', 'programs', 'rendering'}
              and plan['schema'] == 'effigy.workspace-private-preflight.v2'
              and set(plan['agent']) == {'pid', 'ppid', 'uid', 'started_utc', 'executable', 'profile'},
              'fixed preflight target required')
    r.admit_recorded(plan, plan['recorded'], deadline)
    render.validate(plan['rendering'])
    r.require(plan['programs'] == {name: digest(PACKET / name) for name in PROGRAMS}, 'programs differ')
    index = load(a.bound_file(go_index, index_hash))
    go = load(a.bound_file(index['colima']['path'], index['colima']['sha256']))
    validate_go(go, inputs_hash, dict(backend='colima', budget_seconds=plan['budget_seconds'],
                                    engine_identity=r.PROFILE), plan['source_head'], effects=EFFECTS)
    start = datetime.datetime.fromisoformat(go['started_at'])
    end = datetime.datetime.fromisoformat(go['expires_at'])
    r.require(go['operator_authority'] == 'operator (board)' and go['action'] == ACTION
              and type(go['count']) is int and go['count'] == 1 and go['target'] == plan['agent']
              and go['allowance'] == 'new-owner-bound-preflight'
              and go['rendering_sha256'] == render.binding_hash(plan['rendering'])
              and start.tzinfo is not None and end.tzinfo is not None
              and start <= datetime.datetime.now(datetime.timezone.utc) < end
              and 0 < (end-start).total_seconds() <= plan['budget_seconds']
              and deadline.utc <= end, 'fresh explicit preflight authority required')
    return plan, go


def raw_file(directory, name, value):
    fd = os.open(directory / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(value)
        out.flush()
        os.fsync(out.fileno())
    return dict(path=name, sha256=hashlib.sha256(value).hexdigest(), bytes=len(value), private=True)


def capture(directory, deadline, *, _fixture_argv=None):
    """One fixed ps client. Test adapter accepts only newly owned fixture code.

    Bounded pipe reads retain at most LIMIT+1 bytes per stream. Timeout/overflow
    settles ONLY this Popen child by its recorded PID, never the inspected PID.
    No child, pipe or completion callback is left outstanding.
    """
    deadline.remaining()
    argv = PS if _fixture_argv is None else _fixture_argv
    started = now()
    child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             env={'PATH': '/usr/bin:/bin', 'TZ': 'UTC', 'LC_ALL': 'C'})
    buffers = {'stdout': bytearray(), 'stderr': bytearray()}
    failure = None
    cleanup_signal = False
    end = min(deadline.monotonic, time.monotonic() + 2)
    try:
        exclusive(directory / 'client-started.json', dict(argv=argv, pid=child.pid, started=started,
                                                        fixture_only=_fixture_argv is not None))
        with selectors.DefaultSelector() as queue:
            for name in buffers:
                queue.register(getattr(child, name), selectors.EVENT_READ, name)
            while queue.get_map():
                left = min(deadline.remaining(), end - time.monotonic())
                r.require(left > 0, 'client timeout')
                for key, _ in queue.select(left):
                    chunk = os.read(key.fileobj.fileno(), min(4096, LIMIT + 1 - len(buffers[key.data])))
                    if not chunk:
                        queue.unregister(key.fileobj)
                    else:
                        buffers[key.data].extend(chunk)
                        r.require(len(buffers[key.data]) <= LIMIT, 'client output truncated')
            child.wait(timeout=max(0.001, min(deadline.remaining(), end-time.monotonic())))
    except Exception as error:
        failure = type(error).__name__
    finally:
        if child.poll() is None:
            cleanup_signal = True
            child.kill()  # Recorded own observation client only; no target signaling.
        child.wait()
        child.stdout.close()
        child.stderr.close()
        # Persist original bounded bytes BEFORE decoding, identity tests or assertions.
        streams = {name: raw_file(directory, name + '.private', bytes(data)) for name, data in buffers.items()}
        result = dict(argv=argv, pid=child.pid, started=started, finished=now(), exit=child.returncode,
                      waited=True, client_cleanup_signal=cleanup_signal, failure=failure,
                      streams=streams, truncated=any(len(x) > LIMIT for x in buffers.values()))
        exclusive(directory / 'client-result.json', result)
    r.require(failure is None and child.returncode == 0 and not buffers['stderr'], 'observer failed')
    return bytes(buffers['stdout'])


class NativeObserver:
    fixture_only = False

    def metadata(self, plan, deadline, directory):
        # Fixed paths from the fully authenticated, recorded-admitted request.
        for name in ('control', 'shared'):
            deadline.remaining()
            pin = a.binding(plan[name])
            info = Path(pin['path']).lstat()
            exclusive(directory / (name + '-metadata.json'), dict(path=pin['path'], device=info.st_dev,
                inode=info.st_ino, uid=info.st_uid, mode=stat.S_IMODE(info.st_mode)))
            _, fd = open_qualification_root(pin)
            try:
                verify_qualification_root(pin, fd)
                if name == 'shared':
                    r.require(not os.listdir(fd), 'shared root changed')
            finally:
                os.close(fd)
        owner = Path(r.CONTROL) / 'owner.json'
        info = owner.lstat()
        owner_hash = digest(owner)
        exclusive(directory / 'owner-metadata.json', dict(sha256=owner_hash, uid=info.st_uid,
                  mode=stat.S_IMODE(info.st_mode)))
        r.require(owner_hash == a.OWNER_SHA and info.st_uid == 501
                  and stat.S_IMODE(info.st_mode) == 0o600, 'owner receipt changed')
        for key in ('root_disk', 'data_disk'):
            deadline.remaining()
            pin = plan[key]
            path = Path(pin['path'])
            found = path.lstat()
            exclusive(directory / (key + '-metadata.json'), dict(path=str(path), device=found.st_dev,
                inode=found.st_ino, uid=found.st_uid, logical_bytes=found.st_size))
            r.require(path.resolve() == path and stat.S_ISREG(found.st_mode)
                      and (found.st_dev, found.st_ino, found.st_uid, found.st_size)
                      == (pin['device'], pin['inode'], pin['uid'], pin['logical_bytes']), 'disk identity changed')
        for name, pin in plan['executables'].items():
            deadline.remaining()
            found_hash = digest(Path(pin['path']))
            exclusive(directory / (name + '-executable.json'), dict(path=pin['path'], sha256=found_hash))
            r.require(found_hash == pin['sha256'], 'executable changed')
        canonical = str(Path('/opt/homebrew/bin/limactl').resolve(strict=True))
        exclusive(directory / 'executable-alias.json', dict(canonical=canonical))
        r.require(canonical == plan['agent']['executable'], 'executable alias changed')
        role_facts = render.observe_paths(plan['rendering'], directory, deadline)
        return dict(root_owner_disk_executable_pins_verified=True, **role_facts)

    def observe(self, directory, deadline):
        return capture(directory, deadline)


def parse(raw, expected, rendering_binding):
    # Unknown rendering stays quarantined; it is never included in public receipts.
    text = raw.decode('ascii')
    r.require(len(text.splitlines()) == 1 and not re.search(r'[^\x20-\x7e\n]', text), 'unsafe output')
    row = text.strip().split(maxsplit=9)
    r.require(len(row) == 10, 'incomplete identity')
    started = datetime.datetime.strptime(' '.join(row[3:8]), '%a %b %d %H:%M:%S %Y').strftime('%Y-%m-%dT%H:%M:%SZ')
    comm, rendering = row[8:]
    category = render.admit_command(comm, rendering.split(), rendering_binding)
    # Alias resolution and executable digest are separate metadata guards.
    # Neither full nor prefix comm rendering identifies an executable by itself.
    identity = dict(pid=int(row[0]), ppid=int(row[1]), uid=int(row[2]), started_utc=started,
                    executable=expected['executable'], profile=r.PROFILE)
    r.require(identity == expected, 'target identity differs')
    return rendering, category


def preflight(inputs, inputs_hash, go_index, index_hash, deadline, evidence, *, _observer=None):
    """One call, no retry. Evidence must be a fresh owner-only scratch directory.

    Four pins MUST originate in the authenticated owner dispatch. _observer is
    only an in-process source-test seam; it cannot be chosen by JSON, CLI or env.
    """
    directory = Path(evidence)
    r.require(directory.is_absolute() and directory.is_relative_to(a.SCRATCH)
              and not directory.is_relative_to(Path(r.CONTROL)) and not directory.is_relative_to(Path(r.SHARED))
              and not directory.is_relative_to(a.SCRATCH / 'q')
              and directory.resolve() == directory and directory.stat().st_uid == os.geteuid()
              and stat.S_IMODE(directory.stat().st_mode) == 0o700 and not list(directory.iterdir()),
              'fresh private evidence directory required')
    result = dict(status='refused', started=now(), target_signals=0, watches=0, sigint_claims=0,
                  resumes=0, attempts=0, closure_verified=False, health_verified=False,
                  inputs_sha256=inputs_hash, go_index_sha256=index_hash)
    try:
        plan, go = authorize(inputs, inputs_hash, go_index, index_hash, deadline)
        result.update(source_head=plan['source_head'], decision_id=go['decision_id'])
        io = NativeObserver() if _observer is None else _observer
        result['fixture_only'] = io.fixture_only
        result['metadata'] = io.metadata(plan, deadline, directory)
        deadline.remaining()
        # Persist intent before the sole client: existing evidence cannot be replayed.
        exclusive(directory / 'observation-intent.json', dict(argv=PS, started=now()))
        result['attempts'] = 1
        raw = io.observe(directory, deadline)
        rendering, category = parse(raw, plan['agent'], plan['rendering'])
        deadline.remaining()
        raw_file(directory, 'admitted-rendering.private', rendering.encode('ascii'))
        result.update(status='identity-observed-closure-unknown', ps_rendering_not_kernel_argv=True, rendering_category=category)
    except Exception as error:
        result['failure_type'] = type(error).__name__  # No raw output/error interpolation.
    finally:
        result['finished'] = now()
        exclusive(directory / 'preflight-result.json', result)
    return result
