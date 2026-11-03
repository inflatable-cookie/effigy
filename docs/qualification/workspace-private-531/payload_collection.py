"""Separately GO-gated evidence collection only. No CLI, adoption or admission.

The four pins come only from an authenticated future Queue owner dispatch.
This callable does not arm its own authority. Existing preflight is untouched.
"""
import datetime
import hashlib
import os
from pathlib import Path
import stat

import administrative_recovery as a
from common import PACKET, digest, exclusive, load, source_head, validate_go
import recovery as r

EFFECT = 'workspace-private-531-guestagent-evidence-r1'
ACTION = 'one-guestagent-metadata-hash'
PROGRAMS = ('common.py', 'recovery.py', 'administrative_recovery.py', 'payload_collection.py',
            'payload-public-digests.json', 'payload_collect_once.py')
RETAINED_SHA = '36c6f0bc058c9c3cfc9ba3d5848f7afcccbf5f2314459351935155c1165dcf48'
LIMIT = 16 * 1024 * 1024


def clock():
    import time
    return dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), monotonic=time.monotonic())


def authorize(inputs, ih, index, index_hash, deadline):
    import time
    deadline.remaining()
    r.require(0 < (deadline.utc-datetime.datetime.now(datetime.timezone.utc)).total_seconds() <= 10
              and 0 < deadline.monotonic-time.monotonic() <= 10, 'ten-second collection cap')
    plan = load(a.bound_file(inputs, ih))
    head = source_head(PACKET.parents[2])
    r.require(set(plan) == {'schema', 'source_head', 'programs', 'target_path', 'owner_uid',
                           'retained_stdout', 'budget_seconds'}
              and plan['schema'] == 'effigy.guestagent-evidence.v1' and plan['source_head'] == head
              and type(plan['budget_seconds']) is int and 0 < plan['budget_seconds'] <= 600
              and type(plan['owner_uid']) is int and plan['owner_uid'] == 501, 'exact evidence input required')
    r.require(plan['programs'] == {name: digest(PACKET/name) for name in PROGRAMS}, 'program binding differs')
    go_index = load(a.bound_file(index, index_hash))
    go = load(a.bound_file(go_index['colima']['path'], go_index['colima']['sha256']))
    validate_go(go, ih, dict(backend='colima', budget_seconds=plan['budget_seconds'], engine_identity=r.PROFILE), head, effects=EFFECT)
    start = datetime.datetime.fromisoformat(go['started_at']);end = datetime.datetime.fromisoformat(go['expires_at'])
    r.require(go['operator_authority'] == 'operator (board)' and go['action'] == ACTION
              and type(go['count']) is int and go['count'] == 1
              and go['allowance'] == 'new-guestagent-evidence'
              and go['target_path_sha256'] == hashlib.sha256(plan['target_path'].encode()).hexdigest()
              and start.tzinfo is not None and end.tzinfo is not None
              and start <= datetime.datetime.now(datetime.timezone.utc) < end
              and 0 < (end-start).total_seconds() <= plan['budget_seconds'] and deadline.utc <= end,
              'fresh separately scoped operator evidence GO required')
    retained = plan['retained_stdout']
    r.require(retained['sha256'] == RETAINED_SHA, 'wrong retained receipt')
    raw = a.bound_file(retained['path'], retained['sha256']).read_bytes()
    atoms = raw.decode('ascii').strip().split(maxsplit=9)[9].split()
    r.require(atoms.count('--guestagent') == 1 and atoms[atoms.index('--guestagent')+1] == plan['target_path'],
              'target differs from exact retained payload role')
    path = Path(plan['target_path'])
    public = load(PACKET/'payload-public-digests.json', 'payload_collect_once.py')
    prefix = Path('/opt')/'homebrew'
    canonical = prefix/'Cellar'/'lima'/'2.2.1'/'share'/'lima'/public['payload_name']
    locator = prefix/'share'/'lima'/public['payload_name']
    r.require(str(path) == plan['target_path'] and path in (canonical, locator), 'closed public package locator required')
    return plan, canonical, public


def facts(path):
    value = path.lstat()
    return dict(path=str(path), device=value.st_dev, inode=value.st_ino, uid=value.st_uid,
                mode=stat.S_IMODE(value.st_mode), size=value.st_size, type=stat.S_IFMT(value.st_mode))


def hash_regular(path, owner, deadline, directory):
    """Called only after authority/closed-target gates; own-file tests use this producer."""
    before = facts(path);exclusive(directory/'payload-before.private.json', before)
    r.require(before['type'] == stat.S_IFREG and before['uid'] == owner
              and before['mode'] & 0o022 == 0 and 0 < before['size'] <= LIMIT, 'unsafe or oversized regular payload')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        initial = os.fstat(fd)
        r.require((initial.st_dev, initial.st_ino, initial.st_uid, initial.st_size)
                  == (before['device'], before['inode'], owner, before['size']), 'file changed before read')
        h = hashlib.sha256();total = 0
        while True:
            deadline.remaining()
            data = os.read(fd, min(1024*1024, LIMIT+1-total))
            if not data:break
            if total == 0:r.require(data.startswith(b'\x1f\x8b'), 'not a gzip payload')
            total += len(data);r.require(total <= LIMIT, 'payload grew beyond cap');h.update(data)
        after = os.fstat(fd)
        r.require((after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns)
                  == (initial.st_dev, initial.st_ino, initial.st_size, initial.st_mtime_ns, initial.st_ctime_ns)
                  and total == before['size'], 'payload changed during read')
        observed = facts(path);exclusive(directory/'payload-after.private.json', observed)
        r.require(observed == before, 'visible payload replaced')
        return dict(compressed_file_sha256=h.hexdigest(), bytes=total, fd_closed_on_return=True)
    finally:
        os.close(fd)


class NativeCollector:
    fixture_only = False

    def collect(self, plan, canonical, deadline, directory):
        path = Path(plan['target_path'])
        # Only the privately bound lexical path and its exact derived canonical
        # package location are in scope. No directory listing or discovery.
        r.require(path.resolve(strict=True) == canonical, 'canonical package target differs')
        chain = []
        for entry in dict.fromkeys([path, *path.parents, canonical, *canonical.parents]):
            deadline.remaining();value = facts(entry);chain.append(value)
        exclusive(directory/'path-chain.private.json', chain)
        for item in chain:
            if item['path'] in (str(path), str(canonical)):continue
            r.require(item['uid'] in (0, plan['owner_uid'])
                      and (item['type'] == stat.S_IFLNK or (item['type'] == stat.S_IFDIR and not item['mode'] & 0o022)),
                      'unsafe parent chain')
        r.require(canonical.resolve(strict=True) == canonical, 'canonical alias refused')
        return hash_regular(canonical, plan['owner_uid'], deadline, directory)


def collect(inputs, inputs_hash, index, index_hash, deadline, evidence, *, _collector=None):
    directory = Path(evidence)
    r.require(directory.is_absolute() and directory.is_relative_to(a.SCRATCH)
              and not directory.is_relative_to(Path(r.CONTROL)) and not directory.is_relative_to(Path(r.SHARED))
              and not directory.is_relative_to(a.SCRATCH/'q') and directory.resolve() == directory
              and directory.stat().st_uid == os.geteuid() and stat.S_IMODE(directory.stat().st_mode) == 0o700
              and not list(directory.iterdir()), 'fresh owner-only evidence root required')
    result = dict(status='refused', started=clock(), attempts=0, admitted=False, health=False, closure=False,
                  target_signals=0, watches=0, sigint_claims=0, resumes=0)
    try:
        plan, canonical, public = authorize(inputs, inputs_hash, index, index_hash, deadline)
        io = NativeCollector() if _collector is None else _collector
        exclusive(directory/'collection-intent.json', dict(source_head=plan['source_head'], inputs_sha256=inputs_hash))
        result.update(attempts=1, fixture_only=io.fixture_only)
        observed = io.collect(plan, canonical, deadline, directory)
        deadline.remaining()
        exclusive(directory/'payload-hash.private.json', observed)
        # A public reference comparison is evidence, NEVER provenance/admission
        # of the installed package or authority for another private route.
        result.update(digest_domain='compressed-file-bytes',
            reference_matches=[item['kind'] for item in public['artifacts']
                               if item['compressed_payload_sha256'] == observed['compressed_file_sha256']],
            installed_package_provenance_verified=False)
        r.require(bool(result['reference_matches']), 'no independent reference match')
        result['status'] = 'reference-match-evidence-only'
    except Exception as error:
        result['failure_type'] = type(error).__name__
    finally:
        result['finished'] = clock();exclusive(directory/'collection-result.json', result)
    return result
