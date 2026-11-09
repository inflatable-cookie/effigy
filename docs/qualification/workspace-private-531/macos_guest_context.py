"""Current macOS guest-context helper: the probe, stage and verify modes that the C2, C3 and C4 manifest
argv run inside the Colima guest as `python3 -I -B -c <this file> <mode> <spec>`.

Copied from the historical image_context.py, which is preserved unchanged for the 531 packet. The one
contract change: `stage` also returns the parent binding, so the C4 verify spec can be derived from the
C3 stage output. No effects on import. This file is pinned by sha256 in macos_stage_executor.py.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

RECIPE_SHA256 = 'c00d5bd909e65969c70dc78b16781812a98e507c20a068cd573d70a20480fa1a'
SHARED = Path('/Volumes/Builds/scratch/effigy/531-colima-qualification')


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def binding(path):
    info = path.lstat()
    return dict(path=str(path), device=info.st_dev, inode=info.st_ino, uid=info.st_uid)


def open_parent(expected):
    path = Path(expected['path'])
    require(path.is_absolute() and str(path) == expected['path'] and '..' not in path.parts
            and path != Path('/') and path.resolve(strict=True) == path
            and not any(p.is_symlink() for p in (path, *path.parents))
            and not path.is_relative_to(SHARED)
            and not path.is_relative_to(Path('/Volumes/Builds/scratch/effigy/531c')),
            'private canonical guest parent required')
    require(all(type(expected[k]) is int and expected[k] >= 0 for k in ('device', 'inode', 'uid')),
            'actual guest parent identity required')
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        held, visible = os.fstat(fd), path.lstat()
        require(binding(path) == expected and held == visible and held.st_uid == os.geteuid()
                and not stat.S_IMODE(held.st_mode) & 0o022, 'guest parent changed or foreign')
        return path, fd
    except BaseException:
        os.close(fd)
        raise


def verify_context(parent, name, expected=None):
    require(re.fullmatch(r'effigy-531-image-[0-9a-f]{32}', name), 'fresh context token required')
    path, parent_fd = open_parent(parent)
    try:
        fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
        try:
            held = os.fstat(fd)
            require(held == os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
                    and held.st_uid == os.geteuid() and stat.S_IMODE(held.st_mode) == 0o700
                    and os.listdir(fd) == ['Dockerfile'], 'context changed, foreign or nonexclusive')
            file_fd = os.open('Dockerfile', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
            with os.fdopen(file_fd, 'rb') as source:
                info = os.fstat(source.fileno())
                require(stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid()
                        and info.st_nlink == 1 and stat.S_IMODE(info.st_mode) == 0o600,
                        'recipe ownership changed')
                digest = hashlib.sha256(source.read(8193)).hexdigest()
            require(digest == RECIPE_SHA256, 'recipe readback hash mismatch')
            result = dict(path=str(path / name), device=held.st_dev, inode=held.st_ino,
                          uid=held.st_uid, file_device=info.st_dev, file_inode=info.st_ino,
                          sha256=digest)
            require(expected is None or result == expected, 'context identity changed')
            return result
        finally:
            os.close(fd)
    finally:
        os.close(parent_fd)


def stage(parent, name, recipe, expected_hash):
    # Missing/wrong bytes refuse before even creating a guest context.
    require(expected_hash == RECIPE_SHA256 and 0 < len(recipe) <= 8192
            and hashlib.sha256(recipe).hexdigest() == expected_hash, 'reviewed recipe hash required')
    require(re.fullmatch(r'effigy-531-image-[0-9a-f]{32}', name), 'fresh context token required')
    _, parent_fd = open_parent(parent)
    try:
        os.mkdir(name, mode=0o700, dir_fd=parent_fd)  # Existing identity always refuses.
        fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
        try:
            require(os.fstat(fd) == os.stat(name, dir_fd=parent_fd, follow_symlinks=False),
                    'created context replaced')
            file_fd = os.open('Dockerfile', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                              0o600, dir_fd=fd)
            with os.fdopen(file_fd, 'wb') as target:
                target.write(recipe)
                target.flush()
                os.fsync(target.fileno())
            os.fsync(fd)
            os.fsync(parent_fd)
        finally:
            os.close(fd)
    finally:
        os.close(parent_fd)
    result = verify_context(parent, name)
    result['parent'] = parent
    return result


def probe():
    parent = Path.home()
    expected = binding(parent)
    _, fd = open_parent(expected)
    os.close(fd)
    def mounts(args):
        result = subprocess.run(['findmnt', '-J'] + args,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5)
        require(result.returncode == 0, 'guest mount observation unavailable')
        return json.loads(result.stdout)['filesystems']
    storage = mounts(['-T', str(parent), '-o', 'TARGET,SOURCE,FSTYPE'])
    require(len(storage) == 1 and storage[0]['fstype'] == 'ext4', 'private native ext4 parent required')
    shares = mounts(['-t', 'virtiofs,9p,fuse.sshfs', '-o', 'TARGET,SOURCE,FSTYPE,OPTIONS'])
    return dict(parent=expected, storage=storage, shares=shares, uid=os.geteuid(),
                architecture=os.uname().machine, machine_id=Path('/etc/machine-id').read_text().strip())


def main():
    mode = sys.argv[1]
    if mode == 'probe':
        result = probe()
    else:
        spec = json.loads(sys.argv[2])
        if mode == 'stage':
            result = stage(spec['parent'], spec['name'], sys.stdin.buffer.read(8193), spec['sha256'])
        else:
            require(mode == 'verify', 'unknown guest context operation')
            result = verify_context(spec['parent'], spec['name'], spec['context'])
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
