"""Kernel-enforced allocation bound for the Linux setup stage tree (source controls only).

Mechanism candidate `ext4-project-quota`: a native ext4 project quota may bound charged allocation
when limit enforcement and writer custody are independently established. Root may bypass a hard
limit and an inode owner may change project attributes; the current route proves neither boundary.
The intended metric is RETAINED ALLOCATION of the tree (extents charged to the project), not
cumulative writes: an overwrite or a delete-and-rewrite can reuse allocation and is not counted.
It does not bound writers outside the tree, such as root-owned system paths or Docker storage.

Observed 2026-11-08 (747 originals): root ext4 with neither feature and no quota tools. That is
absence. The verifier refuses it, and nothing here enables, mounts, or provisions anything.
Production admits no mechanism (linux_setup.SUPPORTED_HARD_LIMITS is empty), so every witness refuses.

A witness carries the five route originals (text, sha256, waited exit). It is verified against exact
request facts: boot id, owned uid, stage root, and the exact entry set. Originals are hashed and
checked for consistency with each other; a caller label is never enforcement.
"""
import ctypes
import hashlib
import os
import re
import struct
import uuid
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
# Candidate row reservations. E19 is deliberately omitted: its aggregate
# 1 MiB ceiling cannot be enforced by independently resetting per-file limits.
ROW_RESERVATIONS = {'E8': 64 << 20, 'E9a': 40 << 20, 'E9b': 500 << 20, 'E9c': 40 << 20}
PROJECT_CEILING = 1 << 30
# if_dqblk dqb_valid bits, linux/quota.h enum: QIF_BLIMITS_B = 0, QIF_SPACE_B = 1
QIF_BLIMITS = 1 << 0
QIF_SPACE = 1 << 1
ROOT_UID = 0                                      # the supervisor creates its gate FIFO as root
ROUTES = ('mountinfo', 'dumpe2fs', 'by-uuid', 'quota', 'tree')
HELPER = Path(__file__).resolve().parent / 'linux_allocation_probe.py'
WRITER_HELPER = Path(__file__).resolve().parent / 'linux_transfer.py'
HEX64 = re.compile(r'^[0-9a-f]{64}$')
BOOT_UUID = re.compile(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')


def inspect_candidate_superblock(raw):
    """Check a prospective offline pre-state; raw bytes alone grant no authority.

    The caller must separately authenticate a complete original and verify its
    checksum. The fixed layout is the v1.47.2 ext4 superblock, not a device read.
    """
    if not isinstance(raw, bytes) or len(raw) != 1024:
        raise Refusal('superblock original must be exactly 1024 bytes')
    magic, state, inode_size = (struct.unpack_from('<H', raw, offset)[0] for offset in (0x38, 0x3a, 0x58))
    incompat, ro_compat = (struct.unpack_from('<I', raw, offset)[0] for offset in (0x60, 0x64))
    project_inode = struct.unpack_from('<I', raw, 0x26c)[0]
    if magic != 0xef53 or raw[0x68:0x78] != uuid.UUID(ROOT_UUID).bytes:
        raise Refusal('superblock magic or UUID differs from the native root')
    if inode_size != 256 or state != 1 or incompat & 4:
        raise Refusal('superblock is not the expected clean offline pre-state')
    if ro_compat & (0x100 | 0x2000) or project_inode != 0:
        raise Refusal('project or quota features are already present')
    if raw[0x200:0x240] != bytes(64):
        raise Refusal('stored mount-option bytes are not entirely empty')
    return {'uuid': ROOT_UUID, 'inode_size': inode_size, 'stored_mount_options_empty': True}


def offline_version_plan():
    """Fixed future read-only observations; constructing them runs nothing."""
    return {'tune2fs': ['/sbin/tune2fs', '-l', ROOT_DEVICE],
            'e2fsck': ['/sbin/e2fsck', '-V']}


def _offline_streams(item, label, keys):
    if not isinstance(item, dict) or set(item) != keys | {'stdout', 'stderr', 'stdout_sha256',
                                                          'stderr_sha256', 'exit'} or \
            type(item['exit']) is not int or item['exit'] != 0:
        raise Refusal(label + ' original or waited exit is missing')
    for stream in ('stdout', 'stderr'):
        value, digest = item[stream], item[stream + '_sha256']
        if not isinstance(value, str) or not isinstance(digest, str) or not HEX64.fullmatch(digest):
            raise Refusal(label + ' stream or hash is malformed')
        try:
            raw = value.encode('utf-8')
        except UnicodeEncodeError as exc:
            raise Refusal(label + ' stream is not UTF-8') from exc
        if len(raw) > 128 * 1024 or hashlib.sha256(raw).hexdigest() != digest:
            raise Refusal(label + ' stream is oversized or differs from its hash')


def inspect_offline_versions(originals):
    """Check both retained streams and positive waited exits, without granting authority.

    The caller must independently bind the executables, device, boot and
    observation route. A matching banner is not a capability or enforcement proof.
    """
    if not isinstance(originals, dict) or set(originals) != {'tune2fs', 'e2fsck'}:
        raise Refusal('offline tool originals are incomplete')
    for tool, banner_stream in (('tune2fs', 'stdout'), ('e2fsck', 'stderr')):
        item = originals[tool]
        _offline_streams(item, tool, set())
        lines = item[banner_stream].splitlines()
        if not lines or not re.fullmatch(re.escape(tool) + r' 1\.47\.2 \([^()\r\n]+\)', lines[0]):
            raise Refusal(tool + ' version is missing or unsupported')
    return {'e2fsprogs_version': '1.47.2'}


def offline_operation_plan():
    """Fixed sequence for future review; these commands are never run here."""
    return [('precheck', ['/sbin/e2fsck', '-f', '-n', ROOT_DEVICE]),
            ('features', ['/sbin/tune2fs', '-O', 'project', '-Q', 'prjquota', ROOT_DEVICE]),
            ('mount-options', ['/sbin/tune2fs', '-E', 'mount_opts=prjquota', ROOT_DEVICE]),
            ('postcheck', ['/sbin/e2fsck', '-f', '-n', ROOT_DEVICE])]


def inspect_offline_progress(boot_id, attempted, originals):
    """Check an exact prospective sequence; supplied records cannot authorize it.

    An independent durable, one-use record must establish every attempt, the
    actual boot, unmounted device and original streams. A missing completion is
    unknown and must not be treated as an unstarted step or retried.
    """
    if not isinstance(boot_id, str) or not BOOT_UUID.fullmatch(boot_id):
        raise Refusal('offline boot identity is missing')
    plan = offline_operation_plan()
    if not isinstance(attempted, list) or not isinstance(originals, list) or \
            len(attempted) != len(originals) or len(attempted) > len(plan):
        raise Refusal('offline attempt is incomplete or interrupted')
    for index, (name, argv) in enumerate(plan[:len(attempted)]):
        if attempted[index] != name:
            raise Refusal('offline attempt set or order differs')
        item = originals[index]
        _offline_streams(item, name, {'name', 'argv', 'boot_id'})
        if item['name'] != name or item['argv'] != argv or item['boot_id'] != boot_id:
            raise Refusal('offline operation, argv or boot identity differs')
    return {'next': plan[len(attempted)][0] if len(attempted) < len(plan) else None}


def validate_candidate_request(request):
    """Validate a prospective request's shape, without granting authority."""
    if not isinstance(request, dict) or set(request) != {'row', 'nonce', 'boot_id'}:
        raise Refusal('quota request must contain only row, nonce and boot identity')
    row, nonce, boot = request['row'], request['nonce'], request['boot_id']
    if not isinstance(row, str) or row not in ROW_RESERVATIONS:
        raise Refusal('row has no fixed project reservation')
    if not isinstance(nonce, str) or not HEX64.fullmatch(nonce) or \
            not isinstance(boot, str) or not BOOT_UUID.fullmatch(boot):
        raise Refusal('quota request nonce or boot identity is invalid')
    return {'row': row, 'nonce': nonce, 'boot_id': boot}


def prospective_row_limit(row, charged_bytes):
    """Build fixed quota fields only; this does not authorize or invoke a setter.

    The caller must obtain a genuine, current charge under exclusive custody.
    No production route is armed to consume this prospective record.
    """
    if not isinstance(row, str) or row not in ROW_RESERVATIONS:
        raise Refusal('row has no fixed project reservation')
    if not isinstance(charged_bytes, int) or isinstance(charged_bytes, bool) or charged_bytes < 0:
        raise Refusal('project charge is invalid')
    reservation = ROW_RESERVATIONS[row]
    if charged_bytes + reservation > PROJECT_CEILING:
        raise Refusal('row reservation exceeds the project ceiling')
    try:
        return probe.limit_record(charged_bytes, reservation)
    except probe.Refused as exc:
        raise Refusal('row reservation is not representable') from exc


def _invoke_candidate_setter(row, charged_bytes, call):
    """Source-only fixed syscall boundary; no command entry or authority calls it.

    The future root route must establish fresh preconditions and one-use authority
    before calling this function, then retain an independent post-observation.
    A successful syscall return alone is not proof of enforcement.
    """
    if ctypes.sizeof(ctypes.c_void_p) != 8 or ctypes.sizeof(probe.DQBlk) != 72 or \
            probe.DQBlk.valid.offset != 64:
        raise Refusal('unsupported quota setter ABI')
    record = prospective_row_limit(row, charged_bytes)
    try:
        result = call(probe.Q_SETQUOTA, ROOT_DEVICE, PROJECT_ID, ctypes.byref(record))
    except (probe.Refused, OSError) as exc:
        raise Refusal('project quota setter failed or its result is unknown') from exc
    if not isinstance(result, tuple) or len(result) != 2 or type(result[0]) is not int or \
            type(result[1]) is not int or result[0] != 0:
        raise Refusal('project quota setter failed or its result is unknown')
    return {'setter_exit': 0, 'hard_qb': record.bhard}


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


def writer_tool_route(helper=WRITER_HELPER):
    """Fixed, read-only custody observation; it cannot authorize a writer."""
    dependencies = ('linux_supervisor.py', 'preparation_resources.py')
    return {'argv': ['/usr/bin/python3', '-I', '-B', str(helper), '--observe-writer-tools'],
            'sha256': helper_sha256(helper),
            'dependencies': {name: helper_sha256(Path(helper).parent / name) for name in dependencies}}


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
        if not isinstance(text, str) or not HEX64.fullmatch(digest or '') or type(code) is not int or code != 0:
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
            isinstance(uid, bool) or uid <= 0 or not isinstance(root, str) or not root or \
            not isinstance(entries, (set, frozenset, list, tuple)) or '.' not in entries:
        raise Refusal('allocation facts are incomplete: boot, uid, root or exact entries missing')
    return boot, uid, root, frozenset(entries)


def verify(witness, limit_bytes, facts, supported):
    """Check originals and exact facts, then refuse while enforcement and custody are unproven."""
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
    if valid < 0 or valid & ~0x3f or not valid & QIF_BLIMITS or not valid & QIF_SPACE:
        raise Refusal('quota record does not mark its block limit and space usage valid')
    if charged < 0:
        raise Refusal('project charge is negative')
    if hard <= 0:
        raise Refusal('project has no active hard block limit')
    headroom = hard - charged
    if headroom < limit_bytes:
        raise Refusal('limit exceeds the project headroom left after current charges')
    if headroom > limit_bytes:
        raise Refusal('project headroom exceeds the row ceiling')

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
        if kind == 'dir' and owner == ROOT_UID:
            mode = fields.get('mode')
            if not isinstance(mode, str) or not re.fullmatch(r'[0-7]{1,4}', mode) or int(mode, 8) & 0o022:
                raise Refusal('root-owned directory ' + path + ' is writable by others or has invalid mode')
        elif owner != uid:
            raise Refusal('entry ' + path + ' is owned by another uid')
        if _int(fields.get('projid'), 'projid') != PROJECT_ID:
            raise Refusal('entry ' + path + ' carries another project id')
        if kind == 'dir':
            xflags = _int(fields.get('xflags'), 'xflags')
            if not 0 <= xflags <= 0xffffffff or not xflags & probe.FS_XFLAG_PROJINHERIT:
                raise Refusal('directory ' + path + ' does not inherit its project')
        if kind == 'reg' and _int(fields.get('nlink'), 'nlink') != 1:
            raise Refusal('entry ' + path + ' is hardlinked; its charge is not owned by the tree alone')
        blocks = _int(fields.get('blocks512'), 'blocks512')
        if blocks < 0:
            raise Refusal('entry ' + path + ' has negative allocation')
        tree_bytes += blocks * 512
    if entries['.'].get('type') != 'dir':
        raise Refusal('stage root is not a directory')
    if charged < tree_bytes:
        raise Refusal('kernel charge is below the tree bytes; accounting does not cover the tree')
    # An owner in the initial user namespace can change project attributes. A
    # pre-run tree and a quota format do not establish confinement or limit
    # enforcement throughout the row. Never promote these observations to a
    # hard-bound witness until both have an independently reviewed proof.
    raise Refusal('project quota enforcement and writer confinement are unproven')
