"""Read-only process, writable-mount and descriptor closure for native exploration.

A process name is never writer identity. The caller supplies exact cgroups and
physically bounded mounts; observations retain PID birth, executable digest,
mount originals and descriptor facts. This observer grants no proof credit.
"""
import ctypes
import hashlib
import os
from pathlib import Path
import stat
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
from linux_loop_bound import visible_mounts, verify_actor






def birth(proc):
    return (proc / 'stat').read_text().rpartition(')')[2].split()[19]


def members(cgroup):
    root = Path('/sys/fs/cgroup') / cgroup.lstrip('/')
    if root.resolve(strict=True) != root or not root.is_relative_to('/sys/fs/cgroup/system.slice'):
        raise ValueError('exact system service cgroup required')
    return sorted({int(value) for file in root.rglob('cgroup.procs') for value in file.read_text().split()})


def file_fact(path, **extra):
    info = path.stat()
    descriptor = os.open(path, os.O_PATH)
    try:
        raw = ctypes.create_string_buffer(128)
        library = ctypes.CDLL(None, use_errno=True)
        library.fstatfs.argtypes = [ctypes.c_int, ctypes.c_void_p]
        library.fstatfs.restype = ctypes.c_int
        if library.fstatfs(descriptor, raw) != 0:
            raise OSError(ctypes.get_errno(), 'writer filesystem observation failed')
        magic = struct.unpack_from('@l', raw.raw)[0]
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_mode) != (info.st_dev, info.st_ino, info.st_mode):
            raise ValueError('writer descriptor changed during observation')
        fields = dict(line.split(':', 1) for line in Path('/proc/self/fdinfo/' + str(descriptor)).read_text().splitlines())
        mount_id = int(fields['mnt_id'])
    finally:
        os.close(descriptor)
    kind = ('regular' if stat.S_ISREG(info.st_mode) else 'block' if stat.S_ISBLK(info.st_mode)
            else 'character' if stat.S_ISCHR(info.st_mode) else 'other')
    return dict(extra, kind=kind, filesystem_magic=magic, mount_id=mount_id, device='%d:%d' % (os.major(info.st_dev), os.minor(info.st_dev)),
                inode=info.st_ino, allocated_bytes=info.st_blocks * 512, links=info.st_nlink)


def actor(pid):
    proc = Path('/proc') / str(pid)
    initial = birth(proc)
    executable = proc / 'exe'
    executable_info = executable.stat()
    result = {'pid': pid, 'birth': initial, 'executable': os.readlink(executable),
              'executable_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
              'status': (proc / 'status').read_text(), 'cgroup': (proc / 'cgroup').read_text(),
              'mountinfo': (proc / 'mountinfo').read_text(),
              'descriptors': [], 'shared_writable_mappings': [], 'mount_visibility': []}
    for target in sorted({row['target'] for row in storage.mount_rows(result['mountinfo'])}):
        descriptor = os.open(str(proc / 'root') + target, os.O_PATH)
        try:
            fields = dict(line.split(':', 1) for line in Path('/proc/self/fdinfo/' + str(descriptor)).read_text().splitlines())
            info = os.fstat(descriptor)
            result['mount_visibility'].append({'target': target, 'mount_id': int(fields['mnt_id']),
                'device': '%d:%d' % (os.major(info.st_dev), os.minor(info.st_dev)),
                'read_only': bool(os.fstatvfs(descriptor).f_flag & os.ST_RDONLY)})
        finally:
            os.close(descriptor)
    for fd in sorted((proc / 'fd').iterdir()):
        fields = dict(line.split(':', 1) for line in (proc / 'fdinfo' / fd.name).read_text().splitlines())
        flags = int(fields['flags'].strip(), 8)
        if flags & os.O_ACCMODE in (os.O_WRONLY, os.O_RDWR):
            fact = file_fact(fd, fd=int(fd.name), flags=flags, target=os.readlink(fd))
            after = dict(line.split(':', 1) for line in (proc / 'fdinfo' / fd.name).read_text().splitlines())
            if (any(fields[key] != after[key] for key in ('flags', 'mnt_id', 'ino'))
                    or fact['mount_id'] != int(fields['mnt_id']) or fact['inode'] != int(fields['ino'])):
                raise ValueError('writer descriptor identity or access flags changed')
            result['descriptors'].append(fact)
    for line in (proc / 'maps').read_text().splitlines():
        fields = line.split(None, 5)
        if fields[1][1] == 'w' and fields[1][3] == 's' and int(fields[4]) > 0:
            result['shared_writable_mappings'].append(file_fact(
                proc / 'map_files' / fields[0], mapping=line))
    final_executable = executable.stat()
    if (final_executable.st_dev, final_executable.st_ino) != (executable_info.st_dev, executable_info.st_ino):
        raise ValueError('writer executed another image during observation')
    if (proc / 'mountinfo').read_text() != result['mountinfo']:
        raise ValueError('writer mounts changed during observation')
    if birth(proc) != initial:
        raise ValueError('writer PID was replaced during observation')
    return result


def observe(cgroups, devices, engine_root, memory_max):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise RuntimeError('native root read-only writer observer required')
    controls = {}
    for group in cgroups:
        root = Path('/sys/fs/cgroup') / group.lstrip('/')
        actual = (root / 'memory.max').read_text().strip()
        swap = (root / 'memory.swap.max').read_text().strip()
        if actual != str(memory_max) or swap != '0':
            raise ValueError('writer cgroup memory or swap limit differs')
        controls[group] = {'memory.max': actual, 'memory.swap.max': swap}
    before = {group: members(group) for group in cgroups}
    if not all(before.values()):
        raise ValueError('writer cgroup is empty')
    actors = [actor(pid) for pid in sorted({pid for group in before.values() for pid in group})]
    after = {group: members(group) for group in cgroups}
    if before != after:
        raise ValueError('writer cohort changed during observation')
    verified, findings = [], []
    for item in actors:
        try:
            verified.append(verify_actor(item, devices, engine_root, memory_max))
        except ValueError as error:
            findings.append({'pid': item['pid'], 'reason': str(error)})
    return {'cgroups': before, 'controls': controls, 'actors': actors,
            'verified': verified, 'complete': not findings, 'findings': findings,
            'memory_max': memory_max, 'exploratory': True, 'qualification_credit': False}
