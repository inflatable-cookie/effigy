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
COUNTERS = ('logical', 'allocated')


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
                           'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid,
                           'size': len(data)}
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
        for key in COUNTERS:
            if type(old.get(key)) is not int or type(new.get(key)) is not int or old[key] < 0 or new[key] < old[key]:
                raise Refusal('extent counter missing or decreased: ' + name)
        logical += new['logical'] - old['logical']
        allocated += new['allocated'] - old['allocated']
    if max(logical, allocated) > CAP:
        raise Refusal('extent growth exceeds observation cap')
    return {'logical': logical, 'allocated': allocated}


def lstat(path):
    """One package entry, or a refusal that names it; never a zeroed counter."""
    try:
        return Path(path).lstat()
    except OSError as exc:
        raise Refusal('package entry unreadable: ' + str(path)) from exc


def package_sample(root, exclude=frozenset()):
    """Sample every owned regular package file; extents and the descriptor are excluded.

    The exclusion only separates stable objects (charged by extent inventory and
    descriptor bytes) from ancillary metadata. Nothing is skipped silently: a
    symlink, a non-regular entry, a hardlink or alias, a foreign owner or an
    entry that cannot be read is a refusal, including for an excluded path.
    """
    root = Path(root)
    if root.resolve() != root or root.is_symlink() or not root.is_dir():
        raise Refusal('package is not canonical')
    exclude = frozenset(exclude)
    seen, result = set(), {}

    def unreadable(error):
        raise Refusal('package directory unreadable: ' + str(error))

    for directory, dirs, files in os.walk(root, followlinks=False, onerror=unreadable):
        for name in dirs + files:
            path = Path(directory) / name
            info = lstat(path)
            if stat.S_ISLNK(info.st_mode):
                raise Refusal('package contains a symlink: ' + str(path))
            if stat.S_ISDIR(info.st_mode):
                continue
            if not stat.S_ISREG(info.st_mode):
                raise Refusal('package contains a non-regular file: ' + str(path))
            if info.st_nlink != 1:
                raise Refusal('package contains an aliased file: ' + str(path))
            if info.st_uid != os.geteuid():
                raise Refusal('package contains a foreign-owned file: ' + str(path))
            for field in ('st_size', 'st_blocks'):
                if type(getattr(info, field)) is not int or getattr(info, field) < 0:
                    raise Refusal('package counter unreadable: ' + str(path))
            if str(path) in exclude:
                continue
            ident = (info.st_dev, info.st_ino)
            if ident in seen:
                raise Refusal('package file alias: ' + str(path))
            seen.add(ident)
            result[str(path.relative_to(root))] = {'device': info.st_dev, 'inode': info.st_ino,
                                                   'uid': info.st_uid,
                                                   'logical': info.st_size,
                                                   'allocated': info.st_blocks * 512}
    return result


class AncillaryAccount:
    """Conservative sampled charge for owned ancillary metadata objects.

    The package's stable ownership objects are the package and disk roots and the
    extents the descriptor names. Every other regular file inside the package is
    an ancillary metadata object: Parallels owns it and rewrites it, replaces it
    atomically (same path, new device/inode) or rotates it through temporary
    names while its VM runs. A sample cannot prove a replacement harmless in
    general, and this one observed backup event proves nothing about the rest, so
    the account is one-sided: an incarnation is charged its greatest observed
    logical and allocated size, a replacement keeps the previous incarnation's
    whole charge, and a disappearance refunds nothing. Every identity and counter
    event is retained with path, device, inode, uid and both counter
    observations. Unreadable, aliased, symlinked or foreign-owned entries stay
    refusals. This is sampled accounting with a sampled ceiling, not a hard quota.
    """
    def __init__(self):
        self.incarnations = {}
        self.charges = {}
        self.samples = 0
        self.held = {key: 0 for key in COUNTERS}
        self.events = []

    def _event(self, kind, path, previous, current):
        self.events.append({'kind': kind, 'path': path,
                            'previous': None if previous is None else dict(previous),
                            'current': None if current is None else dict(current)})

    @staticmethod
    def _observed(row):
        return {'device': row['device'], 'inode': row['inode'], 'uid': row['uid'],
                'logical': row['logical'], 'allocated': row['allocated']}

    def observe(self, sample):
        """Charge one sample; return the held totals and event count."""
        self.samples += 1
        seen = {}
        for path in sorted(sample):
            row = sample[path]
            observed = self._observed(row)
            ident = (row['device'], row['inode'])
            previous = self.incarnations.get(path)
            if previous is None:
                self._event('appeared', path, None, observed)
            elif previous != ident:
                charge = self.charges[(path,) + previous]
                self._event('replaced', path, dict(charge['charged'], device=previous[0],
                                                   inode=previous[1], uid=charge['uid']), observed)
            record = self.charges.get((path,) + ident)
            if record is None:
                record = {'path': path, 'uid': row['uid'], 'first_sample': self.samples,
                          'charged': {key: row[key] for key in COUNTERS},
                          'observed': {key: row[key] for key in COUNTERS}}
                self.charges[(path,) + ident] = record
                for key in COUNTERS:
                    self.held[key] += row[key]
            else:
                if any(row[key] < record['observed'][key] for key in COUNTERS):
                    self._event('shrank', path, dict(record['observed'], device=ident[0],
                                                     inode=ident[1], uid=record['uid']), observed)
                for key in COUNTERS:
                    if row[key] > record['charged'][key]:
                        self.held[key] += row[key] - record['charged'][key]
                        record['charged'][key] = row[key]
                    record['observed'][key] = row[key]
            record['last_sample'] = self.samples
            seen[path] = ident
        for path in sorted(set(self.incarnations) - set(seen)):
            ident = self.incarnations[path]
            charge = self.charges[(path,) + ident]
            self._event('disappeared', path, dict(charge['charged'], device=ident[0],
                                                  inode=ident[1], uid=charge['uid']), None)
        self.incarnations = seen
        return {'objects': len(seen), 'incarnations': len(self.charges),
                'events': len(self.events), 'held': dict(self.held)}


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
        self.exclude = frozenset({str(self.disk / name) for name in initial} | {str(self.descriptor_path)})
        self.descriptor_identity = self.descriptor_facts()
        self.identity_events = []
        self.ancillary = AncillaryAccount()
        self.ancillary.observe(package_sample(self.package, self.exclude))
        # The first sample is the baseline the charge is measured against, not an event.
        self.ancillary.events.clear()
        self.package_initial = dict(self.ancillary.held)

    def descriptor_facts(self):
        try:
            info = self.descriptor_path.lstat()
        except OSError as exc:
            raise Refusal('descriptor unreadable during observation') from exc
        if not stat.S_ISREG(info.st_mode):
            raise Refusal('descriptor is not a regular file during observation')
        if info.st_nlink != 1:
            raise Refusal('descriptor is an aliased file during observation')
        if info.st_uid != os.geteuid():
            raise Refusal('descriptor is foreign-owned during observation')
        return {'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid, 'size': info.st_size}

    def descriptor_bytes(self, facts):
        try:
            fd = os.open(self.descriptor_path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
        except OSError as exc:
            raise Refusal('descriptor unreadable during observation') from exc
        with os.fdopen(fd, 'rb') as source:
            opened = os.fstat(source.fileno())
            if (opened.st_dev, opened.st_ino) != (facts['device'], facts['inode']):
                raise Refusal('descriptor identity changed while reading')
            return source.read()

    def check(self):
        try:
            stable = all((p.lstat().st_dev, p.lstat().st_ino) == identity and p.resolve() == p
                         for p, identity in zip((self.package, self.disk), self.identities))
        except OSError as exc:
            raise Refusal('package or disk unreadable during observation') from exc
        if not stable:
            raise Refusal('package or disk identity changed')
        descriptor = self.descriptor_facts()
        content = self.descriptor_bytes(descriptor)
        if descriptor != self.descriptor_identity:
            self.identity_events.append({'object': 'descriptor', 'source': 'sampled',
                                         'previous': self.descriptor_identity, 'current': descriptor,
                                         'content_identical': content == self.descriptor})
            self.descriptor_identity = descriptor
        if content != self.descriptor:
            raise Refusal('descriptor content changed during observation')
        current = inventory(self.disk, self.descriptor)
        growth = extent_growth(self.initial, current)
        extent_growth(self.last, current)
        self.last = current
        held = self.ancillary.observe(package_sample(self.package, self.exclude))
        charged = {key: held['held'][key] - self.package_initial[key] for key in COUNTERS}
        if any(value > CAP for value in charged.values()):
            raise Refusal('observed ancillary metadata growth exceeds cap')
        return {'extents': growth, 'package': charged,
                'metadata': {'objects': held['objects'], 'incarnations': held['incarnations'],
                             'events': held['events'], 'held': held['held']},
                'descriptor_identity_events': len(self.identity_events)}
