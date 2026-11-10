"""Injected controls for the ext4 project-quota allocation bound (linux_allocation_bound.py).

Originals are rendered in the helper's own key=value format; the observed 747 features line is
copied from the retained root dumpe2fs original. The helper runs with injected kernel calls, and
the tree walk runs on a real fixture under TMPDIR (lstat-only, removed afterwards). No VM, SSH,
mount, quota, tune2fs, account or service is touched. A passing control is source evidence only.
"""
import ctypes
import hashlib
import inspect
import io
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_allocation_bound as bound
import linux_allocation_witness as witness
import linux_rescue_input as rescue
import linux_rescue_input_proof as rescue_proof
import linux_allocation_probe as probe
import linux_supervisor as supervisor
from linux_native_support import PVM_HOME
from linux_supervisor import Refusal

BOOT = 'boot-1234'
UID = 1001
ROOT = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3'
HISTORICAL_BASE_DIRS = ('source', 'qual-root', 'driver', 'inputs', 'inputs/go', 'frozen', 'metadata',
                        'docker-config', 'toolchain', 'old-823d859d', 'docker-build-ctx')
LIMIT = bound.ROW_RESERVATIONS['E9a']
MECH = bound.MECHANISM
# Retained 747 root dumpe2fs features line (observation-filesystem.raw), unchanged.
OBSERVED_747_FEATURES = ('has_journal ext_attr resize_inode dir_index orphan_file filetype needs_recovery extent '
                         '64bit flex_bg metadata_csum_seed sparse_super large_file huge_file dir_nlink extra_isize '
                         'metadata_csum orphan_present')
ENTRIES = (
    ('.', 'dir', '8:2', UID, 531, 3, 8),
    ('inputs', 'dir', '8:2', UID, 531, 2, 8),
    ('inputs/archive.tar', 'reg', '8:2', UID, 531, 1, 65536),
)
# F2: the kernel charge must equal the owned tree census exactly, so every retained fixture derives
# its charged bytes from the same blocks512 entries the tree original reports (E7c's directories
# allocate legitimately before the first quota set; the census covers them).
TREE_CENSUS = sum(entry[6] * 512 for entry in ENTRIES)
CHARGED = TREE_CENSUS
HARD = CHARGED + LIMIT
NONCE = 'd' * 64


def facts(**over):
    base = {'boot_id': BOOT, 'uid': UID, 'root': ROOT, 'entries': {'.', 'inputs', 'inputs/archive.tar'}}
    base.update(over)
    return base


def original(text, code=0):
    return {'text': text, 'sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(), 'exit': code}


def render(mount_dev='8:2', mount_fs='ext4', mount_source='/dev/sda2', mount_options='rw,relatime',
           uuid=bound.ROOT_UUID, features=None, link='../../sda2', boot=BOOT, quota_dev='8:2',
           fmt=4, fmt_rc=0, quota_rc=0, hard_qb=None, charged=None, project=531, qb_unit=1024,
           entries=ENTRIES, tree_root=ROOT, tree_boot=BOOT, tree_project=531, valid=3):
    features = OBSERVED_747_FEATURES + ' project quota' if features is None else features
    # The charge and the hard limit follow the tree census unless a control names them explicitly;
    # the F2 equality at both production sites requires them equal for a complete witness.
    charged = sum(entry[6] for entry in entries) * 512 if charged is None else charged
    hard_qb = (charged + LIMIT) // 1024 if hard_qb is None else hard_qb
    texts = {
        'mountinfo': '36 1 %s / / %s shared:1 - %s %s rw\n' % (mount_dev, mount_options, mount_fs, mount_source),
        'dumpe2fs': 'Filesystem UUID:          %s\nFilesystem features:      %s\nFilesystem state:         clean\n'
                    % (uuid, features),
        'by-uuid': link + '\n',
        'quota': '\n'.join([
            'boot_id=%s' % boot, 'device=/dev/sda2', 'dev=%s' % quota_dev, 'project_id=%d' % project,
            'qb_unit_bytes=%d' % qb_unit, 'getfmt_rc=%d getfmt_errno=0 fmt=%d' % (fmt_rc, fmt),
            'getquota_rc=%d getquota_errno=0' % quota_rc,
            'bhard_qb=%d bsoft_qb=0 curspace_bytes=%d valid=%d' % (hard_qb, charged, valid)]) + '\n',
        'tree': '\n'.join(['boot_id=%s' % tree_boot, 'root=%s' % tree_root, 'project_id=%d' % tree_project] +
                          ['entry path=%s type=%s dev=%s ino=%d uid=%d gid=0 mode=700 nlink=%d projid=%d '
                           'xflags=%d blocks512=%d size=0' % (p, k, d, 10 + i, u, n, pr,
                                                          probe.FS_XFLAG_PROJINHERIT if k == 'dir' else 0, b)
                           for i, (p, k, d, u, pr, n, b) in enumerate(entries)]) + '\n',
    }
    enforcement_text = 'status=refused phase=edquot bytes=%d\n' % LIMIT
    return {'mechanism': MECH, 'row': 'E9a', 'nonce': NONCE, 'boot_id': BOOT, 'device': bound.ROOT_DEVICE,
            'project': bound.PROJECT_ID, 'charged_bytes': charged, 'limit_bytes': LIMIT, 'setter_exit': 0,
            'custody': {'entry_sha256': hashlib.sha256(Path(supervisor.__file__).read_bytes()).hexdigest(),
                        'approval_digest': 'e' * 64, 'device': bound.ROOT_DEVICE, 'project': bound.PROJECT_ID},
            'enforcement': {'argv': ['--enforce', bound.QUOTA_ROOT_PROBE, str(LIMIT)], 'exit': 3,
                            'bytes_allocated': LIMIT, 'phase': 'edquot', 'reservation_bytes': LIMIT,
                            'stdout': '', 'stderr': enforcement_text,
                            'stdout_sha256': hashlib.sha256(b'').hexdigest(),
                            'stderr_sha256': hashlib.sha256(enforcement_text.encode()).hexdigest()},
            'observations': {r: original(texts[r]) for r in bound.ROUTES}}


def replace_original(witness, route, text=None, **changes):
    observations = dict(witness['observations'])
    item = dict(observations[route])
    if text is not None:
        item['text'] = text
        item['sha256'] = hashlib.sha256(text.encode('utf-8')).hexdigest()
    item.update(changes)
    observations[route] = item
    return dict(witness, observations=observations)


def census_of(root):
    """The owned tree census the F2 equality compares: lstat blocks over the fixture, like the helper."""
    root = Path(root)
    total = os.lstat(root).st_blocks * 512
    for child in sorted(root.rglob('*')):
        total += os.lstat(child).st_blocks * 512
    return total


# ---------------------------------------------------------------- fresh-project accounting witness


def build_witness_image(path, block_bytes=1024, groups=3, ipg=8, inodes_count=17, desc64=True,
                        csum_seed=True, live_inodes=(1, 2, 11, 12), inode_size=256, state=1,
                        ro_extra=0, ro_drop_csum=False, incompat_extra=0, quota_inum=0,
                        mount_opts=b'', last_orphan=0, desc_size=None, first_data_block=None,
                        log_cluster=None, sb_patches=None, patches=None):
    """A checksum-correct synthetic ext4 layout for the offline collector (fixture bytes only).

    Everything the collector verifies is constructed consistently, so a refusal can only come from
    the one field a control corrupts. `sb_patches(image)` corrupts superblock bytes after the
    checksum is stored (checksum-drift controls); `patches(image, tables, seed)` corrupts descriptor
    or inode bytes and may recompute descriptor checksums for controls that must pass them.
    """
    bs = block_bytes
    fdb = 1 if bs == 1024 else 0
    blocks_per_group = 3 if bs == 1024 else 13
    blocks_count = fdb + blocks_per_group * groups
    desc_bytes = 64 if desc64 else 32
    sb = bytearray(1024)
    struct.pack_into('<I', sb, 0x0, inodes_count)
    struct.pack_into('<I', sb, 0x4, blocks_count)
    struct.pack_into('<I', sb, 0x10, 3)
    struct.pack_into('<I', sb, 0x14, fdb if first_data_block is None else first_data_block)
    log = {1024: 0, 4096: 2}.get(bs, 2)
    struct.pack_into('<I', sb, 0x18, log)
    struct.pack_into('<I', sb, 0x1c, log if log_cluster is None else log_cluster)
    struct.pack_into('<I', sb, 0x20, blocks_per_group)
    struct.pack_into('<I', sb, 0x24, blocks_per_group)
    struct.pack_into('<I', sb, 0x28, ipg)
    struct.pack_into('<H', sb, 0x38, 0xEF53)
    struct.pack_into('<H', sb, 0x3a, state)
    struct.pack_into('<I', sb, 0x54, 11)
    struct.pack_into('<H', sb, 0x58, inode_size)
    incompat = witness.IN_FILETYPE | witness.IN_EXTENTS | witness.IN_FLEX_BG | incompat_extra
    if desc64:
        incompat |= witness.IN_64BIT
        struct.pack_into('<H', sb, 0xfe, desc_size if desc_size is not None else 64)
    if csum_seed:
        incompat |= witness.IN_CSUM_SEED
    struct.pack_into('<I', sb, 0x60, incompat)
    ro = 0x1 | 0x2 | 0x8 | 0x20 | 0x40 | witness.RO_METADATA_CSUM | ro_extra
    if ro_drop_csum:
        ro = (ro & ~witness.RO_METADATA_CSUM) | witness.RO_GDT_CSUM
    struct.pack_into('<I', sb, 0x64, ro)
    sb[0x175] = 1
    sb[0x68:0x78] = uuid.UUID(bound.ROOT_UUID).bytes
    sb[0xe8:0xec] = struct.pack('<I', last_orphan)
    sb[0x200:0x240] = mount_opts.ljust(64, b'\0')[:64]
    struct.pack_into('<I', sb, 0x26c, quota_inum)
    if csum_seed:
        struct.pack_into('<I', sb, 0x270, 0x5EED5EED)
        seed = 0x5EED5EED
    else:
        seed = witness.crc32c(0xFFFFFFFF, bytes(sb[0x68:0x78]))
    struct.pack_into('<I', sb, 0x3fc, witness.crc32c(0xFFFFFFFF, bytes(sb[:0x3fc])))
    gdt_block = fdb + 1
    gdt_blocks = -(-groups * desc_bytes // bs)
    image = bytearray(blocks_count * bs)
    image[1024:2048] = sb
    tables, next_block = [], gdt_block + gdt_blocks
    table_span = max(1, -(-(ipg * witness.INODE_BYTES) // bs))
    for group in range(groups):
        desc = bytearray(desc_bytes)
        table_block = next_block + group * table_span
        tables.append(table_block)
        struct.pack_into('<I', desc, 0x0, gdt_block)
        struct.pack_into('<I', desc, 0x4, gdt_block)
        struct.pack_into('<I', desc, 0x8, table_block)
        offset = gdt_block * bs + group * desc_bytes
        image[offset:offset + desc_bytes] = desc
    seed_for_desc = seed
    for group, table_block in enumerate(tables):
        desc_offset = gdt_block * bs + group * desc_bytes
        crc = witness._descriptor_checksum(seed_for_desc, group, bytes(image[desc_offset:desc_offset + desc_bytes]))
        struct.pack_into('<H', image, desc_offset + witness.BG_CHECKSUM, crc)
    for ino in live_inodes:
        inode = bytearray(inode_size)
        struct.pack_into('<H', inode, 0x0, 0x41ED)
        struct.pack_into('<H', inode, 0x1a, 2 if ino == 2 else 1)
        struct.pack_into('<I', inode, 0x64, 0x1234)
        if inode_size > witness.EXT4_GOOD_OLD_INODE_SIZE:
            struct.pack_into('<H', inode, 0x80, 32)
        group, slot = divmod(ino - 1, ipg)
        work = bytearray(inode)
        work[witness.I_CHECKSUM_LO:witness.I_CHECKSUM_LO + 2] = bytes(2)
        work[witness.I_CHECKSUM_HI:witness.I_CHECKSUM_HI + 2] = bytes(2)
        crc = witness.crc32c(witness.crc32c(witness.crc32c(seed, struct.pack('<I', ino)),
                                            inode[witness.I_GENERATION:witness.I_GENERATION + 4]), work)
        struct.pack_into('<H', inode, witness.I_CHECKSUM_LO, crc & 0xFFFF)
        if inode_size > witness.EXT4_GOOD_OLD_INODE_SIZE:
            struct.pack_into('<H', inode, witness.I_CHECKSUM_HI, crc >> 16)
        start = tables[group] * bs + slot * witness.INODE_BYTES
        image[start:start + inode_size] = inode
    if sb_patches is not None:
        sb_patches(image)
    if patches is not None:
        patches(image, tables, seed)
    path.write_bytes(bytes(image))
    return path, blocks_count * bs, tables


def patch_inode_field(inode_no, offset, data, ipg=8, block_bytes=1024):
    """Corrupt one field of one inode slot in the assembled fixture image."""
    def apply(image, tables, seed):
        group, slot = divmod(inode_no - 1, ipg)
        start = tables[group] * block_bytes + slot * witness.INODE_BYTES + offset
        image[start:start + len(data)] = data
    return apply


def patch_desc_table(group, table_block, gdt_block=2, block_bytes=1024, desc_bytes=64):
    """Point one group's inode table elsewhere, keeping its descriptor checksum consistent."""
    def apply(image, tables, seed):
        offset = gdt_block * block_bytes + group * desc_bytes
        struct.pack_into('<I', image, offset + witness.BG_INODE_TABLE, table_block)
        struct.pack_into('<I', image, offset + witness.BG_INODE_TABLE_HI, 0)
        desc = bytes(image[offset:offset + desc_bytes])
        struct.pack_into('<H', image, offset + witness.BG_CHECKSUM,
                         witness._descriptor_checksum(seed, group, desc))
    return apply


def collect_witness(path, end, boot=None, nonce=None, chunk_bytes=None, read=None):
    """Run the actual collector against fixture bytes through one injected read boundary."""
    boot = RESCUE_BOOT if boot is None else boot
    nonce = NONCE if nonce is None else nonce
    if read is None:
        fd = os.open(str(path), os.O_RDONLY)
        def read(offset, length):
            if offset + length > end:
                return b''                       # an out-of-extent read arrives short, never zero-filled
            return os.pread(fd, length, offset)
    if chunk_bytes is not None:
        with patch.object(witness, 'CHUNK_BYTES', chunk_bytes):
            return witness.collect(read, boot, nonce)
    return witness.collect(read, boot, nonce)


class VerifierChecks(unittest.TestCase):
    def verify(self, witness=None, limit=LIMIT, fx=None):
        return bound.verify(witness if witness is not None else render(), limit, fx or facts(), {MECH})

    def test_offline_versions_require_both_original_streams_and_waited_success(self):
        def item(stdout, stderr, exit=0):
            return {'stdout': stdout, 'stderr': stderr, 'exit': exit,
                    'stdout_sha256': hashlib.sha256(stdout.encode()).hexdigest(),
                    'stderr_sha256': hashlib.sha256(stderr.encode()).hexdigest()}
        tune = item('tune2fs 1.47.2 (1-Jan-2025)\nFilesystem UUID: x\n', '')
        check = item('', 'e2fsck 1.47.2 (1-Jan-2025)\nUsing EXT2FS Library\n')
        originals = {'tune2fs': tune, 'e2fsck': check}
        self.assertEqual(bound.offline_version_plan(), {'tune2fs': ['/sbin/tune2fs', '-l', '/dev/sda2'],
                                                       'e2fsck': ['/sbin/e2fsck', '-V']})
        self.assertEqual(bound.inspect_offline_versions(originals), {'e2fsprogs_version': '1.47.2'})
        invalid = ({'tune2fs': tune}, dict(originals, e2fsck=item('', '', exit=0)),
                   dict(originals, tune2fs=item('tune2fs 1.47.1 (date)\n', '')),
                   dict(originals, e2fsck=item('', 'e2fsck 1.47.2 (date)\n', exit=False)),
                   dict(originals, e2fsck=dict(check, stderr_sha256='0' * 64)),
                   dict(originals, tune2fs=item('tune2fs 1.47.2 (date)\n', 'x' * (128 * 1024 + 1))))
        for values in invalid:
            with self.subTest(values=str(values)[:120]), self.assertRaises(Refusal):
                bound.inspect_offline_versions(values)

    def test_offline_steps_refuse_interruption_changed_commands_and_unknown_exits(self):
        boot = '12345678-1234-1234-1234-123456789abc'
        empty_hash = hashlib.sha256(b'').hexdigest()
        plan = bound.offline_operation_plan()
        records = [{'name': name, 'argv': argv, 'boot_id': boot, 'stdout': '', 'stderr': '',
                    'stdout_sha256': empty_hash, 'stderr_sha256': empty_hash, 'exit': 0}
                   for name, argv in plan]
        names = [name for name, _ in plan]
        for count in range(len(plan) + 1):
            expected = names[count] if count < len(names) else None
            self.assertEqual(bound.inspect_offline_progress(boot, names[:count], records[:count]),
                             {'next': expected})
        for count in range(1, len(plan) + 1):
            with self.subTest(interrupted=names[count - 1]), self.assertRaises(Refusal):
                bound.inspect_offline_progress(boot, names[:count], records[:count - 1])
        cases = (dict(records[0], exit=None), dict(records[0], exit=1), dict(records[0], exit=False),
                 dict(records[0], boot_id='00000000-0000-0000-0000-000000000000'),
                 dict(records[0], argv=['/sbin/e2fsck', '-y', bound.ROOT_DEVICE]),
                 dict(records[0], stdout='tampered'))
        for record in cases:
            with self.subTest(record=record), self.assertRaises(Refusal):
                bound.inspect_offline_progress(boot, names[:1], [record])
        with self.assertRaises(Refusal):
            bound.inspect_offline_progress(boot, names[1:2], records[1:2])
        with self.assertRaises(Refusal):
            bound.inspect_offline_progress('unknown', [], [])

    def test_offline_superblock_candidate_refuses_foreign_dirty_or_nonempty_prestate(self):
        raw = bytearray(1024)
        struct.pack_into('<H', raw, 0x38, 0xef53)
        struct.pack_into('<H', raw, 0x3a, 1)
        struct.pack_into('<H', raw, 0x58, 256)
        raw[0x68:0x78] = uuid.UUID(bound.ROOT_UUID).bytes
        self.assertTrue(bound.inspect_candidate_superblock(bytes(raw))['stored_mount_options_empty'])
        mutations = ((0x38, b'\0\0'), (0x3a, b'\0\0'), (0x58, b'\x80\0'),
                     (0x60, b'\x04'), (0x64, b'\x00\x01'), (0x65, b'\x20'),
                     (0x68, b'\0'), (0x200, b'p'), (0x23f, b'x'), (0x26c, b'\x01'))
        for offset, value in mutations:
            changed = bytearray(raw)
            changed[offset:offset + len(value)] = value
            with self.subTest(offset=offset), self.assertRaises(Refusal):
                bound.inspect_candidate_superblock(bytes(changed))
        with self.assertRaises(Refusal):
            bound.inspect_candidate_superblock(bytes(raw[:-1]))

    def test_fixed_setter_builds_only_the_project_block_limit(self):
        record = bound.quota_limit_record(CHARGED, 64 << 20)
        self.assertEqual(record.bhard, (CHARGED + (64 << 20)) // probe.QB_UNIT_BYTES)
        self.assertEqual((record.bsoft, record.valid), (0, bound.QIF_BLIMITS))
        for charged, row in ((0, 0), (-1, LIMIT), (CHARGED, True), (1, LIMIT), (1 << 90, LIMIT)):
            with self.subTest(charged=charged, row=row), self.assertRaises(Refusal):
                bound.quota_limit_record(charged, row)
        self.assertEqual(bound.quota_row_reservation('E19'), 1 << 20)
        self.assertIn('E19', bound.QUOTA_GROUP_ROWS)
        with self.assertRaises(Refusal):
            bound.quota_row_reservation('E19.0')

    def test_complete_witness_is_admitted_and_missing_enforcement_refuses(self):
        witness = render()
        result = self.verify(witness)
        self.assertEqual((result['limit_bytes'], result['row']), (LIMIT, 'E9a'))
        without = {k: v for k, v in witness.items() if k != 'enforcement'}
        with self.assertRaisesRegex(Refusal, 'enforcement observation is missing'):
            self.verify(without)
        for change in ({'phase': 'ok'}, {'exit': 0}, {'bytes_allocated': LIMIT - 1},
                       {'argv': ['--enforce', '/etc/passwd', str(LIMIT)]},
                       {'stderr_sha256': '0' * 64}):
            with self.subTest(change=change):
                bad = dict(witness, enforcement=dict(witness['enforcement'], **change))
                with self.assertRaises(Refusal):
                    self.verify(bad)

    def test_setter_and_approval_custody_are_part_of_the_bound(self):
        witness = render()
        for change in ({'setter_exit': 1}, {'custody': dict(witness['custody'], approval_digest='bad')},
                       {'custody': dict(witness['custody'], entry_sha256='0' * 64)},
                       {'custody': dict(witness['custody'], device='/dev/sdb2')},
                       {'nonce': 'bad'}, {'boot_id': 'someone-else'}, {'limit_bytes': LIMIT + 1024},
                       {'device': '/dev/sdb2'}, {'project': 530}):
            with self.subTest(change=list(change)):
                with self.assertRaises(Refusal):
                    self.verify(dict(witness, **change))

    def test_witness_admission_requires_originals_and_activation_cannot_be_a_label(self):
        incomplete = {k: v for k, v in render().items() if k != 'enforcement'}
        with self.assertRaises(Refusal):
            self.verify(incomplete)
        for activation in (None, {'completed': True}):
            with self.subTest(activation=activation), self.assertRaises(Refusal):
                bound.inspect_activation_completion(activation)
        with self.assertRaises(Refusal):
            bound.verify(render(), LIMIT, facts(), frozenset())
        with self.assertRaisesRegex(Refusal, 'allocation facts are incomplete'):
            bound.verify(render(), LIMIT, {}, {MECH})

    def test_retained_747_absence_is_refused_as_unprovisioned_not_as_enforced(self):
        witness = render(features=OBSERVED_747_FEATURES)
        with self.assertRaisesRegex(Refusal, 'lacks project,quota; enforcement is not provisioned'):
            self.verify(witness)

    def test_mechanism_label_without_originals_or_admission_refuses(self):
        for witness in ({'mechanism': MECH}, {'mechanism': MECH, 'enforcement': 'kernel', 'limits': [LIMIT]},
                        {'mechanism': 'polling', 'observations': render()['observations']}):
            with self.subTest(witness=witness), self.assertRaises(Refusal):
                self.verify(witness)
        with self.assertRaises(Refusal):
            bound.verify(render(), LIMIT, facts(), {'other'})

    def test_originals_must_be_complete_hashed_and_exit_zero(self):
        good = render()
        tampered = replace_original(good, 'quota', text='boot_id=%s\n' % BOOT,
                                    sha256=good['observations']['quota']['sha256'])
        missing = dict(good, observations={k: v for k, v in good['observations'].items() if k != 'tree'})
        extra = dict(good, observations=dict(good['observations'], other=original('x')))
        bad_exit = replace_original(good, 'mountinfo', exit=1)
        false_exit = replace_original(good, 'mountinfo', exit=False)
        not_record = dict(good, observations=dict(good['observations'], dumpe2fs='text'))
        refused_text = 'status=refused phase=open-reg errno=13\n'
        refused_line = replace_original(good, 'tree', text=refused_text,
                                        sha256=hashlib.sha256(refused_text.encode()).hexdigest())
        for label, witness in (('tampered', tampered), ('missing', missing), ('extra', extra),
                               ('exit', bad_exit), ('false-exit', false_exit), ('record', not_record),
                               ('helper-refused', refused_line)):
            with self.subTest(label), self.assertRaises(Refusal):
                self.verify(witness)

    def test_root_mount_must_be_the_writable_native_ext4_device(self):
        cases = (dict(mount_fs='tmpfs'), dict(mount_source='/dev/loop0'), dict(mount_options='ro,relatime'),
                 dict(mount_dev='7:0'))
        for change in cases:
            with self.subTest(change=change), self.assertRaises(Refusal):
                self.verify(render(**change))
        two_roots = render()
        two_roots['observations']['mountinfo'] = original(
            '36 1 8:2 / / rw shared:1 - ext4 /dev/sda2 rw\n37 1 8:3 / / rw - ext4 /dev/sdb1 rw\n')
        with self.assertRaisesRegex(Refusal, 'absent or ambiguous'):
            self.verify(two_roots)

    def test_superblock_uuid_and_by_uuid_link_must_name_the_root(self):
        with self.assertRaises(Refusal):
            self.verify(render(uuid='00000000-0000-0000-0000-000000000000'))
        with self.assertRaises(Refusal):
            self.verify(render(link='../../sdb1'))

    def test_quota_record_must_be_active_fresh_and_for_this_device_and_project(self):
        cases = (dict(boot='boot-9999'), dict(quota_dev='7:0'), dict(project=530), dict(fmt_rc=3),
                 dict(fmt=0), dict(quota_rc=-1), dict(hard_qb=0), dict(qb_unit=512))
        for change in cases:
            with self.subTest(change=change), self.assertRaises(Refusal):
                self.verify(render(**change))

    def test_quota_valid_bits_must_mark_both_block_limit_and_space_usage(self):
        for valid in (0, bound.QIF_BLIMITS, bound.QIF_SPACE, bound.QIF_BLIMITS | (1 << 2), 67):
            with self.subTest(valid=valid), self.assertRaisesRegex(Refusal, 'valid'):
                self.verify(render(valid=valid))
        self.assertEqual(self.verify(render(valid=bound.QIF_BLIMITS | bound.QIF_SPACE))['mechanism'], MECH)

    def test_headroom_must_be_exactly_the_row_reservation(self):
        headroom = HARD - CHARGED
        self.assertEqual(self.verify(limit=headroom)['headroom_bytes'], headroom)
        with self.assertRaisesRegex(Refusal, 'exceeds the project headroom|differs from the charged row'):
            self.verify(limit=headroom + 1)
        with self.assertRaisesRegex(Refusal, 'headroom exceeds the row ceiling'):
            self.verify(render(hard_qb=(CHARGED + (1 << 30)) // 1024))
        with self.assertRaisesRegex(Refusal, 'no active hard block limit|headroom'):
            self.verify(render(hard_qb=CHARGED // 1024))
        for bad in (0, -1, True, None, 1.5):
            with self.subTest(limit=bad), self.assertRaises(Refusal):
                self.verify(limit=bad)

    def test_owned_object_set_is_exact_for_extra_missing_and_renamed_entries(self):
        renamed = ENTRIES[:2] + (('inputs/renamed.tar', 'reg', '8:2', UID, 531, 1, 65536),)
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(entries=renamed))
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(entries=ENTRIES[:2]))
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(), fx=facts(entries={'.', 'inputs'}))

    def test_each_owned_object_must_match_device_uid_project_and_single_link(self):
        cases = (
            (('inputs/archive.tar', 'lnk', '8:2', UID, 531, 1, 0),),
            (('inputs/archive.tar', 'reg', '7:0', UID, 531, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', 0, 531, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', UID, 0, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', UID, 531, 2, 65536),),
        )
        for case in cases:
            entries = ENTRIES[:2] + case
            with self.subTest(entry=case[0]), self.assertRaises(Refusal):
                self.verify(render(entries=entries))
        root_is_file = (('.', 'reg', '8:2', UID, 531, 1, 8),) + ENTRIES[1:]
        with self.assertRaises(Refusal):
            self.verify(render(entries=root_is_file))

    def test_directories_require_inheritance_and_root_custody_is_not_user_writable(self):
        good = render()
        original_tree = good['observations']['tree']['text']
        no_inherit = replace_original(good, 'tree', text=original_tree.replace('xflags=512', 'xflags=0', 1))
        with self.assertRaisesRegex(Refusal, 'does not inherit'):
            self.verify(no_inherit)
        root_entries = (('.', 'dir', '8:2', 0, 531, 3, 8),) + ENTRIES[1:]
        rooted = render(entries=root_entries)
        self.assertEqual(self.verify(rooted)['mechanism'], MECH)
        unsafe = replace_original(rooted, 'tree', text=rooted['observations']['tree']['text'].replace(
            'mode=700', 'mode=777', 1))
        with self.assertRaisesRegex(Refusal, 'writable by others'):
            self.verify(unsafe)

    def test_supervisor_fifos_are_admitted_only_as_blockless_single_links(self):
        gate = ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 1, 0)
        payload = ('inputs/transfer.fifo', 'fifo', '8:2', UID, -1, 1, 0)
        fx = facts(entries={'.', 'inputs', 'inputs/archive.tar', 'inputs/gate.fifo', 'inputs/transfer.fifo'})
        self.assertEqual(self.verify(render(entries=ENTRIES + (gate, payload)), fx=fx)['mechanism'], MECH)
        bad = (
            ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 1, 8),
            ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 2, 0),
            ('inputs/gate.fifo', 'fifo', '8:2', 4242, -1, 1, 0),
            ('inputs/gate.fifo', 'fifo', '7:0', 0, -1, 1, 0),
            ('inputs/gate.fifo', 'other', '8:2', 0, -1, 1, 0),
        )
        for entry in bad:
            with self.subTest(entry=entry), self.assertRaises(Refusal):
                self.verify(render(entries=ENTRIES + (entry,)), fx=fx)

    def test_real_fifo_in_the_tree_is_listed_without_blocking_or_opening(self):
        dir_ = Path(tempfile.mkdtemp(prefix='fifo-tree-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, dir_, True)
        root = dir_ / 'stage'
        (root / 'inputs').mkdir(parents=True)
        os.mkfifo(root / 'inputs' / 'gate.fifo', 0o600)
        lines = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: (531, probe.FS_XFLAG_PROJINHERIT))
        fifo = [ln for ln in lines if 'path=inputs/gate.fifo' in ln]
        self.assertEqual(len(fifo), 1)
        self.assertIn('type=fifo', fifo[0])
        self.assertIn('projid=-1', fifo[0])

    def test_kernel_charge_must_equal_the_tree_census_and_multiple_files_are_summed(self):
        with self.assertRaisesRegex(Refusal, 'below the tree bytes'):
            self.verify(render(charged=1 << 20, hard_qb=((1 << 20) + LIMIT) // 1024))
        two_files = ENTRIES + (('inputs/second.tar', 'reg', '8:2', UID, 531, 1, 65536),)
        fx = facts(entries={'.', 'inputs', 'inputs/archive.tar', 'inputs/second.tar'})
        with self.assertRaisesRegex(Refusal, 'below the tree bytes'):
            self.verify(render(entries=two_files, charged=CHARGED), fx=fx)
        with self.assertRaisesRegex(Refusal, 'exceeds the owned tree census'):
            self.verify(render(entries=two_files, charged=(66 << 20),
                               hard_qb=((66 << 20) + LIMIT) // 1024), fx=fx)
        # An exact charge over the extended tree passes: every census byte is covered, none beyond.
        exact = TREE_CENSUS + 65536 * 512
        result = self.verify(render(entries=two_files, charged=exact,
                                    hard_qb=(exact + LIMIT) // 1024), fx=fx)
        self.assertEqual(result['tree_bytes'], exact)

    def test_facts_must_bind_boot_uid_root_and_exact_entries(self):
        for fx in (facts(boot_id=None), facts(boot_id=''), facts(uid=True), facts(uid=0), facts(root=''),
                   facts(entries=None), facts(entries={'inputs'})):
            with self.subTest(fx=fx), self.assertRaises(Refusal):
                self.verify(fx=fx)
        with self.assertRaises(Refusal):
            self.verify(render(boot='boot-old'))
        with self.assertRaises(Refusal):
            self.verify(render(tree_boot='boot-old'))
        with self.assertRaises(Refusal):
            self.verify(render(tree_root='/elsewhere'))


class HelperChecks(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix='alloc-probe-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.dir, True)

    def fake_call(self, fmt, bhard, rc_quota=0, rc_fmt=0, errno=0):
        def call(cmd, device, project_id, addr):
            if cmd == probe.Q_GETFMT:
                addr._obj.value = fmt
                return rc_fmt, errno if rc_fmt else 0
            addr._obj.bhard = bhard
            addr._obj.cur = CHARGED
            addr._obj.valid = 3
            return rc_quota, errno if rc_quota else 0
        return call

    def test_fsgetxattr_parser_keeps_project_and_inheritance_flag(self):
        def ioctl(fd, command, data):
            self.assertEqual(command, probe.FS_IOC_FSGETXATTR)
            data[:] = probe.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0, 531, 0, b'\0' * 8)
        with patch.object(probe.fcntl, 'ioctl', side_effect=ioctl):
            self.assertEqual(probe._project_attributes(7), (531, probe.FS_XFLAG_PROJINHERIT))

    def test_project_assignment_is_fixed_and_refuses_nonempty_prestate(self):
        original = probe.FSXATTR.pack(0x40, 1, 2, 0, 3, b'\0' * 8)
        result = probe.FSXATTR.unpack(probe.prospective_project_assignment(original))
        self.assertEqual(result, (0x40 | probe.FS_XFLAG_PROJINHERIT, 1, 2, bound.PROJECT_ID, 3, b'\0' * 8))
        for bad in (b'', probe.FSXATTR.pack(0, 0, 0, 531, 0, b'\0' * 8),
                    probe.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0, 0, 0, b'\0' * 8),
                    probe.FSXATTR.pack(0, 0, 0, 0, 0, b'x' * 8)):
            with self.subTest(bad=bad), self.assertRaises(probe.Refused):
                probe.prospective_project_assignment(bad)

    def test_command_entry_refuses_setter_before_kernel_call(self):
        with patch.object(probe, '_quotactl') as call, patch.object(sys, 'stderr'):
            self.assertEqual(probe.main(['set', bound.ROOT_DEVICE, str(bound.PROJECT_ID)]), 2)
            call.assert_not_called()

    def test_prospective_limit_reserves_exact_row_and_changes_only_block_limits(self):
        record = probe.limit_record(CHARGED, LIMIT)
        self.assertEqual(record.bhard * probe.QB_UNIT_BYTES, CHARGED + LIMIT)
        self.assertEqual(record.valid, bound.QIF_BLIMITS)
        self.assertEqual(record.bsoft, 0)
        for charged, row in ((0, 0), (-1, LIMIT), (CHARGED, True), (1, LIMIT),
                             (1 << 90, LIMIT)):
            with self.subTest(charged=charged, row=row), self.assertRaises(probe.Refused):
                probe.limit_record(charged, row)

    def test_quota_lines_feed_the_verifier_in_kernel_units(self):
        lines = probe.quota_lines('/dev/sda2', 531, call=self.fake_call(4, HARD // 1024),
                                  read_boot=lambda: BOOT, stat_device=lambda p: type('S', (), {'st_rdev': os.makedev(8, 2)})())
        text = '\n'.join(lines) + '\n'
        self.assertIn('bhard_qb=%d' % (HARD // 1024), text)
        self.assertIn('qb_unit_bytes=1024', text)
        self.assertIn('dev=8:2', text)
        witness = replace_original(render(), 'quota', text=text)
        self.assertEqual(bound.verify(witness, LIMIT, facts(), {MECH})['charged_bytes'], CHARGED)

    def test_kernel_refusal_is_printed_not_turned_into_a_zero_limit(self):
        lines = probe.quota_lines('/dev/sda2', 531, call=self.fake_call(0, 0, rc_quota=-1, errno=3),
                                  read_boot=lambda: BOOT, stat_device=lambda p: type('S', (), {'st_rdev': os.makedev(8, 2)})())
        self.assertIn('getquota_rc=-1 getquota_errno=3', lines)
        self.assertFalse([line for line in lines if line.startswith('bhard_qb')])

    def test_tree_walk_is_lstat_only_and_reports_links_and_every_entry(self):
        root = self.dir / 'stage'
        (root / 'inputs').mkdir(parents=True)
        (root / 'inputs' / 'archive.tar').write_bytes(b'x' * 10)
        os.link(root / 'inputs' / 'archive.tar', root / 'inputs' / 'alias.tar')
        os.symlink(root / 'inputs' / 'archive.tar', root / 'link')
        lines = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: (531, probe.FS_XFLAG_PROJINHERIT))
        entries = {ln.split()[1].split('=')[1]: ln for ln in lines if ln.startswith('entry ')}
        self.assertEqual(set(entries), {'.', 'inputs', 'inputs/archive.tar', 'inputs/alias.tar', 'link'})
        self.assertIn('type=lnk', entries['link'])
        self.assertIn('nlink=2', entries['inputs/archive.tar'])
        self.assertIn('projid=531', entries['inputs/archive.tar'])

    def test_tree_refuses_unsafe_names_and_read_failures(self):
        root = self.dir / 'unsafe'
        root.mkdir()
        (root / 'has space').write_bytes(b'')
        with self.assertRaisesRegex(probe.Refused, 'unsafe-name'):
            probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: (531, probe.FS_XFLAG_PROJINHERIT))
        clean = self.dir / 'clean'
        clean.mkdir()
        (clean / 'file').write_bytes(b'')

        def failing(fd):
            raise OSError(13, 'denied')
        with self.assertRaisesRegex(probe.Refused, 'fsgetxattr-'):
            probe.tree_lines(str(clean), 531, read_boot=lambda: BOOT, read_project=failing)

    def test_end_to_end_local_fixture_passes_only_with_matching_originals(self):
        root = self.dir / 'e2e'
        (root / 'inputs').mkdir(parents=True)
        (root / 'inputs' / 'archive.tar').write_bytes(b'y' * 4096)
        tree = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: (531, probe.FS_XFLAG_PROJINHERIT))
        fields = [dict(tok.split('=', 1) for tok in ln.split()[1:]) for ln in tree if ln.startswith('entry ')]
        entries = tuple((f['path'], f['type'], f['dev'], int(f['uid']), int(f['projid']), int(f['nlink']),
                         int(f['blocks512'])) for f in fields)
        mount_dev = fields[0]['dev']
        witness = render(entries=entries, tree_root=str(root), mount_dev=mount_dev, quota_dev=mount_dev)
        witness = replace_original(witness, 'tree', text='\n'.join(tree) + '\n')
        fx = facts(uid=os.getuid(), root=str(root), entries={f['path'] for f in fields})
        self.assertEqual(bound.verify(witness, LIMIT, fx, {MECH})['mechanism'], MECH)


class FixedPathChecks(unittest.TestCase):
    """The retained installed entry and verifier agree on each fixed path and reservation."""

    def test_fixed_paths_and_the_reservation_table_cannot_drift(self):
        self.assertEqual(bound.ROW_RESERVATIONS, supervisor.QUOTA_ROWS)
        self.assertEqual(bound.QUOTA_ROOT, supervisor.QUOTA_ROOT)
        self.assertEqual(bound.PROJECT_ID, supervisor.QUOTA_PROJECT)
        self.assertEqual(bound.QB_UNIT_BYTES, supervisor.QB_UNIT_BYTES)
        self.assertEqual(bound.QUOTA_ROOT_PROBE,
                         supervisor.QUOTA_ROOT + '/' + supervisor.QUOTA_PROBE_NAME)
        self.assertEqual(bound._forced_command(), supervisor.forced_command())


class QuotaAdminChecks(unittest.TestCase):
    """The fixed root quota-administration role, with every kernel/syscall boundary injected."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='quota-admin-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.root, True)
        (self.root / 'inputs').mkdir()
        (self.root / 'inputs' / 'archive.tar').write_bytes(b'x' * 4096)
        self.boot = '12345678-1234-1234-1234-123456789abc'
        patcher = patch.object(supervisor, 'QUOTA_ROOT', str(self.root))
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(bound, 'QUOTA_ROOT_PROBE', str(self.root) + '/' + supervisor.QUOTA_PROBE_NAME)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(supervisor, '_project_attributes',
                               lambda fd: (bound.PROJECT_ID, probe.FS_XFLAG_PROJINHERIT))
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(supervisor, 'device_numbers', lambda device: '8:2')
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(supervisor, 'device_pair', lambda info: (8, 2))
        patcher.start()
        self.addCleanup(patcher.stop)

    def state(self, **over):
        state = {'hard': 0, 'cur': census_of(self.root), 'fmt': 4, 'rc_fmt': 0, 'rc_get': 0,
                 'rc_set': 0, 'probe': 'edquot'}
        state.update(over)
        return state

    def ops(self, state, writes=None):
        calls = []

        def runner(argv, timeout):
            calls.append(list(argv))
            if argv[-1] == '/proc/self/mountinfo':
                text = b'36 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw\n'
            elif argv[0].endswith('readlink'):
                text = b'../../sda2\n'
            else:
                text = ('Filesystem UUID:          d488be7b-682f-41ac-b79b-57bbf495a01d\n'
                        'Filesystem features:      has_journal orphan_file project quota\n').encode()
            return {'returncode': 0, 'stdout': text, 'stderr': b'', 'timed_out': False}

        def quotactl(cmd, device, project, addr):
            calls.append((cmd, device, project))
            block = addr._obj
            if cmd == probe.Q_SETQUOTA:
                if state['rc_set'] != 0:
                    return state['rc_set'], 122
                state['hard'] = block.bhard * probe.QB_UNIT_BYTES
                if writes is not None:
                    writes.append(block.bhard)
                return 0, 0
            if cmd == probe.Q_GETFMT:
                addr._obj.value = state['fmt']
                return state['rc_fmt'], 0
            block.bhard = state['hard'] // probe.QB_UNIT_BYTES
            block.bsoft = 0
            block.cur = state['cur'] // probe.QB_UNIT_BYTES
            block.valid = bound.QIF_BLIMITS | bound.QIF_SPACE
            return state['rc_get'], 0

        def enforce(root, name, reservation, uid, gid):
            calls.append(('enforce', name, reservation))
            if state['probe'] == 'edquot':
                return {'argv': ['--enforce', root + '/' + name, str(reservation)], 'exit': 3,
                        'bytes_allocated': reservation, 'phase': 'edquot',
                        'reservation_bytes': reservation, 'stdout': '',
                        'stderr': 'status=refused phase=edquot bytes=%d\n' % reservation,
                        'stdout_sha256': hashlib.sha256(b'').hexdigest(),
                        'stderr_sha256': hashlib.sha256(
                            ('status=refused phase=edquot bytes=%d\n' % reservation).encode()).hexdigest()}
            return {'argv': ['--enforce', root + '/' + name, str(reservation)], 'exit': 5,
                    'bytes_allocated': reservation, 'phase': 'no-edquot',
                    'reservation_bytes': reservation, 'stdout': '',
                    'stderr': 'status=failed phase=no-edquot\n',
                    'stdout_sha256': hashlib.sha256(b'').hexdigest(),
                    'stderr_sha256': hashlib.sha256(b'status=failed phase=no-edquot\n').hexdigest()}

        ops = supervisor.QuotaOps(runner=runner, quotactl=quotactl, probe=enforce)
        ops.account = lambda: (UID, UID)
        return ops, calls

    def request(self, row='E9a'):
        return {'row': row, 'nonce': NONCE, 'boot_id': self.boot}

    def test_the_fixed_role_sets_the_exact_row_headroom_and_emits_an_admissible_witness(self):
        ops, calls = self.ops(self.state())
        witness = supervisor.quota_admin(self.request(), ops, 'f' * 64)
        self.assertEqual(witness['row'], 'E9a')
        self.assertEqual(witness['limit_bytes'], bound.ROW_RESERVATIONS['E9a'])
        self.assertEqual(witness['charged_bytes'], census_of(self.root))
        self.assertEqual(witness['enforcement']['phase'], 'edquot')
        self.assertEqual(set(witness['observations']), set(bound.ROUTES))
        self.assertEqual((probe.Q_SETQUOTA, '/dev/sda2', 531) in calls, True)
        fx = facts(boot_id=self.boot, root=str(self.root), uid=os.getuid(),
                   entries={'.', 'inputs', 'inputs/archive.tar'})
        result = bound.verify(witness, bound.ROW_RESERVATIONS['E9a'], fx, {MECH})
        self.assertEqual(result['headroom_bytes'], bound.ROW_RESERVATIONS['E9a'])

    def test_the_role_refuses_unknown_row_or_boot_and_a_failed_setter(self):
        ops, calls = self.ops(self.state())
        for request in (self.request('E19.0'), self.request('foreign'),
                        dict(self.request(), boot_id='not-a-boot'),
                        dict(self.request(), device='/dev/sdb2'),
                        dict(self.request(), limit=1 << 20), {'row': 'E9a'}):
            with self.subTest(request=request), self.assertRaises(Refusal):
                supervisor.quota_admin(request, ops, 'f' * 64)
        self.assertEqual(calls, [])
        failing, calls = self.ops(self.state(rc_set=5))
        with self.assertRaisesRegex(Refusal, 'setter failed'):
            supervisor.quota_admin(self.request(), failing, 'f' * 64)
        self.assertEqual([c for c in calls if c == 'enforce'], [])

    def test_a_concurrent_charge_or_a_missing_edquot_never_credit_the_row(self):
        ops, _ = self.ops(self.state())
        inner = ops._quotactl
        seen = {'gets': 0}

        def moving_quota(cmd, device, project, addr):
            result = inner(cmd, device, project, addr)
            if cmd == probe.Q_GETQUOTA:
                seen['gets'] += 1
                if seen['gets'] >= 2:
                    addr._obj.cur = (census_of(self.root) + 4096) // probe.QB_UNIT_BYTES
            return result
        ops._quotactl = moving_quota
        with self.assertRaisesRegex(Refusal, 'charge moved|headroom is not exactly'):
            supervisor.quota_admin(self.request(), ops, 'f' * 64)
        refused, _ = self.ops(self.state(probe='no-edquot'))
        with self.assertRaisesRegex(Refusal, 'not refused with EDQUOT|row ceiling'):
            supervisor.quota_admin(self.request(), refused, 'f' * 64)

    def test_an_interrupted_probe_is_not_an_enforcement_observation(self):
        for detail in ({'phase': 'no-edquot', 'bytes': bound.ROW_RESERVATIONS['E9a']},
                       {'phase': 'edquot', 'bytes': bound.ROW_RESERVATIONS['E9a'] - 1},
                       {'phase': 'edquot'}, None):
            with self.subTest(detail=detail), self.assertRaises(Refusal):
                supervisor.enforcement_probe(str(self.root), supervisor.QUOTA_PROBE_NAME,
                                             bound.ROW_RESERVATIONS['E9a'], UID, UID,
                                             writer=lambda reservation, d=detail: d)
        with self.assertRaisesRegex(Refusal, 'test seams are refused for root runs'), \
                patch.object(supervisor.os, 'geteuid', return_value=0):
            supervisor.enforcement_probe(str(self.root), supervisor.QUOTA_PROBE_NAME, 4096, 0, 0,
                                         writer=lambda reservation: {'phase': 'edquot', 'bytes': reservation})

    def test_the_first_quota_read_requires_the_exact_census_in_both_directions(self):
        census = census_of(self.root)
        writes = []
        under, under_calls = self.ops(self.state(cur=census - 4096), writes=writes)
        with self.assertRaisesRegex(Refusal, 'below the tree bytes; accounting does not cover the tree'):
            supervisor.quota_admin(self.request(), under, 'f' * 64)
        over, _ = self.ops(self.state(cur=census + 4096))
        with self.assertRaisesRegex(Refusal, 'exceeds the owned tree census'):
            supervisor.quota_admin(self.request(), over, 'f' * 64)
        self.assertEqual([c for c in under_calls if c[0] == probe.Q_SETQUOTA], [],
                         'an inexact first read must refuse before any limit is set')

    def test_the_post_enforcement_reread_requires_the_exact_census_too(self):
        census = census_of(self.root)

        def growing_probe(root, name, reservation, uid, gid):
            # The probe must leave the tree exactly as it found it; here it legitimately extends an
            # existing entry instead, so the entry set is unchanged but the census is not.
            with open(Path(root) / 'inputs' / 'archive.tar', 'ab') as handle:
                handle.write(b'z' * 4096)
            return {'argv': ['--enforce', str(Path(root) / name), str(reservation)], 'exit': 3,
                    'bytes_allocated': reservation, 'phase': 'edquot', 'reservation_bytes': reservation,
                    'stdout': '', 'stderr': 'status=refused phase=edquot bytes=%d\n' % reservation,
                    'stdout_sha256': hashlib.sha256(b'').hexdigest(),
                    'stderr_sha256': hashlib.sha256(
                        ('status=refused phase=edquot bytes=%d\n' % reservation).encode()).hexdigest()}
        ops, _ = self.ops(self.state())
        ops._probe = growing_probe
        with self.assertRaisesRegex(Refusal, 'post-enforcement kernel charge'):
            supervisor.quota_admin(self.request(), ops, 'f' * 64)
        self.assertGreater(census_of(self.root), census, 'the probe extended the tree in this control')

    def test_e7c_legitimate_nonzero_charge_passes_the_exact_first_row_equality(self):
        # After the genuine E7p zero anchor, E7c creates the eleven directories under BASE; they
        # allocate before the first quota set, so the census is nonzero and must still be exact.
        for name in HISTORICAL_BASE_DIRS:
            (self.root / name).mkdir(mode=0o700, exist_ok=True)
        census = census_of(self.root)
        self.assertGreater(census, 0)
        ops, _ = self.ops(self.state(cur=census))
        witness = supervisor.quota_admin(self.request(), ops, 'f' * 64)
        self.assertEqual(witness['charged_bytes'], census)
        entries = {'.', 'inputs', 'inputs/archive.tar'} | set(HISTORICAL_BASE_DIRS)
        fx = facts(boot_id=self.boot, root=str(self.root), uid=os.getuid(), entries=entries)
        admitted = bound.verify(witness, bound.ROW_RESERVATIONS['E9a'], fx, {MECH})
        self.assertEqual(admitted['tree_bytes'], census)
        texts = witness['observations']
        limit = bound.ROW_RESERVATIONS['E9a']
        for drift in (census - 1024, census + 1024):   # KB-aligned: keeps the headroom exact
            # Keep the exact row headroom while drifting the charge, so the census equality is the
            # gate that refuses (not the headroom arithmetic).
            quota = '\n'.join(
                line.replace('curspace_bytes=%d' % census, 'curspace_bytes=%d' % drift)
                    .replace('bhard_qb=%d' % ((census + limit) // 1024),
                             'bhard_qb=%d' % ((drift + limit) // 1024))
                for line in texts['quota']['text'].splitlines()) + '\n'
            sha = hashlib.sha256(quota.encode('utf-8')).hexdigest()
            tampered = dict(witness, observations={**texts, 'quota': dict(texts['quota'], text=quota,
                                                                         sha256=sha)})
            with self.subTest(drift=drift), self.assertRaisesRegex(Refusal, 'tree census|below the tree bytes'):
                bound.verify(tampered, limit, fx, {MECH})


class InventoryChecks(unittest.TestCase):
    def test_allocation_modules_are_in_the_request_closure(self):
        import preparation_requests as requests
        for name in ('linux_allocation_bound.py', 'linux_allocation_probe.py'):
            with self.subTest(name=name):
                self.assertIn(name, requests.REQUIRED_MODULE_NAMES)


class ProjectAssignmentChecks(unittest.TestCase):
    """The fixed project-assignment role, run through the uninjected production dispatcher.

    Only the kernel and transport boundaries are faked: the subprocess runner, quotactl and the two
    fsxattr ioctls. Approval consumption, request validation, the role body, the retained witness, its
    admission and the plan's expectation are the production code paths.
    """

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='project-assign-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.root, True)
        self.base = self.root / 'BASE'
        self.base.mkdir()
        self.approvals = self.root / 'approvals'
        self.approvals.mkdir(mode=0o700)
        self.attributes = {}
        self.sets = []
        info = self.base.stat()
        self.device = '%d:%d' % (os.major(info.st_dev), os.minor(info.st_dev))
        self.mount_source = bound.ROOT_DEVICE
        self.features = 'has_journal ext_attr 64bit project quota'
        self.quota = {'fmt': 4, 'fmt_rc': 0, 'rc': 0, 'cur': 0, 'curinodes': 0,
                      'bhard': 0, 'bsoft': 0, 'ihard': 0, 'isoft': 0}
        self.during_set = None
        patchers = [patch.object(supervisor, 'PROJECT_ROOT', str(self.base)),
                    patch.object(bound, 'PROJECT_ROOT', str(self.base)),
                    patch.object(supervisor, '_subprocess_runner', self.runner),
                    patch.object(supervisor.QuotaOps, 'read', lambda ops_self, path: BOOT_ID + '\n'),
                    patch.object(supervisor.QuotaOps, 'quotactl', self.quotactl),
                    patch.object(supervisor.fcntl, 'ioctl', self.ioctl)]
        for patcher in patchers:
            patcher.start()
            self.addCleanup(patcher.stop)

    def runner(self, argv, timeout):
        if argv == ['/bin/cat', '/proc/self/mountinfo']:
            text = '36 1 %s / / rw,relatime shared:1 - ext4 %s rw\n' % (self.device, self.mount_source)
        elif argv == ['/usr/sbin/dumpe2fs', '-h', bound.ROOT_DEVICE]:
            text = 'Filesystem UUID:          %s\nFilesystem features:      %s\n' % (bound.ROOT_UUID, self.features)
        elif argv == ['/usr/bin/readlink', '/dev/disk/by-uuid/' + bound.ROOT_UUID]:
            text = self.mount_source + '\n'
        else:
            raise AssertionError('unexpected fixed route: %r' % (argv,))
        return {'returncode': 0, 'stdout': text.encode(), 'stderr': b''}

    def quotactl(self, cmd, device, project, addr):
        self.assertEqual(device, bound.ROOT_DEVICE)
        self.assertEqual(project, bound.PROJECT_ID)
        if cmd == supervisor.Q_GETFMT:
            addr._obj.value = self.quota['fmt']
            return self.quota['fmt_rc'], 0
        record = addr._obj
        for name in ('bhard', 'bsoft', 'cur', 'ihard', 'isoft', 'curinodes'):
            setattr(record, name, self.quota[name])
        record.valid = 3
        return self.quota['rc'], 0

    def ioctl(self, fd, request, data):
        key = os.fstat(fd).st_ino
        if request == supervisor.FS_IOC_FSGETXATTR:
            data[:] = self.attributes.get(key, supervisor.FSXATTR.pack(0, 0, 0, 0, 0, bytes(8)))
            return 0
        if request == supervisor.FS_IOC_FSSETXATTR:
            if self.during_set is not None:
                self.during_set()
            self.sets.append(key)
            self.attributes[key] = bytes(data)
            return 0
        raise AssertionError('unexpected ioctl %#x' % request)

    def dispatch(self, request=None):
        body = json.dumps({'nonce': os.urandom(32).hex(), 'boot_id': BOOT_ID} if request is None else request,
                          sort_keys=True)
        digest = hashlib.sha256(body.encode()).hexdigest()
        record = self.approvals / (digest + '.json')
        record.write_text(body)
        record.chmod(0o600)
        out, err = io.StringIO(), io.StringIO()
        with patch.dict(os.environ, {'EFFIGY_APPROVAL_ROOT': str(self.approvals)}), \
                redirect_stdout(out), redirect_stderr(err):
            code = supervisor.main(['--approved-project', digest])
        return code, out.getvalue(), err.getvalue(), digest

    def witness(self):
        code, out, err, digest = self.dispatch()
        self.assertEqual(code, 0, err)
        return json.loads(out.strip().splitlines()[-1]), digest

    def test_the_uninjected_dispatcher_assigns_only_the_fixed_project(self):
        witness, digest = self.witness()
        admitted = bound.verify_project_assignment(
            witness, {'boot_id': BOOT_ID, 'root': str(self.base), 'device': bound.ROOT_DEVICE,
                      'uid': os.geteuid()})
        self.assertTrue(admitted['assigned'])
        self.assertEqual(admitted['approval_digest'], digest)
        self.assertEqual(self.sets, list(self.attributes))
        flags, extent, count, project, cowextent, padding = supervisor.FSXATTR.unpack(
            self.attributes[self.sets[0]])
        self.assertEqual(project, bound.PROJECT_ID)
        self.assertEqual(flags, probe.FS_XFLAG_PROJINHERIT)
        self.assertEqual((extent, count, cowextent, padding), (0, 0, 0, bytes(8)))
        self.assertEqual(os.listdir(self.base), [], 'the assignment must not create an object')

    def test_the_role_refuses_a_nonempty_base_and_leaves_it_unchanged(self):
        (self.base / 'inputs').mkdir()
        code, _out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('not empty', err)
        self.assertEqual(self.sets, [])

    def test_an_already_assigned_or_inheriting_prestate_refuses(self):
        for original in (supervisor.FSXATTR.pack(0, 0, 0, bound.PROJECT_ID, 0, bytes(8)),
                         supervisor.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0, 0, 0, bytes(8))):
            with self.subTest(original=original.hex()):
                self.attributes.clear()
                self.sets = []
                self.attributes[self.base.stat().st_ino] = original
                code, _out, err, _digest = self.dispatch()
                self.assertEqual(code, 3)
                self.assertIn('empty pre-state', err)
                self.assertEqual(self.sets, [])

    def test_an_unavailable_or_charged_project_record_refuses_before_the_set(self):
        for change, message in (({'rc': -1}, 'project record'), ({'cur': 4096}, 'already charged'),
                                ({'bhard': 8}, 'already carries a quota limit'),
                                ({'fmt': 0}, 'quota format is not active')):
            with self.subTest(change=change):
                self.quota = {'fmt': 4, 'fmt_rc': 0, 'rc': 0, 'cur': 0, 'curinodes': 0,
                              'bhard': 0, 'bsoft': 0, 'ihard': 0, 'isoft': 0}
                self.quota.update(change)
                self.attributes.clear()
                self.sets = []
                code, _out, err, _digest = self.dispatch()
                self.assertEqual(code, 3)
                self.assertIn(message, err)
                self.assertEqual(self.sets, [], 'no attribute may change without the unused proof')

    def test_a_partial_ioctl_refuses_and_keeps_no_witness(self):
        def refuse(fd, request, data):
            self.assertEqual(request, supervisor.FS_IOC_FSSETXATTR)
            raise OSError(25, 'inappropriate ioctl')
        self.attributes.clear()
        with patch.object(supervisor.fcntl, 'ioctl',
                          lambda fd, request, data: refuse(fd, request, data) if
                          request == supervisor.FS_IOC_FSSETXATTR else
                          self.ioctl(fd, request, data)):
            code, out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('inappropriate ioctl', err)
        self.assertEqual(out, '')

    def test_a_replaced_base_descriptor_is_refused(self):
        def replace():
            os.rename(str(self.base), str(self.root / 'BASE-moved'))
            self.base.mkdir()
        self.during_set = replace
        code, _out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('replaced, aliased or recreated', err)

    def test_wrong_boot_device_or_medium_refuses_before_the_set(self):
        code, _out, err, _digest = self.dispatch({'nonce': os.urandom(32).hex(), 'boot_id': OTHER_BOOT})
        self.assertEqual(code, 3)
        self.assertIn('boot identity differs', err)
        self.mount_source = '/dev/sda1'
        code, _out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('not the native ext4 device', err)
        self.mount_source = bound.ROOT_DEVICE
        self.features = 'has_journal ext_attr 64bit'
        code, _out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('offline activation has not happened', err)
        self.assertEqual(self.sets, [])

    def test_a_foreign_or_aliased_root_refuses(self):
        alias = self.root / 'alias'
        alias.symlink_to(self.base)
        with patch.object(supervisor, 'PROJECT_ROOT', str(alias)), \
                patch.object(bound, 'PROJECT_ROOT', str(alias)):
            code, _out, err, _digest = self.dispatch()
        self.assertEqual(code, 3)
        self.assertIn('not canonical', err)

    def test_the_request_cannot_name_a_path_project_device_or_limit(self):
        code, _out, err, _digest = self.dispatch({'nonce': os.urandom(32).hex(), 'boot_id': BOOT_ID,
                                                  'project': 77, 'device': '/dev/sdb1'})
        self.assertEqual(code, 3)
        self.assertIn('only a nonce and boot identity', err)
        self.assertEqual(self.sets, [])

    def test_the_role_requires_its_arguments_and_a_usable_kernel_abi(self):
        request = {'nonce': os.urandom(32).hex(), 'boot_id': BOOT_ID}
        signature = inspect.signature(supervisor.project_assign)
        for name in ('request', 'ops', 'approval_digest'):
            with self.subTest(name=name):
                self.assertIs(signature.parameters[name].default, inspect.Parameter.empty)
        with self.assertRaises(TypeError):
            supervisor.project_assign(request, supervisor.QuotaOps())
        verifier = inspect.signature(bound.verify_project_assignment)
        self.assertEqual(set(verifier.parameters), {'witness', 'facts'})
        for name in ('witness', 'facts'):
            self.assertIs(verifier.parameters[name].default, inspect.Parameter.empty)
        self.assertEqual(bound.fsxattr_abi(), supervisor.fsxattr_abi())
        with patch.object(supervisor, 'FSXATTR', struct.Struct('=4I8s')), \
                self.assertRaises(Refusal):
            supervisor.fsxattr_abi()
        self.assertEqual(supervisor.FS_IOC_FSGETXATTR, 0x801C581F)
        self.assertEqual(supervisor.FS_IOC_FSSETXATTR, 0x401C5820)

    def test_the_witness_is_refused_when_any_fact_drifts(self):
        witness, _digest = self.witness()
        facts = {'boot_id': BOOT_ID, 'root': str(self.base), 'device': bound.ROOT_DEVICE,
                 'uid': os.geteuid()}
        bound.verify_project_assignment(witness, facts)
        cases = (
            ('assigned', False, {'assigned': False}),
            ('device', '/dev/sda1', {'device': '/dev/sda1'}),
            ('project', 7, {'project': 7}),
            ('root', '/tmp', {'root': '/tmp'}),
            ('nonce', 'x', {'nonce': 'x'}),
            ('abi', {'struct_size': 24, 'get': 1, 'set': 2}, {'abi': {'struct_size': 24, 'get': 1, 'set': 2}}),
            ('custody', {}, {'custody': {}}),
            ('setter', {'ioctl': 'FS_IOC_FSSETXATTR', 'count': 2, 'exit': 0},
             {'setter': {'ioctl': 'FS_IOC_FSSETXATTR', 'count': 2, 'exit': 0}}),
            ('setter-exit', {'ioctl': 'FS_IOC_FSSETXATTR', 'count': 1, 'exit': 9},
             {'setter': {'ioctl': 'FS_IOC_FSSETXATTR', 'count': 1, 'exit': 9}}),
            ('base-entries', ['inputs'], {'base': dict(witness['base'], entries=['inputs'])}),
            ('base-uid', 1001, {'base': dict(witness['base'], uid=1001)}),
            ('facts-uid', 0, {'uid': 0}),
            ('base-mode', 0o777, {'base': dict(witness['base'], mode=0o777)}),
            ('before', supervisor.FSXATTR.pack(0, 4, 0, 0, 0, bytes(8)).hex(),
             {'attributes': {'before': supervisor.FSXATTR.pack(0, 4, 0, 0, 0, bytes(8)).hex(),
                             'after': witness['attributes']['after']}}),
            ('after', supervisor.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0, 531, 4, bytes(8)).hex(),
             {'attributes': {'before': witness['attributes']['before'],
                             'after': supervisor.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0, 531, 4,
                                                              bytes(8)).hex()}}),
            ('extra', 1, {'extra': 1}),
        )
        for label, _value, change in cases:
            with self.subTest(case=label):
                drifted = dict(witness, **change)
                with self.assertRaises(Refusal):
                    bound.verify_project_assignment(drifted, facts)
        for label, witness_change in (
                ('foreign-boot', {'boot_id': OTHER_BOOT}),
                ('charged-project', {'observations': replace_original(
                    witness, 'project', text=witness['observations']['project']['text']
                    .replace('curspace_bytes=0', 'curspace_bytes=4096'))['observations']}),
                ('missing-feature', {'observations': replace_original(
                    witness, 'dumpe2fs', text=witness['observations']['dumpe2fs']['text']
                    .replace('project quota', ''))['observations']}),
                ('foreign-device-mount', {'observations': replace_original(
                    witness, 'mountinfo', text=witness['observations']['mountinfo']['text']
                    .replace('ext4 /dev/sda2', 'ext4 /dev/sdb2'))['observations']})):
            with self.subTest(case=label):
                with self.assertRaises(Refusal):
                    bound.verify_project_assignment(dict(witness, **witness_change), facts)
        with self.assertRaises(Refusal):
            bound.verify_project_assignment(witness, {'boot_id': BOOT_ID, 'root': str(self.base),
                                                       'device': '/dev/sda1', 'uid': os.geteuid()})


BOOT_ID = '11111111-2222-3333-4444-555555555555'
OTHER_BOOT = '99999999-8888-7777-6666-555555555555'
RESCUE_BOOT = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'


def activation_records(count=None):
    """A complete, exact activation sequence for the fixed boundaries (fixture facts)."""
    records = []
    for index, step in enumerate(bound.activation_plan()[:count]):
        name = step['name']
        record = {'name': name, 'disposition': 'exited', 'waited_exit': 0,
                  'boot_id': None if name == 'snapshot' else (RESCUE_BOOT if name in bound.RESCUE_BOUNDARIES else (BOOT_ID if name == 'quiesce' else OTHER_BOOT))}
        if name == 'quiesce':
            record.update(rows_idle=True, writers_stopped=True, vm_id=bound.ACTIVATION_VM, preboot_id=BOOT_ID)
        elif name == 'snapshot':
            record.update(snapshot_id='{4d475daf-8aa1-497a-9a38-355f21fcd867}', space_bytes=64 << 30,
                          quiescent=True, vm_id=bound.ACTIVATION_VM, preboot_id=BOOT_ID)
        elif name == 'rescue-boot':
            record.update(medium_sha256=bound.ACTIVATION_MEDIUM['iso_sha256'],
                          tools={'tune2fs': bound.OFFLINE_TOOL_VERSION, 'e2fsck': bound.OFFLINE_TOOL_VERSION},
                          root_unmounted=True)
        elif name in ('precheck', 'features'):
            record.update(root_unmounted=True)
        elif name == 'mount-options':
            record.update(root_unmounted=True, stored_options_empty=True)
        elif name == 'postcheck':
            record.update(root_unmounted=True, features_active=True)
        elif name == 'return-boot':
            record.update(root_mounted_rw=True)
        elif name == 'project-assign':
            record.update(assigned=True)
        elif name == 'quota':
            record.update(limit_set=True)
        elif name == 'enforce':
            record.update(phase='edquot')
        records.append(record)
    return records


class ActivationRouteChecks(unittest.TestCase):
    """The fixed activation sequence, its boundaries and its refuse-don't-resume rules."""

    def test_the_plan_composes_the_accepted_offline_argv_without_running_them(self):
        plan = bound.activation_plan()
        self.assertEqual([step['name'] for step in plan], [name for name, _k, _r in bound.ACTIVATION_BOUNDARIES])
        offline = dict(bound.offline_operation_plan())
        for step in plan:
            if step['kind'] == 'offline':
                self.assertEqual(step['argv'], offline[step['name']])
            else:
                self.assertIsNone(step['argv'])
        self.assertEqual([step['name'] for step in plan if step['argv']],
                         ['precheck', 'features', 'mount-options', 'postcheck'])

    def test_the_recorded_medium_ships_the_source_qualified_toolset(self):
        self.assertEqual(bound.ACTIVATION_MEDIUM['e2fsprogs'].split('-')[0], bound.OFFLINE_TOOL_VERSION)
        self.assertEqual(len(bound.ACTIVATION_MEDIUM['iso_sha256']), 64)
        self.assertEqual(bound.ACTIVATION_MEDIUM['url'], rescue.URL)
        self.assertGreater(bound.ACTIVATION_MEDIUM['iso_bytes'], 2 << 30)

    def test_a_complete_sequence_is_accepted_and_names_the_next_boundary(self):
        records = activation_records()
        result = bound.inspect_activation_progress(records)
        self.assertIsNone(result['next'])
        self.assertEqual(result['completed'], [step['name'] for step in bound.activation_plan()])
        self.assertEqual(result['snapshot_id'], records[1]['snapshot_id'])
        for count in range(1, len(records)):
            with self.subTest(count=count):
                partial = bound.inspect_activation_progress(records[:count])
                self.assertEqual(partial['next'], bound.activation_plan()[count]['name'])

    def test_an_interrupted_boundary_is_unknown_and_never_resumes(self):
        records = activation_records()
        for index, step in enumerate(records):
            for disposition in ('unknown', 'refused', 'timed-out'):
                with self.subTest(boundary=step['name'], disposition=disposition):
                    broken = [dict(item) for item in records[:index + 1]]
                    broken[index]['disposition'] = disposition
                    with self.assertRaises(Refusal):
                        bound.inspect_activation_progress(broken)

    def test_every_boundary_must_carry_a_zero_waited_exit(self):
        records = activation_records()
        for index, step in enumerate(records):
            for broken_exit in (1, 3, 137, -1, None, '0', 0.0, True):
                with self.subTest(boundary=step['name'], waited_exit=broken_exit):
                    broken = [dict(item) for item in records[:index + 1]]
                    broken[index]['waited_exit'] = broken_exit
                    with self.assertRaisesRegex(Refusal, 'positive waited exit'):
                        bound.inspect_activation_progress(broken)
            with self.subTest(boundary=step['name'], waited_exit='missing'):
                broken = [dict(item) for item in records[:index + 1]]
                del broken[index]['waited_exit']
                with self.assertRaisesRegex(Refusal, 'missing or unexpected facts'):
                    bound.inspect_activation_progress(broken)

    def test_the_offline_steps_need_a_zero_exit_not_only_an_exited_label(self):
        records = activation_records()
        offline = [step['name'] for step in bound.activation_plan() if step['kind'] == 'offline']
        self.assertEqual(offline, ['precheck', 'features', 'mount-options', 'postcheck'])
        for name in offline:
            with self.subTest(boundary=name):
                broken = [dict(item) for item in records]
                target = next(item for item in broken if item['name'] == name)
                target['waited_exit'] = 1
                with self.assertRaisesRegex(Refusal, name + ' has no positive waited exit'):
                    bound.inspect_activation_progress(broken)
        # A nonzero exit before the offline steps is refused at that boundary, not carried forward.
        for index, step in enumerate(records):
            with self.subTest(boundary=step['name'], prefix=index + 1):
                broken = [dict(item) for item in records[:index + 1]]
                broken[index]['waited_exit'] = 1
                with self.assertRaisesRegex(Refusal, 'nothing resumes or restores'):
                    bound.inspect_activation_progress(broken)

    def test_a_repeated_or_unplanned_boundary_refuses(self):
        records = activation_records()
        repeated = records[:4] + [dict(records[3])]
        with self.assertRaises(Refusal):
            bound.inspect_activation_progress(repeated)
        restore = dict(records[-1], name='restore')
        with self.assertRaises(Refusal):
            bound.inspect_activation_progress(records[:-1] + [restore])

    def test_wrong_boot_tools_options_or_snapshot_refuses(self):
        cases = (
            ('snapshot-id', 'snapshot', {'snapshot_id': 'latest'}),
            ('snapshot-space', 'snapshot', {'space_bytes': 0}),
            ('snapshot-quiescent', 'snapshot', {'quiescent': False}),
            ('medium', 'rescue-boot', {'medium_sha256': '0' * 64}),
            ('tools', 'rescue-boot', {'tools': {'tune2fs': '1.47.3', 'e2fsck': '1.47.3'}}),
            ('unmounted', 'precheck', {'root_unmounted': False}),
            ('options', 'mount-options', {'stored_options_empty': False}),
            ('features', 'postcheck', {'features_active': False}),
            ('mounted', 'return-boot', {'root_mounted_rw': False}),
            ('assigned', 'project-assign', {'assigned': False}),
            ('limit', 'quota', {'limit_set': False}),
            ('enforce', 'enforce', {'phase': 'ok'}),
            ('offline-boot', 'features', {'boot_id': BOOT_ID}),
            ('returned-boot', 'project-assign', {'boot_id': RESCUE_BOOT}),
        )
        for label, name, change in cases:
            with self.subTest(case=label):
                records = activation_records()
                target = next(item for item in records if item['name'] == name)
                target.update(change)
                with self.assertRaises(Refusal):
                    bound.inspect_activation_progress(records)

    def test_pre_rescue_and_returned_boot_continuity(self):
        for name, changes in (
            ('quiesce', {'vm_id': 'other'}),
            ('quiesce', {'preboot_id': OTHER_BOOT}),
            ('snapshot', {'vm_id': 'other'}),
            ('snapshot', {'preboot_id': OTHER_BOOT}),
            ('snapshot', {'boot_id': BOOT_ID}),
            ('rescue-boot', {'boot_id': BOOT_ID}),
            ('return-boot', {'boot_id': BOOT_ID}),
            ('quota', {'boot_id': BOOT_ID}),
            ('enforce', {'boot_id': BOOT_ID}),
        ):
            with self.subTest(name=name, changes=changes):
                records = activation_records()
                next(item for item in records if item['name'] == name).update(changes)
                with self.assertRaises(Refusal):
                    bound.inspect_activation_progress(records)

    def test_completion_requires_the_full_sequence_originals_and_a_genuine_accounting_witness(self):
        from linux_diagnostic_proof import PRLCTL, PRLCTL_SHA256
        empty = hashlib.sha256(b'').hexdigest()
        originals = [{'name': name, 'argv': argv, 'boot_id': RESCUE_BOOT, 'stdout': '', 'stderr': '',
                      'stdout_sha256': empty, 'stderr_sha256': empty, 'exit': 0}
                     for name, argv in bound.offline_operation_plan()]
        records = activation_records()
        def source(argv, text):
            return {'argv': argv, 'tool_sha256': PRLCTL_SHA256, 'stdout': text, 'stderr': '',
                    'stdout_sha256': hashlib.sha256(text.encode()).hexdigest(),
                    'stderr_sha256': empty, 'exit': 0}
        def vm(state):
            return json.dumps([{'ID': bound.ACTIVATION_VM, 'State': state,
                                'Home': PVM_HOME + '/', 'Home path': PVM_HOME + '/config.pvs',
                                'Hardware': {'hdd0': {'image': PVM_HOME + '/harddisk1.hdd',
                                                     'enabled': True}}}])
        list_argv = [PRLCTL, 'list', '--info', bound.ACTIVATION_VM, '--json']
        boot_argv = [PRLCTL, 'exec', bound.ACTIVATION_VM, '/bin/cat', '/proc/sys/kernel/random/boot_id']
        pre = {'running_vm': source(list_argv, vm('running')), 'boot': source(boot_argv, BOOT_ID + '\n'),
               'stopped_vm': source(list_argv, vm('stopped'))}
        # The accounting witness comes from the actual collector over a checksum-correct fixture
        # layout, bound to this activation's rescue boot.
        root = Path(tempfile.mkdtemp(prefix='completion-witness-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, root, True)
        image, end, _tables = build_witness_image(root / 'offline.img')
        good = collect_witness(image, end)
        def completion(recs, off=originals, before=pre, accounting=good):
            payload = {'records': recs, 'offline_originals': off, 'pre_originals': before}
            if accounting is not None:
                payload['accounting'] = accounting
            return bound.inspect_activation_completion(payload)
        result = completion(records)
        self.assertEqual(result['completed'], [step['name'] for step in bound.activation_plan()])
        self.assertEqual(result['accounting']['kind'], witness.MECHANISM_KIND)
        self.assertEqual(result['accounting']['boot_id'], RESCUE_BOOT)
        with self.assertRaisesRegex(Refusal, 'accounting witness'):
            completion(records, accounting=None)
        stale = dict(good, boot_id=OTHER_BOOT)
        with self.assertRaisesRegex(Refusal, 'another boot'):
            completion(records, accounting=stale)
        drifted = dict(good, counts=dict(good['counts'], zero_project=good['counts']['slots_checked'] - 1))
        with self.assertRaisesRegex(Refusal, 'fresh project'):
            completion(records, accounting=drifted)
        for count in range(len(records)):
            with self.subTest(count=count), self.assertRaises(Refusal):
                completion(records[:count])
        for bad in (originals[:-1], [dict(originals[0], argv=['wrong'])] + originals[1:],
                    [dict(originals[0], stdout='changed')] + originals[1:]):
            with self.assertRaises(Refusal):
                completion(records, bad)
        for before in (dict(pre, boot=source(boot_argv, OTHER_BOOT + '\n')),
                       dict(pre, stopped_vm=source(list_argv, vm('running'))),
                       dict(pre, running_vm=dict(pre['running_vm'], tool_sha256='0' * 64))):
            with self.assertRaises(Refusal):
                completion(records, before=before)
        with self.assertRaises(Refusal):
            completion(records + records[:1])

    def test_a_boundary_with_missing_or_extra_facts_refuses(self):
        records = activation_records()
        del records[0]['rows_idle']
        with self.assertRaises(Refusal):
            bound.inspect_activation_progress(records)
        records = activation_records(2)
        records[1]['extra'] = 1
        with self.assertRaises(Refusal):
            bound.inspect_activation_progress(records)
        with self.assertRaises(Refusal):
            bound.inspect_activation_progress(activation_records() + activation_records(1))


class RescueInputChecks(unittest.TestCase):
    def test_cross_checkout_request_identity_and_dispatch_paths(self):
        temp_root = Path(tempfile.mkdtemp(prefix='rescue-relocation-', dir=os.environ['TMPDIR']))
        source_root = Path(rescue_proof.REPO)
        roles = [*(('docs/qualification/workspace-private-531/' + name) for name in rescue_proof.SOURCES),
                 *(('docs/qualification/workspace-private-531/ubuntu-2604-metadata/' + name)
                   for name in rescue_proof.META_HASHES), 'config/tasks.toml']

        def make_checkout(name):
            root = temp_root / name
            for role in roles:
                target = root / role
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source_root / role, target)
            subprocess.run(['/usr/bin/git', 'init', '-q', str(root)], check=True)
            subprocess.run(['/usr/bin/git', '-C', str(root), 'add', '--', *roles], check=True)
            env = dict(os.environ, GIT_AUTHOR_NAME='Fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                       GIT_COMMITTER_NAME='Fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid',
                       GIT_AUTHOR_DATE='2026-01-01T00:00:00Z', GIT_COMMITTER_DATE='2026-01-01T00:00:00Z')
            subprocess.run(['/usr/bin/git', '-C', str(root), 'commit', '-q', '-m', 'fixed rescue inputs'],
                           check=True, env=env)
            return root

        first, second = make_checkout('checkout-one'), make_checkout('checkout-two')
        first_head = subprocess.check_output(['/usr/bin/git', '-C', str(first), 'rev-parse', 'HEAD']).decode().strip()
        second_head = subprocess.check_output(['/usr/bin/git', '-C', str(second), 'rev-parse', 'HEAD']).decode().strip()
        self.assertEqual(first_head, second_head)
        self.assertEqual(subprocess.check_output(['/usr/bin/git', '-C', str(first), 'status', '--porcelain']), b'')
        self.assertEqual(subprocess.check_output(['/usr/bin/git', '-C', str(second), 'status', '--porcelain']), b'')

        def checkout_paths(root):
            here = root / 'docs/qualification/workspace-private-531'
            return patch.object(rescue_proof, 'REPO', root), patch.object(rescue_proof, 'HERE', here), \
                patch.object(rescue_proof, 'META', here / 'ubuntu-2604-metadata')

        fixture = tempfile.mkdtemp(prefix='rescue-input-', dir=os.environ['TMPDIR'])
        evidence = tempfile.mkdtemp(prefix='rescue-evidence-', dir=os.environ['TMPDIR'])
        with checkout_paths(first)[0], checkout_paths(first)[1], checkout_paths(first)[2]:
            stable_first = rescue_proof.source_inputs()
            old_first = {str(first / role): value for role, value in stable_first['repository'].items()}
            request = rescue_proof.plan(fixture, evidence, first_head, 'b' * 64)
        with checkout_paths(second)[0], checkout_paths(second)[1], checkout_paths(second)[2]:
            stable_second = rescue_proof.source_inputs()
            old_second = {str(second / role): value for role, value in stable_second['repository'].items()}
            self.assertNotEqual(old_first, old_second)
            self.assertEqual(stable_first, stable_second)
            runtime = rescue_proof.runtime_inputs(request['inputs'])
            self.assertTrue(all(key.startswith(str(second) + '/') for key in runtime
                                if key.endswith('.py') or key.endswith('.toml') or '/ubuntu-2604-metadata/' in key))
            for malformed in (
                    dict(request['inputs'], repository=dict(request['inputs']['repository'], extra='0' * 64)),
                    dict(request['inputs'], repository={}),
                    dict(request['inputs'], repository={'../escape': '0' * 64}),
                    dict(request['inputs'], machine=dict(request['inputs']['machine'], **{str(second / roles[0]): '0' * 64}))):
                with self.assertRaises(rescue.Refusal):
                    rescue_proof.runtime_inputs(malformed)

            calls = []
            valid_status = ('[GNUPG:] VALIDSIG ' + rescue.FINGERPRINT +
                            ' 2026-08-27 1 0 4 0 1 10 00 ' + rescue.FINGERPRINT + '\n')
            def fake_stage(root, argv, *args, **kwargs):
                calls.append((argv, args[2]))
                if len(calls) == 1:
                    Path(argv[7]).write_bytes(b'fixture keyring')
                elif len(calls) == 2:
                    Path(root, 'stdout').write_text(valid_status)
                else:
                    return {'disposition': 'failed', 'waited_exit': 1}
                return {'disposition': 'exited', 'waited_exit': 0}
            with patch.object(rescue_proof.stage, 'run_stage', side_effect=fake_stage):
                wrong_head = dict(request, head='c' * 40)
                with self.assertRaisesRegex(rescue.Refusal, 'source head changed'):
                    rescue_proof.execute(wrong_head, evidence + '/wrong-request', 'd' * 64,
                                         time.monotonic() + 60)
                with self.assertRaisesRegex(rescue.Refusal, 'acquisition failed'):
                    rescue_proof.execute(request, evidence + '/request', 'c' * 64, time.monotonic() + 60)
                with self.assertRaises(FileExistsError):
                    rescue_proof.execute(request, evidence + '/request', 'd' * 64, time.monotonic() + 60)
            self.assertEqual(len(calls), 3)
            self.assertIn(str(second / roles[0]), calls[0][1])
            self.assertEqual(set(os.listdir(evidence)), {'rescue-' + 'b' * 64 + '.claim',
                                                        'rescue-' + 'b' * 64})

    def test_rescue_root_refuses_foreign_and_non_private_directories(self):
        foreign = '/Volumes/Other/rescue-root'
        with self.assertRaisesRegex(rescue.Refusal, 'canonical under removable Stash'):
            rescue_proof.root_identity(foreign)
        public = tempfile.mkdtemp(prefix='rescue-public-root-', dir=os.environ['TMPDIR'])
        os.chmod(public, 0o755)
        with self.assertRaisesRegex(rescue.Refusal, 'mode-0700'):
            rescue_proof.root_identity(public)

    def test_signed_image_and_manifest_pins_must_agree(self):
        sums = (rescue_proof.META / 'SHA256SUMS').read_text()
        manifest = (rescue_proof.META / 'ubuntu-26.04.1-live-server-arm64.manifest').read_text()
        self.assertTrue(rescue_proof.metadata_matches(sums, manifest))
        self.assertEqual(rescue.NAME, 'ubuntu-26.04.1-live-server-arm64.iso')
        self.assertEqual(rescue.SIZE, 3076861952)
        self.assertFalse(rescue_proof.metadata_matches(sums, manifest.replace(
            'e2fsprogs\t1.47.2-3ubuntu4', 'e2fsprogs\t1.47.2-3ubuntu5')))

    def test_unarmed_and_changed_url_refuse_without_transport(self):
        result = subprocess.run([sys.executable, '-I', '-B', rescue.__file__], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b'ordinary rescue input execution is unarmed', result.stderr)
        with self.assertRaises(rescue.Refusal):
            rescue.main()
        for url in ('https://example.com/', rescue.URL + '?other', rescue.URL.replace('https:', 'http:')):
            with self.subTest(url=url), self.assertRaises(rescue.Refusal):
                rescue.open_fixed_https(url)
        with self.assertRaises(rescue.Refusal):
            rescue.open_fixed_https(rescue.URL, {'Authorization': 'x'})

    def test_proof_entry_is_unarmed_and_checks_full_signature(self):
        result = subprocess.run([sys.executable, '-I', '-B', rescue_proof.__file__],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b'ordinary execution is unarmed', result.stderr)
        status = ('[GNUPG:] VALIDSIG ' + rescue.FINGERPRINT + ' 2026-08-27 1 0 4 0 1 10 00 '
                  + rescue.FINGERPRINT + '\n')
        good = {'disposition': 'exited', 'waited_exit': 0}
        self.assertTrue(rescue_proof.verified_signature(good, status))
        for bad in (status.replace(rescue.FINGERPRINT, 'D94AA3F0EFE21092'),
                    status.replace(rescue.FINGERPRINT, 'A' * 40), '', status + status):
            self.assertFalse(rescue_proof.verified_signature(good, bad))
        self.assertFalse(rescue_proof.verified_signature(dict(good, waited_exit=1), status))
        self.assertFalse(rescue_proof.verified_signature(dict(good, waited_exit=True), status))
        with patch.object(rescue_proof, 'GPG', '/missing/effigy-gpg'):
            with self.assertRaisesRegex(rescue.Refusal, 'verifier unavailable'):
                rescue_proof.source_inputs()

    def test_forged_metadata_and_expired_checkpoint_refuse(self):
        pins = dict(rescue_proof.META_HASHES, SHA256SUMS='0' * 64)
        with patch.object(rescue_proof, 'META_HASHES', pins), self.assertRaisesRegex(rescue.Refusal, 'metadata'):
            rescue_proof.source_inputs()
        with patch.object(rescue.inputs, 'fetch_pinned_https') as fetch:
            def expired():
                raise rescue.Refusal('deadline expired')
            with self.assertRaisesRegex(rescue.Refusal, 'deadline'):
                rescue.fetch_fixed('/fixture/' + rescue.NAME, checkpoint=expired)
            fetch.assert_not_called()

    def test_proof_plan_binds_fixture_identity_and_refuses_collisions(self):
        fixture = tempfile.mkdtemp(prefix='rescue-fixture-')
        evidence = tempfile.mkdtemp(prefix='rescue-evidence-')
        with patch.object(rescue_proof, 'source_inputs', return_value={'fixture-only': '0' * 64}):
            plan = rescue_proof.plan(fixture, evidence, 'a' * 40, 'b' * 64)
        self.assertEqual(plan['destination'], fixture + '/' + rescue.NAME)
        self.assertEqual((plan['bytes'], plan['sha256'], plan['duration_seconds']),
                         (rescue.SIZE, rescue.SHA256, 1800))
        # A plan made with injected fixture inputs cannot pass the uninjected binding comparison.
        with patch.object(rescue_proof, 'source_inputs', return_value={'actual-input': '1' * 64}):
            with self.assertRaisesRegex(rescue.Refusal, 'bindings drifted'):
                rescue_proof.execute(plan, evidence + '/request', 'c' * 64, time.monotonic() + 60)
        self.assertEqual(os.listdir(evidence), [])
        Path(fixture, 'foreign').write_bytes(b'x')
        with self.assertRaises(rescue.Refusal):
            rescue_proof.plan(fixture, evidence, 'a' * 40, 'b' * 64)
        with self.assertRaises(rescue.Refusal):
            rescue_proof.plan(evidence, evidence, 'a' * 40, 'b' * 64)

    def test_proof_dispatch_uses_fixed_commands_and_failed_fixture_cannot_issue_receipt(self):
        fixture = tempfile.mkdtemp(prefix='rescue-dispatch-fixture-')
        evidence = tempfile.mkdtemp(prefix='rescue-dispatch-evidence-')
        sources = {'fixture-source': '0' * 64}
        calls = []
        status = ('[GNUPG:] VALIDSIG ' + rescue.FINGERPRINT + ' 2026-08-27 1 0 4 0 1 10 00 '
                  + rescue.FINGERPRINT + '\n')
        def fake_stage(root, argv, *args, **kwargs):
            calls.append(argv)
            if len(calls) == 1:
                Path(argv[7]).write_bytes(b'fixture keyring')
            elif len(calls) == 2:
                Path(root, 'stdout').write_text(status)
            else:
                return {'disposition': 'failed', 'waited_exit': 1}
            return {'disposition': 'exited', 'waited_exit': 0}
        def fake_git(argv, **kwargs):
            return b'a' * 40 + b'\n' if 'rev-parse' in argv else b''
        with patch.object(rescue_proof, 'source_inputs', return_value=sources), \
                patch.object(rescue_proof, 'runtime_inputs', return_value={'/fixture-source': '0' * 64}), \
                patch.object(rescue_proof.stage, 'run_stage', side_effect=fake_stage), \
                patch.object(rescue_proof.subprocess, 'check_output', side_effect=fake_git):
            request = rescue_proof.plan(fixture, evidence, 'a' * 40, 'b' * 64)
            with self.assertRaisesRegex(rescue.Refusal, 'acquisition failed'):
                rescue_proof.execute(request, evidence + '/request', 'c' * 64, time.monotonic() + 60)
        self.assertEqual(calls[0][:3], [rescue_proof.GPG, '--no-options', '--homedir'])
        self.assertEqual(calls[1][:4], [rescue_proof.GPGV, '--keyring', calls[0][7], '--status-fd'])
        self.assertEqual(calls[2][:5], [request['python'], '-I', '-B', '-c', rescue_proof.CHILD])
        self.assertEqual(calls[2][6], request['destination'])
        occurrence = Path(evidence, 'rescue-' + 'b' * 64)
        self.assertFalse((occurrence / 'result.json').exists())
        self.assertEqual(os.listdir(fixture), [])

    def test_bounded_transport_refuses_redirect_encoding_truncation_and_overflow(self):
        data = b'fixture'
        digest = hashlib.sha256(data).hexdigest()
        root = Path(tempfile.mkdtemp(prefix='rescue-input-'))

        class Response(io.BytesIO):
            def __init__(self, body, status, headers):
                super().__init__(body)
                self.status, self.headers = status, headers
            def getheader(self, key):
                return self.headers.get(key)

        class Connection:
            def close(self):
                pass

        cases = ((302, {'Content-Length': str(len(data))}, data),
                 (200, {'Content-Length': str(len(data)), 'Content-Encoding': 'gzip'}, data),
                 (200, {'Content-Length': '0'}, data),
                 (200, {'Content-Length': str(len(data))}, data[:-1]),
                 (200, {'Content-Length': str(len(data))}, data + b'x'))
        for index, (status, headers, body) in enumerate(cases):
            with self.subTest(index=index), self.assertRaises(rescue.Refusal):
                rescue.inputs.fetch_pinned_https('fixture', str(root / str(index)), len(data), digest,
                    opener=lambda url, headers, response=Response(body, status, headers): (Connection(), response))
        with self.assertRaises(rescue.Refusal):
            rescue.inputs.fetch_pinned_https('fixture', str(root / 'collision'), 0, digest,
                opener=lambda url, headers: (Connection(), Response(b'', 200, {'Content-Length': '0'})))
        partial = root / 'partial'
        with self.assertRaises(rescue.Refusal):
            rescue.inputs.fetch_pinned_https('fixture', str(partial), len(data), digest,
                opener=lambda url, headers: (Connection(), Response(data[:-1], 200, {'Content-Length': str(len(data))})))
        self.assertEqual(partial.read_bytes(), data[:-1])
        with self.assertRaises(rescue.Refusal):
            rescue.inputs.fetch_pinned_https('fixture', str(partial), len(data), digest,
                opener=lambda url, headers: (Connection(), Response(data, 200, {'Content-Length': str(len(data))})))
        wrong_hash = root / 'wrong-hash'
        with self.assertRaises(rescue.Refusal):
            rescue.inputs.fetch_pinned_https('fixture', str(wrong_hash), len(data), '0' * 64,
                opener=lambda url, headers: (Connection(), Response(data, 200, {'Content-Length': str(len(data))})))
        self.assertEqual(wrong_hash.read_bytes(), data)

    def test_existing_image_refuses_before_opening_transport(self):
        root = Path(tempfile.mkdtemp(prefix='rescue-collision-'))
        destination = root / rescue.NAME
        destination.write_bytes(b'existing')
        with patch.object(rescue.inputs, 'fetch_pinned_https') as fetch:
            with self.assertRaisesRegex(rescue.Refusal, 'before network'):
                rescue.fetch_fixed(str(destination), checkpoint=lambda: None)
            fetch.assert_not_called()
        self.assertEqual(destination.read_bytes(), b'existing')

    def test_fixed_fetch_uses_exact_pins_and_rejects_invalid_destination(self):
        with self.assertRaises(rescue.Refusal):
            rescue.fetch_fixed('/not-canonical/other', checkpoint=lambda: None)
        with patch.object(rescue.inputs, 'fetch_pinned_https', return_value={'fixture': True}) as fetch:
            result = rescue.fetch_fixed('/fixture/' + rescue.NAME, checkpoint=lambda: None)
        self.assertEqual(result, {'fixture': True})
        args, kwargs = fetch.call_args
        self.assertEqual(args, (rescue.URL, '/fixture/' + rescue.NAME, rescue.SIZE, rescue.SHA256))
        self.assertIs(kwargs['opener'], rescue.open_fixed_https)


class WitnessCollectorChecks(unittest.TestCase):
    """The fixed offline collector over checksum-correct fixture bytes; no device is ever opened."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix='alloc-witness-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.dir, True)

    def image(self, name='witness.img', **kwargs):
        return build_witness_image(self.dir / name, **kwargs)

    def test_the_pinned_crc32c_matches_the_standard_vector_and_chunks_incrementally(self):
        self.assertEqual(witness.crc32c(0xFFFFFFFF, b'123456789') ^ 0xFFFFFFFF, 0xE3069283)
        split = witness.crc32c(witness.crc32c(0xFFFFFFFF, b'hello '), b'world')
        self.assertEqual(split, witness.crc32c(0xFFFFFFFF, b'hello world'))

    def test_the_complete_supported_zero_case_collects_and_verifies(self):
        image, end, _tables = self.image()
        collected = collect_witness(image, end)
        counts = collected['counts']
        self.assertEqual(counts['slots_checked'], 17)
        self.assertEqual((counts['zero_project'], counts['zero_projinherit']), (17, 17))
        self.assertEqual(counts['checksums_verified'] + counts['zero_base_inodes'], 17)
        summary = witness.verify_fresh_project_witness(collected, RESCUE_BOOT)
        self.assertEqual(summary['inodes_count'], 17)
        self.assertEqual(summary['project'], bound.PROJECT_ID)
        self.assertNotIn(b'531', image.read_bytes()[-2048:], 'the fixture itself carries no project')

    def test_an_alternative_supported_layout_collects_the_same_way(self):
        image, end, _tables = self.image('alt.img', block_bytes=4096, desc64=False, csum_seed=False,
                                         inodes_count=12)
        collected = collect_witness(image, end)
        self.assertEqual(collected['geometry']['block_bytes'], 4096)
        self.assertEqual(collected['geometry']['descriptor_bytes'], 32)
        self.assertEqual(collected['geometry']['checksum_seed_source'], 'uuid')
        witness.verify_fresh_project_witness(collected, RESCUE_BOOT)

    def test_nonzero_project_fields_refuse_with_the_surfaced_counterexample(self):
        cases = ((1, r'inode 1 \(group 0, slot 0'),                      # first checked slot
                 (16, r'inode 16 \(group 1, slot 7'),                    # last slot of a middle group
                 (17, r'inode 17 \(group 2, slot 0'))                    # last checked slot of the fs
        for ino, expected in cases:
            with self.subTest(inode=ino):
                image, end, _tables = self.image('proj-%d.img' % ino,
                                                 patches=patch_inode_field(ino, witness.I_PROJID,
                                                                           struct.pack('<I', 531)))
                with self.assertRaisesRegex(witness.Refusal, expected + r'.*carries project 531'):
                    collect_witness(image, end)

    def test_a_chunk_boundary_slot_still_refuses_when_the_granularity_shrinks(self):
        image, end, _tables = self.image('boundary.img',
                                         patches=patch_inode_field(17, witness.I_PROJID,
                                                                   struct.pack('<I', 531)))
        with patch.object(witness, 'CHUNK_BYTES', 512):
            with self.assertRaisesRegex(witness.Refusal, 'inode 17 .* carries project 531'):
                collect_witness(image, end, chunk_bytes=512)

    def test_the_inheritance_flag_refuses_as_a_nonzero_relevant_field(self):
        image, end, _tables = self.image(
            'inherit.img', patches=patch_inode_field(12, witness.I_FLAGS,
                                                     struct.pack('<I', witness.EXT4_PROJINHERIT_FL)))
        with self.assertRaisesRegex(witness.Refusal, 'inode 12 .* project-inheritance'):
            collect_witness(image, end)

    def test_an_inode_or_descriptor_or_superblock_checksum_drift_refuses(self):
        inode_drift = patch_inode_field(11, 0x14, struct.pack('<I', 7))          # i_dtime, base inode
        image, end, _tables = self.image('inode-csum.img', patches=inode_drift)
        with self.assertRaisesRegex(witness.Refusal, 'inode 11 .* checksum does not verify'):
            collect_witness(image, end)

        def desc_drift(image, tables, seed):
            offset = 2 * 1024 + 1 * 64
            image[offset + 4] ^= 0xFF
        image, end, _tables = self.image('desc-csum.img', patches=desc_drift)
        with self.assertRaisesRegex(witness.Refusal, 'group 1 descriptor checksum'):
            collect_witness(image, end)

        def sb_drift(image):
            image[1024 + 0x10] ^= 0xFF
        image, end, _tables = self.image('sb-csum.img', sb_patches=sb_drift)
        with self.assertRaisesRegex(witness.Refusal, 'superblock checksum does not verify'):
            collect_witness(image, end)

    def test_unsupported_or_nonfresh_layouts_refuse(self):
        def set_bytes(offset, data):
            def apply(image):
                image[1024 + offset:1024 + offset + len(data)] = data
            return apply
        cases = (
            ('magic', dict(sb_patches=set_bytes(0x38, b'\x00\x00')), 'magic is not ext4'),
            ('uuid', dict(sb_patches=set_bytes(0x68, b'\x01')), 'differs from the native root'),
            ('unclean', dict(state=2), 'not the clean offline state'),
            ('inode-size', dict(inode_size=128), 'inode size 128 is unsupported'),
            ('quota-bit', dict(ro_extra=witness.RO_QUOTA), 'already present: quota'),
            ('project-bit', dict(ro_extra=witness.RO_PROJECT), 'already present: project'),
            ('bigalloc-bit', dict(ro_extra=witness.RO_BIGALLOC), 'already present: bigalloc'),
            ('no-metadata-csum', dict(ro_drop_csum=True), 'gdt_csum'),
            ('inline-data', dict(incompat_extra=0x8000), 'outside the supported layout'),
            ('csum-type', dict(sb_patches=set_bytes(0x175, b'\x02')), 'checksum type is not crc32c'),
            ('quota-inode', dict(quota_inum=13), 'quota inode already recorded'),
            ('mount-opts', dict(mount_opts=b'prjquota'), 'stored mount-option bytes'),
            ('orphan-list', dict(last_orphan=42), 'orphan inodes are still listed'),
            ('desc-size', dict(desc_size=128), 'descriptor size 128'),
            ('first-block', dict(first_data_block=2), 'contradicts the 1024-byte block size'),
            ('clusters', dict(log_cluster=1), 'clusters differ from blocks'),
            ('bitmap-bounds', dict(ipg=8193), 'exceeds the one-block bitmap bound'),
            ('unclosed-inodes', dict(inodes_count=999), 'not closed by 3 groups'),
            ('few-inodes', dict(inodes_count=5), 'below the reserved inode range'),
        )
        for name, kwargs, expected in cases:
            with self.subTest(case=name):
                image, end, _tables = self.image('layout-%s.img' % name, **kwargs)
                with self.assertRaisesRegex(witness.Refusal, expected):
                    collect_witness(image, end)

    def test_table_locations_that_leave_the_filesystem_or_alias_refuse(self):
        image, end, _tables = self.image('table-zero.img',
                                         patches=patch_desc_table(1, 0))
        with self.assertRaisesRegex(witness.Refusal, 'group 1 inode table block 0 is outside'):
            collect_witness(image, end)
        image, end, _tables = self.image('table-out.img',
                                         patches=patch_desc_table(1, 4096))
        with self.assertRaisesRegex(witness.Refusal, 'outside the filesystem'):
            collect_witness(image, end)
        image, end, _tables = self.image('table-alias.img',
                                         patches=patch_desc_table(1, 3))
        with self.assertRaisesRegex(witness.Refusal, 'alias one another'):
            collect_witness(image, end)
        image, end, _tables = self.image('table-gdt.img',
                                         patches=patch_desc_table(1, 2))
        with self.assertRaisesRegex(witness.Refusal, 'overlaps the already read'):
            collect_witness(image, end)

    def test_partial_reads_and_missing_boundaries_refuse(self):
        image, end, _tables = self.image()
        fd = os.open(str(image), os.O_RDONLY)
        self.addCleanup(os.close, fd)
        def short(offset, length):
            return os.pread(fd, length - 1, offset)
        with self.assertRaisesRegex(witness.Refusal, 'short or malformed reads refuse'):
            witness.collect(short, RESCUE_BOOT, NONCE)
        with self.assertRaisesRegex(witness.Refusal, 'injected read boundary'):
            witness.collect(None, RESCUE_BOOT, NONCE)
        with self.assertRaisesRegex(witness.Refusal, 'boot identity'):
            witness.collect(lambda offset, length: b'', 'not-a-boot', NONCE)
        with self.assertRaisesRegex(witness.Refusal, 'nonce is invalid'):
            witness.collect(lambda offset, length: b'', RESCUE_BOOT, 'short')

    def test_tail_slots_beyond_the_inode_count_are_read_but_are_not_inode_facts(self):
        # Slot 24 is beyond inodes_count 17: the scan cannot charge it, so its bytes are not project
        # facts and must not refuse (the soundness boundary the collector documents).
        image, end, _tables = self.image('tail.img',
                                         patches=patch_inode_field(24, witness.I_PROJID,
                                                                   struct.pack('<I', 531)))
        collected = collect_witness(image, end)
        self.assertEqual(collected['counts']['tail_slots'], 7)
        self.assertEqual(collected['counts']['slots_checked'], 17)

    def test_retained_bounds_refuse_before_the_witness_grows(self):
        image, end, _tables = self.image()
        with patch.object(witness, 'MAX_CHUNKS', 2):
            with self.assertRaisesRegex(witness.Refusal, 'chunk retained bound'):
                collect_witness(image, end)
        with patch.object(witness, 'MAX_WITNESS_BYTES', 200):
            with self.assertRaisesRegex(witness.Refusal, 'record ceiling'):
                collect_witness(image, end)


class WitnessConsumerChecks(unittest.TestCase):
    """The completion-side verifier re-derives everything from the retained witness alone."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix='alloc-consumer-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.dir, True)
        image, end, _tables = build_witness_image(self.dir / 'good.img')
        self.good = collect_witness(image, end)

    def tampered(self, **changes):
        return dict(self.good, **changes)

    def reject(self, candidate, expected):
        with self.assertRaisesRegex(witness.Refusal, expected):
            witness.verify_fresh_project_witness(candidate, RESCUE_BOOT)

    def test_the_genuine_witness_passes_and_binds_only_its_own_rescue_boot(self):
        summary = witness.verify_fresh_project_witness(self.good, RESCUE_BOOT)
        self.assertEqual(summary['kind'], witness.MECHANISM_KIND)
        self.reject(self.tampered(boot_id=BOOT_ID), 'another boot')
        self.reject(self.tampered(boot_id=RESCUE_BOOT.replace('a', 'b', 1)), 'another boot')

    def test_the_fixed_root_identity_cannot_be_replaced(self):
        self.reject(self.tampered(device='/dev/sdb2'), 'another device')
        self.reject(self.tampered(uuid='0' * 36), 'another device')
        self.reject(self.tampered(project=530), 'another device, UUID or project')
        self.reject(self.tampered(nonce='nope'), 'one-use nonce')
        self.reject(self.tampered(kind='other'), 'another mechanism')
        self.reject(self.tampered(mechanism='ext4-project-attribute'), 'another mechanism')
        self.reject({k: v for k, v in self.good.items() if k != 'counts'}, 'unexpected shape')

    def test_drifted_geometry_counts_or_features_refuse(self):
        geometry = dict(self.good['geometry'])
        for field, value, expected in (
                ('block_bytes', 2048, 'contradicts the block size'),
                ('groups', 4, 'does not close arithmetically'),
                ('inodes_count', 18, 'slot coverage'),
                ('descriptor_bytes', 32, 'descriptor size is outside'),
                ('blocks_per_group', 0, 'bitmap bound'),
                ('checksum_seed_source', 'elsewhere', 'seed source is unknown')):
            with self.subTest(field=field):
                self.reject(self.tampered(geometry=dict(geometry, **{field: value})), expected)
        self.reject(self.tampered(filesystem_bytes=self.good['filesystem_bytes'] + 1),
                    'extent differs from the geometry')
        features = dict(self.good['superblock'])
        self.reject(self.tampered(superblock=dict(features, checksum_verified=False)),
                    'do not record the verified offline pre-state')
        self.reject(self.tampered(superblock=dict(features, ro_compat=features['ro_compat'] |
                                                  witness.RO_QUOTA)),
                    'outside the supported offline pre-state')

    def test_retained_inode_table_locations_and_counts_must_close(self):
        self.reject(self.tampered(inode_tables=self.good['inode_tables'][:-8]),
                    'do not match the group count')
        self.reject(self.tampered(inode_tables=struct.pack('<3I', 3, 30, 7).hex()),
                    'outside the filesystem|incomplete|outside or spans')
        counts = dict(self.good['counts'])
        self.reject(self.tampered(counts=dict(counts, zero_project=counts['zero_project'] - 1)),
                    'not record a fully fresh project')
        self.reject(self.tampered(counts=dict(counts, tail_slots=0)), 'tail-slot count')
        self.reject(self.tampered(counts=dict(counts, checksums_verified=99)),
                    'checksum counters do not cover')

    def test_coverage_chunks_must_tile_the_planned_extents_exactly(self):
        self.reject(self.tampered(coverage={'chunks': self.good['coverage']['chunks'],
                                            'bytes': self.good['coverage']['bytes'] + 1}),
                    'coverage bytes')
        dropped = self.good['coverage']['chunks'][1:]
        self.reject(self.tampered(coverage={'chunks': dropped,
                                            'bytes': self.good['coverage']['bytes']}),
                    'incomplete|gap|bytes')
        duplicated = self.good['coverage']['chunks'] + [self.good['coverage']['chunks'][2]]
        self.reject(self.tampered(coverage={'chunks': duplicated,
                                            'bytes': self.good['coverage']['bytes']}),
                    'duplicates|gap|outside or spans')
        malformed = [dict(chunk, sha256='nothex') for chunk in self.good['coverage']['chunks']]
        self.reject(self.tampered(coverage={'chunks': malformed,
                                            'bytes': self.good['coverage']['bytes']}),
                    'digest are malformed')
        outside = [dict(self.good['coverage']['chunks'][0], offset=99 * 1024)]
        self.reject(self.tampered(coverage={'chunks': outside + self.good['coverage']['chunks'][1:],
                                            'bytes': self.good['coverage']['bytes']}),
                    'outside or spans|incomplete')


if __name__ == '__main__':
    unittest.main()
