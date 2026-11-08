"""Private, non-qualifying Linux observation controls.

This module alone cannot activate a native observation. The Nucleus proof-only
entry must bind the exact source, programs, roots and a fresh launch window.
"""
import hashlib
import os
from pathlib import Path
import stat
import xml.etree.ElementTree as ET

from preparation_stage import Refusal, exclusive
import preparation_observations as observations

CAP = 1024 ** 3


def retain_originals(root, sources, prefix):
    """Retain independent originals even when another original cannot be read."""
    facts, errors = {}, []
    for name, source in sources.items():
        try:
            path = Path(source)
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid() or info.st_size > 16 * 1024 * 1024:
                raise Refusal('not an independent regular file')
            fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
            with os.fdopen(fd, 'rb') as source_file:
                opened = os.fstat(source_file.fileno())
                if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                    raise Refusal('original identity changed while opening')
                data = source_file.read()
                final = os.fstat(source_file.fileno())
                if (final.st_size != len(data) or (opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) !=
                        (final.st_size, final.st_mtime_ns, final.st_ctime_ns)):
                    raise Refusal('original changed while reading')
            exclusive(Path(root) / (prefix + '-' + name), data)
            facts[name] = {'sha256': hashlib.sha256(data).hexdigest(),
                           'device': info.st_dev, 'inode': info.st_ino, 'size': len(data)}
        except (OSError, Refusal) as exc:
            errors.append(name + ': ' + str(exc))
    return facts, errors


def inventory(directory, descriptor):
    """Inventory all descriptor extents, without choosing a writable top."""
    root = Path(directory)
    if root.resolve() != root or root.is_symlink() or not root.is_dir():
        raise Refusal('disk directory is not canonical')
    try:
        xml = ET.fromstring(descriptor)
    except ET.ParseError as exc:
        raise Refusal('invalid disk descriptor') from exc
    images = xml.findall('./StorageData/Storage/Image')
    guids = [image.findtext('GUID') for image in images]
    if not guids:
        raise Refusal('empty descriptor inventory')
    try:
        mappings = [observations.descriptor_chain(descriptor, guid) for guid in guids]
    except observations.Refusal as exc:
        raise Refusal(str(exc)) from exc
    mapping = mappings[0]
    names = list(mapping['files'].values())
    if any(not name.endswith('.hds') for name in names):
        raise Refusal('descriptor names a non-hds extent')
    if not names or set(names) != {p.name for p in root.iterdir() if p.name.endswith('.hds')}:
        raise Refusal('extent inventory differs from descriptor')
    result, identities = {}, set()
    for name in sorted(names):
        path = root / name
        info = path.lstat()
        ident = (info.st_dev, info.st_ino)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid() or ident in identities:
            raise Refusal('extent alias or non-regular file')
        identities.add(ident)
        guid = next(g for g, file in mapping['files'].items() if file == name)
        result[name] = {'path': str(path), 'device': info.st_dev, 'inode': info.st_ino,
                        'guid': guid, 'parent': mapping['parents'][guid],
                        'logical': info.st_size, 'allocated': info.st_blocks * 512}
    return result


def extent_growth(before, after):
    if not before or set(before) != set(after):
        raise Refusal('extent inventory changed')
    logical = allocated = 0
    for name, old in before.items():
        new = after[name]
        if any(old.get(k) != new.get(k) for k in ('path', 'device', 'inode', 'guid', 'parent')):
            raise Refusal('extent identity changed: ' + name)
        for key in ('logical', 'allocated'):
            if type(old.get(key)) is not int or type(new.get(key)) is not int or old[key] < 0 or new[key] < old[key]:
                raise Refusal('extent counter missing or decreased: ' + name)
        logical += new['logical'] - old['logical']
        allocated += new['allocated'] - old['allocated']
    if max(logical, allocated) > CAP:
        raise Refusal('extent growth exceeds observation cap')
    return {'logical': logical, 'allocated': allocated}


def package_counters(root):
    """Count every regular package file; reject aliases and unaccountable entries."""
    root = Path(root)
    if root.resolve() != root or root.is_symlink() or not root.is_dir():
        raise Refusal('package is not canonical')
    seen, result = set(), {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = Path(directory) / name
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                raise Refusal('package contains a symlink')
            if stat.S_ISDIR(info.st_mode):
                continue
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid() or info.st_size > 16 * 1024 * 1024:
                raise Refusal('package contains an unaccountable file')
            ident = (info.st_dev, info.st_ino)
            if ident in seen:
                raise Refusal('package file alias')
            seen.add(ident)
            result[str(path.relative_to(root))] = {'device': info.st_dev, 'inode': info.st_ino,
                                                    'logical': info.st_size, 'allocated': info.st_blocks * 512}
    return result


class ExtentMonitor:
    """Live sampled observations, not a kernel hard quota."""
    def __init__(self, package, disk, descriptor, initial):
        self.package = Path(package)
        self.disk = Path(disk)
        self.descriptor = descriptor
        self.descriptor_path = self.disk / 'DiskDescriptor.xml'
        self.identities = [(p.lstat().st_dev, p.lstat().st_ino) for p in (self.package, self.disk)]
        self.initial = initial
        self.last = initial
        self.package_last = package_counters(package)
        self.package_initial = {key: sum(row[key] for row in self.package_last.values()) for key in ('logical', 'allocated')}
        self.package_growth = {'logical': 0, 'allocated': 0}

    def check(self):
        if any((p.lstat().st_dev, p.lstat().st_ino) != identity or p.resolve() != p for p, identity in zip((self.package, self.disk), self.identities)):
            raise Refusal('package or disk identity changed')
        if self.descriptor_path.read_bytes() != self.descriptor:
            raise Refusal('descriptor changed during observation')
        current = inventory(self.disk, self.descriptor)
        growth = extent_growth(self.initial, current)
        extent_growth(self.last, current)
        self.last = current
        package = package_counters(self.package)
        if set(self.package_last) - set(package):
            raise Refusal('package file disappeared during observation')
        for path, new in package.items():
            old = self.package_last.get(path)
            if old is not None and any(old[k] != new[k] for k in ('device', 'inode')):
                raise Refusal('package file identity changed: ' + path)
            for key in self.package_growth:
                self.package_growth[key] += max(0, new[key] - (old[key] if old else 0))
        self.package_last = package
        if any(value > CAP for value in self.package_growth.values()):
            raise Refusal('observed package growth exceeds cap')
        return {'extents': growth, 'package': dict(self.package_growth)}
