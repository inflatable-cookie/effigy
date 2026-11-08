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
# One live sample may repeat once when a protected object's listing or identity moved between
# its observation and the package walk. A persistent failure refuses; both attempts are retained.
ATTEMPTS = 2
_ISSUER = object()
# Ancillary-metadata ownership modes. STRICT is the default for every other caller: a foreign
# owner refuses. OBSERVED is the narrow read-only inventory mode the Linux diagnostic uses for
# incidental metadata only: the kernel's stat of a regular non-aliased object supplies its byte
# counters and an observed owner, and grants that object no input, config, extent, executable or
# ownership authority. A bound input keeps STRICT in either mode.
STRICT = 'strict'
OBSERVED = 'observed'
CUSTODY_OWN = 'package-owner'
CUSTODY_FOREIGN = 'observed-foreign'


def stat_identity(info, relative=None):
    """The complete observed identity of one entry, or explicit unknowns.

    A refusal keeps this: the fact that explains a refusal is evidence, and losing it loses the
    diagnosis. An absent fact stays None instead of a substituted zero.
    """
    if info is None:
        return {'relative': relative, 'device': None, 'inode': None, 'uid': None, 'mode': None,
                'nlink': None, 'type': None, 'logical': None, 'allocated': None}
    shape = info.st_mode
    for test, name in ((stat.S_ISREG, 'regular'), (stat.S_ISLNK, 'symlink'),
                       (stat.S_ISDIR, 'directory'), (stat.S_ISFIFO, 'fifo'),
                       (stat.S_ISSOCK, 'socket'), (stat.S_ISCHR, 'character'),
                       (stat.S_ISBLK, 'block')):
        if test(shape):
            kind = name
            break
    else:
        kind = 'unknown'
    return {'relative': relative, 'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid,
            'mode': stat.S_IMODE(shape), 'nlink': info.st_nlink, 'type': kind,
            'logical': info.st_size, 'allocated': info.st_blocks * 512}


class ObservedRefusal(Refusal):
    """A refusal that keeps the identity facts which explain it.

    Nothing in `observation` is a charge, a trust decision or a custody claim: it is the
    kernel's answer about one entry at the moment of refusal.
    """

    def __init__(self, message, phase, operation, path, error=None, relative=None, info=None):
        code = getattr(error, 'errno', None)
        self.observation = {'phase': phase, 'operation': operation, 'path': str(path), 'errno': code,
                            'strerror': None if code is None else os.strerror(code)}
        self.observation.update(stat_identity(info, relative))
        suffix = '' if code is None else ' (errno %d %s)' % (code, self.observation['strerror'])
        super().__init__('%s: %s%s' % (message, path, suffix))


class ObservationRefusal(ObservedRefusal):
    """A transient live-observation failure: the sample repeats once and both attempts stay."""


class CustodyRefusal(ObservedRefusal):
    """A definitive refusal about one entry: recorded with its facts and never retried."""


class Witness:
    """Protected objects observed by one live sample, consumed by that sample's package walk.

    Only ExtentMonitor issues one, so a plain mapping, a witness built without the issuer or a
    witness from an earlier sample is not an observation of this sample and refuses.
    """
    def __init__(self, issuer, entries):
        if issuer is not _ISSUER:
            raise Refusal('protected observation was not issued by the monitor')
        self.entries = entries
        self.consumed = False


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
        try:
            info = path.lstat()
        except OSError as exc:
            raise ObservationRefusal('extent unreadable', 'extent-verify', 'lstat', path, exc,
                                     relative=name) from exc
        ident = (info.st_dev, info.st_ino)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid():
            # A descriptor-named extent is a bound input: no mode relaxes its custody, and the
            # refusal keeps the observed identity that explains it.
            raise CustodyRefusal('extent alias or non-regular file', 'extent-verify', 'lstat', path,
                                 relative=name, info=info)
        if ident in identities:
            raise CustodyRefusal('extent alias or non-regular file', 'extent-verify', 'lstat', path,
                                 relative=name, info=info)
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
        raise ObservationRefusal('package entry unreadable', 'package-walk', 'lstat', path, exc) from exc


def package_sample(root, protected=None, ancillary_ownership=STRICT, bound=()):
    """Sample every package file; protected objects are listed, not looked up again.

    A protected object (the descriptor and every extent) was observed by this same sample, and
    its witness is consumed here once. Its directory entry must list as a regular entry with the
    observed inode, which the listing reports without a second path lookup. Every other entry is
    lstat-ed: a symlink, a non-regular entry, a hardlink or alias or an entry that cannot be read
    is a refusal in every mode.

    `ancillary_ownership` selects the owner rule for incidental metadata only. STRICT, the
    default, refuses a foreign owner. OBSERVED keeps that owner as a measurement: the entry's
    stat supplies its byte counters and its observed owner, is never read, and confers no input,
    config, extent, executable or ownership authority. Paths in `bound` are bound inputs and keep
    the STRICT owner refusal in either mode, and an observed-mode entry on another device crosses
    a custody boundary and refuses rather than being charged to this package.
    """
    if ancillary_ownership not in (STRICT, OBSERVED):
        raise Refusal('unsupported ancillary ownership mode')
    owner = os.geteuid()
    bound = frozenset(str(path) for path in bound)
    root = Path(root)
    if root.resolve() != root or root.is_symlink() or not root.is_dir():
        raise Refusal('package is not canonical')
    device = root.lstat().st_dev
    if protected is None:
        protected = Witness(_ISSUER, {})
    if not isinstance(protected, Witness) or protected.consumed:
        raise Refusal('protected observation is forged or stale')
    protected.consumed = True
    expected, listed = protected.entries, set()
    seen, result = set(), {}
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            with os.scandir(directory) as scan:
                entries = list(scan)
        except OSError as exc:
            raise ObservationRefusal('package directory unreadable', 'package-walk', 'scandir', directory, exc) from exc
        for entry in entries:
            path = Path(directory) / entry.name
            key = str(path)
            relative = str(path.relative_to(root))
            if key in expected:
                # The listing reports the kind and inode without a second pathname lookup, so a
                # protected object that moved under this sample keeps its observed-shape refusal
                # instead of being re-read. Its owner is verified by the bound-input checks that
                # ran before this walk: the retained descriptor identity and every inventory entry.
                if not entry.is_file(follow_symlinks=False):
                    raise Refusal('protected object is not a regular entry: ' + key)
                if entry.inode() != expected[key]['inode']:
                    raise ObservationRefusal('protected object identity moved before the listing',
                                             'package-walk', 'listing', path, relative=relative)
                listed.add(key)
                continue
            info = lstat(path)
            if stat.S_ISLNK(info.st_mode):
                raise CustodyRefusal('package contains a symlink', 'package-walk', 'lstat', path,
                                     relative=relative, info=info)
            if stat.S_ISDIR(info.st_mode):
                if ancillary_ownership == OBSERVED and info.st_dev != device:
                    raise CustodyRefusal('package directory crosses a device boundary',
                                         'package-walk', 'lstat', path, relative=relative, info=info)
                pending.append(path)
                continue
            if not stat.S_ISREG(info.st_mode):
                raise CustodyRefusal('package contains a non-regular file', 'package-walk', 'lstat',
                                     path, relative=relative, info=info)
            if info.st_nlink != 1:
                raise CustodyRefusal('package contains an aliased file', 'package-walk', 'lstat',
                                     path, relative=relative, info=info)
            foreign = info.st_uid != owner
            if foreign and (ancillary_ownership != OBSERVED or key in bound):
                # A bound input, and every entry in the default strict mode, keeps the owner
                # refusal; the observed identity stays with the refusal that names it.
                raise CustodyRefusal('package contains a foreign-owned file', 'package-walk',
                                     'lstat', path, relative=relative, info=info)
            if ancillary_ownership == OBSERVED and info.st_dev != device:
                raise CustodyRefusal('package entry crosses a device boundary', 'package-walk',
                                     'lstat', path, relative=relative, info=info)
            for field in ('st_size', 'st_blocks'):
                if type(getattr(info, field)) is not int or getattr(info, field) < 0:
                    raise CustodyRefusal('package counter unreadable', 'package-walk', 'lstat',
                                         path, relative=relative, info=info)
            ident = (info.st_dev, info.st_ino)
            if ident in seen:
                raise CustodyRefusal('package file alias', 'package-walk', 'lstat', path,
                                     relative=relative, info=info)
            seen.add(ident)
            result[relative] = {'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid,
                                'custody': CUSTODY_FOREIGN if foreign else CUSTODY_OWN,
                                'logical': info.st_size, 'allocated': info.st_blocks * 512}
    missing = sorted(set(expected) - listed)
    if missing:
        raise ObservationRefusal('protected object absent from the package listing',
                                 'package-walk', 'listing', missing[0])
    return result


class AncillaryAccount:
    """Conservative sampled charge for ancillary metadata objects.

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
    observations. An identity change at a sampled path is charged in full even
    when the new device/inode equals an earlier incarnation's: a sample cannot
    tell an atomically replaced file from a reused inode number. An observed owner
    is a measurement of this charge, never a trust decision: a path that keeps its
    identity but changes hands is retained as an explicit owner event and confers
    nothing. Unreadable, aliased, symlinked or non-regular entries stay refusals.
    A foreign owner stays a refusal unless the caller selected the observed mode,
    which charges the entry's bytes without reading it or granting it any input,
    config, extent or ownership authority. This is sampled accounting with a
    sampled ceiling, not a hard quota.
    """
    def __init__(self):
        self.current = {}
        self.samples = 0
        self.incarnations = 0
        self.held = {key: 0 for key in COUNTERS}
        self.events = []

    def _event(self, kind, path, previous, current):
        self.events.append({'kind': kind, 'path': path,
                            'previous': None if previous is None else dict(previous),
                            'current': None if current is None else dict(current)})

    @staticmethod
    def _observed(row):
        return {'device': row['device'], 'inode': row['inode'], 'uid': row['uid'],
                'custody': row.get('custody', CUSTODY_OWN),
                'logical': row['logical'], 'allocated': row['allocated']}

    @staticmethod
    def _facts(record):
        return {'device': record['device'], 'inode': record['inode'], 'uid': record['uid'],
                'custody': record.get('custody', CUSTODY_OWN),
                'logical': record['charged']['logical'],
                'allocated': record['charged']['allocated']}

    def observe(self, sample):
        """Charge one sample; return the held totals and event count."""
        self.samples += 1
        seen = {}
        for path in sorted(sample):
            row = sample[path]
            observed = self._observed(row)
            ident = (row['device'], row['inode'])
            record = self.current.get(path)
            if record is None or (record['device'], record['inode']) != ident:
                # A path that appears, or whose identity changed, is a new incarnation and is
                # charged in full. A sampled path cannot distinguish an atomically replaced file
                # from a reused inode number, so returning to an earlier inode is charged again
                # rather than credited against the earlier incarnation.
                self._event('appeared' if record is None else 'replaced', path,
                            None if record is None else self._facts(record), observed)
                record = {'device': observed['device'], 'inode': observed['inode'],
                          'uid': observed['uid'], 'custody': observed['custody'],
                          'charged': {key: observed[key] for key in COUNTERS},
                          'observed': {key: observed[key] for key in COUNTERS}}
                self.current[path] = record
                self.incarnations += 1
                for key in COUNTERS:
                    self.held[key] += observed[key]
            else:
                if observed['uid'] != record['uid'] or observed['custody'] != record['custody']:
                    # Same object, a different observed owner: that is an observation of this
                    # charge, not a new incarnation and not proof of who wrote the bytes.
                    self._event('owner-changed', path, self._facts(record), observed)
                    record['uid'] = observed['uid']
                    record['custody'] = observed['custody']
                if any(observed[key] < record['observed'][key] for key in COUNTERS):
                    self._event('shrank', path, dict(record['observed'], device=ident[0],
                                                     inode=ident[1], uid=record['uid'],
                                                     custody=record['custody']), observed)
                for key in COUNTERS:
                    if observed[key] > record['charged'][key]:
                        self.held[key] += observed[key] - record['charged'][key]
                        record['charged'][key] = observed[key]
                    record['observed'][key] = observed[key]
            record['last_sample'] = self.samples
            seen[path] = record
        for path in sorted(set(self.current) - set(seen)):
            self._event('disappeared', path, self._facts(self.current.pop(path)), None)
        return {'objects': len(seen), 'incarnations': self.incarnations,
                'events': len(self.events), 'held': dict(self.held),
                'owners': sorted({row['uid'] for row in seen.values()}),
                'foreign_owned': sum(1 for row in seen.values()
                                     if row.get('custody') == CUSTODY_FOREIGN)}


class ExtentMonitor:
    """Live sampled observations, not a kernel hard quota."""
    def __init__(self, package, disk, descriptor, initial, bound=(), ancillary_ownership=STRICT):
        if ancillary_ownership not in (STRICT, OBSERVED):
            raise Refusal('unsupported ancillary ownership mode')
        self.package = Path(package)
        self.disk = Path(disk)
        self.descriptor = descriptor
        self.descriptor_path = self.disk / 'DiskDescriptor.xml'
        self.ancillary_ownership = ancillary_ownership
        # Bound inputs keep the strict owner refusal in every mode; the descriptor and the
        # descriptor-named extents are bound by their own verification in the same sample.
        self.bound = frozenset(str(Path(path)) for path in bound)
        self.identities = [(p.lstat().st_dev, p.lstat().st_ino) for p in (self.package, self.disk)]
        self.initial = initial
        self.last = initial
        self.descriptor_identity = self.descriptor_facts()
        self.identity_events = []
        self.observation_failures = []
        self.ancillary = AncillaryAccount()
        self.package_initial = {key: 0 for key in COUNTERS}
        self._observe()
        # The first sample is the baseline the charge is measured against, not an event.
        self.ancillary.events.clear()
        self.package_initial = dict(self.ancillary.held)

    def descriptor_facts(self):
        try:
            info = self.descriptor_path.lstat()
        except OSError as exc:
            raise ObservationRefusal('descriptor unreadable during observation', 'descriptor-verify',
                                     'lstat', self.descriptor_path, exc) from exc
        if not stat.S_ISREG(info.st_mode):
            raise Refusal('descriptor is not a regular file during observation')
        if info.st_nlink != 1:
            raise Refusal('descriptor is an aliased file during observation')
        if info.st_uid != os.geteuid():
            # A bound input keeps strict custody; the refusal keeps the observed identity.
            raise CustodyRefusal('descriptor is foreign-owned during observation', 'descriptor-verify',
                                 'lstat', self.descriptor_path,
                                 relative=str(self.descriptor_path.name), info=info)
        return {'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid, 'size': info.st_size}

    def descriptor_bytes(self, facts):
        try:
            fd = os.open(self.descriptor_path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
        except OSError as exc:
            raise ObservationRefusal('descriptor unreadable during observation', 'descriptor-verify',
                                     'open', self.descriptor_path, exc) from exc
        with os.fdopen(fd, 'rb') as source:
            opened = os.fstat(source.fileno())
            if (opened.st_dev, opened.st_ino) != (facts['device'], facts['inode']):
                raise ObservationRefusal('descriptor identity changed while reading', 'descriptor-verify',
                                         'fstat', self.descriptor_path)
            return source.read()

    def _sample(self):
        """One genuine sample: descriptor bytes and extents first, then the package listing."""
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
        entries = {str(self.descriptor_path): {'inode': descriptor['inode']}}
        entries.update({str(self.disk / name): {'inode': row['inode']} for name, row in current.items()})
        sample = package_sample(self.package, Witness(_ISSUER, entries), self.ancillary_ownership,
                                self.bound)
        self.last = current
        return {'extents': growth, 'held': self.ancillary.observe(sample),
                'ancillary_ownership': self.ancillary_ownership}

    def _observe(self):
        """Repeat a sample once when a protected object moved under it; keep every failed attempt."""
        for attempt in range(1, ATTEMPTS + 1):
            try:
                return self._sample(), attempt
            except CustodyRefusal as exc:
                # Definitive: recorded once with its observed identity, never retried.
                self.observation_failures.append(dict(exc.observation, attempt=attempt))
                raise
            except ObservationRefusal as exc:
                self.observation_failures.append(dict(exc.observation, attempt=attempt))
                if attempt == ATTEMPTS:
                    raise

    def check(self):
        try:
            stable = all((p.lstat().st_dev, p.lstat().st_ino) == identity and p.resolve() == p
                         for p, identity in zip((self.package, self.disk), self.identities))
        except OSError as exc:
            raise Refusal('package or disk unreadable during observation') from exc
        if not stable:
            raise Refusal('package or disk identity changed')
        sampled, attempts = self._observe()
        held = sampled['held']
        charged = {key: held['held'][key] - self.package_initial[key] for key in COUNTERS}
        if any(value > CAP for value in charged.values()):
            raise Refusal('observed ancillary metadata growth exceeds cap')
        return {'extents': sampled['extents'], 'package': charged,
                'ancillary_ownership': sampled['ancillary_ownership'],
                'metadata': {'objects': held['objects'], 'incarnations': held['incarnations'],
                             'events': held['events'], 'held': held['held'],
                             'owners': held['owners'], 'foreign_owned': held['foreign_owned']},
                'descriptor_identity_events': len(self.identity_events), 'attempts': attempts}
