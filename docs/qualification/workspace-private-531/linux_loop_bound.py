"""Pure physical-loop receipt checks, shared by native observers and offline acceptance.

No filesystem, process, mount, SSH or VM operation is performed here. The caller
must bind originals to the independently frozen source/resource packet.
"""
import hashlib
import json
from pathlib import Path
import re
import struct
import uuid

MECHANISM = 'root-owned-initialized-ext4-loop-v1'
ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'
METADATA_TOLERANCE = 1048576


def mount_field(value):
    return re.sub(r'\\([0-7]{3})', lambda match: chr(int(match[1], 8)), value)



def mount_rows(text):
    result = []
    for line in text.splitlines():
        left, right = line.split(' - ', 1)
        fields, fs = left.split(), right.split()
        if len(fields) < 6 or len(fs) < 3:
            raise ValueError('incomplete mount observation')
        result.append({'device': fields[2], 'root': mount_field(fields[3]),
                       'target': mount_field(fields[4]), 'options': fields[5].split(','),
                       'fstype': fs[0], 'source': mount_field(fs[1]),
                       'super_options': fs[2].split(',')})
    return result



def one_mount(rows, target):
    matches = [row for row in rows if row['target'] == target]
    if len(matches) != 1:
        raise ValueError('mount is absent or stacked: ' + target)
    return matches[0]



def superblock(raw, *, project_quota):
    if len(raw) != 1024 or struct.unpack_from('<H', raw, 0x38)[0] != 0xef53:
        raise ValueError('ext4 superblock is unavailable')
    features = struct.unpack_from('<I', raw, 0x64)[0]
    if project_quota and features & (0x100 | 0x2000) != 0x100 | 0x2000:
        raise ValueError('project/quota features are absent')
    block_log = struct.unpack_from('<I', raw, 0x18)[0]
    if block_log > 6:
        raise ValueError('unsupported ext4 block size')
    blocks = struct.unpack_from('<I', raw, 4)[0]
    incompat = struct.unpack_from('<I', raw, 0x60)[0]
    if incompat & 0x80:
        blocks |= struct.unpack_from('<I', raw, 0x150)[0] << 32
    return {'uuid': str(uuid.UUID(bytes=raw[0x68:0x78])), 'ro_compat': features,
            'capacity_bytes': blocks * (1024 << block_log)}



def verify_extents(extents, capacity):
    logical = 0
    physical = []
    if not extents:
        raise ValueError('image has no allocated extents')
    for index, extent in enumerate(extents):
        start, disk, length, flags = (extent[key] for key in ('logical', 'physical', 'length', 'flags'))
        if (start != logical or disk <= 0 or length <= 0 or flags & ~0x801
                or bool(flags & 1) != (index == len(extents) - 1)):
            raise ValueError('image allocation has holes, unknown flags or an incomplete extent map')
        logical += length
        physical.append((disk, disk + length))
    if logical != capacity:
        raise ValueError('allocated extents do not cover the exact fixed image')
    ordered = sorted(physical)
    if any(left[1] > right[0] for left, right in zip(ordered, ordered[1:])):
        raise ValueError('image extents overlap physically')
    return {'allocated_coverage_bytes': logical, 'extents': extents}



def inode_geometry(raw):
    filesystem = superblock(raw, project_quota=False)
    incompat = struct.unpack_from('<I', raw, 0x60)[0]
    block = 1024 << struct.unpack_from('<I', raw, 0x18)[0]
    size = struct.unpack_from('<H', raw, 0x58)[0]
    descriptor = struct.unpack_from('<H', raw, 0xfe)[0] if incompat & 0x80 else 32
    total, per = struct.unpack_from('<I', raw, 0)[0], struct.unpack_from('<I', raw, 0x28)[0]
    if (not total or not per or size != 256 or descriptor not in (32, 64)
            or incompat & 0x10 or struct.unpack_from('<I', raw, 0x64)[0] & 0x200
            or struct.unpack_from('<I', raw, 0x48)[0] != 0):
        raise ValueError('unsupported ext4 inode geometry')
    return dict(inode_size=size, descriptor_size=descriptor, block_size=block,
                descriptor_base=(struct.unpack_from('<I', raw, 0x14)[0] + 1) * block,
                capacity_bytes=filesystem['capacity_bytes'], total_inodes=total, per_group=per)


def inode_offset(raw, number, descriptor):
    geometry = inode_geometry(raw)
    if type(number) is not int or not 0 < number <= geometry['total_inodes'] or len(descriptor) != geometry['descriptor_size']:
        raise ValueError('complete inode identity and group descriptor required')
    table = struct.unpack_from('<I', descriptor, 8)[0]
    if len(descriptor) == 64:
        table |= struct.unpack_from('<I', descriptor, 0x28)[0] << 32
    offset = table * geometry['block_size'] + ((number - 1) % geometry['per_group']) * geometry['inode_size']
    if not 0 < offset <= geometry['capacity_bytes'] - geometry['inode_size']:
        raise ValueError('inode table exceeds its filesystem')
    return offset


def external_xattr_deduction(original):
    raw = bytes.fromhex(original['superblock_hex'])
    geometry = inode_geometry(raw)
    references = {}
    seen = set()
    for item in original['references']:
        number = item['number']
        inode = bytes.fromhex(item['inode_hex'])
        descriptor = bytes.fromhex(item['group_descriptor_hex'])
        if (number in seen or len(inode) != geometry['inode_size']
                or inode_offset(raw, number, descriptor) != item['inode_offset']):
            raise ValueError('external xattr inode identity differs')
        seen.add(number)
        acl = struct.unpack_from('<I', inode, 0x68)[0] | struct.unpack_from('<H', inode, 0x76)[0] << 32
        sectors = struct.unpack_from('<I', inode, 0x1c)[0] | struct.unpack_from('<H', inode, 0x74)[0] << 32
        if (acl <= 0 or (acl + 1) * geometry['block_size'] > geometry['capacity_bytes']
                or struct.unpack_from('<I', inode, 0x20)[0] & 0x40000
                or type(item['charged_sectors']) is not int or sectors != item['charged_sectors']
                or sectors * 512 < geometry['block_size']):
            raise ValueError('external xattr block or inode charge differs')
        references[acl] = references.get(acl, 0) + 1
    if set(original['headers']) != {str(block) for block in references}:
        raise ValueError('external xattr block originals are incomplete')
    for block, count in references.items():
        header = bytes.fromhex(original['headers'][str(block)])
        if len(header) != 32:
            raise ValueError('external xattr header is incomplete')
        magic, refcount, blocks = struct.unpack_from('<III', header)
        if magic != 0xea020000 or refcount < count or blocks != 1:
            raise ValueError('external xattr references contradict their shared block header')
    return sum(count - 1 for count in references.values()) * geometry['block_size']


def orphan_layout(raw):
    """ext4's source-defined orphan inode, never a guessed metadata allowance."""
    filesystem = superblock(raw, project_quota=False)
    compat, incompat = struct.unpack_from('<II', raw, 0x5c)
    inode = struct.unpack_from('<I', raw, 0x280)[0]
    if not compat & 0x1000:  # EXT4_FEATURE_COMPAT_ORPHAN_FILE
        if inode:
            raise ValueError('orphan inode exists without its filesystem feature')
        return None
    block = 1024 << struct.unpack_from('<I', raw, 0x18)[0]
    first, per_group = struct.unpack_from('<I', raw, 0x14)[0], struct.unpack_from('<I', raw, 0x28)[0]
    size = struct.unpack_from('<H', raw, 0x58)[0]
    descriptor = struct.unpack_from('<H', raw, 0xfe)[0] if incompat & 0x80 else 32
    if (not 0 < inode <= per_group or size != 256 or descriptor not in (32, 64)
            or incompat & 0x10 or struct.unpack_from('<I', raw, 0x64)[0] & 0x200):
        raise ValueError('unsupported ext4 orphan inode geometry')
    return dict(inode=inode, inode_size=size, descriptor_size=descriptor,
                descriptor_offset=(first + 1) * block, block_size=block,
                capacity_bytes=filesystem['capacity_bytes'])


def orphan_inode_offset(raw, descriptor):
    layout = orphan_layout(raw)
    if layout is None or len(descriptor) != layout['descriptor_size']:
        raise ValueError('complete orphan group descriptor required')
    table = struct.unpack_from('<I', descriptor, 8)[0]
    if len(descriptor) == 64:
        table |= struct.unpack_from('<I', descriptor, 0x28)[0] << 32
    offset = table * layout['block_size'] + (layout['inode'] - 1) * layout['inode_size']
    if not 0 < offset <= layout['capacity_bytes'] - layout['inode_size']:
        raise ValueError('orphan inode table exceeds its filesystem')
    return offset


def orphan_allocation(original):
    raw = bytes.fromhex(original['superblock_hex'])
    layout = orphan_layout(raw)
    if layout is None:
        if original['group_descriptor_hex'] is not None or original['inode_hex'] is not None:
            raise ValueError('unexpected orphan inode original')
        return 0
    descriptor = bytes.fromhex(original['group_descriptor_hex'])
    inode = bytes.fromhex(original['inode_hex'])
    offset = orphan_inode_offset(raw, descriptor)
    if len(inode) != layout['inode_size'] or original['inode_offset'] != offset:
        raise ValueError('orphan inode bytes or offset differ')
    mode, uid, gid, links = (struct.unpack_from('<H', inode, value)[0] for value in (0, 2, 0x18, 0x1a))
    flags = struct.unpack_from('<I', inode, 0x20)[0]
    sectors = struct.unpack_from('<I', inode, 0x1c)[0] | struct.unpack_from('<H', inode, 0x74)[0] << 32
    size = struct.unpack_from('<I', inode, 4)[0] | struct.unpack_from('<I', inode, 0x6c)[0] << 32
    if (mode != 0o100600 or uid or gid or links != 1 or flags != 0x80000
            or inode[0x78:0x7c] != bytes(4) or not 0 < size <= 16 * 1024 ** 2
            or size % layout['block_size'] or sectors * 512 != size):
        raise ValueError('orphan inode allocation or custody differs')
    magic, entries, maximum, depth = struct.unpack_from('<HHHH', inode, 0x28)
    logical, length, high, low = struct.unpack_from('<IHHI', inode, 0x34)
    start = (high << 32) | low
    if (magic != 0xf30a or entries != 1 or maximum != 4 or depth or logical
            or length & 0x8000 or length * layout['block_size'] != size or start <= 0
            or (start + length) * layout['block_size'] > layout['capacity_bytes']):
        raise ValueError('orphan inode does not have one complete initialized extent')
    return size


def verify_accounting(value):
    before, after = value['used_bytes_before'], value['used_bytes_after']
    tree = value['inode_allocated_bytes']
    internal = orphan_allocation(value['orphan_inode']) if 'orphan_inode' in value else 0
    deduction = external_xattr_deduction(value['external_xattrs']) if 'external_xattrs' in value else 0
    if 'orphan_inode' in value and (type(value.get('visible_inode_allocated_bytes')) is not int
                     or tree != value['visible_inode_allocated_bytes'] - deduction + internal):
        raise ValueError('visible and internal inode allocation disagree')
    if (not all(type(number) is int and number >= 0 for number in (before, after, tree))
            or before != after or not 0 <= before - tree <= METADATA_TOLERANCE):
        raise ValueError('tree allocation and stable filesystem usage exceed the metadata tolerance: '
                         + repr({'before': before, 'after': after, 'tree': tree, 'gap': before - tree}))
    return dict(value, metadata_bytes=before - tree, metadata_tolerance_bytes=METADATA_TOLERANCE,
                internal_inode_allocated_bytes=internal, shared_xattr_deduction_bytes=deduction)



def visible_mounts(actor):
    rows = mount_rows(actor['mountinfo'])
    by_id = {int(line.split()[0]): row for line, row in zip(actor['mountinfo'].splitlines(), rows)}
    if (len(by_id) != len(rows) or len(actor['mount_visibility']) != len({row['target'] for row in rows})
            or set(item['target'] for item in actor['mount_visibility']) != set(row['target'] for row in rows)):
        raise ValueError('mount visibility observation is incomplete')
    visible = {}
    for item in actor['mount_visibility']:
        row = by_id.get(item['mount_id'])
        opened = item.get('opened_path', item['target'])
        if (row is None or row['device'] != item['device']
                or not Path(item['target']).is_relative_to(Path(opened))
                or not Path(opened).is_relative_to(Path(row['target']))
                or (item.get('missing', False) is not (opened != item['target']))
                or (opened != item['target'] and row['target'] == item['target'])
                or item['read_only'] != ('ro' in row['options'])):
            raise ValueError('visible mount descriptor disagrees with mountinfo')
        visible[item['mount_id']] = row
    return list(visible.values())



def verify_actor(actor, devices, engine_root, memory_max):
    """Check persistent allocation routes, separately from bounded kernel/RAM writes."""
    if not actor['birth'] or memory_max <= 0:
        raise ValueError('writer birth or memory ceiling is absent')
    writable = []
    mounts = visible_mounts(actor)
    for row in mounts:
        if 'rw' not in row['options']:
            continue
        if row['fstype'] == 'overlay':
            options = dict(part.split('=', 1) for part in row['super_options'] if '=' in part)
            for key in ('upperdir', 'workdir'):
                value = Path(options.get(key, ''))
                if not value.is_absolute() or '..' in value.parts or not value.is_relative_to(Path(engine_root)):
                    raise ValueError('overlay allocation escapes the bounded engine')
            kind = 'bounded-overlay'
        elif row['device'] in devices:
            kind = 'bounded-filesystem'
        elif row['fstype'] in ('tmpfs', 'devtmpfs'):
            # The actor's cgroup memory.max/swap.max is independently read back.
            # A tmpfs allocation is memory, not native root disk allocation.
            kind = 'kernel-memory-filesystem'
        elif row['fstype'] in ('proc', 'cgroup2', 'devpts', 'mqueue', 'nsfs'):
            kind = 'kernel-interface'
        else:
            raise ValueError('unbounded writable filesystem: ' + row['target'])
        writable.append(dict(row, allocation_class=kind))
    for item in actor['descriptors'] + actor['shared_writable_mappings']:
        if item['kind'] == 'block':
            raise ValueError('writer holds a writable block-device descriptor')
        if item['kind'] == 'regular' and item['device'] not in devices:
            if item['filesystem_magic'] == 0x01021994:  # TMPFS_MAGIC, including memfd
                continue
            if any(row['device'] == item['device'] and row['fstype'] in ('proc', 'cgroup2')
                   for row in mounts):
                continue
            # An overlay descriptor carries the overlay device, not upperdir's
            # device. It must resolve to this actor's verified writable overlay.
            if not any(row['device'] == item['device'] and row['fstype'] == 'overlay'
                       and 'rw' in row['options'] for row in mounts):
                raise ValueError('writer holds an unbounded writable file descriptor or mapping')
    return {'pid': actor['pid'], 'birth': actor['birth'], 'writable_mounts': writable}


# Fixed outer-file metadata allowance, separate from inner ext4 metadata. Fully
# initialized extents and disabled discard prevent payload writes from creating
# more outer extents. This is retained allocation, never cumulative write IO.
IMAGE_METADATA_MAX = 65536


def loop_status(raw):
    """Linux loop_info64 original (LOOP_GET_STATUS64), no device operation."""
    if len(raw) != 232:
        raise ValueError('loop status original has the wrong size')
    device, inode, rdevice, offset, size, number, crypt, key_size, flags = struct.unpack_from('<QQQQQIIII', raw)
    if crypt or key_size or flags & ~20:  # only AUTOCLEAR / DIRECT_IO; never READ_ONLY
        raise ValueError('loop is encrypted, read-only or has unsupported flags')
    return dict(backing_device=device, backing_inode=inode, backing_rdevice=rdevice,
                offset=offset, size_limit=size, number=number, flags=flags)


def image_custody(value, pin, *, child_mounts=()):
    """Re-derive one initialized image bound against externally frozen pins.

    The authenticated collector retains raw superblocks, mountinfo and ioctl
    bytes. A caller-provided success flag or reduced observation never suffices.
    """
    required = {'image', 'image_device', 'image_inode', 'image_bytes', 'filesystem_uuid', 'mountpoint'}
    if set(pin) != required or type(pin['image_bytes']) is not int or pin['image_bytes'] <= 0:
        raise ValueError('exact frozen physical image pins required')
    image = Path(pin['image'])
    if not image.is_absolute() or str(image) != pin['image'] or '..' in image.parts:
        raise ValueError('noncanonical frozen image path')
    facts = value['originals']
    info = facts['image_stat']
    if (info['device'] != pin['image_device'] or info['inode'] != pin['image_inode']
            or info['size'] != pin['image_bytes'] or info['uid'] != 0 or info['gid'] != 0
            or info['mode'] != 0o100600 or info['links'] != 1
            or not pin['image_bytes'] <= info['blocks'] * 512 <= pin['image_bytes'] + IMAGE_METADATA_MAX):
        raise ValueError('image custody, fixed size or allocation differs')
    ancestors = facts['ancestors']
    if ([item['path'] for item in ancestors] != [str(parent) for parent in image.parents]
            or any(item['uid'] != 0 or item['mode'] & 0o022 or item['mode'] & 0o170000 != 0o040000
                   for item in ancestors)):
        raise ValueError('image ancestor custody is incomplete or writable')
    rows = mount_rows(facts['mountinfo'])
    native, mounted = one_mount(rows, '/'), one_mount(rows, pin['mountpoint'])
    if (native['source'] != '/dev/sda2' or native['fstype'] != 'ext4' or native['root'] != '/'
            or native['device'] != facts['image_device_number']):
        raise ValueError('native root mount association differs')
    outer = superblock(bytes.fromhex(facts['root_superblock_hex']), project_quota=False)
    inner = superblock(bytes.fromhex(facts['image_superblock_hex']), project_quota=False)
    if (outer['uuid'] != ROOT_UUID or inner['uuid'] != pin['filesystem_uuid']
            or inner['capacity_bytes'] != pin['image_bytes']):
        raise ValueError('raw root or image filesystem identity/capacity differs')
    loop = loop_status(bytes.fromhex(facts['loop_status_hex']))
    association = facts['loop']
    if (loop['backing_device'] != pin['image_device'] or loop['backing_inode'] != pin['image_inode']
            or loop['backing_rdevice'] != 0 or loop['offset'] != 0 or loop['size_limit'] != 0
            or association['backing_file'] != pin['image'] or association['capacity_bytes'] != pin['image_bytes']
            or association['discard_max_bytes'] != 0 or association['device'] != mounted['device']):
        raise ValueError('kernel loop association, capacity or discard policy differs')
    if (mounted['fstype'] != 'ext4' or mounted['root'] != '/'
            or not {'rw', 'nodev', 'nosuid'} <= set(mounted['options'])
            or any(row != mounted and (row['device'] == mounted['device']
                   or (row['target'].startswith(pin['mountpoint'] + '/')
                       and row['target'] not in child_mounts)) for row in rows)):
        raise ValueError('image mount is unsafe, aliased or nested')
    extents = value['allocation_map']['extents']
    verify_extents(extents, pin['image_bytes'])
    return dict(pin, device=mounted['device'],
                image_metadata_bytes=info['blocks'] * 512 - pin['image_bytes'],
                initialized=not any(item['flags'] & ~1 for item in extents))


def verify_image(value, pin, *, child_mounts=()):
    bound = image_custody(value, pin, child_mounts=child_mounts)
    if not bound.pop('initialized'):
        raise ValueError('image contains uninitialized allocated extents')
    return bound
