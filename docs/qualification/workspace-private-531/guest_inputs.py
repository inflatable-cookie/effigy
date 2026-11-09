"""Fixed finite Colima guest runtime-input transfer and guest-side layout derivation.

One representation for the exact reviewed OCI metadata/blob/deb set, used on both sides:

    MAGIC
    HEADER\\n  <compact JSON {"entries": [{"path","bytes","sha256"}, ...], "total_bytes": n}>
    DATA <bytes>\\n <exactly that many payload bytes>      (once per entry, in header order)
    END\\n

The host writes it from the pinned prefetch (`write_stream`); the guest consumes it into the two fixed
sibling directories `<parent>/<context>.oci` and `<parent>/<context>.debs` (`transfer`) or re-derives and
re-checks them immediately before the build (`check-inputs`). There is no archive, extraction, traversal,
link, device, shell, program, destination or filename choice: every path is one of the fixed layout's
allowlisted names, every identity is a reviewed pin, and an unexpected, duplicated, missing, partial or
overrun record refuses. Partial trees are retained, never adopted or pruned.

Stdlib only and self-contained: the guest receives this file's text as a `python3 -c` program, so it must
not import a sibling module. The host imports the same functions, so one implementation defines the
framing, the caps and the layout on both sides.

Run in the guest as `python3 -I -B -c <this file> transfer <hex spec>` (the stream on stdin) or
`python3 -I -B -c <this file> check-inputs <hex spec>`.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

MAGIC = b'EFFIGY-COLIMA-INPUTS-1\n'
OCI_INDEX_MEDIA_TYPE = 'application/vnd.oci.image.index.v1+json'
# Framing (magic, header, one data label per entry, END) and the two generated metadata files share one bound.
FRAMING_CAP_BYTES = 64 * 1024
PER_INPUT_CAP_BYTES = 32 * 1024 * 1024
AGGREGATE_CAP_BYTES = 48 * 1024 * 1024
MAX_ENTRIES = 64
CHUNK = 1024 * 1024
OCI_LAYOUT_PATH = 'oci/oci-layout'
INDEX_PATH = 'oci/index.json'
UPSTREAM_PATH = 'oci/index.upstream.json'
BLOB_DIR = 'oci/blobs/sha256'
DEB_DIR = 'debs'
OCI_LAYOUT_BYTES = b'{"imageLayoutVersion":"1.0.0"}\n'
NAME = re.compile(r'^effigy-531-image-[0-9a-f]{32}$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
DIGEST = re.compile(r'^sha256:[0-9a-f]{64}$')
DEB = re.compile(r'^debs/[A-Za-z0-9][A-Za-z0-9+.-]*_[A-Za-z0-9.+~:-]+_(arm64|all)\.deb$')
SHARED = Path('/Volumes/Builds/scratch/effigy/531-colima-qualification')
EXCLUDED = Path('/Volumes/Builds/scratch/effigy/531c')


class Refusal(ValueError):
    """Contradictory or unsupported transfer input. Not a closure, and never a retry."""


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def positive_int(value):
    return type(value) is int and value > 0


def hex_digest(digest, label='oci digest'):
    if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
        raise Refusal(label + ' must be sha256:<64 lowercase hex>')
    return digest[len('sha256:'):]


# ---------------------------------------------------------------------------------------------------
# The reviewed layout: one identity per file, derived from the pins and the upstream index.
# ---------------------------------------------------------------------------------------------------

def checked_layout(layout):
    """The pin-derived layout the spec carries: metadata digests, blob descriptors and the exact deb set.
    Any other value refuses before a byte is framed or received."""
    if not isinstance(layout, dict) or set(layout) != {'index', 'manifest', 'config', 'layer', 'debs'}:
        raise Refusal('layout must be the reviewed index/manifest/config/layer/deb set')
    index = layout['index']
    if not isinstance(index, dict) or set(index) != {'digest', 'bytes', 'media_type'}:
        raise Refusal('layout index must be its digest, size and media type')
    if index['media_type'] != OCI_INDEX_MEDIA_TYPE:
        raise Refusal('layout index media type differs from the reviewed OCI index')
    hex_digest(index['digest'], 'index digest')
    if not positive_int(index['bytes']):
        raise Refusal('index bytes must be positive')
    blobs = []
    for name in ('manifest', 'config', 'layer'):
        item = layout[name]
        if not isinstance(item, dict) or set(item) != {'digest', 'bytes'}:
            raise Refusal(f'layout {name} must be a digest and its size')
        hex_digest(item['digest'], name + ' digest')
        if not positive_int(item['bytes']):
            raise Refusal(f'{name} bytes must be positive')
        blobs.append(item['digest'])
    if len(set(blobs)) != 3:
        raise Refusal('manifest, config and layer digests must be distinct')
    debs = layout['debs']
    if not isinstance(debs, list) or not debs or len(debs) > MAX_ENTRIES:
        raise Refusal('layout debs must be the reviewed non-empty list')
    seen = set()
    for deb in debs:
        if not isinstance(deb, dict) or set(deb) != {'name', 'bytes', 'sha256'}:
            raise Refusal('each deb must be a name, its size and its sha256')
        if not isinstance(deb['name'], str) or not DEB.fullmatch(DEB_DIR + '/' + deb['name']):
            raise Refusal('deb name is not an allowlisted arm64/all package file')
        if deb['name'] in seen:
            raise Refusal('deb set repeats a package file')
        seen.add(deb['name'])
        if not positive_int(deb['bytes']) or not isinstance(deb['sha256'], str) or not HEX64.fullmatch(deb['sha256']):
            raise Refusal('each deb needs its exact size and sha256')
    if len(blobs) + len(debs) > MAX_ENTRIES:
        raise Refusal('layout exceeds the fixed entry count')
    return dict(index=dict(index), manifest=dict(layout['manifest']), config=dict(layout['config']),
                layer=dict(layout['layer']),
                debs=[dict(name=deb['name'], bytes=deb['bytes'], sha256=deb['sha256']) for deb in debs])


def pin_layout(pin):
    """The reviewed layout for one runtime pin (`runtime_image_inputs.production_pin` or its fixture stand-in)."""
    if not isinstance(pin, dict):
        raise Refusal('runtime pin required')
    try:
        index = pin['index']
        layout = dict(index={'digest': index['digest'], 'bytes': index['bytes'],
                             'media_type': index['media_type']},
                      manifest={'digest': pin['manifest']['digest'], 'bytes': pin['manifest']['bytes']},
                      config={'digest': pin['config']['digest'], 'bytes': pin['config']['bytes']},
                      layer={'digest': pin['layer']['digest'], 'bytes': pin['layer']['bytes']},
                      debs=[{'name': deb['name'], 'bytes': deb['bytes'], 'sha256': deb['sha256']}
                            for deb in pin['debs']])
    except (KeyError, TypeError) as exc:
        raise Refusal('runtime pin lacks the layout identities') from exc
    return checked_layout(layout)


def pinned_bytes(layout):
    """The reviewed payload bytes of the OCI metadata, blobs and debs (the fetched set, before the two generated
    metadata files and the framing)."""
    layout = checked_layout(layout)
    return sum([layout['index']['bytes'], layout['manifest']['bytes'], layout['config']['bytes'],
                layout['layer']['bytes']] + [deb['bytes'] for deb in layout['debs']])


def layout_entries(layout, upstream_bytes):
    """The exact allowlisted file identities for the two guest directories, derived again from the received
    upstream index. Paths are the fixed layout; sizes and digests are the reviewed pins."""
    layout = checked_layout(layout)
    if not isinstance(upstream_bytes, (bytes, bytearray)):
        raise Refusal('upstream index bytes required')
    upstream_bytes = bytes(upstream_bytes)
    if sha256(upstream_bytes) != hex_digest(layout['index']['digest']):
        raise Refusal('upstream index differs from its pinned digest')
    if len(upstream_bytes) != layout['index']['bytes']:
        raise Refusal('upstream index differs from its pinned size')
    try:
        upstream = json.loads(upstream_bytes)
    except ValueError as exc:
        raise Refusal('upstream index is not JSON') from exc
    if not isinstance(upstream, dict) or upstream.get('mediaType') != layout['index']['media_type']:
        raise Refusal('upstream index media type differs from the pin')
    matches = []
    for item in upstream.get('manifests') or []:
        if not isinstance(item, dict):
            continue
        platform = item.get('platform') or {}
        if platform.get('os') == 'linux' and platform.get('architecture') == 'arm64':
            matches.append(item)
    if len(matches) != 1:
        raise Refusal('upstream index must name exactly one linux/arm64 manifest')
    descriptor = matches[0]
    manifest = layout['manifest']
    if descriptor.get('digest') != manifest['digest'] or descriptor.get('size') != manifest['bytes']:
        raise Refusal('the arm64 descriptor differs from the pinned manifest')
    if (descriptor.get('platform') or {}).get('variant') not in (None, 'v8'):
        raise Refusal('the arm64 variant is not v8')
    filtered = dict(schemaVersion=2, mediaType=layout['index']['media_type'], manifests=[descriptor])
    index_bytes = (json.dumps(filtered, separators=(',', ':')) + '\n').encode()
    entries = [
        dict(path=OCI_LAYOUT_PATH, bytes=len(OCI_LAYOUT_BYTES), sha256=sha256(OCI_LAYOUT_BYTES)),
        dict(path=INDEX_PATH, bytes=len(index_bytes), sha256=sha256(index_bytes)),
        dict(path=UPSTREAM_PATH, bytes=layout['index']['bytes'], sha256=hex_digest(layout['index']['digest'])),
    ]
    for name in ('manifest', 'config', 'layer'):
        digest = hex_digest(layout[name]['digest'])
        entries.append(dict(path=BLOB_DIR + '/' + digest, bytes=layout[name]['bytes'], sha256=digest))
    for deb in layout['debs']:
        entries.append(dict(path=DEB_DIR + '/' + deb['name'], bytes=deb['bytes'], sha256=deb['sha256']))
    check_entries(entries, aggregate_bytes=AGGREGATE_CAP_BYTES, per_input_bytes=PER_INPUT_CAP_BYTES)
    return entries, {OCI_LAYOUT_PATH: OCI_LAYOUT_BYTES, INDEX_PATH: index_bytes}


def check_path(path):
    """Only the fixed layout's names. No absolute path, traversal, link, archive or caller choice."""
    if not isinstance(path, str) or not path or len(path) > 200 or '\x00' in path or '\n' in path:
        raise Refusal('entry path is not a fixed layout name')
    if path in (OCI_LAYOUT_PATH, INDEX_PATH, UPSTREAM_PATH):
        return
    if path.startswith(BLOB_DIR + '/'):
        if not HEX64.fullmatch(path[len(BLOB_DIR) + 1:]):
            raise Refusal('blob name is not one sha256 hex digest')
        return
    if DEB.fullmatch(path):
        return
    raise Refusal('entry path is not in the fixed runtime-input layout')


def check_entries(entries, *, aggregate_bytes, per_input_bytes):
    """Entry shape, caps and uniqueness. The per-input and aggregate caps are exact: a zero cap refuses every
    entry, and an entry over its cap refuses before any of its bytes are written."""
    if not isinstance(entries, list) or not entries or len(entries) > MAX_ENTRIES:
        raise Refusal('entries must be a non-empty bounded list')
    seen, total = set(), 0
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != {'path', 'bytes', 'sha256'}:
            raise Refusal('each entry is a path, its size and its sha256')
        check_path(entry['path'])
        if entry['path'] in seen:
            raise Refusal('entry list repeats a path')
        seen.add(entry['path'])
        if not positive_int(entry['bytes']):
            raise Refusal('entry size must be positive: no empty runtime input')
        if not isinstance(entry['sha256'], str) or not HEX64.fullmatch(entry['sha256']):
            raise Refusal('entry sha256 must be 64 lowercase hex')
        if entry['path'].startswith(BLOB_DIR + '/') and entry['path'] != BLOB_DIR + '/' + entry['sha256']:
            raise Refusal('blob entry name differs from its sha256')
        if entry['bytes'] > per_input_bytes:
            raise Refusal('entry exceeds the per-input cap')
        total += entry['bytes']
        if total > aggregate_bytes:
            raise Refusal('entry set exceeds the aggregate cap')
    if total > AGGREGATE_CAP_BYTES:
        raise Refusal('entry set exceeds the reviewed aggregate bound')
    return total


def check_limits(limits):
    if not isinstance(limits, dict) or set(limits) != {'per_input_bytes', 'aggregate_bytes', 'framing_bytes'}:
        raise Refusal('limits must state the per-input, aggregate and framing caps')
    for name, value in limits.items():
        if type(value) is not int or value < 0:
            raise Refusal('caps must be non-negative integers')
    if limits['per_input_bytes'] > PER_INPUT_CAP_BYTES or limits['aggregate_bytes'] > AGGREGATE_CAP_BYTES:
        raise Refusal('caps exceed the reviewed transfer bounds')
    if limits['framing_bytes'] > FRAMING_CAP_BYTES:
        raise Refusal('framing cap exceeds the reviewed bound')
    if limits['per_input_bytes'] > limits['aggregate_bytes']:
        raise Refusal('per-input cap exceeds the aggregate cap')
    return limits


def reviewed_limits(layout):
    """The exact caps for one layout: its largest reviewed input, and the reviewed payload bytes plus one framing
    bound for the two generated metadata files and the framing itself."""
    layout = checked_layout(layout)
    largest = max([layout['index']['bytes'], layout['manifest']['bytes'], layout['config']['bytes'],
                   layout['layer']['bytes']] + [deb['bytes'] for deb in layout['debs']])
    return check_limits(dict(per_input_bytes=largest,
                             aggregate_bytes=pinned_bytes(layout) + FRAMING_CAP_BYTES,
                             framing_bytes=FRAMING_CAP_BYTES))


def checked_parent(parent):
    """The C2 probe's parent binding: path, device, inode and uid only."""
    if not isinstance(parent, dict) or set(parent) != {'path', 'device', 'inode', 'uid'}:
        raise Refusal('parent binding must be the probe path, device, inode and uid')
    if not isinstance(parent['path'], str) or not parent['path']:
        raise Refusal('parent path required')
    if not all(type(parent[k]) is int and parent[k] >= 0 for k in ('device', 'inode', 'uid')):
        raise Refusal('parent identity must be actual integers')
    return parent


def checked_spec(spec):
    """One reviewed spec: parent binding, context name, layout and caps, and nothing else."""
    if not isinstance(spec, dict) or set(spec) != {'parent', 'name', 'layout', 'limits'}:
        raise Refusal('spec must state parent, name, layout and limits only')
    checked_parent(spec['parent'])
    if not isinstance(spec['name'], str) or not NAME.fullmatch(spec['name']):
        raise Refusal('spec context name must be the fresh 32-hex image name')
    layout = checked_layout(spec['layout'])
    limits = check_limits(spec['limits'])
    if limits != reviewed_limits(layout):
        raise Refusal('spec caps differ from the reviewed layout caps')
    return dict(parent=dict(spec['parent']), name=spec['name'], layout=layout, limits=limits)


def transfer_spec(pin, parent, name):
    """The fixed transfer/check spec for one verified guest context: reviewed layout, exact caps, and the
    chain-derived parent binding and context name."""
    if not isinstance(name, str) or not NAME.fullmatch(name):
        raise Refusal('guest context name must be the fresh 32-hex image name')
    checked_parent(parent)
    layout = pin_layout(pin)
    return dict(parent=dict(parent), name=name, layout=layout, limits=reviewed_limits(layout))


def spec_bytes(spec):
    return compact(checked_spec(spec))


# ---------------------------------------------------------------------------------------------------
# Host side: one framed stream, written exclusively from the pinned prefetch.
# ---------------------------------------------------------------------------------------------------

def _open_source(path, expected_bytes):
    """Open one prefetched input by descriptor: one owned read-only regular file, no link, the pinned size."""
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    except OSError as exc:
        raise Refusal(f'{os.path.basename(path)}: input cannot be opened: {exc.strerror}') from exc
    try:
        info = os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1
                or stat.S_IMODE(info.st_mode) not in (0o600, 0o400)):
            raise Refusal(f'{os.path.basename(path)}: input must be one owned private regular file')
        if info.st_size != expected_bytes:
            raise Refusal(f'{os.path.basename(path)}: size differs from the reviewed pin')
    except BaseException:
        os.close(fd)
        raise
    return fd


def write_stream(destination, entries, sources, limits, checkpoint=None):
    """Write the fixed finite transfer stream: MAGIC, one header, one counted DATA record per entry, END.

    Each payload is read from its pinned source (or supplied as the two generated metadata byte strings), hashed
    again while copying, and the per-input and aggregate caps are enforced before and during the copy. An
    existing destination is a collision: never adopted, and a partial stream is retained at the destination."""
    check_limits(limits)
    total = check_entries(entries, aggregate_bytes=limits['aggregate_bytes'],
                          per_input_bytes=limits['per_input_bytes'])
    header = compact(dict(entries=entries, total_bytes=total))
    if len(header) + 1 > limits['framing_bytes']:
        raise Refusal('framing exceeds its cap')
    try:
        fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    except FileExistsError as exc:
        raise Refusal(f'{os.path.basename(destination)}: name already taken; never adopted') from exc
    written, digest = 0, hashlib.sha256()
    with os.fdopen(fd, 'wb') as out:
        def emit(data):
            nonlocal written
            out.write(data)
            digest.update(data)
            written += len(data)

        emit(MAGIC)
        emit(b'HEADER\n' + header + b'\n')
        for entry in entries:
            if entry['path'] not in sources:
                raise Refusal(f"{entry['path']}: no pinned source for this entry")
            source = sources[entry['path']]
            emit(b'DATA %d\n' % entry['bytes'])
            payload_digest, copied = hashlib.sha256(), 0
            if isinstance(source, (bytes, bytearray)):
                source_fd, chunks = None, [bytes(source)]
            else:
                source_fd = _open_source(source, entry['bytes'])
                chunks = iter(lambda: os.read(source_fd, CHUNK), b'')
            try:
                for chunk in chunks:
                    if not chunk:
                        continue
                    copied += len(chunk)
                    if copied > entry['bytes']:
                        raise Refusal(f"{entry['path']}: source is larger than its pinned size")
                    payload_digest.update(chunk)
                    emit(chunk)
                    if checkpoint is not None:
                        checkpoint()
            finally:
                if source_fd is not None:
                    os.close(source_fd)
            if copied != entry['bytes']:
                raise Refusal(f"{entry['path']}: source is shorter than its pinned size")
            if payload_digest.hexdigest() != entry['sha256']:
                raise Refusal(f"{entry['path']}: source differs from its pinned sha256")
        emit(b'END\n')
        out.flush()
        os.fsync(out.fileno())
    readback = hashlib.sha256()
    with open(destination, 'rb') as check:
        for block in iter(lambda: check.read(CHUNK), b''):
            readback.update(block)
    if readback.hexdigest() != digest.hexdigest():
        raise Refusal('transfer stream readback differs from what was written')
    return dict(path=destination, bytes=written, sha256=digest.hexdigest(), entries=entries, total_bytes=total)


# ---------------------------------------------------------------------------------------------------
# Guest side: exclusive directory creation, bounded receive, and the layout derived again.
# ---------------------------------------------------------------------------------------------------

class Names:
    """The two fixed sibling directories one verified context name derives."""

    def __init__(self, context):
        self.context = context
        self.oci = context + '.oci'
        self.debs = context + '.debs'


def open_parent(expected):
    """Open the chain-verified guest parent no-follow: canonical, private, owned, and its exact binding."""
    checked_parent(expected)
    path = Path(expected['path'])
    try:
        canonical = path.is_absolute() and str(path) == expected['path'] and '..' not in path.parts \
            and path != Path('/') and path.resolve(strict=True) == path \
            and not any(p.is_symlink() for p in (path, *path.parents)) \
            and not path.is_relative_to(SHARED) and not path.is_relative_to(EXCLUDED)
    except OSError as exc:
        raise Refusal('private canonical guest parent required') from exc
    if not canonical:
        raise Refusal('private canonical guest parent required')
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        held, visible = os.fstat(fd), path.lstat()
        if (not stat.S_ISDIR(held.st_mode) or held.st_uid != os.geteuid()
                or stat.S_IMODE(held.st_mode) & 0o022
                or (held.st_dev, held.st_ino, held.st_uid) != (expected['device'], expected['inode'], expected['uid'])
                or (visible.st_dev, visible.st_ino) != (held.st_dev, held.st_ino)):
            raise Refusal('guest parent changed or foreign')
        return path, fd
    except BaseException:
        os.close(fd)
        raise


def list_files(root_fd):
    """Every file below one owned directory, by relative path. Links, devices and foreign modes refuse."""
    found, stack = [], [('', root_fd)]
    held = []
    try:
        while stack:
            prefix, fd = stack.pop()
            for name in sorted(os.listdir(fd)):
                if name.startswith('.'):
                    raise Refusal('unexpected hidden entry in the guest input tree')
                child = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
                info = os.fstat(child)
                if stat.S_ISDIR(info.st_mode):
                    if stat.S_IMODE(info.st_mode) != 0o700 or info.st_uid != os.geteuid():
                        os.close(child)
                        raise Refusal('guest input directory is not owned 0700')
                    held.append(child)
                    stack.append((prefix + name + '/', child))
                elif stat.S_ISREG(info.st_mode):
                    if info.st_uid != os.geteuid() or info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600:
                        os.close(child)
                        raise Refusal('guest input file is not one owned 0600 file')
                    os.close(child)
                    found.append(prefix + name)
                else:
                    os.close(child)
                    raise Refusal('guest input tree holds a link or device')
    finally:
        for fd in held:
            os.close(fd)
    return sorted(found)


def _create_tree(names, parent_fd):
    """Exclusive creation of the two fixed directories and the one blob subdirectory. Existing names refuse."""
    dirs = {}
    for name in (names.oci, names.debs):
        try:
            os.mkdir(name, 0o700, dir_fd=parent_fd)
        except FileExistsError as exc:
            raise Refusal(f'{name}: guest input directory already exists; never adopted') from exc
    dirs[names.oci] = os.open(names.oci, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
    dirs[names.debs] = os.open(names.debs, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
    os.mkdir('blobs', 0o700, dir_fd=dirs[names.oci])
    blobs = os.open('blobs', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=dirs[names.oci])
    dirs[names.oci + '/blobs'] = blobs
    os.mkdir('sha256', 0o700, dir_fd=blobs)
    dirs[names.oci + '/blobs/sha256'] = os.open('sha256', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=blobs)
    return dirs


def target_dir(path, dirs, names):
    """The one fixed directory each allowlisted path lives in, and its file name."""
    if path.startswith(BLOB_DIR + '/'):
        return dirs[names.oci + '/blobs/sha256'], path[len(BLOB_DIR) + 1:]
    if path.startswith(DEB_DIR + '/'):
        return dirs[names.debs], path[len(DEB_DIR) + 1:]
    return dirs[names.oci], path[len('oci/'):]


def _read_line(stream, cap, label):
    line = stream.readline(cap + 1)
    if not line.endswith(b'\n') or len(line) > cap:
        raise Refusal(label + ' line is missing or exceeds its cap')
    return line[:-1]


def _read_exact(stream, count):
    """Read exactly count bytes in bounded chunks. A short source refuses; nothing is buffered whole."""
    remaining = count
    while remaining:
        chunk = stream.read(min(CHUNK, remaining))
        if not chunk:
            raise Refusal('transfer source ended before its declared size')
        remaining -= len(chunk)
        yield chunk


def receive(stream, spec, parent_fd, parent_path):
    """Consume one framed stream into the two fixed directories. Caps and hashes are enforced while copying."""
    limits = spec['limits']
    if _read_line(stream, 128, 'magic') + b'\n' != MAGIC:
        raise Refusal('transfer stream magic differs')
    if _read_line(stream, 16, 'header label') != b'HEADER':
        raise Refusal('transfer stream has no HEADER record')
    header_line = _read_line(stream, limits['framing_bytes'], 'header')
    try:
        header = json.loads(header_line)
    except ValueError as exc:
        raise Refusal('transfer header is not JSON') from exc
    if not isinstance(header, dict) or set(header) != {'entries', 'total_bytes'}:
        raise Refusal('transfer header must state entries and the total')
    entries = header['entries']
    declared = check_entries(entries, aggregate_bytes=limits['aggregate_bytes'],
                             per_input_bytes=limits['per_input_bytes'])
    if not positive_int(header['total_bytes']) or header['total_bytes'] != declared:
        raise Refusal('transfer header total differs from its entries')
    names = Names(spec['name'])
    dirs = _create_tree(names, parent_fd)
    try:
        for entry in entries:
            directory, name = target_dir(entry['path'], dirs, names)
            if _read_line(stream, 32, 'data') != b'DATA %d' % entry['bytes']:
                raise Refusal(f"{entry['path']}: data record does not match its declared size")
            try:
                fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
            except FileExistsError as exc:
                raise Refusal(f"{entry['path']}: guest input name already taken; never adopted") from exc
            digest = hashlib.sha256()
            with os.fdopen(fd, 'wb') as out:
                for chunk in _read_exact(stream, entry['bytes']):
                    digest.update(chunk)
                    out.write(chunk)
                out.flush()
                os.fchmod(out.fileno(), 0o600)
                os.fsync(out.fileno())
            if digest.hexdigest() != entry['sha256']:
                raise Refusal(f"{entry['path']}: received bytes differ from the declared sha256")
        if _read_line(stream, 8, 'end') != b'END':
            raise Refusal('transfer stream does not end with END')
        if stream.read(1):
            raise Refusal('transfer stream carries data after END')
    finally:
        for fd in dirs.values():
            os.fsync(fd)
            os.close(fd)
    return verify_dirs(spec, parent_fd, parent_path)


def _read_relative(root_fd, relative, cap):
    """Read one bounded file below a held directory, no-follow, without following any intermediate link."""
    parts = relative.split('/')
    opened = []
    fd = root_fd
    try:
        for part in parts[:-1]:
            fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            opened.append(fd)
        file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        try:
            info = os.fstat(file_fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size > cap:
                raise Refusal(f'{relative}: guest input is not one bounded regular file')
            with os.fdopen(file_fd, 'rb', closefd=False) as source:
                data = source.read(cap + 1)
        finally:
            os.close(file_fd)
    finally:
        for descriptor in opened:
            os.close(descriptor)
    if len(data) > cap:
        raise Refusal(f'{relative}: exceeds its cap')
    return data


def _hash_relative(root_fd, relative, entry):
    """Stream one guest input through sha256 and compare it with its pinned identity."""
    parts = relative.split('/')
    opened = []
    fd = root_fd
    try:
        for part in parts[:-1]:
            fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            opened.append(fd)
        file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        try:
            info = os.fstat(file_fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size != entry['bytes']:
                raise Refusal(f"{entry['path']}: guest input size differs from its pin")
            digest, total = hashlib.sha256(), 0
            with os.fdopen(file_fd, 'rb', closefd=False) as source:
                for block in iter(lambda: source.read(CHUNK), b''):
                    total += len(block)
                    if total > entry['bytes']:
                        raise Refusal(f"{entry['path']}: guest input grew past its pin")
                    digest.update(block)
        finally:
            os.close(file_fd)
    finally:
        for descriptor in opened:
            os.close(descriptor)
    if total != entry['bytes'] or digest.hexdigest() != entry['sha256']:
        raise Refusal(f"{entry['path']}: guest input differs from its pinned size or sha256")
    return True


def derive(entries, layout, oci_fd):
    """Re-derive the OCI relationships on the guest: one arm64 manifest, its pinned config and one layer present."""
    by_path = {entry['path']: entry for entry in entries}
    manifest_name = BLOB_DIR + '/' + hex_digest(layout['manifest']['digest'])
    config_name = BLOB_DIR + '/' + hex_digest(layout['config']['digest'])
    layer_name = BLOB_DIR + '/' + hex_digest(layout['layer']['digest'])
    for name in (manifest_name, config_name, layer_name):
        if name not in by_path:
            raise Refusal('layout blob is missing from the guest input tree')
    try:
        manifest = json.loads(_read_relative(oci_fd, manifest_name.split('/', 1)[1], by_path[manifest_name]['bytes']))
        config = json.loads(_read_relative(oci_fd, config_name.split('/', 1)[1], by_path[config_name]['bytes']))
    except ValueError as exc:
        raise Refusal('guest image manifest or config is not JSON') from exc
    if not isinstance(manifest, dict) or not isinstance(config, dict):
        raise Refusal('guest image manifest and config must be objects')
    config_ref = manifest.get('config') or {}
    layers = manifest.get('layers') or []
    if (config_ref.get('digest') != layout['config']['digest'] or config_ref.get('size') != layout['config']['bytes']
            or len(layers) != 1 or layers[0].get('digest') != layout['layer']['digest']
            or layers[0].get('size') != layout['layer']['bytes']):
        raise Refusal('guest image manifest does not name the pinned config and layer')
    if config.get('os') != 'linux' or config.get('architecture') != 'arm64':
        raise Refusal('guest image config is not linux/arm64')
    return dict(index_sha256=hex_digest(layout['index']['digest']),
                manifest_digest=layout['manifest']['digest'], manifest_bytes=layout['manifest']['bytes'],
                config_digest=layout['config']['digest'], layer_digest=layout['layer']['digest'],
                platform='linux/arm64', deb_count=len(layout['debs']),
                total_bytes=sum(entry['bytes'] for entry in entries))


def verify_dirs(spec, parent_fd, parent_path):
    """Open the two guest input directories and derive the layout again from what is on disk.

    The entry set must be exactly the fixed layout for the received upstream index, every file must carry its
    pinned size and sha256, and the index, manifest and config must describe one linux/arm64 image."""
    names = Names(spec['name'])
    held = {}
    try:
        for name in (names.oci, names.debs):
            try:
                fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
            except OSError as exc:
                raise Refusal(f'{name}: guest input directory is missing: {exc.strerror}') from exc
            held[name] = fd
            info = os.fstat(fd)
            visible = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
            if ((info.st_dev, info.st_ino) != (visible.st_dev, visible.st_ino)
                    or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700):
                raise Refusal(f'{name}: guest input directory changed, foreign or non-exclusive')
        oci_files = list_files(held[names.oci])
        deb_files = list_files(held[names.debs])
        upstream = _read_relative(held[names.oci], 'index.upstream.json', spec['layout']['index']['bytes'])
        entries, _ = layout_entries(spec['layout'], upstream)
        actual = sorted(['oci/' + rel for rel in oci_files] + ['debs/' + rel for rel in deb_files])
        if actual != sorted(entry['path'] for entry in entries):
            raise Refusal('guest input tree holds missing or extra entries')
        for entry in entries:
            if entry['path'].startswith('oci/'):
                _hash_relative(held[names.oci], entry['path'][len('oci/'):], entry)
            else:
                _hash_relative(held[names.debs], entry['path'][len('debs/'):], entry)
        derived = derive(entries, spec['layout'], held[names.oci])
        dirs = []
        for name, rows in ((names.oci, oci_files), (names.debs, deb_files)):
            info = os.fstat(held[name])
            dirs.append(dict(path=str(parent_path / name), device=info.st_dev, inode=info.st_ino,
                             uid=info.st_uid, mode=stat.S_IMODE(info.st_mode), entries=rows))
        return dict(parent=dict(spec['parent']), context=spec['name'], dirs=dirs, entries=entries, derived=derived)
    finally:
        for fd in held.values():
            os.close(fd)


def transfer(spec, stream):
    spec = checked_spec(spec)
    parent_path, parent_fd = open_parent(spec['parent'])
    try:
        return receive(stream, spec, parent_fd, parent_path)
    finally:
        os.close(parent_fd)


def check_inputs(spec):
    spec = checked_spec(spec)
    parent_path, parent_fd = open_parent(spec['parent'])
    try:
        return verify_dirs(spec, parent_fd, parent_path)
    finally:
        os.close(parent_fd)


def canonical_spec(encoded):
    """Decode one hex-encoded spec and require it to be the canonical bytes of the reviewed spec."""
    if not isinstance(encoded, str) or not encoded or len(encoded) > 2 * FRAMING_CAP_BYTES:
        raise Refusal('guest input spec must be bounded lowercase hex')
    try:
        raw = bytes.fromhex(encoded)
    except ValueError as exc:
        raise Refusal('guest input spec is not hexadecimal') from exc
    try:
        spec = json.loads(raw.decode())
    except (ValueError, UnicodeDecodeError) as exc:
        raise Refusal('guest input spec is not JSON') from exc
    if compact(checked_spec(spec)) != raw:
        raise Refusal('guest input spec is not the canonical transfer spec')
    return spec


def main(args, stdin=None, stdout=None):
    """`transfer` consumes the framed stream from stdin; `check-inputs` re-derives the written tree."""
    if not isinstance(args, list) or len(args) != 2:
        raise Refusal('guest inputs take one mode and one hex spec')
    mode, encoded = args
    if mode not in ('transfer', 'check-inputs'):
        raise Refusal('unknown guest input mode')
    spec = canonical_spec(encoded)
    if mode == 'transfer':
        receipt = transfer(spec, stdin if stdin is not None else sys.stdin.buffer)
    else:
        receipt = check_inputs(spec)
    target = stdout if stdout is not None else sys.stdout
    target.write(json.dumps(receipt, sort_keys=True) + '\n')
    target.flush()
    return receipt


if __name__ == '__main__':
    try:
        main(sys.argv[1:])
        sys.exit(0)
    except Exception as exc:
        sys.stderr.write('refused: ' + str(exc) + '\n')
        sys.exit(2)
