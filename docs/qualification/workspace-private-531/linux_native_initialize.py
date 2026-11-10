"""Initialize only task 829's existing images without changing their bytes.

The host entry requires the task-owned pre-change snapshot and closes the exact
engine first. This role independently requires empty engine custody, unmounts
only the verified images, rewrites each original block unchanged, and verifies
its complete before/after digest. It never formats, truncates, replaces or grows
an image. Failure retains every image and re-establishes only verified mounts.
"""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
from linux_loop_bound import verify_image
from linux_native_resume import IMAGES, INODES


def pins(image, target, capacity, filesystem_uuid):
    return {'image': str(image), 'image_device': os.stat('/').st_dev,
            'image_inode': INODES[image.name], 'image_bytes': capacity,
            'filesystem_uuid': filesystem_uuid, 'mountpoint': str(target)}


def mounted(target):
    return any(row['target'] == str(target) for row in storage.mount_rows(Path('/proc/self/mountinfo').read_text()))


def unassociated(image):
    for path in Path('/sys/block').glob('loop*/loop/backing_file'):
        try:
            backing = path.read_text().strip()
        except FileNotFoundError:
            continue
        if Path('/' + backing.lstrip('/')) == image:
            raise ValueError('unmounted image still has a loop association')


def rewrite_same(fd, capacity):
    before = hashlib.sha256()
    offset = 0
    while offset < capacity:
        block = os.pread(fd, min(1048576, capacity - offset), offset)
        if len(block) != min(1048576, capacity - offset):
            raise ValueError('image read is incomplete')
        before.update(block)
        count = 0
        while count < len(block):
            written = os.pwrite(fd, block[count:], offset + count)
            if written <= 0:
                raise ValueError('image rewrite is incomplete')
            count += written
        offset += len(block)
    os.fsync(fd)
    after = hashlib.sha256()
    offset = 0
    while offset < capacity:
        block = os.pread(fd, min(1048576, capacity - offset), offset)
        if not block:
            raise ValueError('image verification read is incomplete')
        after.update(block)
        offset += len(block)
    if before.digest() != after.digest():
        raise ValueError('same-byte initialization changed image content')
    allocation = storage.allocated_extents(fd, capacity)
    if any(item['flags'] & ~1 for item in allocation['extents']):
        raise ValueError('outer image still has uninitialized extents')
    return {'content_sha256_before': before.hexdigest(), 'content_sha256_after': after.hexdigest(),
            'rewritten_bytes': capacity, 'allocation_map': allocation}


def initialize(stages):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise ValueError('native fixed-image activation role required')
    raw = subprocess.check_output(['systemctl', 'show', '--all', 'effigy-829-rootful.service',
                                   '--property=ActiveState,MainPID,ControlGroup'], text=True)
    state = dict(line.split('=', 1) for line in raw.splitlines())
    group = Path('/sys/fs/cgroup/system.slice/effigy-829-rootful.service')
    if (state != {'ActiveState': 'inactive', 'MainPID': '0', 'ControlGroup': ''}
            or any(file.read_text().split() for file in group.rglob('cgroup.procs'))):
        raise ValueError('engine custody is not independently closed')
    for image, target, capacity, filesystem_uuid in IMAGES:
        value = storage.observe(image, target, capacity, project_quota=False)
        pin = pins(image, target, capacity, filesystem_uuid)
        if (value['image_inode'] != pin['image_inode'] or value['filesystem']['uuid'] != filesystem_uuid):
            raise ValueError('existing task-owned image identity differs')
    try:
        for image, target, _, _ in IMAGES:
            subprocess.run(['umount', '--', str(target)], check=True)
            if mounted(target):
                raise ValueError('owned image mount remains')
            unassociated(image)
            stages.append({'image': str(image), 'unmounted': True})
        for image, _, capacity, _ in IMAGES:
            unassociated(image)
            fd = os.open(image, os.O_RDWR | os.O_NOFOLLOW)
            try:
                info = os.fstat(fd)
                if (not stat.S_ISREG(info.st_mode) or info.st_ino != INODES[image.name]
                        or info.st_dev != os.stat('/').st_dev or info.st_uid != 0 or info.st_gid != 0
                        or info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600 or info.st_size != capacity):
                    raise ValueError('offline image descriptor differs')
                result = rewrite_same(fd, capacity)
                visible = image.lstat()
                if (visible.st_dev, visible.st_ino, visible.st_size) != (info.st_dev, info.st_ino, capacity):
                    raise ValueError('offline image path changed')
                stages.append({'image': str(image), **result})
            finally:
                os.close(fd)
    finally:
        # No general repair: restore only these exact images, after rechecking
        # the immutable inode/size and absence of an existing loop association.
        restore_errors = []
        for image, target, capacity, _ in IMAGES:
            try:
                if not mounted(target):
                    if image.lstat().st_ino != INODES[image.name] or image.stat().st_size != capacity:
                        raise ValueError('image changed; refusing restoration')
                    unassociated(image)
                    subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(image), str(target)], check=True)
                value = storage.disable_discard(image, target, capacity)
                stages.append({'image': str(image), 'restored': True,
                               'discard_max_bytes': value['originals']['loop']['discard_max_bytes']})
            except Exception as error:
                restore_errors.append(str(error))
                stages.append({'image': str(image), 'restore_error': str(error)})
        if restore_errors:
            raise RuntimeError('owned image restoration failed: ' + '; '.join(restore_errors))
    for image, target, capacity, filesystem_uuid in IMAGES:
        value = storage.observe(image, target, capacity, project_quota=False)
        stages.append({'image': str(image), 'bound': verify_image(value, pins(image, target, capacity, filesystem_uuid))})
    return {'schema': 'effigy.linux-loop-initialization.v1', 'stages': stages,
            'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
            'exploratory': True, 'qualification_credit': False}


if __name__ == '__main__':
    stages = []
    try:
        print(json.dumps(initialize(stages), indent=2))
    except BaseException as error:
        print(json.dumps({'passed': False, 'error': str(error), 'stages': stages,
                          'exploratory': True, 'qualification_credit': False}, indent=2))
        raise
