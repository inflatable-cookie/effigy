"""Fixed offline fresh-project accounting witness for ext4 project quota (source controls only).

The accounting premise (linux-setup.md): a v1.47.2 `tune2fs` enabling the project and quota features
computes the initial project usage by scanning inodes (lib/support/mkquota.c v1.47.2, SHA-256
`81f9bcec788b67384b92577ac8a36738c320570e985a2aad9619883cc3caaeeb`), where `quota_data_add` is void
and silently skips a charge when its dquot cannot be allocated, and `quota_compare_and_update` can
overwrite a scan failure with a successful close. A zero tool exit therefore cannot prove complete
accounting. The narrow sound alternative is the fresh-project proof implemented here: before the
features are enabled, an offline collector shows that *no* inode slot of the supported filesystem
carries project 531 (or the project-inheritance flag). The v1.47.2 scan charges a project only from
an inode's raw `i_projid`, so a filesystem with no such field anywhere cannot produce a nonzero
project-531 initial usage, whatever the scan drops: the fresh project starts complete at zero. The
already implemented E7p zero anchor (`linux_allocation_bound.verify_project_assignment`) then shows
the record still uncharged at assignment, and the row consumers require exact kernel-charge versus
tree-census equality at every read afterwards.

The collector is fixed to the reviewed root UUID, device and project (constants from
`linux_allocation_bound`). It derives every offset from the bytes it reads; it takes no path, no
command, no caller offset and no transport hook: bytes arrive only through one injected
`read(offset, length)` boundary, which a reviewed native route binds to the offline device and which
controls replace with fixture files. It never runs by itself and never opens anything.

Supported layout (the observed 747 root family; anything else refuses): 1024..65536-byte power-of-two
blocks without bigalloc, 256-byte inodes, exactly the supported incompat feature set
(filetype/extent/64bit/flex_bg/metadata_csum_seed), metadata_csum present, and no quota, project,
gdt_csum, snapshot, readonly, shared-blocks or orphan-present ro-compat bit. Layout and offsets are
taken from the pinned v1.47.2 `lib/ext2fs/ext2_fs.h` (SHA-256
`6acb20acf7ae2c811b3526ff8cd38b0b79ec185bacdd9471b44645b8673b48bc`), checksum semantics from the
pinned v1.47.2 `lib/ext2fs/csum.c` (SHA-256
`ac0bce4c789ed25f380f11eef9086b9afcbb9b00b77d8fb2552da966f22fe0f1`). Superblock, group descriptor and
inode checksums are re-derived and must verify; the group descriptor checksum makes the inode-table
locations trusted before they are read.

Coverage is closed arithmetic over actual bytes: the ledger records every read range, refuses
overlapping, duplicated, out-of-filesystem or short reads, and the retained witness lists every chunk
with its SHA-256, the per-group table locations, the exact geometry and counters derived from the
bytes (slots checked, zeros observed, checksums verified). Every checked slot must read project 0 and
no inheritance flag; slots beyond `s_inodes_count` are not inodes and are counted but not constrained.
The retained witness carries no inode or file content: only digests, geometry and counters, inside the
fixed byte ceiling. The consumer (`verify_fresh_project_witness`) re-derives the arithmetic, the
extents and the counters from the retained witness alone, so a bare success label, a rehashed foreign
witness, a stale boot or a drifted count refuses. Provenance of the bytes remains, as for every
retained original here, a native premise: independently authenticated, durable one-use custody.
"""
import bisect
import hashlib
import json
import re
import struct
import uuid

from linux_supervisor import Refusal
import linux_allocation_bound as bound

MECHANISM_KIND = 'fresh-project-zero-accounting'
SB_OFFSET = 1024                  # primary superblock position (v1.47.2 layout)
SB_BYTES = 1024
CHUNK_BYTES = 1 << 20             # fixed read granularity; a module constant, never a caller argument
# Retained-output bounds, derived: the production-scale root (128 GiB, 4 KiB blocks, 32768
# blocks/group) has at most 65536 groups, so inode tables total at most ~2.2 GiB across <= 2180
# 1 MiB chunks plus the ~4 MiB group descriptor table across <= 5 chunks; 4096 chunks covers that
# with margin, and their records plus the packed table locations stay under the 1,048,576-byte
# record ceiling every retained record here already uses (linux-setup.md, approval installer).
MAX_CHUNKS = 4096
MAX_WITNESS_BYTES = 1 << 20
INODE_BYTES = 256
EXT4_GOOD_OLD_INODE_SIZE = 128
# v1.47.2 ext2_fs.h feature bits.
RO_METADATA_CSUM = 0x400
RO_QUOTA = 0x100
RO_PROJECT = 0x2000
RO_GDT_CSUM = 0x10
RO_BIGALLOC = 0x200
RO_HAS_SNAPSHOT = 0x80
RO_READONLY = 0x1000
RO_SHARED_BLOCKS = 0x4000
RO_ORPHAN_PRESENT = 0x10000
IN_FILETYPE, IN_EXTENTS, IN_64BIT, IN_FLEX_BG, IN_CSUM_SEED = 0x2, 0x40, 0x80, 0x200, 0x2000
SUPPORTED_INCOMPAT = IN_FILETYPE | IN_EXTENTS | IN_64BIT | IN_FLEX_BG | IN_CSUM_SEED
REFUSED_RO_COMPAT = (RO_QUOTA | RO_PROJECT | RO_GDT_CSUM | RO_BIGALLOC | RO_HAS_SNAPSHOT |
                     RO_READONLY | RO_SHARED_BLOCKS | RO_ORPHAN_PRESENT)
RO_NAMES = ((RO_QUOTA, 'quota'), (RO_PROJECT, 'project'), (RO_GDT_CSUM, 'gdt_csum'),
            (RO_BIGALLOC, 'bigalloc'), (RO_HAS_SNAPSHOT, 'has_snapshot'), (RO_READONLY, 'readonly'),
            (RO_SHARED_BLOCKS, 'shared_blocks'), (RO_ORPHAN_PRESENT, 'orphan_present'))
# v1.47.2 ext2_fs.h inode and group-descriptor field offsets.
I_FLAGS, I_GENERATION, I_CHECKSUM_LO, I_EXTRA_ISIZE, I_CHECKSUM_HI, I_PROJID = \
    0x20, 0x64, 0x7c, 0x80, 0x82, 0x9c
BG_INODE_TABLE, BG_INODE_TABLE_HI, BG_CHECKSUM = 0x08, 0x28, 0x1e
EXT4_PROJINHERIT_FL = 0x20000000
EXT4_INODE_CSUM_HI_EXTRA_END = I_CHECKSUM_HI + 2 - EXT4_GOOD_OLD_INODE_SIZE   # 4
EXT2_GOOD_OLD_FIRST_INO = 11
EXT2_CRC32C_CHKSUM = 1
DESC_BYTES_NO_64BIT = 32
DESC_BYTES_64BIT = 64
NONCE = re.compile(r'^[0-9a-f]{64}$')

_CRC32C_POLY = 0x82F63B78
_CRC32C_TABLE = []
for _byte in range(256):
    _crc = _byte
    for _ in range(8):
        _crc = (_crc >> 1) ^ _CRC32C_POLY if _crc & 1 else _crc >> 1
    _CRC32C_TABLE.append(_crc)


def crc32c(crc, data):
    """The raw e2fsprogs `ext2fs_crc32c_le`: no initial or final inversion, running state passed in."""
    table = _CRC32C_TABLE
    for byte in data:
        crc = table[(crc ^ byte) & 0xFF] ^ (crc >> 8)
    return crc


def _le16(data, offset):
    return struct.unpack_from('<H', data, offset)[0]


def _le32(data, offset):
    return struct.unpack_from('<I', data, offset)[0]


class _Ledger:
    """Every read is recorded; overlap, duplication, short and out-of-filesystem reads refuse."""

    def __init__(self, read):
        self._read = read
        self._ranges = []
        self.chunks = []
        self.bytes = 0
        self.end = None

    def bind(self, end):
        """Bind the filesystem extent after the superblock; earlier reads stayed inside it."""
        self.end = end

    def read(self, offset, length):
        if type(offset) is not int or type(length) is not int or isinstance(offset, bool) or \
                isinstance(length, bool) or offset < 0 or length <= 0:
            raise Refusal('coverage read bounds are invalid')
        end = offset + length
        if self.end is not None and end > self.end:
            raise Refusal('coverage read %d..%d is outside the filesystem extent (%d bytes)'
                          % (offset, end, self.end))
        for start, stop in self._ranges:
            if offset < stop and start < end:
                raise Refusal('coverage read %d..%d overlaps the already read %d..%d'
                              % (offset, end, start, stop))
        if not callable(self._read):
            raise Refusal('accounting collection requires an injected read boundary')
        data = self._read(offset, length)
        if not isinstance(data, (bytes, bytearray, memoryview)) or len(data) != length:
            raise Refusal('coverage read %d..%d returned %s bytes; short or malformed reads refuse'
                          % (offset, end, 'no' if data is None else str(len(data))))
        data = bytes(data)
        if len(self.chunks) >= MAX_CHUNKS:
            raise Refusal('coverage needs more than the %d-chunk retained bound' % MAX_CHUNKS)
        bisect.insort(self._ranges, (offset, end))
        self.chunks.append({'offset': offset, 'bytes': length,
                            'sha256': hashlib.sha256(data).hexdigest()})
        self.bytes += length
        return data

    def read_chunked(self, offset, length):
        """Read one planned extent in fixed chunks; returns the extent bytes concatenated."""
        parts, position = [], offset
        remaining = length
        while remaining:
            take = min(CHUNK_BYTES, remaining)
            parts.append(self.read(position, take))
            position += take
            remaining -= take
        return b''.join(parts)


def _inode_checksum(seed, inode_no, inode):
    """The v1.47.2 `ext2fs_inode_csum_verify` semantics, including the all-zero-base acceptance."""
    if inode[:EXT4_GOOD_OLD_INODE_SIZE] == bytes(EXT4_GOOD_OLD_INODE_SIZE):
        return None                                   # the pinned source accepts an all-zero inode
    extra = _le16(inode, I_EXTRA_ISIZE)
    has_hi = extra >= EXT4_INODE_CSUM_HI_EXTRA_END
    provided = _le16(inode, I_CHECKSUM_LO)
    if has_hi:
        provided |= _le16(inode, I_CHECKSUM_HI) << 16
    work = bytearray(inode)
    work[I_CHECKSUM_LO:I_CHECKSUM_LO + 2] = bytes(2)
    if has_hi:
        work[I_CHECKSUM_HI:I_CHECKSUM_HI + 2] = bytes(2)
    crc = crc32c(seed, struct.pack('<I', inode_no))
    crc = crc32c(crc, inode[I_GENERATION:I_GENERATION + 4])
    crc = crc32c(crc, work)
    if not has_hi:
        crc &= 0xFFFF
    return None if provided == crc else 'computed %d' % crc


def _descriptor_checksum(seed, group, desc):
    """The v1.47.2 metadata_csum group-descriptor checksum (crc32c truncated to 16 bits)."""
    work = bytearray(desc)
    work[BG_CHECKSUM:BG_CHECKSUM + 2] = bytes(2)
    crc = crc32c(seed, struct.pack('<I', group))
    crc = crc32c(crc, work)
    return crc & 0xFFFF


def _parse_superblock(ledger):
    """Parse and fully check the primary superblock; returns the geometry and the checksum seed."""
    sb = ledger.read(SB_OFFSET, SB_BYTES)
    if _le16(sb, 0x38) != 0xEF53:
        raise Refusal('superblock magic is not ext4')
    raw_uuid = sb[0x68:0x78]
    if raw_uuid != uuid.UUID(bound.ROOT_UUID).bytes:
        raise Refusal('superblock UUID %s differs from the native root'
                      % str(uuid.UUID(bytes=raw_uuid)))
    if _le16(sb, 0x3a) != 1:
        raise Refusal('superblock is not the clean offline state')
    if _le16(sb, 0x58) != INODE_BYTES:
        raise Refusal('inode size %d is unsupported; the layout needs %d-byte inodes'
                      % (_le16(sb, 0x58), INODE_BYTES))
    log_block = _le32(sb, 0x18)
    block_bytes = 1024 << log_block
    if block_bytes > 65536:
        raise Refusal('block size %d is unsupported' % block_bytes)
    if _le32(sb, 0x1c) != log_block:
        raise Refusal('allocation clusters differ from blocks (bigalloc-like layout)')
    first_data_block = _le32(sb, 0x14)
    if first_data_block != (1 if block_bytes == 1024 else 0):
        raise Refusal('first data block %d contradicts the %d-byte block size'
                      % (first_data_block, block_bytes))
    incompat = _le32(sb, 0x60)
    unsupported = incompat & ~SUPPORTED_INCOMPAT
    if unsupported:
        raise Refusal('incompat features %#x are outside the supported layout' % unsupported)
    ro = _le32(sb, 0x64)
    present = [name for bit, name in RO_NAMES if ro & bit]
    if present:
        raise Refusal('ro-compat features already present: ' + ', '.join(present))
    if not ro & RO_METADATA_CSUM:
        raise Refusal('layout lacks metadata_csum; table locations are not checksummed')
    if sb[0x175] != EXT2_CRC32C_CHKSUM:
        raise Refusal('superblock checksum type is not crc32c')
    if _le32(sb, 0x3fc) != crc32c(0xFFFFFFFF, sb[:0x3fc]):
        raise Refusal('superblock checksum does not verify')
    quotas = {'user': _le32(sb, 0x240), 'group': _le32(sb, 0x244), 'project': _le32(sb, 0x26c)}
    charged = {kind: value for kind, value in quotas.items() if value}
    if charged:
        raise Refusal('quota inode already recorded: ' +
                      ', '.join('%s=%d' % item for item in sorted(charged.items())))
    if _le32(sb, 0xe8):
        raise Refusal('orphan inodes are still listed; the state is not the offline pre-state')
    if sb[0x200:0x240] != bytes(64):
        raise Refusal('stored mount-option bytes are not entirely empty')
    if incompat & IN_64BIT:
        if _le16(sb, 0xfe) != DESC_BYTES_64BIT:
            raise Refusal('descriptor size %d is outside the supported 64-bit layout'
                          % _le16(sb, 0xfe))
        desc_bytes = DESC_BYTES_64BIT
    else:
        desc_bytes = DESC_BYTES_NO_64BIT
    blocks_count = _le32(sb, 0x4) | (_le32(sb, 0x150) << 32 if incompat & IN_64BIT else 0)
    blocks_per_group, inodes_per_group = _le32(sb, 0x20), _le32(sb, 0x28)
    inodes_count = _le32(sb, 0x0)
    if blocks_count <= first_data_block:
        raise Refusal('filesystem block count %d is empty' % blocks_count)
    if not 0 < blocks_per_group <= block_bytes * 8 or not 0 < inodes_per_group <= block_bytes * 8:
        raise Refusal('group shape %d blocks / %d inodes exceeds the one-block bitmap bound'
                      % (blocks_per_group, inodes_per_group))
    if inodes_count < EXT2_GOOD_OLD_FIRST_INO:
        raise Refusal('inode count %d is below the reserved inode range' % inodes_count)
    groups = -(-(blocks_count - first_data_block) // blocks_per_group)
    if groups < 1 or inodes_count > groups * inodes_per_group:
        raise Refusal('inode count %d is not closed by %d groups of %d slots'
                      % (inodes_count, groups, inodes_per_group))
    if _le32(sb, 0x10) > inodes_count:
        raise Refusal('free inode count exceeds the inode count')
    if _le32(sb, 0x54) < EXT2_GOOD_OLD_FIRST_INO:
        raise Refusal('first non-reserved inode is inside the reserved range')
    if incompat & IN_CSUM_SEED:
        seed = _le32(sb, 0x270)
        seed_source = 'stored'
    else:
        seed = crc32c(0xFFFFFFFF, raw_uuid)
        seed_source = 'uuid'
    ledger.bind(blocks_count * block_bytes)
    geometry = {'block_bytes': block_bytes, 'blocks_count': blocks_count,
                'blocks_per_group': blocks_per_group, 'inodes_count': inodes_count,
                'inodes_per_group': inodes_per_group, 'first_data_block': first_data_block,
                'descriptor_bytes': desc_bytes, 'groups': groups,
                'checksum_seed_source': seed_source, 'incompat': incompat, 'ro_compat': ro}
    facts = {'sha256': hashlib.sha256(sb).hexdigest(), 'seed': seed}
    return geometry, facts


def collect(read, boot_id, nonce):
    """Produce the fresh-project witness from the injected offline read boundary.

    Reads the primary superblock, the checksummed group descriptor table and every inode table,
    requiring project 0 and no inheritance flag in every inode slot up to `s_inodes_count`. Refusals
    name the exact inode, group, chunk or field that failed, so a counterexample surfaces instead of
    hiding behind a success shape.
    """
    if not isinstance(boot_id, str) or not bound.BOOT_UUID.fullmatch(boot_id):
        raise Refusal('accounting boot identity is missing')
    if not isinstance(nonce, str) or not NONCE.fullmatch(nonce):
        raise Refusal('accounting witness nonce is invalid')
    ledger = _Ledger(read)
    geometry, superblock = _parse_superblock(ledger)
    block_bytes = geometry['block_bytes']
    desc_bytes = geometry['descriptor_bytes']
    groups = geometry['groups']
    gdt_offset = (geometry['first_data_block'] + 1) * block_bytes
    gdt_bytes = -(-groups * desc_bytes // block_bytes) * block_bytes
    gdt = ledger.read_chunked(gdt_offset, gdt_bytes)
    tables = []
    for group in range(groups):
        desc = gdt[group * desc_bytes:(group + 1) * desc_bytes]
        if _le16(desc, BG_CHECKSUM) != _descriptor_checksum(superblock['seed'], group, desc):
            raise Refusal('group %d descriptor checksum does not verify' % group)
        table = _le32(desc, BG_INODE_TABLE)
        if geometry['incompat'] & IN_64BIT:
            table |= _le32(desc, BG_INODE_TABLE_HI) << 32
        if table <= 0 or table >= geometry['blocks_count']:
            raise Refusal('group %d inode table block %d is outside the filesystem' % (group, table))
        if table >= 1 << 32:
            raise Refusal('group %d inode table block %d exceeds the retained 32-bit location'
                          % (group, table))
        tables.append(table)
    if len(set(tables)) != groups:
        raise Refusal('group inode tables alias one another; the coverage would double-count')
    table_bytes = geometry['inodes_per_group'] * INODE_BYTES
    inodes_count = geometry['inodes_count']
    slots_checked = slots_tail = zero_project = zero_inherit = checksums = zero_base = 0
    for group, table in enumerate(tables):
        data = ledger.read_chunked(table * block_bytes, table_bytes)
        for slot in range(geometry['inodes_per_group']):
            inode_no = group * geometry['inodes_per_group'] + slot + 1
            inode = data[slot * INODE_BYTES:(slot + 1) * INODE_BYTES]
            if inode_no > inodes_count:
                slots_tail += 1
                continue
            extra = _le16(inode, I_EXTRA_ISIZE)
            if EXT4_GOOD_OLD_INODE_SIZE + extra > INODE_BYTES:
                raise Refusal('inode %d extra-isize %d exceeds the %d-byte inode'
                              % (inode_no, extra, INODE_BYTES))
            project = _le32(inode, I_PROJID)
            if project:
                raise Refusal('inode %d (group %d, slot %d, chunk offset %d) carries project %d: the '
                              'project is not fresh' % (inode_no, group, slot, table * block_bytes
                                                        + slot * INODE_BYTES + I_PROJID, project))
            if _le32(inode, I_FLAGS) & EXT4_PROJINHERIT_FL:
                raise Refusal('inode %d (group %d, slot %d) already carries the project-inheritance '
                              'flag: the project is not fresh' % (inode_no, group, slot))
            zero_project += 1
            zero_inherit += 1
            outcome = _inode_checksum(superblock['seed'], inode_no, inode)
            if outcome is not None:
                raise Refusal('inode %d (group %d, slot %d) checksum does not verify (%s)'
                              % (inode_no, group, slot, outcome))
            if inode[:EXT4_GOOD_OLD_INODE_SIZE] == bytes(EXT4_GOOD_OLD_INODE_SIZE):
                zero_base += 1
            else:
                checksums += 1
            slots_checked += 1
    witness = {
        'kind': MECHANISM_KIND,
        'mechanism': bound.MECHANISM,
        'nonce': nonce,
        'boot_id': boot_id,
        'device': bound.ROOT_DEVICE,
        'uuid': bound.ROOT_UUID,
        'project': bound.PROJECT_ID,
        'filesystem_bytes': geometry['blocks_count'] * block_bytes,
        'superblock': {'sha256': superblock['sha256'], 'incompat': geometry['incompat'],
                       'ro_compat': geometry['ro_compat'], 'checksum_verified': True,
                       'quota_inodes_zero': True, 'stored_mount_options_empty': True,
                       'state_clean': True},
        'geometry': {key: geometry[key] for key in
                     ('block_bytes', 'blocks_count', 'blocks_per_group', 'inodes_count',
                      'inodes_per_group', 'first_data_block', 'descriptor_bytes', 'groups',
                      'checksum_seed_source')},
        'inode_tables': struct.pack('<%dI' % groups, *tables).hex(),
        'coverage': {'chunks': ledger.chunks, 'bytes': ledger.bytes},
        'counts': {'slots_checked': slots_checked, 'tail_slots': slots_tail,
                   'zero_project': zero_project, 'zero_projinherit': zero_inherit,
                   'checksums_verified': checksums, 'zero_base_inodes': zero_base},
    }
    if len(json.dumps(witness, sort_keys=True).encode('utf-8')) > MAX_WITNESS_BYTES:
        raise Refusal('retained witness exceeds the %d-byte record ceiling' % MAX_WITNESS_BYTES)
    return witness


def _positive_int(value, name):
    if type(value) is not int or isinstance(value, bool) or value < 0:
        raise Refusal('retained %s is not a non-negative integer' % name)
    return value


def _decode_geometry(witness):
    """Re-derive the closed geometry and every planned extent from the retained witness alone."""
    geometry = witness.get('geometry')
    if not isinstance(geometry, dict) or set(geometry) != {
            'block_bytes', 'blocks_count', 'blocks_per_group', 'inodes_count', 'inodes_per_group',
            'first_data_block', 'descriptor_bytes', 'groups', 'checksum_seed_source'}:
        raise Refusal('witness geometry is missing or has an unexpected shape')
    for field in ('block_bytes', 'blocks_count', 'blocks_per_group', 'inodes_count',
                  'inodes_per_group', 'first_data_block', 'descriptor_bytes', 'groups'):
        _positive_int(geometry[field], field)
    block_bytes = geometry['block_bytes']
    if block_bytes < 1024 or block_bytes > 65536 or block_bytes & (block_bytes - 1):
        raise Refusal('retained block size is not a supported power of two')
    if geometry['first_data_block'] != (1 if block_bytes == 1024 else 0):
        raise Refusal('retained first data block contradicts the block size')
    if geometry['descriptor_bytes'] != (DESC_BYTES_64BIT
                                        if witness['superblock']['incompat'] & IN_64BIT
                                        else DESC_BYTES_NO_64BIT):
        raise Refusal('retained descriptor size is outside the supported layout')
    if geometry['checksum_seed_source'] not in ('stored', 'uuid'):
        raise Refusal('retained checksum seed source is unknown')
    blocks_per_group = geometry['blocks_per_group']
    inodes_per_group = geometry['inodes_per_group']
    if not 0 < blocks_per_group <= block_bytes * 8 or not 0 < inodes_per_group <= block_bytes * 8:
        raise Refusal('retained group shape exceeds the one-block bitmap bound')
    inodes_count = geometry['inodes_count']
    groups = geometry['groups']
    if inodes_count < EXT2_GOOD_OLD_FIRST_INO or groups < 1 or \
            inodes_count > groups * inodes_per_group or \
            groups != -(-(geometry['blocks_count'] - geometry['first_data_block'])
                        // blocks_per_group):
        raise Refusal('retained geometry does not close arithmetically')
    coverage = witness['coverage']
    if not isinstance(coverage, dict) or set(coverage) != {'chunks', 'bytes'}:
        raise Refusal('retained coverage record is missing or has an unexpected shape')
    _positive_int(coverage['bytes'], 'coverage bytes')
    _positive_int(witness['filesystem_bytes'], 'filesystem extent')
    if witness['filesystem_bytes'] != geometry['blocks_count'] * block_bytes:
        raise Refusal('retained filesystem extent differs from the geometry')
    expected = [(geometry['first_data_block'] + 1) * block_bytes,
                -(-groups * geometry['descriptor_bytes'] // block_bytes) * block_bytes]
    packed = witness.get('inode_tables')
    if not isinstance(packed, str) or len(packed) != groups * 8:
        raise Refusal('retained inode-table locations do not match the group count')
    try:
        tables = struct.unpack('<%dI' % groups, bytes.fromhex(packed))
    except (ValueError, struct.error) as exc:
        raise Refusal('retained inode-table locations are malformed') from exc
    table_bytes = inodes_per_group * INODE_BYTES
    extents = [(SB_OFFSET, SB_BYTES), tuple(expected)]
    for group, table in enumerate(tables):
        if table <= 0 or table >= geometry['blocks_count']:
            raise Refusal('group %d retained inode-table location is outside the filesystem' % group)
        extents.append((table * block_bytes, table_bytes))
    if len(set(tables)) != groups:
        raise Refusal('retained inode-table locations alias one another')
    total = SB_BYTES + expected[1] + groups * table_bytes
    if coverage['bytes'] != total:
        raise Refusal('retained coverage bytes %d differ from the planned %d'
                      % (coverage['bytes'], total))
    return geometry, extents, table_bytes, inodes_count


def verify_fresh_project_witness(witness, rescue_boot):
    """Admit one complete fresh-project zero witness bound to the activation's rescue boot.

    Re-derives everything the witness claims from the retained bytes' facts alone: the exact shape,
    the fixed root identity and project, the supported layout, the closed geometry, chunk-tiling of
    the planned extents without gaps, overlaps or duplicates, and counters that close over the actual
    slot counts. A bare success label, a stale or foreign boot, another device or project, a drifted
    count or a rehashed chunk list refuses. Provenance of the underlying reads stays a native premise.
    """
    if not isinstance(witness, dict):
        raise Refusal('fresh-project accounting witness is missing')
    shape = {'kind', 'mechanism', 'nonce', 'boot_id', 'device', 'uuid', 'project',
             'filesystem_bytes', 'superblock', 'geometry', 'inode_tables', 'coverage', 'counts'}
    if set(witness) != shape:
        raise Refusal('fresh-project accounting witness has an unexpected shape')
    if witness['kind'] != MECHANISM_KIND or witness['mechanism'] != bound.MECHANISM:
        raise Refusal('fresh-project accounting witness names another mechanism')
    if witness['boot_id'] != rescue_boot:
        raise Refusal('fresh-project accounting witness is from another boot than the rescue boot '
                      '(stale, rehashed or borrowed)')
    if witness['device'] != bound.ROOT_DEVICE or witness['uuid'] != bound.ROOT_UUID or \
            witness['project'] != bound.PROJECT_ID:
        raise Refusal('fresh-project accounting witness names another device, UUID or project')
    if not isinstance(witness['nonce'], str) or not NONCE.fullmatch(witness['nonce']):
        raise Refusal('fresh-project accounting witness has no one-use nonce')
    superblock = witness['superblock']
    if not isinstance(superblock, dict) or set(superblock) != {
            'sha256', 'incompat', 'ro_compat', 'checksum_verified', 'quota_inodes_zero',
            'stored_mount_options_empty', 'state_clean'}:
        raise Refusal('retained superblock facts are missing or have an unexpected shape')
    if not (superblock['checksum_verified'] and superblock['quota_inodes_zero'] and
            superblock['stored_mount_options_empty'] and superblock['state_clean']):
        raise Refusal('retained superblock facts do not record the verified offline pre-state')
    incompat = superblock['incompat']
    ro = superblock['ro_compat']
    if not isinstance(incompat, int) or not isinstance(ro, int) or \
            incompat & ~SUPPORTED_INCOMPAT or ro & REFUSED_RO_COMPAT or not ro & RO_METADATA_CSUM:
        raise Refusal('retained features are outside the supported offline pre-state')
    geometry, extents, table_bytes, inodes_count = _decode_geometry(witness)
    chunks = witness['coverage']['chunks']
    if not isinstance(chunks, list) or not chunks or len(chunks) > MAX_CHUNKS:
        raise Refusal('retained coverage chunk list is missing or exceeds the bound')
    placed = {start: [] for start, _ in extents}
    for chunk in chunks:
        if not isinstance(chunk, dict) or set(chunk) != {'offset', 'bytes', 'sha256'}:
            raise Refusal('retained coverage chunk is malformed')
        offset, length = chunk['offset'], chunk['bytes']
        if type(offset) is not int or type(length) is not int or offset < 0 or length <= 0 or \
                not isinstance(chunk['sha256'], str) or not bound.HEX64.fullmatch(chunk['sha256']):
            raise Refusal('retained coverage chunk bounds or digest are malformed')
        inside = [start for start, size in extents if start <= offset and offset + length <= start + size]
        if len(inside) != 1:
            raise Refusal('retained chunk %d..%d is outside or spans the planned extents'
                          % (offset, offset + length))
        placed[inside[0]].append((offset, length))
    for start, size in extents:
        runs = sorted(placed[start])
        position = start
        for offset, length in runs:
            if offset != position:
                raise Refusal('retained coverage leaves a gap before %d or duplicates ranges' % position)
            position += length
        if position != start + size:
            raise Refusal('retained coverage is incomplete for the extent at %d (%d of %d bytes)'
                          % (start, position - start, size))
    counts = witness['counts']
    if not isinstance(counts, dict) or set(counts) != {
            'slots_checked', 'tail_slots', 'zero_project', 'zero_projinherit', 'checksums_verified',
            'zero_base_inodes'}:
        raise Refusal('retained counters are missing or have an unexpected shape')
    slots = counts['slots_checked']
    if slots != inodes_count:
        raise Refusal('retained slot coverage %d does not equal the inode count %d'
                      % (slots, inodes_count))
    if counts['tail_slots'] != geometry['groups'] * geometry['inodes_per_group'] - inodes_count:
        raise Refusal('retained tail-slot count does not close over the geometry')
    if counts['zero_project'] != slots or counts['zero_projinherit'] != slots:
        raise Refusal('retained counters do not record a fully fresh project')
    if counts['checksums_verified'] + counts['zero_base_inodes'] != slots:
        raise Refusal('retained checksum counters do not cover every checked slot')
    return {'kind': MECHANISM_KIND, 'boot_id': rescue_boot, 'project': bound.PROJECT_ID,
            'inodes_count': inodes_count, 'groups': geometry['groups'],
            'coverage_bytes': witness['coverage']['bytes'],
            'witness_sha256': hashlib.sha256(
                json.dumps(witness, sort_keys=True).encode('utf-8')).hexdigest()}
