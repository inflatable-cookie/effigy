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

CAP = 1024 ** 3


def retain_originals(root, sources, prefix):
    """Retain independent originals even when another original cannot be read."""
    facts, errors = {}, []
    for name, source in sources.items():
        try:
            path = Path(source)
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise Refusal('not an independent regular file')
            data = path.read_bytes()
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
    names = []
    for image in xml.findall('./StorageData/Storage/Image'):
        name = image.findtext('File')
        if not name or Path(name).name != name or not name.endswith('.hds') or name in names:
            raise Refusal('invalid or duplicate extent name')
        names.append(name)
    if not names or set(names) != {p.name for p in root.iterdir() if p.name.endswith('.hds')}:
        raise Refusal('extent inventory differs from descriptor')
    result, identities = {}, set()
    for name in sorted(names):
        path = root / name
        info = path.lstat()
        ident = (info.st_dev, info.st_ino)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or ident in identities:
            raise Refusal('extent alias or non-regular file')
        identities.add(ident)
        result[name] = {'path': str(path), 'device': info.st_dev, 'inode': info.st_ino,
                        'logical': info.st_size, 'allocated': info.st_blocks * 512}
    return result


def extent_growth(before, after):
    if not before or set(before) != set(after):
        raise Refusal('extent inventory changed')
    logical = allocated = 0
    for name, old in before.items():
        new = after[name]
        if any(old.get(k) != new.get(k) for k in ('path', 'device', 'inode')):
            raise Refusal('extent identity changed: ' + name)
        for key in ('logical', 'allocated'):
            if type(old.get(key)) is not int or type(new.get(key)) is not int or old[key] < 0 or new[key] < old[key]:
                raise Refusal('extent counter missing or decreased: ' + name)
        logical += new['logical'] - old['logical']
        allocated += new['allocated'] - old['allocated']
    if max(logical, allocated) > CAP:
        raise Refusal('extent growth exceeds observation cap')
    return {'logical': logical, 'allocated': allocated}
