"""Local filesystem admission primitives for later authenticated transport."""
import hashlib
import os
from pathlib import Path
import stat


class Refusal(ValueError):
    pass


def empty_root(path, expected_uid):
    path = Path(path)
    info = path.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != expected_uid
            or stat.S_IMODE(info.st_mode) != 0o700 or any(path.iterdir())):
        raise Refusal('qualification root must be owned, private and empty')
    if path.resolve() != path:
        raise Refusal('qualification root must be canonical')
    return {'path': str(path), 'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}


def verify_root(path, identity):
    if empty_root(path, identity['uid']) != identity:
        raise Refusal('qualification root identity drift')


def place_file(directory, name, data, expected_hash, uid):
    directory = Path(directory)
    info = directory.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != uid or stat.S_IMODE(info.st_mode) != 0o700
            or directory.resolve() != directory or not name or name in ('.', '..') or '/' in name):
        raise Refusal('unsafe placement directory or name')
    if hashlib.sha256(data).hexdigest() != expected_hash:
        raise Refusal('placement hash mismatch')
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(directory / name, flags, 0o400)
    try:
        with os.fdopen(fd, 'wb') as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        with open(directory / name, 'rb') as source:
            if hashlib.sha256(source.read()).hexdigest() != expected_hash:
                raise Refusal('placement readback mismatch')
    except BaseException:
        # Retain partial files: the spent intent must be reconciled, not replayed.
        raise
    return {'path': str(directory / name), 'sha256': expected_hash, 'bytes': len(data)}


def filesystem_controls(directory, uid):
    """Exercise actual local primitives in an empty, private, retained directory."""
    import array
    import fcntl
    import socket

    directory = Path(directory)
    empty_root(directory, uid)
    flags = os.O_RDWR | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(directory / 'control-source', flags, 0o600)
    try:
        os.write(fd, b'placement-control')
        os.fsync(fd)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        left, right = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            left.sendmsg([b'x'], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', [fd]))])
            _, ancillary, _, _ = right.recvmsg(1, socket.CMSG_SPACE(array.array('i').itemsize))
            received = array.array('i')
            for level, kind, value in ancillary:
                if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                    received.frombytes(value[:array.array('i').itemsize])
            if len(received) != 1:
                raise Refusal('descriptor transfer failed')
            try:
                os.lseek(received[0], 0, os.SEEK_SET)
                if os.read(received[0], 17) != b'placement-control':
                    raise Refusal('descriptor readback failed')
            finally:
                os.close(received[0])
        finally:
            left.close()
            right.close()
        os.link(directory / 'control-source', directory / 'control-published', follow_symlinks=False)
        try:
            os.link(directory / 'control-source', directory / 'control-published', follow_symlinks=False)
        except FileExistsError:
            pass
        else:
            raise Refusal('no-clobber publication failed')
        directory_fd = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
        return {'device': os.fstat(fd).st_dev, 'inode': os.fstat(fd).st_ino,
                'socket_fd_transfer': True, 'lock': True, 'no_clobber': True, 'fsync': True}
    finally:
        os.close(fd)
