"""Read-only custody of task-owned loop filesystems on the fixed native root.

This supplements, rather than impersonates, the older root-partition activation
route. No mount, formatting, quota assignment or sealed receipt is performed.
"""
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import sys

from linux_loop_bound import (ROOT_UUID, METADATA_TOLERANCE, mount_field, mount_rows, one_mount,
                              superblock, verify_extents, verify_accounting, orphan_layout, orphan_inode_offset, orphan_allocation, inode_geometry, inode_offset, external_xattr_deduction)


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










# Linux UAPI fiemap.h/fs.h: 32-byte header, 56-byte extents. This is a
# descriptor read, not a filefrag text classifier. Only allocated ordinary or
# unwritten extents qualify; delayed, shared and unknown allocation refuses.
FIEMAP_IOCTL = 0xc020660b




def allocated_extents(fd, capacity):
    import fcntl
    extents = []
    start = 0
    while start < capacity:
        raw = bytearray(32 + 256 * 56)
        struct.pack_into('<QQIIII', raw, 0, start, capacity - start, 1, 0, 256, 0)
        fcntl.ioctl(fd, FIEMAP_IOCTL, raw, True)
        _, _, flags, count, slots, reserved = struct.unpack_from('<QQIIII', raw)
        if flags != 1 or not 0 < count <= 256 or slots != 256 or reserved:
            raise ValueError('incomplete or unsupported FIEMAP response')
        for index in range(count):
            values = struct.unpack_from('<QQQQQIIII', raw, 32 + index * 56)
            logical, physical, length, r1, r2, bits, r3, r4, r5 = values
            if any((r1, r2, r3, r4, r5)) or logical != start or length <= 0:
                raise ValueError('noncontiguous or malformed FIEMAP response')
            extents.append(dict(logical=logical, physical=physical, length=length, flags=bits))
            start += length
        if extents[-1]['flags'] & 1:
            break
    return verify_extents(extents, capacity)




def orphan_original(device):
    """Read the exact internal orphan inode through this filesystem's block fd."""
    sysfs = Path('/sys/dev/block') / ('%d:%d' % (os.major(device), os.minor(device)))
    fd = os.open('/dev/' + sysfs.resolve(strict=True).name, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        if os.fstat(fd).st_rdev != device:
            raise ValueError('accounting block device differs')
        raw = os.pread(fd, 1024, 1024)
        layout = orphan_layout(raw)
        result = dict(superblock_hex=raw.hex(), group_descriptor_hex=None, inode_hex=None)
        if layout is not None:
            descriptor = os.pread(fd, layout['descriptor_size'], layout['descriptor_offset'])
            offset = orphan_inode_offset(raw, descriptor)
            result.update(group_descriptor_hex=descriptor.hex(), inode_offset=offset,
                          inode_hex=os.pread(fd, layout['inode_size'], offset).hex())
        orphan_allocation(result)
        return result
    finally:
        os.close(fd)


class AllocationMetadata:
    """Retain real shared-EA references so per-inode charges count a block once."""
    def __init__(self, device):
        node = (Path('/sys/dev/block') / ('%d:%d' % (os.major(device), os.minor(device)))).resolve(strict=True)
        self.fd = os.open('/dev/' + node.name, os.O_RDONLY | os.O_NOFOLLOW)
        if os.fstat(self.fd).st_rdev != device:
            os.close(self.fd)
            raise ValueError('allocation metadata device differs')
        try:
            self.raw = os.pread(self.fd, 1024, 1024)
            self.geometry = inode_geometry(self.raw)
        except BaseException:
            os.close(self.fd)
            raise
        self.groups = {}
        self.original = {'superblock_hex': self.raw.hex(), 'references': [], 'headers': {}}

    def close(self):
        os.close(self.fd)

    def charge(self, info):
        group = (info.st_ino - 1) // self.geometry['per_group']
        if group not in self.groups:
            self.groups[group] = os.pread(self.fd, self.geometry['descriptor_size'],
                self.geometry['descriptor_base'] + group * self.geometry['descriptor_size'])
        descriptor = self.groups[group]
        offset = inode_offset(self.raw, info.st_ino, descriptor)
        inode = os.pread(self.fd, self.geometry['inode_size'], offset)
        acl = struct.unpack_from('<I', inode, 0x68)[0] | struct.unpack_from('<H', inode, 0x76)[0] << 32
        if not acl:
            return
        sectors = struct.unpack_from('<I', inode, 0x1c)[0] | struct.unpack_from('<H', inode, 0x74)[0] << 32
        if sectors != info.st_blocks:
            raise ValueError('inode allocation changed during metadata census')
        header = os.pread(self.fd, 32, acl * self.geometry['block_size']).hex()
        prior = self.original['headers'].setdefault(str(acl), header)
        if prior != header:
            raise ValueError('external xattr block changed during census')
        self.original['references'].append({'number': info.st_ino, 'inode_hex': inode.hex(),
            'group_descriptor_hex': descriptor.hex(), 'inode_offset': offset, 'charged_sectors': info.st_blocks})


def accounting(root):
    """Quiescent no-follow census of the whole dedicated filesystem, including metadata roots."""
    root = Path(root)
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    metadata = None
    try:
        identity = os.fstat(fd)
        # Closing a writer does not force ext4 delayed allocation. Flush this
        # exact filesystem before comparing block allocation with its inode tree.
        import ctypes
        library = ctypes.CDLL(None, use_errno=True)
        library.syncfs.argtypes = [ctypes.c_int]
        library.syncfs.restype = ctypes.c_int
        if library.syncfs(fd) != 0:
            raise OSError(ctypes.get_errno(), 'allocation filesystem flush failed')
        before = os.fstatvfs(fd)
        metadata = AllocationMetadata(identity.st_dev)
        seen = set()
        allocated = 0

        def visit(directory):
            nonlocal allocated
            initial = os.fstat(directory)
            if initial.st_dev != identity.st_dev:
                raise ValueError('allocation census crossed a filesystem')
            key = (initial.st_dev, initial.st_ino)
            if key in seen:
                raise ValueError('allocation census found an aliased directory')
            seen.add(key)
            allocated += initial.st_blocks * 512
            metadata.charge(initial)
            with os.scandir(directory) as entries:
                for entry in entries:
                    info = entry.stat(follow_symlinks=False)
                    if info.st_dev != identity.st_dev:
                        raise ValueError('allocation census crossed a filesystem')
                    if stat.S_ISDIR(info.st_mode):
                        child = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
                        try:
                            opened = os.fstat(child)
                            if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                                raise ValueError('allocation census directory changed')
                            visit(child)
                        finally:
                            os.close(child)
                    else:
                        key = (info.st_dev, info.st_ino)
                        if key not in seen:
                            seen.add(key)
                            allocated += info.st_blocks * 512
                            metadata.charge(info)
                        visible = os.stat(entry.name, dir_fd=directory, follow_symlinks=False)
                        if (visible.st_ino, visible.st_size, visible.st_blocks, visible.st_mtime_ns, visible.st_ctime_ns) != (
                                info.st_ino, info.st_size, info.st_blocks, info.st_mtime_ns, info.st_ctime_ns):
                            raise ValueError('allocation census entry changed')
            final = os.fstat(directory)
            if (final.st_mtime_ns, final.st_ctime_ns) != (initial.st_mtime_ns, initial.st_ctime_ns):
                raise ValueError('allocation census directory changed')

        visit(fd)
        internal = orphan_original(identity.st_dev)
        layout = orphan_layout(bytes.fromhex(internal['superblock_hex']))
        if layout and (identity.st_dev, layout['inode']) in seen:
            raise ValueError('internal orphan inode was also reachable by name')
        after = os.fstatvfs(fd)
        if root.stat().st_ino != identity.st_ino or root.stat().st_dev != identity.st_dev:
            raise ValueError('allocation census root changed')
        return verify_accounting({'device': identity.st_dev, 'root_inode': identity.st_ino,
                'distinct_inodes': len(seen), 'visible_inode_allocated_bytes': allocated,
                'orphan_inode': internal, 'external_xattrs': metadata.original,
                'inode_allocated_bytes': allocated - external_xattr_deduction(metadata.original) + orphan_allocation(internal),
                'used_bytes_before': (before.f_blocks - before.f_bfree) * before.f_frsize,
                'used_bytes_after': (after.f_blocks - after.f_bfree) * after.f_frsize})
    finally:
        if metadata is not None:
            metadata.close()
        os.close(fd)


def accounting_view(root):
    """Census a whole filesystem through a private, nonrecursive readonly bind.

    A separately bounded child mount must not hide the covered parent inodes.
    The child namespace dies with this waited observer; no global mount changes.
    """
    import subprocess
    code = """import os,json,tempfile,subprocess,sys
from pathlib import Path
sys.path.insert(0,sys.argv[2])
import linux_native_storage as storage
source=Path(sys.argv[1]); before=source.stat()
os.unshare(os.CLONE_NEWNS)
subprocess.run(['mount','--make-rprivate','/'],check=True)
target=Path(tempfile.mkdtemp(prefix='census-',dir='/var/lib/effigy-829/helpers'))
subprocess.run(['mount','--bind',str(source),str(target)],check=True)
subprocess.run(['mount','-o','remount,bind,ro',str(target)],check=True)
opened=target.stat()
assert (opened.st_dev,opened.st_ino)==(before.st_dev,before.st_ino)
raw=Path('/proc/self/mountinfo').read_text()
rows=storage.mount_rows(raw)
mounted=storage.one_mount(rows,str(target))
assert mounted['root']=='/' and 'ro' in mounted['options']
assert not any(row['target'].startswith(str(target)+'/') for row in rows)
result=storage.accounting(target)
assert (source.stat().st_dev,source.stat().st_ino)==(before.st_dev,before.st_ino)
result['view']={'source_path':str(source),'target':str(target),'mountinfo':raw}
print(json.dumps(result))
"""
    raw = subprocess.check_output(['/usr/bin/python3', '-I', '-B', '-c', code,
                                   str(root), str(Path(__file__).resolve().parent)], timeout=120)
    return json.loads(raw)


def observe(image, mountpoint, capacity, *, project_quota, child_mounts=()):
    import fcntl
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise RuntimeError('read-only native root observer required')
    for path in (image, mountpoint):
        if not path.is_absolute() or path.resolve(strict=True) != path:
            raise ValueError('canonical image and mount required')
    ancestors = []
    for parent in image.parents:
        info = parent.stat()
        if info.st_uid != 0 or stat.S_IMODE(info.st_mode) & 0o022:
            raise ValueError('image ancestors are not root-controlled')
        ancestors.append({'path': str(parent), 'uid': info.st_uid, 'mode': info.st_mode})
    mountinfo = Path('/proc/self/mountinfo').read_text()
    rows = mount_rows(mountinfo)
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
        extents = allocated_extents(fd, capacity)
        if info.st_blocks * 512 < capacity:
            raise ValueError('image is not fully preallocated')
        inner_raw = os.pread(fd, 1024, 1024)
        fs = superblock(inner_raw, project_quota=project_quota)
        with open('/dev/sda2', 'rb', buffering=0) as root:
            root.seek(1024)
            root_raw = root.read(1024)
            if superblock(root_raw, project_quota=False)['uuid'] != ROOT_UUID:
                raise ValueError('native root UUID differs')
        device = os.stat(mountpoint).st_dev
        major, minor = os.major(device), os.minor(device)
        if (mounted['device'] != '%d:%d' % (major, minor) or mounted['fstype'] != 'ext4'
                or mounted['root'] != '/' or not {'rw', 'nodev', 'nosuid'} <= set(mounted['options'])
                or (project_quota and 'prjquota' not in mounted['super_options'])):
            raise ValueError('loop mount identity or quota mode differs')
        sysfs = Path('/sys/dev/block/%d:%d/loop' % (major, minor))
        block = os.open(mounted['source'], os.O_RDONLY | os.O_NOFOLLOW)
        try:
            block_info = os.fstat(block)
            if not stat.S_ISBLK(block_info.st_mode) or block_info.st_rdev != device:
                raise ValueError('opened loop block device differs from the mount')
            loop_raw = bytearray(232)
            fcntl.ioctl(block, 0x4c05, loop_raw, True)  # LOOP_GET_STATUS64
        finally:
            os.close(block)
        from linux_loop_bound import loop_status
        loop = loop_status(loop_raw)
        if (loop['backing_device'], loop['backing_inode'], loop['offset'], loop['size_limit']) != (
                info.st_dev, info.st_ino, 0, 0):
            raise ValueError('kernel loop backing inode differs from the owned image')
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
               or (row['target'].startswith(str(mountpoint) + '/')
                   and row['target'] not in child_mounts)) for row in rows):
            raise ValueError('loop has aliases or nested mounts')
        visible = image.stat()
        if (visible.st_dev, visible.st_ino, visible.st_size) != (info.st_dev, info.st_ino, info.st_size):
            raise ValueError('image changed during observation')
        return {'image': str(image), 'image_device': info.st_dev, 'image_inode': info.st_ino,
                'image_bytes': info.st_size, 'image_allocated_bytes': info.st_blocks * 512,
                'inner_capacity_bytes': device_bytes, 'filesystem': fs, 'mount': mounted, 'allocation_map': extents,
                'originals': {'mountinfo': mountinfo, 'root_superblock_hex': root_raw.hex(),
                    'image_superblock_hex': inner_raw.hex(), 'loop_status_hex': loop_raw.hex(),
                    'image_device_number': '%d:%d' % (os.major(info.st_dev), os.minor(info.st_dev)),
                    'image_stat': {'device': info.st_dev, 'inode': info.st_ino, 'size': info.st_size,
                        'uid': info.st_uid, 'gid': info.st_gid, 'mode': info.st_mode,
                        'links': info.st_nlink, 'blocks': info.st_blocks}, 'ancestors': ancestors,
                    'loop': {'device': mounted['device'], 'backing_file': str(image),
                        'capacity_bytes': device_bytes,
                        'discard_max_bytes': int((sysfs.parent / 'queue/discard_max_bytes').read_text())}},
                'native_root_uuid': ROOT_UUID, 'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                'exploratory': True, 'qualification_credit': False}
    finally:
        os.close(fd)


def disable_discard(image, mountpoint, capacity, *, child_mounts=()):
    """Fixed-image activation: prevent a later trim from punching outer holes."""
    before = observe(image, mountpoint, capacity, project_quota=False, child_mounts=child_mounts)
    path = Path('/sys/dev/block') / before['mount']['device'] / 'queue/discard_max_bytes'
    if int(path.read_text()) != 0:
        path.write_text('0\n')
    after = observe(image, mountpoint, capacity, project_quota=False, child_mounts=child_mounts)
    if (after['image_inode'] != before['image_inode'] or after['image_device'] != before['image_device']
            or after['originals']['loop']['discard_max_bytes'] != 0):
        raise ValueError('discard policy or loop backing changed during activation')
    return after


if __name__ == '__main__':
    if len(sys.argv) != 5 or sys.argv[4] not in ('project-quota', 'capacity'):
        raise SystemExit('usage: linux_native_storage.py IMAGE MOUNT CAPACITY project-quota|capacity')
    print(json.dumps(observe(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]),
                             project_quota=sys.argv[4] == 'project-quota'), indent=2))
