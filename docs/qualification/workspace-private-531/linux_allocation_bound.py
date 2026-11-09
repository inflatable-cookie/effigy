"""Kernel-enforced allocation bound for the Linux setup stage tree (source controls only).

Mechanism candidate `ext4-project-quota`: a native ext4 project quota may bound charged allocation
when limit enforcement and writer custody are independently established. Root may bypass a hard
limit and an inode owner may change project attributes; the current route proves neither boundary.
The intended metric is RETAINED ALLOCATION of the tree (extents charged to the project), not
cumulative writes: an overwrite or a delete-and-rewrite can reuse allocation and is not counted.
It does not bound writers outside the tree, such as root-owned system paths or Docker storage.

Observed 2026-11-08 (747 originals): root ext4 with neither feature and no quota tools. That is
absence. The verifier refuses it, and nothing here enables, mounts, or provisions anything.
`linux_setup.SUPPORTED_HARD_LIMITS` admits the mechanism as source-qualified, and admission is not a
grant: a row is credited only from a complete witness produced by the fixed `--approved-quota` role.

A witness carries the five route originals (text, sha256, waited exit). It is verified against exact
request facts: boot id, owned uid, stage root, and the exact entry set. Originals are hashed and
checked for consistency with each other; a caller label is never enforcement.

The same module holds the E7b/E7c project-assignment verifier and the ordered activation route with
its progress guard. Those are source qualifications and controls: they observe nothing on this VM and
grant no snapshot, boot, filesystem, project, quota or native credit.
"""
import ctypes
import hashlib
import json
import os
import re
import struct
import uuid
from pathlib import Path

from linux_supervisor import Refusal, QUOTA_ROWS as ROW_RESERVATIONS, QUOTA_GROUP_ROWS, \
    quota_row_reservation, quota_limit_record, PROJECT_MECHANISM, PROJECT_ROUTES, PROJECT_ROOT
import linux_allocation_probe as probe

MECHANISM = 'ext4-project-quota'
ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'
ROOT_DEVICE = '/dev/sda2'
ROOT_FSTYPE = 'ext4'
REQUIRED_FEATURES = frozenset({'project', 'quota'})
PROJECT_ID = 531                                  # reserved for the stage tree; no other object may carry it
QB_UNIT_BYTES = probe.QB_UNIT_BYTES
PROJECT_CEILING = 1 << 30
# if_dqblk dqb_valid bits, linux/quota.h enum: QIF_BLIMITS_B = 0, QIF_SPACE_B = 1
QIF_BLIMITS = 1 << 0
QIF_SPACE = 1 << 1
ROOT_UID = 0                                      # the supervisor creates its gate FIFO as root
ROUTES = ('mountinfo', 'dumpe2fs', 'by-uuid', 'quota', 'tree')
# The fixed activation medium, recorded from the Ubuntu 26.04 release metadata (see linux-setup.md).
# It is one recorded candidate, not an acquired image: no bytes are downloaded by this module.
ACTIVATION_MEDIUM = {
    'publisher': 'Ubuntu cdimage release metadata for 26.04',
    'product': 'ubuntu-26.04.1-live-server-arm64.iso',
    'url': 'https://cdimage.ubuntu.com/ubuntu/releases/26.04/release/ubuntu-26.04.1-live-server-arm64.iso',
    'iso_sha256': 'af78753c94b924c85c4964c670692a853f7c80299a4064e317edad95462e3168',
    'iso_bytes': 3076861952,
    'sums_sha256': '0daa248426442137c56a5122b89bda08e5854f565a6eed26dec5622094d3e5a3',
    'manifest_sha256': 'ad43a50074efc81bc77c4ef20339f4897fc8264eea5196655fe240fab0d6c86e',
    'signature_sha256': 'bfa9b6e8b87298c3dfed6310029e370c4bd5f553175fb2c57705cd11bfb84086',
    'signature_keyid': 'D94AA3F0EFE21092',
    'e2fsprogs': '1.47.2-3ubuntu4',
}
OFFLINE_TOOL_VERSION = '1.47.2'
ACTIVATION_VM = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
# One exact ordered activation sequence. Each boundary names the facts a record must carry; an
# unlisted boundary (for example a restore) is not part of this route and refuses.
ACTIVATION_BOUNDARIES = (
    ('quiesce', 'precondition', ('rows_idle', 'writers_stopped', 'vm_id', 'preboot_id')),
    ('snapshot', 'operator-effect', ('snapshot_id', 'space_bytes', 'quiescent', 'vm_id', 'preboot_id')),
    ('rescue-boot', 'operator-effect', ('medium_sha256', 'tools', 'root_unmounted')),
    ('precheck', 'offline', ('root_unmounted',)),
    ('features', 'offline', ('root_unmounted',)),
    ('mount-options', 'offline', ('root_unmounted', 'stored_options_empty')),
    ('postcheck', 'offline', ('root_unmounted', 'features_active')),
    ('return-boot', 'operator-effect', ('root_mounted_rw',)),
    ('project-assign', 'fixed-entry', ('assigned',)),
    ('quota', 'fixed-entry', ('limit_set',)),
    ('enforce', 'observation', ('phase',)),
)
RESCUE_BOUNDARIES = frozenset({'rescue-boot', 'precheck', 'features', 'mount-options', 'postcheck'})
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
    """Validate the fixed admin request shape; it names no device, project or limit."""
    if not isinstance(request, dict) or set(request) != {'row', 'nonce', 'boot_id'}:
        raise Refusal('quota request must contain only row, nonce and boot identity')
    row, nonce, boot = request['row'], request['nonce'], request['boot_id']
    if not isinstance(row, str) or row not in ROW_RESERVATIONS:
        raise Refusal('row has no fixed project reservation')
    if not isinstance(nonce, str) or not HEX64.fullmatch(nonce) or \
            not isinstance(boot, str) or not BOOT_UUID.fullmatch(boot):
        raise Refusal('quota request nonce or boot identity is invalid')
    return {'row': row, 'nonce': nonce, 'boot_id': boot}


def placement_aggregate(items):
    """The retained bytes the fixed placement set requires; a group ceiling must cover them all.

    E19 and EGO are aggregate kit ceilings (kit 6.3: E19 placements <= 1 MiB). The reservation is
    the ceiling for the whole set, never a per-file allowance, so the set must fit inside it.
    """
    if not isinstance(items, list) or not items:
        raise Refusal('placement set is empty or not bound')
    total = 0
    for item in items:
        if not isinstance(item, dict) or type(item.get('bytes')) is not int or item['bytes'] <= 0:
            raise Refusal('placement has no exact byte size')
        total += item['bytes']
    return total


def group_ceiling_reason(row, items):
    """None when the fixed placement set fits the group ceiling; otherwise the refusal text."""
    try:
        reservation = quota_row_reservation(row)
        total = placement_aggregate(items)
    except Refusal as exc:
        return str(exc)
    if row not in QUOTA_GROUP_ROWS:
        return 'row is not an aggregate placement group'
    if total > reservation:
        return 'placement set requires %d bytes, above the %d-byte aggregate ceiling' % (total, reservation)
    return None


def prospective_row_limit(row, charged_bytes):
    """Build the fixed quota record for one row; the limit is derived, never caller-chosen."""
    if not isinstance(row, str) or row not in ROW_RESERVATIONS:
        raise Refusal('row has no fixed project reservation')
    if not isinstance(charged_bytes, int) or isinstance(charged_bytes, bool) or charged_bytes < 0:
        raise Refusal('project charge is invalid')
    reservation = ROW_RESERVATIONS[row]
    if charged_bytes + reservation > PROJECT_CEILING:
        raise Refusal('row reservation exceeds the project ceiling')
    return quota_limit_record(charged_bytes, reservation)


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


def _originals(witness, routes=ROUTES):
    observations = witness.get('observations')
    if not isinstance(observations, dict) or set(observations) != set(routes):
        raise Refusal('allocation observation set is incomplete')
    texts = {}
    for route in routes:
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
    """Admit a hard bound only from a complete, exact witness for one row or placement group.

    Requires the five retained originals, the fixed setter custody, the genuine EDQUOT enforcement
    observation, an exact entry set and a headroom equal to the row reservation. A caller label, a
    format bit, a missing original or an observation from another boot/device/project refuses.
    """
    if not isinstance(witness, dict):
        raise Refusal('allocation witness is missing')
    mechanism = witness.get('mechanism')
    if mechanism != MECHANISM or mechanism not in supported:
        raise Refusal('allocation mechanism is not admitted')
    if not isinstance(limit_bytes, int) or isinstance(limit_bytes, bool) or limit_bytes <= 0:
        raise Refusal('limit is not a positive byte count')
    boot, uid, root, expected = _facts(facts)
    row, nonce = witness.get('row'), witness.get('nonce')
    if not isinstance(row, str) or row not in ROW_RESERVATIONS:
        raise Refusal('witness names no fixed row')
    if not isinstance(nonce, str) or not HEX64.fullmatch(nonce):
        raise Refusal('witness has no one-use nonce')
    if witness.get('boot_id') != boot:
        raise Refusal('witness is from another boot')
    if witness.get('device') != ROOT_DEVICE or witness.get('project') != PROJECT_ID:
        raise Refusal('witness names another device or project')
    if witness.get('limit_bytes') != limit_bytes:
        raise Refusal('witness limit differs from the charged row')
    if witness.get('setter_exit') != 0:
        raise Refusal('the fixed quota setter did not report a positive waited exit')
    custody = witness.get('custody')
    entry_sha = hashlib.sha256((Path(__file__).resolve().parent / 'linux_supervisor.py').read_bytes()).hexdigest()
    if not isinstance(custody, dict) or custody.get('entry_sha256') != entry_sha or \
            not isinstance(custody.get('approval_digest'), str) or \
            not HEX64.fullmatch(custody['approval_digest']) or \
            custody.get('device') != ROOT_DEVICE or custody.get('project') != PROJECT_ID:
        raise Refusal('witness does not carry the fixed setter entry and one-use approval custody')
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
    reasons = _enforcement_reasons(witness, row, boot, nonce, limit_bytes)
    if reasons:
        raise Refusal(reasons[0])
    return {'mechanism': MECHANISM, 'row': row, 'nonce': nonce, 'limit_bytes': limit_bytes,
            'charged_bytes': charged, 'hard_bytes': hard, 'headroom_bytes': headroom,
            'enforcement': dict(witness['enforcement']), 'custody': dict(witness['custody']),
            'tree_bytes': tree_bytes}


def _enforcement_reasons(witness, row, boot, nonce, limit_bytes):
    """The enforcement record must be the fixed over-limit probe, refused in the charged scope.

    A feature bit, a quota format or a zero syscall result is not enforcement. This record is a
    genuine allocation attempt at the row ceiling by the payload identity with every capability set
    emptied; EDQUOT is the observation, and anything else refuses.
    """
    item = witness.get('enforcement')
    if not isinstance(item, dict):
        return ['enforcement observation is missing']
    required = {'argv', 'exit', 'bytes_allocated', 'phase', 'reservation_bytes', 'stdout', 'stderr',
                'stdout_sha256', 'stderr_sha256'}
    if set(item) != required:
        return ['enforcement observation has an unexpected shape']
    if item['argv'] != ['--enforce', QUOTA_ROOT_PROBE, str(limit_bytes)]:
        return ['enforcement observation is not the fixed over-limit probe for this row']
    if type(item['exit']) is not int or item['exit'] != 3 or item['phase'] != 'edquot':
        return ['enforcement observation is not an EDQUOT refusal']
    if item['reservation_bytes'] != limit_bytes or item['bytes_allocated'] != limit_bytes:
        return ['enforcement observation does not fill exactly the row reservation']
    for stream in ('stdout', 'stderr'):
        value, digest = item[stream], item[stream + '_sha256']
        if not isinstance(value, str) or not isinstance(digest, str) or not HEX64.fullmatch(digest):
            return ['enforcement %s stream is malformed' % stream]
        if hashlib.sha256(value.encode('utf-8', 'surrogateescape')).hexdigest() != digest:
            return ['enforcement %s stream differs from its hash' % stream]
    if 'phase=edquot' not in item['stderr']:
        return ['enforcement stream does not record the EDQUOT phase']
    return []


QUOTA_ROOT = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3'
QUOTA_ROOT_PROBE = QUOTA_ROOT + '/.effigy-quota-enforcement'


def verify_scope(scope, facts):
    """Check the writer bound that accompanies a charged row: confinement, custody and channel.

    Source basis: a private identity user namespace rejects project and inheritance-flag changes
    (fs/file_attr.c, fs/ioctl.c), empty capability sets remove the CAP_SYS_RESOURCE quota bypass
    (fs/quota/dquot.c) and survive exec, no-new-privileges prevents regaining privileges, and an
    empty supplementary group set removes the docker socket channel for this setup workload. The
    only same-UID entry left is the qualification key, which is restricted to one forced command
    that can write only a reviewed row FIFO. A different value, a missing observation or a second
    channel refuses.
    """
    boot, uid, _, _ = _facts(facts)
    if not isinstance(scope, dict) or set(scope) != {'boot_id', 'row', 'unit', 'process', 'channel', 'tools'}:
        raise Refusal('writer scope observation is missing or has an unexpected shape')
    if scope['boot_id'] != boot:
        raise Refusal('writer scope observation is from another boot')
    return _scope_checks(scope, uid)


def _scope_checks(scope, uid):
    unit = scope['unit']
    expected_unit = {'PrivateUsers': 'identity', 'NoNewPrivileges': 'yes',
                     'SupplementaryGroups': '', 'CapabilityBoundingSet': ''}
    if unit != expected_unit:
        raise Refusal('unit effective confinement differs from the fixed properties')
    process = scope['process']
    if not isinstance(process, dict) or set(process) != {'uid', 'groups', 'cap_eff', 'cap_prm',
                                                         'no_new_privs', 'user_ns', 'init_user_ns',
                                                         'docker_gid'}:
        raise Refusal('payload process confinement observation is incomplete')
    if process['uid'] != uid:
        raise Refusal('payload process uid differs from the reviewed account')
    if process['cap_eff'] or process['cap_prm']:
        raise Refusal('payload process retains capabilities; the quota bypass is not removed')
    if process['no_new_privs'] != 1:
        raise Refusal('payload process has no no-new-privileges guarantee')
    if not isinstance(process['user_ns'], int) or not isinstance(process['init_user_ns'], int) or \
            process['user_ns'] == process['init_user_ns']:
        raise Refusal('payload process is not in a private user namespace; project reassignment is possible')
    if type(process['docker_gid']) is int and process['docker_gid'] in (process['groups'] or []):
        raise Refusal('payload process keeps the docker group; an unconfined channel defeats the bound')
    channel = scope['channel']
    if not isinstance(channel, dict) or set(channel) != {'forced_command', 'sha256', 'key_lines'}:
        raise Refusal('same-uid writer channel observation is incomplete')
    if channel['forced_command'] != _forced_command() or \
            channel['sha256'] != hashlib.sha256(channel['forced_command'].encode()).hexdigest():
        raise Refusal('the qualification key is not restricted to the one forced command')
    if channel['key_lines'] != 1:
        raise Refusal('the qualification key file is not the single reviewed line')
    tools = scope['tools']
    if not isinstance(tools, dict) or not tools:
        raise Refusal('writer tool custody observation is missing')
    for path, record in tools.items():
        if not isinstance(record, dict) or record.get('uid') != 0 or record.get('nlink') != 1 or \
                record.get('mode', 0) & 0o022 or not isinstance(record.get('sha256'), str) or \
                not HEX64.fullmatch(record['sha256']):
            raise Refusal('writer tool custody observation is not the fixed root-owned executable')
    return {'boot_id': scope['boot_id'], 'row': scope['row'], 'uid': uid,
            'unit': dict(unit), 'process': dict(process), 'channel': dict(channel),
            'tool_sha256': {path: record['sha256'] for path, record in sorted(tools.items())}}


def _forced_command():
    return ('/usr/bin/python3 -I -B /usr/local/libexec/effigy-stage-supervisor --transfer-writer')


def witness_digest(witness):
    """Stable digest of a complete witness, so a receipt can bind the exact retained bytes."""
    return hashlib.sha256(json.dumps(witness, sort_keys=True).encode('utf-8')).hexdigest()


# ---------------------------------------------------------------- project assignment (E7b/E7c)

def fsxattr_abi():
    """The reviewed fsxattr ABI, derived here so a drift from the installed role refuses.

    linux/fs.h encodes both requests as _IOC(<dir>, 'X', <nr>, struct fsxattr) and the size field is
    the 28-byte structure. This derivation never reads the guest kernel: it is the fixed header shape
    the role must use, and the installed role's own derivation must equal it.
    """
    size = probe.FSXATTR.size

    def request(direction, number):
        return direction << 30 | size << 16 | ord('X') << 8 | number

    get, setter = request(2, 31), request(1, 32)
    if size != 28 or get != probe.FS_IOC_FSGETXATTR or setter != probe.FS_IOC_FSSETXATTR:
        raise Refusal('fsxattr ioctl ABI differs from the reviewed kernel header')
    return {'struct_size': size, 'get': get, 'set': setter}


def project_facts(facts):
    """The exact facts a project-assignment witness must match: one boot, the reviewed root and device."""
    if not isinstance(facts, dict) or set(facts) != {'boot_id', 'root', 'device', 'uid'}:
        raise Refusal('project assignment facts are incomplete: boot, root, device and uid are required')
    boot, root, device, uid = facts['boot_id'], facts['root'], facts['device'], facts['uid']
    if type(uid) is not int or uid < 0:
        raise Refusal('project assignment owner fact is invalid')
    # The plan's boot identity is the same value the writer-bound facts use: any non-empty, bounded,
    # trimmed identity. The guest role still requires the kernel's UUID form before any change.
    if not isinstance(boot, str) or not boot or len(boot) > 64 or boot != boot.strip():
        raise Refusal('project assignment boot identity is missing')
    if root != PROJECT_ROOT or device != ROOT_DEVICE:
        raise Refusal('project assignment facts name another root or device')
    return boot, root, device, uid


def _project_record(text, boot, dev):
    """Refuse unless the retained project record is active, unconfigured and uncharged."""
    values = _pairs(text, 'project')
    keys = ('getfmt_rc', 'fmt', 'getquota_rc', 'bhard_qb', 'bsoft_qb', 'curspace_bytes', 'curinodes',
            'ihard', 'isoft', 'project_id', 'dev', 'boot_id')
    missing = [key for key in keys if key not in values]
    if missing:
        raise Refusal('project record lacks ' + ','.join(missing))
    try:
        numbers = {key: int(values[key]) for key in keys if key != 'dev' and key != 'boot_id'}
    except ValueError as exc:
        raise Refusal('project record is malformed') from exc
    if numbers['project_id'] != PROJECT_ID or values['dev'] != dev or values['boot_id'] != boot:
        raise Refusal('project record is from another boot, device or project')
    if numbers['getfmt_rc'] != 0 or numbers['fmt'] == 0:
        raise Refusal('project quota format is not active on the root device')
    if numbers['getquota_rc'] != 0:
        raise Refusal('project record is unavailable; the project is not proven unused')
    if any(numbers[key] != 0 for key in ('bhard_qb', 'bsoft_qb', 'ihard', 'isoft')):
        raise Refusal('project already carries a quota limit; it is not unused')
    if numbers['curspace_bytes'] or numbers['curinodes']:
        raise Refusal('project is already charged; it is not unused')
    return {'dev': values['dev'], 'boot_id': values['boot_id'], 'project': PROJECT_ID}


def _fsxattr_value(hextext, label):
    if not isinstance(hextext, str) or not re.fullmatch(r'[0-9a-f]{56}', hextext):
        raise Refusal(label + ' attribute value is not the fixed 28-byte structure')
    return probe.FSXATTR.unpack(bytes.fromhex(hextext))


def verify_project_assignment(witness, facts):
    """Admit one complete project-assignment witness for the E7b/E7c boundary.

    The witness must show one boot, the bound native root device, a root-owned empty BASE on that
    device, an empty pre-state attribute original, an independently read project record with no limit
    and no charge, exactly one FS_IOC_FSSETXATTR, and an independent re-read whose project and
    inheritance are set while every other attribute field is unchanged. A caller label, another
    boot/device/project, a nonempty or replaced directory, a missing original and an unavailable
    project proof all refuse. The witness is evidence about one occurrence; it is not a native proof.
    """
    boot, root, device, uid = project_facts(facts)
    if not isinstance(witness, dict):
        raise Refusal('project assignment witness is missing')
    shape = {'mechanism', 'assigned', 'nonce', 'boot_id', 'device', 'project', 'projinherit', 'root', 'abi',
             'base', 'attributes', 'setter', 'custody', 'observations'}
    if set(witness) != shape:
        raise Refusal('project assignment witness has an unexpected shape')
    if witness['mechanism'] != PROJECT_MECHANISM:
        raise Refusal('project assignment mechanism is not admitted')
    if witness['assigned'] is not True or witness['boot_id'] != boot:
        raise Refusal('project assignment is unassigned or from another boot')
    if witness['device'] != device or witness['project'] != PROJECT_ID or \
            witness['projinherit'] != probe.FS_XFLAG_PROJINHERIT or witness['root'] != root:
        raise Refusal('project assignment names another device, project, flag or root')
    if not isinstance(witness['nonce'], str) or not HEX64.fullmatch(witness['nonce']):
        raise Refusal('project assignment has no one-use nonce')
    if witness['abi'] != fsxattr_abi():
        raise Refusal('project assignment used another fsxattr ABI')
    texts = _originals(witness, PROJECT_ROUTES)
    mount = _root_mount(texts['mountinfo'])
    _check_superblock(texts['dumpe2fs'])
    if os.path.basename(texts['by-uuid'].strip()) != ROOT_DEVICE.rsplit('/', 1)[1]:
        raise Refusal('by-uuid link does not resolve to the native root device')
    _project_record(texts['project'], boot, mount['dev'])
    base = witness['base']
    if not isinstance(base, dict) or set(base) != {'dev', 'ino', 'uid', 'gid', 'mode', 'nlink', 'entries'}:
        raise Refusal('project root identity is missing or has an unexpected shape')
    if base['dev'] != mount['dev']:
        raise Refusal('project root is not on the bound root device')
    if base['uid'] != uid or type(base['mode']) is not int or base['mode'] & 0o022:
        raise Refusal('project root is not owned by the fixed root route or is writable by others')
    if type(base['ino']) is not int or base['ino'] <= 0 or type(base['nlink']) is not int or base['nlink'] < 2:
        raise Refusal('project root descriptor identity is incomplete')
    if base['entries'] != []:
        raise Refusal('project root was not empty when the project was assigned')
    attributes = witness['attributes']
    if not isinstance(attributes, dict) or set(attributes) != {'before', 'after'}:
        raise Refusal('project attribute originals are missing')
    old_flags, old_extent, old_count, old_project, old_cowextent, old_padding = \
        _fsxattr_value(attributes['before'], 'original')
    flags, extent, count, project, cowextent, padding = _fsxattr_value(attributes['after'], 'resulting')
    if old_project != 0 or old_flags & probe.FS_XFLAG_PROJINHERIT or old_padding != bytes(8):
        raise Refusal('the original attributes were not the empty pre-state')
    if project != PROJECT_ID or not flags & probe.FS_XFLAG_PROJINHERIT:
        raise Refusal('the resulting attributes do not carry project 531 with inheritance')
    if (flags, extent, count, cowextent, padding) != \
            (old_flags | probe.FS_XFLAG_PROJINHERIT, old_extent, old_count, old_cowextent, old_padding):
        raise Refusal('the assignment changed a field outside the project and inheritance')
    setter = witness['setter']
    if not isinstance(setter, dict) or set(setter) != {'ioctl', 'count', 'exit'} or \
            setter['ioctl'] != 'FS_IOC_FSSETXATTR' or setter['count'] != 1 or setter['exit'] != 0:
        raise Refusal('the project assignment is not exactly one positive ioctl')
    custody = witness['custody']
    entry_sha = hashlib.sha256((Path(__file__).resolve().parent / 'linux_supervisor.py').read_bytes()).hexdigest()
    if not isinstance(custody, dict) or set(custody) != {'entry_sha256', 'approval_digest', 'device', 'project'} \
            or custody['entry_sha256'] != entry_sha or \
            not isinstance(custody['approval_digest'], str) or not HEX64.fullmatch(custody['approval_digest']) \
            or custody['device'] != ROOT_DEVICE or custody['project'] != PROJECT_ID:
        raise Refusal('the witness does not carry the fixed role entry and one-use approval custody')
    return {'mechanism': PROJECT_MECHANISM, 'assigned': True, 'nonce': witness['nonce'], 'boot_id': boot,
            'device': ROOT_DEVICE, 'project': PROJECT_ID, 'root': root, 'base': dict(base),
            'attributes': {'before': attributes['before'], 'after': attributes['after']},
            'approval_digest': custody['approval_digest']}


# ---------------------------------------------------------------- activation route

def activation_plan():
    """The fixed activation boundaries in order, with the accepted offline argv composed in place.

    Constructing this list runs nothing. The offline boundaries reuse offline_operation_plan()'s exact
    argv objects, so the accepted commands and this route cannot drift.
    """
    offline = dict(offline_operation_plan())
    return [{'name': name, 'kind': kind, 'requires': tuple(requires), 'argv': offline.get(name)}
            for name, kind, requires in ACTIVATION_BOUNDARIES]


def inspect_activation_progress(records):
    """Check one exact activation sequence and refuse every interrupted or unrecorded boundary.

    Supplied records are never authority: the caller must independently authenticate the snapshot,
    the medium, the rescue boot and the unmounted device. Every boundary must also carry the waited
    exit of the command that performed it, and it must be exactly zero: a nonzero, missing, boolean or
    non-integer exit is a failed or unknown result and refuses just as a refused or unknown
    disposition does. What this guard refuses is an out-of-order, repeated, unrecorded, refused or
    unknown boundary and any boundary that is not in the plan (including a restore or a retry). An
    interruption is therefore never a partial success, and nothing resumes or rolls back
    automatically after an unknown result.
    """
    plan = activation_plan()
    if not isinstance(records, list) or len(records) > len(plan):
        raise Refusal('activation record set is incomplete or longer than the plan')
    rescue_boot = normal_boot = preboot = None
    for item, step in zip(records, plan):
        name = step['name']
        if not isinstance(item, dict) or item.get('name') != name:
            raise Refusal('activation boundary set or order differs at ' + name)
        if set(item) != {'name', 'boot_id', 'disposition', 'waited_exit'} | set(step['requires']):
            raise Refusal('activation boundary ' + name + ' has missing or unexpected facts')
        exit_code = item['waited_exit']
        if type(exit_code) is not int or exit_code != 0:
            raise Refusal('activation boundary ' + name + ' has no positive waited exit (non-zero, missing or '
                          'unknown); its outcome is unknown and nothing resumes or restores automatically')
        if item['disposition'] != 'exited':
            raise Refusal('activation boundary ' + name + ' did not complete; its outcome is unknown and '
                          'nothing resumes or restores automatically')
        boot = item['boot_id']
        if name == 'snapshot':
            if boot is not None:
                raise Refusal('a stopped VM has no current guest boot identity')
        elif not isinstance(boot, str) or not BOOT_UUID.fullmatch(boot):
            raise Refusal('activation boundary ' + name + ' has no boot identity')
        if name in RESCUE_BOUNDARIES and name != 'rescue-boot':
            if boot != rescue_boot:
                raise Refusal('activation boundary ' + name + ' is from another boot than the rescue boot')
        elif name in ('return-boot', 'project-assign', 'quota', 'enforce'):
            if rescue_boot is None:
                raise Refusal('activation left no rescue boot identity to return from')
            if boot == rescue_boot:
                raise Refusal('activation boundary ' + name + ' is not on the returned normal boot')
        if name in ('quiesce', 'snapshot'):
            if item['vm_id'] != ACTIVATION_VM or not isinstance(item['preboot_id'], str) or not BOOT_UUID.fullmatch(item['preboot_id']):
                raise Refusal('pre-rescue VM or boot identity is missing or incorrect')
            if name == 'quiesce':
                preboot = item['preboot_id']
                if boot != preboot:
                    raise Refusal('quiesce is not on the observed normal boot')
            elif item['preboot_id'] != preboot:
                raise Refusal('snapshot is not bound to the quiesced boot')
        if name == 'quiesce':
            if item['rows_idle'] is not True or item['writers_stopped'] is not True:
                raise Refusal('activation started while writers were not stopped and idle')
        elif name == 'snapshot':
            if not isinstance(item['snapshot_id'], str) or not re.fullmatch(r'\{[0-9a-f-]{36}\}', item['snapshot_id']):
                raise Refusal('the retained snapshot has no exact identity')
            if type(item['space_bytes']) is not int or item['space_bytes'] <= 0:
                raise Refusal('snapshot space is not a positive observed byte count')
            if item['quiescent'] is not True:
                raise Refusal('the snapshot was not taken from a quiescent filesystem')
        elif name == 'rescue-boot':
            if boot == preboot:
                raise Refusal('rescue boot is not distinct from the pre-rescue boot')
            if item['medium_sha256'] != ACTIVATION_MEDIUM['iso_sha256']:
                raise Refusal('the rescue boot is not the recorded medium')
            if item['tools'] != {'tune2fs': OFFLINE_TOOL_VERSION, 'e2fsck': OFFLINE_TOOL_VERSION}:
                raise Refusal('the rescue tools are not the source-qualified e2fsprogs version')
            if item['root_unmounted'] is not True:
                raise Refusal('the rescue boot does not hold the root offline')
            rescue_boot = boot
        elif name in ('precheck', 'features', 'postcheck'):
            if item['root_unmounted'] is not True:
                raise Refusal('offline step ' + name + ' did not run with the root unmounted')
            if name == 'postcheck' and item['features_active'] is not True:
                raise Refusal('the postcheck does not show the project and quota features active')
        elif name == 'mount-options':
            if item['root_unmounted'] is not True:
                raise Refusal('the stored mount-option change did not run with the root unmounted')
            if item['stored_options_empty'] is not True:
                raise Refusal('the stored mount-option string was not proven empty before it was replaced')
        elif name == 'return-boot':
            if boot == preboot:
                raise Refusal('returned boot is not distinct from the pre-rescue boot')
            if item['root_mounted_rw'] is not True:
                raise Refusal('the normal boot did not return the root mounted writable')
            normal_boot = boot
        elif name == 'project-assign':
            if boot != normal_boot:
                raise Refusal('project assignment is from another returned boot')
            if item['assigned'] is not True:
                raise Refusal('the project assignment did not report an assignment')
        elif name == 'quota':
            if boot != normal_boot:
                raise Refusal('quota is from another returned boot')
            if item['limit_set'] is not True:
                raise Refusal('the quota boundary did not set a row limit')
        elif name == 'enforce':
            if boot != normal_boot:
                raise Refusal('enforcement is from another returned boot')
            if item['phase'] != 'edquot':
                raise Refusal('the enforcement observation is not an EDQUOT refusal')
    if normal_boot is not None and rescue_boot is not None and normal_boot == rescue_boot:
        raise Refusal('the activation never returned to a distinct normal boot')
    return {'completed': [item['name'] for item in records],
            'next': plan[len(records)]['name'] if len(records) < len(plan) else None,
            'snapshot_id': next((item['snapshot_id'] for item in records if item['name'] == 'snapshot'), None)}


def inspect_pre_rescue_originals(originals, records):
    """Bind the named VM's running boot and stopped state to retained source-qualified reads.

    This validates supplied bytes, not their origin; independent one-use custody is still required.
    The stopped state carries no guest boot ID. The boot original was collected while running.
    """
    if not isinstance(originals, dict) or set(originals) != {'running_vm', 'boot', 'stopped_vm'}:
        raise Refusal('pre-rescue originals are incomplete')
    from linux_diagnostic_proof import PRLCTL, PRLCTL_SHA256, check_vm, check_boot_id
    host_argv = [PRLCTL, 'list', '--info', ACTIVATION_VM, '--json']
    boot_argv = [PRLCTL, 'exec', ACTIVATION_VM, '/bin/cat', '/proc/sys/kernel/random/boot_id']
    for name, argv in (('running_vm', host_argv), ('boot', boot_argv), ('stopped_vm', host_argv)):
        item = originals[name]
        _offline_streams(item, name, {'argv', 'tool_sha256'})
        if item['argv'] != argv or item['tool_sha256'] != PRLCTL_SHA256:
            raise Refusal('pre-rescue original has another source or argv')
    try:
        check_vm(originals['running_vm']['stdout'].encode(), 'running')
        check_vm(originals['stopped_vm']['stdout'].encode(), 'stopped')
        boot = check_boot_id(originals['boot']['stdout'].encode())
    except (ValueError, KeyError, TypeError) as exc:
        raise Refusal('pre-rescue VM or boot original is invalid') from exc
    if boot != records[0]['preboot_id'] or boot != records[1]['preboot_id']:
        raise Refusal('pre-rescue originals do not bind the quiesced boot')
    return {'vm_id': ACTIVATION_VM, 'preboot_id': boot}


def inspect_activation_completion(activation):
    """Require the whole sequence and its offline originals; no quota credit is yet possible.

    These records are source controls, not proof of provenance. The independent accounting
    premise remains unresolved: v1.47.2 quota_data_add can lose an allocation failure and
    quota_compare_and_update can overwrite a scan error while closing the quota file.
    """
    if not isinstance(activation, dict) or set(activation) != {'records', 'offline_originals', 'pre_originals'}:
        raise Refusal('activation completion requires records and offline originals')
    records = activation['records']
    progress = inspect_activation_progress(records)
    if progress['next'] is not None:
        raise Refusal('activation is only a prefix, not completion')
    inspect_pre_rescue_originals(activation['pre_originals'], records)
    rescue = next(item['boot_id'] for item in records if item['name'] == 'rescue-boot')
    names = [name for name, _ in offline_operation_plan()]
    if inspect_offline_progress(rescue, names, activation['offline_originals'])['next'] is not None:
        raise Refusal('offline operations are incomplete')
    raise Refusal('independent complete quota accounting is not established')
