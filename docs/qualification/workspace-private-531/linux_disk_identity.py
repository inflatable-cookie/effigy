"""Writable-top selection from a Parallels disk descriptor: the documented shape only.

Source controls only. This module reads the descriptor bytes it is given. It opens no image,
header, VM, host service or guest, and it never reads a file named by the descriptor.

Specification: QEMU "Parallels Disk Format" (docs/interop/prl-xml). The explicit Snapshots/TopGUID
is the top when present; the predefined {5fbaabe3-...} names the top only when TopGUID is absent.
BackupID cannot be the top. Split storage and Padding 1 are outside the specification and refuse.
Unspecified nodes and attributes are uninterpreted: they neither refuse nor change the selection.

Selection is metadata only. It grants no image-body readability, header or cluster-size
compatibility, guest correlation or setup proof.
"""
import hashlib
import json
import re
import xml.etree.ElementTree as ET

import preparation_observations as observations

MECHANISM = 'parallels-descriptor-selection-v1'
MAX_BYTES = 1024 * 1024
PREDEFINED_TOP = '{5fbaabe3-6958-40ff-92a7-860e329aab41}'
BACKUP_ID = '{704718e1-2314-44c8-9087-d78ed36b0f4e}'
ZERO = '{00000000-0000-0000-0000-000000000000}'
GUID = re.compile(r'^\{[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}\}$')
FILE_NAME = re.compile(r'^[A-Za-z0-9._{}-]+\.hds$')
DECIMAL = re.compile(r'^[1-9][0-9]*$')
IMAGE_TYPES = ('Plain', 'Compressed')
NOT_ESTABLISHED = ('image bodies not read; header, cluster-size and encryption compatibility '
                   'not established by metadata selection; disk UID not correlated to any controller or guest')


class Refusal(ValueError):
    pass


def _only(node, tag, where):
    found = node.findall(tag)
    if len(found) != 1:
        raise Refusal('%s needs exactly one %s, found %d' % (where, tag, len(found)))
    return found[0]


def _value(node, tag, where, pattern=None):
    """The exact text of one schema child. Empty or padded text refuses rather than being trimmed."""
    text = _only(node, tag, where).text
    if not text or text != text.strip():
        raise Refusal('%s %s is empty or padded' % (where, tag))
    if pattern is not None and not pattern.fullmatch(text):
        raise Refusal('%s %s is malformed' % (where, tag))
    return text


def _geometry(params):
    values = {tag: _value(params, tag, 'Disk_Parameters') for tag in
              ('Disk_size', 'Cylinders', 'Heads', 'Sectors', 'Padding')}
    if values['Padding'] != '0':
        raise Refusal('Padding is not 0; Padding 1 is outside the specification and refuses')
    for tag in ('Disk_size', 'Cylinders', 'Heads', 'Sectors'):
        if not DECIMAL.fullmatch(values[tag]):
            raise Refusal('Disk_Parameters %s is not a positive integer' % tag)
    size, cylinders, heads, sectors = (int(values[tag]) for tag in ('Disk_size', 'Cylinders', 'Heads', 'Sectors'))
    if cylinders * heads * sectors != size:
        raise Refusal('Cylinders*Heads*Sectors does not equal Disk_size')
    return size


def _storage(storage_data, disk_size):
    storages = storage_data.findall('Storage')
    if len(storages) != 1:
        raise Refusal('StorageData needs exactly one Storage (split storage is outside the specification)')
    storage = storages[0]
    if _value(storage, 'Start', 'Storage') != '0':
        raise Refusal('Storage Start is not 0 (split storage is outside the specification)')
    if _value(storage, 'End', 'Storage') != str(disk_size):
        raise Refusal('Storage End does not equal Disk_size')
    if not DECIMAL.fullmatch(_value(storage, 'Blocksize', 'Storage')):
        raise Refusal('Storage Blocksize is not a positive integer')
    images = storage.findall('Image')
    if not images:
        raise Refusal('Storage has no Image')
    found = {}
    for image in images:
        guid = _value(image, 'GUID', 'Image', GUID)
        kind = _value(image, 'Type', 'Image')
        name = _value(image, 'File', 'Image', FILE_NAME)
        if kind not in IMAGE_TYPES:
            raise Refusal('Image Type is unsupported: ' + kind)
        key = guid.lower()
        if key in found or name.lower() in {v['file'].lower() for v in found.values()}:
            raise Refusal('duplicate Image GUID or File: %s %s' % (guid, name))
        found[key] = {'guid': guid, 'file': name, 'type': kind}
    return found


def _snapshots(snapshots, images):
    if len(snapshots.findall('TopGUID')) > 1:
        raise Refusal('duplicate TopGUID')
    shots = snapshots.findall('Shot')
    parents, seen = {}, set()
    for shot in shots:
        guid = _value(shot, 'GUID', 'Shot', GUID).lower()
        parent = _value(shot, 'ParentGUID', 'Shot', GUID).lower()
        if guid in seen or guid not in images:
            raise Refusal('Shot GUID is duplicate or not an image')
        if parent != ZERO.lower() and parent not in images:
            raise Refusal('Shot ParentGUID is not an image of the descriptor')
        seen.add(guid)
        parents[guid] = parent
    if seen != set(images):
        raise Refusal('Shots and Images are not one-to-one')
    if sum(1 for parent in parents.values() if parent == ZERO.lower()) != 1:
        raise Refusal('the descriptor needs exactly one root snapshot')
    return parents


def _chain(top, parents, images):
    chain, current = [], top
    while current != ZERO.lower():
        if current in chain or current not in parents or len(chain) > len(images):
            raise Refusal('descriptor chain is cyclic, dangling or incomplete')
        chain.append(current)
        current = parents[current]
    return chain


def select_top(descriptor):
    """The writable top and its complete metadata, derived from the exact descriptor bytes."""
    if not isinstance(descriptor, (bytes, bytearray)) or not descriptor or len(descriptor) > MAX_BYTES:
        raise Refusal('descriptor bytes are missing or exceed the bound')
    if b'<!doctype' in bytes(descriptor).lower() or b'<!entity' in bytes(descriptor).lower():
        raise Refusal('DTD or entity declarations are outside the documented shape')
    try:
        root = ET.fromstring(bytes(descriptor))
    except ET.ParseError as exc:
        raise Refusal('invalid disk descriptor XML') from exc
    if root.tag != 'Parallels_disk_image' or root.get('Version') != '1.0':
        raise Refusal('descriptor root is not Parallels_disk_image Version 1.0')
    params = _only(root, 'Disk_Parameters', 'root')
    disk_size = _geometry(params)
    # UID is an unspecified node. It is read only as the disk identity to compare against the reviewed disk.
    disk_uid = _value(params, 'UID', 'Disk_Parameters', GUID)
    images = _storage(_only(root, 'StorageData', 'root'), disk_size)
    snapshots = _only(root, 'Snapshots', 'root')
    parents = _snapshots(snapshots, images)
    for guid in images:
        _chain(guid, parents, images)     # every image reaches the one root: no cycle or dangling branch
    top_nodes = snapshots.findall('TopGUID')
    if top_nodes:
        selector = 'TopGUID'
        top = _value(snapshots, 'TopGUID', 'Snapshots', GUID)
    else:
        selector = 'predefined'
        top = PREDEFINED_TOP
    key = top.lower()
    if key == BACKUP_ID.lower():
        raise Refusal('the BackupID GUID cannot be the top')
    if key not in images:
        raise Refusal('selected top is not an image of the descriptor')
    if key in parents.values():
        raise Refusal('selected top has a child and is not the leaf')
    chain_keys = _chain(key, parents, images)
    for member in chain_keys[:-1]:      # every overlay, the top included; only the root may be Plain
        if images[member]['type'] != 'Compressed':
            raise Refusal('an overlay in the chain is not Compressed; overlays must be expandable')
    try:
        mapping = observations.descriptor_chain(bytes(descriptor), images[key]['guid'])
    except observations.Refusal as exc:
        raise Refusal(str(exc)) from exc
    chain = [images[member]['guid'] for member in chain_keys]
    if mapping['chain'] != chain or any(mapping['files'][images[m]['guid']] != images[m]['file'] for m in chain_keys):
        raise Refusal('descriptor chain differs from the reference validator')
    result = {'mechanism': MECHANISM, 'selector': selector,
              'descriptor_sha256': hashlib.sha256(bytes(descriptor)).hexdigest(),
              'top_guid': images[key]['guid'], 'top_file': images[key]['file'],
              'chain': chain, 'chain_files': [images[m]['file'] for m in chain_keys],
              'images': {image['guid']: {'file': image['file'], 'type': image['type']} for image in images.values()},
              'disk_uid': disk_uid, 'disk_size': disk_size, 'not_established': NOT_ESTABLISHED}
    result['digest'] = hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
    return result


def observation(descriptor):
    """The observation a caller would present for these exact bytes. Verification still re-derives it."""
    selection = select_top(descriptor)
    return {'mechanism': MECHANISM, 'guid': selection['top_guid'], 'result_digest': selection['digest']}
