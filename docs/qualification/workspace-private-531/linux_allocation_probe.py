"""Fixed read-only observation helper for the ext4 project-quota allocation bound (source only).

Two modes. Each prints key=value lines to stdout and writes nothing:

  quota DEVICE PROJECT_ID   Q_GETFMT and Q_GETQUOTA for one project on the block device.
  tree ROOT PROJECT_ID      lstat (never followed) and FS_IOC_FSGETXATTR project id and flags for each entry.

A failed kernel call prints `status=refused` with its errno and exits 3. The verifier in
linux_allocation_bound.py refuses any output it cannot fully parse. Nothing here runs from a
check or a VM route: a reviewed native route must run it, and its bytes are retained as originals.
"""
import ctypes
import fcntl
import os
import stat
import struct
import sys

Q_GETFMT = 0x800004                  # linux/quota.h
Q_GETQUOTA = 0x800007
Q_SETQUOTA = 0x800008
PRJQUOTA = 2
QB_UNIT_BYTES = 1024                 # if_dqblk block limits are in 1 KiB quota blocks (QIF_DQBLKSIZE_BITS = 10)
FS_IOC_FSGETXATTR = 0x801C581F       # _IOR('X', 31, struct fsxattr)
FS_IOC_FSSETXATTR = 0x401C5820       # _IOW('X', 32, struct fsxattr)
FS_XFLAG_PROJINHERIT = 0x00000200
FSXATTR = struct.Struct('=5I8s')     # xflags extsize nextents projid cowextsize pad
BOOT_ID = '/proc/sys/kernel/random/boot_id'
SAFE_CHARS = frozenset('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-')


class DQBlk(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in
                ('bhard', 'bsoft', 'cur', 'ihard', 'isoft', 'curinodes', 'btime', 'itime')] + \
               [('valid', ctypes.c_uint32)]


def limit_record(charged_bytes, row_bytes):
    """Construct a prospective block-limit record; this function performs no syscall.

    A shared project's hard limit must leave exactly the row reservation, not
    merely have enough global headroom.  Refuse fractional quota blocks rather
    than round up beyond the row's ceiling.
    """
    if any(not isinstance(v, int) or isinstance(v, bool) or v < 0
           for v in (charged_bytes, row_bytes)) or row_bytes == 0:
        raise Refused('invalid-limit')
    total = charged_bytes + row_bytes
    if total % QB_UNIT_BYTES or total // QB_UNIT_BYTES > (1 << 64) - 1:
        raise Refused('unrepresentable-limit')
    block = DQBlk()
    block.bhard = total // QB_UNIT_BYTES
    block.valid = 1  # QIF_BLIMITS: modify block limits only
    return block


class Refused(Exception):
    def __init__(self, phase, errno_value=None):
        super().__init__(phase)
        self.phase = phase
        self.errno = errno_value


def prospective_project_assignment(original):
    """Build a fixed project-531/inheritance value, without invoking an ioctl."""
    if not isinstance(original, bytes) or len(original) != FSXATTR.size:
        raise Refused('project-original-malformed')
    flags, extent, count, project, cowextent, padding = FSXATTR.unpack(original)
    if project != 0 or flags & FS_XFLAG_PROJINHERIT or padding != b'\0' * 8:
        raise Refused('project-prestate-not-empty')
    return FSXATTR.pack(flags | FS_XFLAG_PROJINHERIT, extent, count, 531, cowextent, padding)


def _quotactl(cmd, device, project_id, addr):
    libc = ctypes.CDLL(None, use_errno=True)
    if not hasattr(libc, 'quotactl'):
        raise Refused('quotactl-unavailable')
    rc = libc.quotactl(ctypes.c_int(cmd << 8 | PRJQUOTA), device.encode(), ctypes.c_int(project_id), addr)
    return rc, ctypes.get_errno()


def boot_id(path=BOOT_ID):
    with open(path, 'r', encoding='ascii') as handle:
        value = handle.read().strip()
    if len(value) != 36:
        raise Refused('boot-id-malformed')
    return value


def quota_lines(device, project_id, call=_quotactl, read_boot=boot_id, stat_device=os.stat):
    """One Q_GETFMT and one Q_GETQUOTA for the project. Kernel errors are printed, never turned into zeros."""
    info = stat_device(device)
    lines = ['boot_id=' + read_boot(), 'device=' + device, 'dev=%d:%d' % (os.major(info.st_rdev), os.minor(info.st_rdev)),
             'project_id=%d' % project_id, 'qb_unit_bytes=%d' % QB_UNIT_BYTES]
    fmt = ctypes.c_uint32(0)
    rc, err = call(Q_GETFMT, device, project_id, ctypes.byref(fmt))
    lines.append('getfmt_rc=%d getfmt_errno=%d fmt=%d' % (rc, err, fmt.value))
    block = DQBlk()
    rc, err = call(Q_GETQUOTA, device, project_id, ctypes.byref(block))
    lines.append('getquota_rc=%d getquota_errno=%d' % (rc, err))
    if rc == 0:
        lines.append('bhard_qb=%d bsoft_qb=%d curspace_bytes=%d valid=%d' %
                     (block.bhard, block.bsoft, block.cur, block.valid))
    return lines


def _project_attributes(fd, read=None):
    if read is not None:
        return read(fd)
    buf = bytearray(FSXATTR.size)
    fcntl.ioctl(fd, FS_IOC_FSGETXATTR, buf)
    fields = FSXATTR.unpack(bytes(buf))
    return fields[3], fields[0]


def _safe_name(name):
    return bool(name) and name not in ('.', '..') and all(ch in SAFE_CHARS for ch in name)


def tree_lines(root, project_id, read_boot=boot_id, read_project=None):
    """Every entry under root, lstat-first and never followed. An unsafe name refuses the whole walk."""
    lines = ['boot_id=' + read_boot(), 'root=' + root, 'project_id=%d' % project_id]
    if not stat.S_ISDIR(os.lstat(root).st_mode):
        raise Refused('root-not-directory')
    pending = [('.', root)]
    while pending:
        relative, absolute = pending.pop()
        info = os.lstat(absolute)
        kind = 'dir' if stat.S_ISDIR(info.st_mode) else 'reg' if stat.S_ISREG(info.st_mode) else \
            'fifo' if stat.S_ISFIFO(info.st_mode) else 'lnk' if stat.S_ISLNK(info.st_mode) else 'other'
        projid, xflags = -1, -1
        # A FIFO is never opened: opening it can block on the supervisor's gate, and its inode carries no
        # block charge, so its project id is not read.
        if kind in ('dir', 'reg'):
            flags = os.O_RDONLY | os.O_NOFOLLOW | (os.O_DIRECTORY if kind == 'dir' else 0)
            try:
                fd = os.open(absolute, flags)
            except OSError as exc:
                raise Refused('open-' + kind, exc.errno)
            try:
                projid, xflags = _project_attributes(fd, read_project)
            except OSError as exc:
                raise Refused('fsgetxattr-' + kind, exc.errno)
            finally:
                os.close(fd)
        lines.append('entry path=%s type=%s dev=%d:%d ino=%d uid=%d gid=%d mode=%o nlink=%d projid=%d xflags=%d '
                     'blocks512=%d size=%d' % (relative, kind, os.major(info.st_dev), os.minor(info.st_dev),
                                               info.st_ino, info.st_uid, info.st_gid,
                                               stat.S_IMODE(info.st_mode), info.st_nlink, projid, xflags,
                                               info.st_blocks, info.st_size))
        if kind == 'dir':
            for child in sorted(os.listdir(absolute)):
                if not _safe_name(child):
                    raise Refused('unsafe-name')
                child_relative = child if relative == '.' else relative + '/' + child
                pending.append((child_relative, os.path.join(absolute, child)))
    return lines


def main(argv):
    if len(argv) != 3 or argv[0] not in ('quota', 'tree'):
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    try:
        project_id = int(argv[2])
        lines = quota_lines(argv[1], project_id) if argv[0] == 'quota' else tree_lines(argv[1], project_id)
    except Refused as exc:
        print('status=refused phase=%s errno=%s' % (exc.phase, exc.errno))
        return 3
    except (OSError, ValueError) as exc:
        print('status=refused phase=unexpected errno=%s' % getattr(exc, 'errno', None))
        return 3
    print('\n'.join(lines))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
