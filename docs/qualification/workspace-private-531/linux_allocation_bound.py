"""Kernel-enforced allocation bound for the Linux setup stage tree (source controls only).

Mechanism `ext4-project-quota`: the native root ext4 filesystem carries the `quota` and `project`
features with project quota active, and one reserved project id (PROJECT_ID) is set with inheritance
on the stage tree. The kernel refuses any block allocation for that project past its hard limit.
This bounds RETAINED ALLOCATION of the tree (extents charged to the project). It does not bound
cumulative writes: an overwrite or a delete-and-rewrite allocates nothing new and is not counted.
It does not bound writers outside the tree, such as root-owned system paths or Docker storage.

Observed 2026-11-08 (747 originals): root ext4 with neither feature and no quota tools. That is
absence. The verifier refuses it, and nothing here enables, mounts, or provisions anything.
Production admits no mechanism (linux_setup.SUPPORTED_HARD_LIMITS is empty), so every witness refuses.

A witness carries the five route originals (text, sha256, waited exit). It is verified against exact
request facts: boot id, owned uid, stage root, and the exact entry set. Originals are hashed and
checked for consistency with each other; a caller label is never enforcement.
"""
import hashlib
import os
import re
from pathlib import Path

from linux_supervisor import Refusal
import linux_allocation_probe as probe

MECHANISM = 'ext4-project-quota'
ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'
ROOT_DEVICE = '/dev/sda2'
ROOT_FSTYPE = 'ext4'
REQUIRED_FEATURES = frozenset({'project', 'quota'})
PROJECT_ID = 531                                  # reserved for the stage tree; no other object may carry it
QB_UNIT_BYTES = probe.QB_UNIT_BYTES
# if_dqblk dqb_valid bits, linux/quota.h enum: QIF_BLIMITS_B = 0, QIF_SPACE_B = 1
QIF_BLIMITS = 1 << 0
QIF_SPACE = 1 << 1
ROOT_UID = 0                                      # the supervisor creates its gate FIFO as root
ROUTES = ('mountinfo', 'dumpe2fs', 'by-uuid', 'quota', 'tree')
HELPER = Path(__file__).resolve().parent / 'linux_allocation_probe.py'
HEX64 = re.compile(r'^[0-9a-f]{64}$')


def helper_sha256(path=HELPER):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def route_plan(tree_root, helper=HELPER):
    """Fixed, read-only routes. Nothing runs from here: a reviewed native route consumes this exact list."""
    py = ('/usr/bin/python3', '-I', '-B', str(helper))
    return [
        {'route': 'mountinfo', 'argv': ['/bin/cat', '/proc/self/mountinfo']},
        {'route': 'dumpe2fs', 'argv': ['/usr/sbin/dumpe2fs', '-h', ROOT_DEVICE]},
        {'route': 'by-uuid', 'argv': ['/usr/bin/readlink', '/dev/disk/by-uuid/' + ROOT_UUID]},
        {'route': 'quota', 'argv': list(py + ('quota', ROOT_DEVICE, str(PROJECT_ID)))},
        {'route': 'tree', 'argv': list(py + ('tree', tree_root, str(PROJECT_ID)))},
    ], helper_sha256(helper)


def _originals(witness):
    observations = witness.get('observations')
    if not isinstance(observations, dict) or set(observations) != set(ROUTES):
        raise Refusal('allocation observation set is incomplete')
    texts = {}
    for route in ROUTES:
        item = observations[route]
        if not isinstance(item, dict):
            raise Refusal(route + ' original is not a record')
        text, digest, code = item.get('text'), item.get('sha256'), item.get('exit')
        if not isinstance(text, str) or not HEX64.fullmatch(digest or '') or code != 0:
            raise Refusal(route + ' original is incomplete or its command failed')
        if hashlib.sha256(text.encode('utf-8')).hexdigest() != digest:
            raise Refusal(route + ' original does not match its retained hash')
        if 'status=refused' in text:
            raise Refusal(route + ' helper refused: ' + text.strip().splitlines()[-1])
        texts[route] = text
    return texts


def _root_mount(text):
    found = []
    for line in text.splitlines():
        head, sep, tail = line.partition(' - ')
        fields = head.split()
        if not sep or len(fields) < 6 or len(tail.split()) < 2:
            raise Refusal('mountinfo line is malformed')
        if fields[4] == '/':
            found.append({'dev': fields[2], 'options': set(fields[5].split(',')),
                          'fstype': tail.split()[0], 'source': tail.split()[1]})
    if len(found) != 1:
        raise Refusal('root mount is absent or ambiguous')
    mount = found[0]
    if mount['fstype'] != ROOT_FSTYPE or mount['source'] != ROOT_DEVICE:
        raise Refusal('root mount is not the native ext4 device')
    if 'rw' not in mount['options'] or 'ro' in mount['options']:
        raise Refusal('root mount is not writable')
    return mount


def _check_superblock(text):
    uuid, features = None, None
    for line in text.splitlines():
        key, _, value = line.partition(':')
        if key == 'Filesystem UUID':
            uuid = value.strip()
        elif key == 'Filesystem features':
            features = set(value.split())
    if uuid != ROOT_UUID:
        raise Refusal('superblock UUID differs from the native root')
    if features is None:
        raise Refusal('superblock has no feature line')
    missing = REQUIRED_FEATURES - features
    if missing:
        raise Refusal('root ext4 lacks ' + ','.join(sorted(missing)) + '; enforcement is not provisioned')


def _pairs(text, name):
    values = {}
    for line in text.splitlines():
        for token in line.split():
            key, sep, value = token.partition('=')
            if not sep or key in values:
                raise Refusal(name + ' line is malformed or repeats a key')
            values[key] = value
    return values


def _entries(text):
    entries = {}
    header = {}
    for line in text.splitlines():
        tokens = line.split()
        if tokens and tokens[0] == 'entry':
            fields = _pairs(' '.join(tokens[1:]), 'tree entry')
            path = fields.get('path')
            if path is None or path in entries:
                raise Refusal('tree entry has no path or repeats one')
            entries[path] = fields
        else:
            header.update(_pairs(line, 'tree header'))
    return header, entries


def _int(value, name):
    if value is None or not re.fullmatch(r'-?\d+', value):
        raise Refusal(name + ' is missing or not an integer')
    return int(value)


def _facts(facts):
    boot, uid, root = facts.get('boot_id'), facts.get('uid'), facts.get('root')
    entries = facts.get('entries')
    if not isinstance(boot, str) or not boot or len(boot) > 64 or boot != boot.strip() or not isinstance(uid, int) or \
            isinstance(uid, bool) or not isinstance(root, str) or not root or \
            not isinstance(entries, (set, frozenset, list, tuple)) or '.' not in entries:
        raise Refusal('allocation facts are incomplete: boot, uid, root or exact entries missing')
    return boot, uid, root, frozenset(entries)


def verify(witness, limit_bytes, facts, supported):
    """Return the admitted bound, or raise Refusal. Every check reads an original or an exact fact."""
    mechanism = witness.get('mechanism') if isinstance(witness, dict) else None
    if mechanism != MECHANISM or mechanism not in supported:
        raise Refusal('allocation mechanism is not admitted')
    if not isinstance(limit_bytes, int) or isinstance(limit_bytes, bool) or limit_bytes <= 0:
        raise Refusal('limit is not a positive byte count')
    boot, uid, root, expected = _facts(facts)
    texts = _originals(witness)
    mount = _root_mount(texts['mountinfo'])
    _check_superblock(texts['dumpe2fs'])
    if os.path.basename(texts['by-uuid'].strip()) != ROOT_DEVICE.rsplit('/', 1)[1]:
        raise Refusal('by-uuid link does not resolve to the native root device')

    quota = _pairs(texts['quota'], 'quota')
    if quota.get('boot_id') != boot or quota.get('device') != ROOT_DEVICE:
        raise Refusal('quota original is from another boot or device')
    if quota.get('dev') != mount['dev'] or _int(quota.get('project_id'), 'project_id') != PROJECT_ID:
        raise Refusal('quota original names another device or project')
    if _int(quota.get('getfmt_rc'), 'getfmt_rc') != 0 or _int(quota.get('fmt'), 'fmt') == 0:
        raise Refusal('project quota format is not active on the root device')
    if _int(quota.get('getquota_rc'), 'getquota_rc') != 0 or _int(quota.get('qb_unit_bytes'), 'unit') != QB_UNIT_BYTES:
        raise Refusal('project quota record is unavailable')
    hard = _int(quota.get('bhard_qb'), 'bhard_qb') * QB_UNIT_BYTES
    charged = _int(quota.get('curspace_bytes'), 'curspace_bytes')
    valid = _int(quota.get('valid'), 'valid')
    if not valid & QIF_BLIMITS or not valid & QIF_SPACE:
        raise Refusal('quota record does not mark its block limit and space usage valid')
    if hard <= 0:
        raise Refusal('project has no active hard block limit')
    headroom = hard - charged
    if headroom < limit_bytes:
        raise Refusal('limit exceeds the project headroom left after current charges')

    header, entries = _entries(texts['tree'])
    if header.get('boot_id') != boot or header.get('root') != root or \
            _int(header.get('project_id'), 'project_id') != PROJECT_ID:
        raise Refusal('tree original is from another boot, root or project')
    if frozenset(entries) != expected:
        raise Refusal('owned object set differs from the exact request facts (extra, missing or renamed entry)')
    tree_bytes = 0
    for path, fields in entries.items():
        kind = fields.get('type')
        if kind not in ('dir', 'reg', 'fifo'):
            raise Refusal('entry ' + path + ' is a ' + str(kind) + ', not an owned directory, file or FIFO')
        if fields.get('dev') != mount['dev']:
            raise Refusal('entry ' + path + ' is on another device')
        owner = _int(fields.get('uid'), 'uid')
        if kind == 'fifo':
            # A FIFO allocates no blocks and carries no project charge, so its project id is not read.
            # The supervisor gate is root-owned; a payload FIFO is owned by the payload uid.
            if owner not in (uid, ROOT_UID):
                raise Refusal('FIFO ' + path + ' is owned by another uid')
            if _int(fields.get('blocks512'), 'blocks512') != 0 or _int(fields.get('nlink'), 'nlink') != 1:
                raise Refusal('FIFO ' + path + ' holds blocks or is linked')
            continue
        if owner != uid:
            raise Refusal('entry ' + path + ' is owned by another uid')
        if _int(fields.get('projid'), 'projid') != PROJECT_ID:
            raise Refusal('entry ' + path + ' carries another project id')
        if kind == 'reg' and _int(fields.get('nlink'), 'nlink') != 1:
            raise Refusal('entry ' + path + ' is hardlinked; its charge is not owned by the tree alone')
        tree_bytes += _int(fields.get('blocks512'), 'blocks512') * 512
    if entries['.'].get('type') != 'dir':
        raise Refusal('stage root is not a directory')
    if charged < tree_bytes:
        raise Refusal('kernel charge is below the tree bytes; accounting does not cover the tree')
    return {'mechanism': MECHANISM, 'limit_bytes': limit_bytes, 'project_id': PROJECT_ID,
            'device': ROOT_DEVICE, 'boot_id': boot, 'charged_bytes': charged,
            'remaining_after_bytes': headroom - limit_bytes, 'entries': len(entries)}
