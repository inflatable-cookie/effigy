"""Unsealed ENOSPC control for a task-owned, physically bounded filesystem.

Run as the native account or the actual non-root container writer. Only the
newly created probe inode is truncated after observation; its directory remains.
"""
import errno
import json
import os
from pathlib import Path
import sys
import tempfile


def probe(root, capacity):
    root = root.resolve(strict=True)
    allowed = (Path('/Volumes/Stash/scratch/effigy/linux-native-829'),
               Path('/Volumes/Stash/scratch/effigy/linux-native-829/qualification'),
               Path('/var/lib/effigy-829/rootful/client'))
    if (os.geteuid() != 1001 or capacity not in (1073741824, 2147483648, 3221225472, 42949672960)
            or not any(root == parent or root.is_relative_to(parent) for parent in allowed)):
        raise ValueError('task-owned native writer and capacity required')
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    if int(status['CapEff'].strip(), 16):
        raise ValueError('capacity writer has effective capabilities')
    before = os.statvfs(root)
    if not 0 < before.f_blocks * before.f_frsize <= capacity or root.stat().st_dev == os.stat('/').st_dev:
        raise ValueError('capacity control does not match its physical filesystem')
    directory = Path(tempfile.mkdtemp(prefix='enospc-', dir=root))
    path = directory / 'payload'
    fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    initial = os.fstat(fd)
    try:
        block = bytes(1024 * 1024)
        failure = None
        while os.lseek(fd, 0, os.SEEK_CUR) <= capacity:
            try:
                os.write(fd, block)
            except OSError as error:
                failure = error.errno
                break
        if failure != errno.ENOSPC:
            raise RuntimeError('physical ENOSPC was not observed: ' + str(failure))
        allocated = os.fstat(fd)
        if not 0 < allocated.st_size <= capacity or allocated.st_blocks * 512 > capacity:
            raise RuntimeError('probe allocation exceeds the physical ceiling')
        result = {'qualification_credit': False, 'errno': failure,
                  'uid': os.geteuid(), 'cap_eff': status['CapEff'].strip(),
                  'seccomp': status['Seccomp'].strip(), 'path': str(path),
                  'device': allocated.st_dev, 'inode': allocated.st_ino,
                  'written_bytes': allocated.st_size, 'allocated_bytes': allocated.st_blocks * 512,
                  'capacity_bytes': capacity, 'filesystem_bytes': before.f_blocks * before.f_frsize}
    finally:
        visible = path.lstat()
        if (visible.st_dev, visible.st_ino) != (initial.st_dev, initial.st_ino):
            os.close(fd)
            raise RuntimeError('probe inode changed; retained without truncation')
        os.ftruncate(fd, 0)
        os.fsync(fd)
        os.close(fd)
    result['retained_empty_inode'] = path.stat().st_size == 0
    return result


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('usage: linux_native_capacity.py ROOT CAPACITY')
    print(json.dumps(probe(Path(sys.argv[1]), int(sys.argv[2])), indent=2))
