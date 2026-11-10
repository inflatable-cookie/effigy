"""Read-only custody of task-owned loop filesystems on the fixed native root.

This supplements, rather than impersonates, the older root-partition activation
route. No mount, formatting, quota assignment or sealed receipt is performed.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import sys
import uuid

ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'


def verify_workload(row, checkout, engine_root):
    """Admit only the observed bounded fixture's ordinary Docker workload."""
    host = row['HostConfig']
    if (host['Privileged'] or host.get('CapAdd') or any(host.get(key) == 'host'
            for key in ('PidMode', 'IpcMode', 'NetworkMode', 'UTSMode', 'UsernsMode', 'CgroupnsMode'))):
        raise ValueError('privileged, added-capability or host-namespace workload refused')
    if any('unconfined' in option for option in host.get('SecurityOpt') or []):
        raise ValueError('default workload security profile was disabled')
    roots = (Path(checkout), Path(engine_root))
    for mount in row['Mounts']:
        if mount['Type'] == 'tmpfs':
            continue
        source = Path(mount['Source'])
        if (not source.is_absolute() or str(source) != mount['Source'] or '..' in source.parts
                or not any(source.is_relative_to(root) for root in roots)):
            raise ValueError('workload mount escapes the owned bounded roots')
    return {'mounts': row['Mounts'], 'host_config_sha256': hashlib.sha256(
            json.dumps(host, sort_keys=True).encode()).hexdigest()}


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


def observe(image, mountpoint, capacity, *, project_quota):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise RuntimeError('read-only native root observer required')
    for path in (image, mountpoint):
        if not path.is_absolute() or path.resolve(strict=True) != path:
            raise ValueError('canonical image and mount required')
    for parent in image.parents:
        info = parent.stat()
        if info.st_uid != 0 or stat.S_IMODE(info.st_mode) & 0o022:
            raise ValueError('image ancestors are not root-controlled')
    rows = mount_rows(Path('/proc/self/mountinfo').read_text())
    native = one_mount(rows, '/')
    mounted = one_mount(rows, str(mountpoint))
    if native['fstype'] != 'ext4' or native['source'] != '/dev/sda2' or native['root'] != '/':
        raise ValueError('native root association differs')
    fd = os.open(image, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1
                or stat.S_IMODE(info.st_mode) != 0o600 or info.st_size != capacity
                or info.st_dev != os.stat('/').st_dev):
            raise ValueError('image owner, size, mode or backing differs')
        fs = superblock(os.pread(fd, 1024, 1024), project_quota=project_quota)
        with open('/dev/sda2', 'rb', buffering=0) as root:
            root.seek(1024)
            if superblock(root.read(1024), project_quota=False)['uuid'] != ROOT_UUID:
                raise ValueError('native root UUID differs')
        device = os.stat(mountpoint).st_dev
        major, minor = os.major(device), os.minor(device)
        if (mounted['device'] != '%d:%d' % (major, minor) or mounted['fstype'] != 'ext4'
                or mounted['root'] != '/' or 'rw' not in mounted['options']
                or (project_quota and 'prjquota' not in mounted['super_options'])):
            raise ValueError('loop mount identity or quota mode differs')
        sysfs = Path('/sys/dev/block/%d:%d/loop' % (major, minor))
        device_bytes = int((sysfs.parent / 'size').read_text()) * 512
        if device_bytes != capacity or fs['capacity_bytes'] != capacity:
            raise ValueError('kernel device or ext4 capacity differs from the image')
        if (Path('/' + (sysfs / 'backing_file').read_text().strip().lstrip('/')) != image
                or int((sysfs / 'offset').read_text()) != 0
                or int((sysfs / 'sizelimit').read_text()) != 0):
            raise ValueError('loop backing file, offset or size limit differs')
        # Reject nested or same-device alternate mounts: these would escape a
        # path-only census even when the loop image itself remains bounded.
        if any(row != mounted and (row['device'] == mounted['device']
               or row['target'].startswith(str(mountpoint) + '/')) for row in rows):
            raise ValueError('loop has aliases or nested mounts')
        visible = image.stat()
        if (visible.st_dev, visible.st_ino, visible.st_size) != (info.st_dev, info.st_ino, info.st_size):
            raise ValueError('image changed during observation')
        return {'image': str(image), 'image_device': info.st_dev, 'image_inode': info.st_ino,
                'image_bytes': info.st_size, 'image_allocated_bytes': info.st_blocks * 512,
                'inner_capacity_bytes': device_bytes, 'filesystem': fs, 'mount': mounted,
                'native_root_uuid': ROOT_UUID, 'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                'exploratory': True, 'qualification_credit': False}
    finally:
        os.close(fd)


if __name__ == '__main__':
    if len(sys.argv) != 5 or sys.argv[4] not in ('project-quota', 'capacity'):
        raise SystemExit('usage: linux_native_storage.py IMAGE MOUNT CAPACITY project-quota|capacity')
    print(json.dumps(observe(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]),
                             project_quota=sys.argv[4] == 'project-quota'), indent=2))
