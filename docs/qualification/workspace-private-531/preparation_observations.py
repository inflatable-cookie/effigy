"""Conservative comparisons of injected observations; no host or guest actions."""
import hashlib
import xml.etree.ElementTree as ET


class Refusal(ValueError):
    pass


def compare_config(before, after, volatile_paths):
    """Only explicitly identified element text can differ; structure stays exact."""
    if not before or not after:
        raise Refusal('both original config byte streams are required')
    allow = set(volatile_paths)
    if len(allow) != len(volatile_paths) or any(not p.startswith('/') or '*' in p for p in allow):
        raise Refusal('volatile paths must be unique and exact')
    seen = set()

    def compare(a, b, path):
        if a.tag != b.tag or a.attrib != b.attrib or a.tail != b.tail or len(a) != len(b):
            raise Refusal('config structure or attributes changed at ' + path)
        if a.text != b.text:
            if path not in allow or len(a):
                raise Refusal('protected config content changed at ' + path)
            seen.add(path)
        counts = {}
        for child_a, child_b in zip(a, b):
            counts[child_a.tag] = counts.get(child_a.tag, 0) + 1
            compare(child_a, child_b, path + '/' + child_a.tag + '[' + str(counts[child_a.tag]) + ']')

    try:
        a, b = ET.fromstring(before), ET.fromstring(after)
        compare(a, b, '/' + a.tag + '[1]')
    except ET.ParseError as exc:
        raise Refusal('invalid config XML') from exc
    return {'before_sha256': hashlib.sha256(before).hexdigest(),
            'after_sha256': hashlib.sha256(after).hexdigest(),
            'changed_volatile_paths': sorted(seen)}


def compare_extents(before, after, chain, cap=1024 ** 3):
    """Require the same complete descriptor mapping and file identities."""
    if not before or set(before) != set(after) or not chain or len(set(chain)) != len(chain):
        raise Refusal('extent inventory or chain changed')
    if not set(chain) <= set(before):
        raise Refusal('chain contains an unaccounted extent')
    total = 0
    for guid, old in before.items():
        new = after[guid]
        for key in ('path', 'parent', 'device', 'inode'):
            if key not in old or old[key] != new.get(key):
                raise Refusal('extent substitution: ' + guid)
        if not isinstance(old.get('size'), int) or not isinstance(new.get('size'), int):
            raise Refusal('missing extent size: ' + guid)
        if old['size'] < 0 or new['size'] < old['size']:
            raise Refusal('extent shrank: ' + guid)
        total += new['size'] - old['size']
    for index, guid in enumerate(chain[:-1]):
        if before[guid]['parent'] != chain[index + 1]:
            raise Refusal('descriptor chain mismatch: ' + guid)
    if total > cap or cap > 1024 ** 3 or cap < 0:
        raise Refusal('extent growth cap exceeded')
    return total


def guest_closure(intent, terminal):
    """A transport exit is not evidence of a guest process having closed."""
    if terminal is None:
        return 'unknown'
    keys = ('nonce', 'boot_id', 'pid', 'start_time', 'session_id', 'deadline_monotonic')
    if any(not intent.get(k) or terminal.get(k) != intent[k] for k in keys):
        raise Refusal('guest process identity drift')
    if terminal.get('waited_exit') is None or terminal.get('session_empty') is not True:
        return 'unknown'
    # An empty session does not rule out a descendant that called setsid.
    return 'unknown'


def descriptor_chain(xml_bytes, top):
    """Resolve an independently identified writable top; never guess the top."""
    import re
    try:
        root = ET.fromstring(xml_bytes)
        images, parents = {}, {}
        guid_pattern = re.compile(r'^\{[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}\}$')
        for item in root.findall('./StorageData/Storage/Image'):
            guid, name = item.findtext('GUID'), item.findtext('File')
            if not guid or not guid_pattern.fullmatch(guid) or not name or '/' in name or name in ('.', '..') or guid in images or name in images.values():
                raise Refusal('invalid or duplicate descriptor image')
            images[guid] = name
        for item in root.findall('./Snapshots/Shot'):
            guid, parent = item.findtext('GUID'), item.findtext('ParentGUID')
            if not guid or guid not in images or not parent or not guid_pattern.fullmatch(parent) or guid in parents:
                raise Refusal('invalid or duplicate descriptor parent')
            parents[guid] = parent
        if not images or set(images) != set(parents) or top not in images:
            raise Refusal('descriptor inventory or top incomplete')
        chain = []
        current = top
        sentinel = '{00000000-0000-0000-0000-000000000000}'
        while current != sentinel:
            if current not in parents or current in chain:
                raise Refusal('descriptor chain missing or cyclic')
            chain.append(current)
            current = parents[current]
        return {'files': images, 'parents': parents, 'chain': chain}
    except ET.ParseError as exc:
        raise Refusal('invalid disk descriptor') from exc


def setup_readiness(system_state, docker_state, authenticated_boot_id):
    if not authenticated_boot_id or system_state != 'running':
        raise Refusal('guest has not positively completed boot')
    if docker_state in ('active', 'activating'):
        return 'do-not-start'
    if docker_state == 'inactive':
        return 'start-only-if-admitted'
    raise Refusal('docker state is not a stable setup baseline')
