"""Authenticate one fresh Stage1 occurrence; never run or infer a VM observation.

The proof-entry activation supplies the occurrence nonce, time window and signer hash
outside caller facts. Its trusted observation producer signs the complete evidence digest
using the existing receipt format. A signed GO for setup is not an observation receipt.
"""
import hashlib
import json
import xml.etree.ElementTree as ET
from pathlib import Path

import linux_receipts
import preparation_observations as observations


# Individually reviewed Stage1 config transitions. Tag sequence, not occurrence index.
# Uptime: Identification/VmUptimeInSeconds is the Parallels running-time counter; 719 moved 394→499
# during one start. USB timestamps: UsbConnectHistory/USBPort/Timestamp is connect-history
# milliseconds; 719 moved four existing ports forward, Location and SystemName unchanged.
# VMNetUuid: Parallels published NetworkAdapter schema (Index, Enabled, Connected, EmulatedType,
# SystemName, UserFriendlyName, AdapterNumber, AdapterName, MAC) does not define VMNetUuid or
# state whether a rewrite is a per-session diagnostic or a virtual-network identity change.
# A brace-wrapped UUID is not evidence it is harmless.
VMNET_UUID_MISSING_FACT = (
    'VMNetUuid transition is unadmitted: Parallels published NetworkAdapter schema does not '
    'define VMNetUuid or state whether a rewrite is a per-session diagnostic or a '
    'virtual-network identity change'
)


def digest_value(stage1):
    """All executable-admission evidence enters the setup plan, including post-state."""
    if not isinstance(stage1, dict):
        return None
    return {key: stage1.get(key) for key in ('before', 'post', 'retained', 'occurrence', 'receipt')}


def observation_digest(stage1):
    body = {key: stage1.get(key) for key in ('before', 'post', 'retained', 'occurrence')}
    return hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()


def _tag_sequence(path):
    return tuple(part.split('[', 1)[0] for part in path.strip('/').split('/') if part)


def _nonneg_int(text):
    return isinstance(text, str) and text.isascii() and text.isdigit()


def _classify_leaf(path, old, new, has_children):
    """Return (kind, admitted, reason). Structure-equal leaves only; unknown paths refuse."""
    if has_children:
        return ('unknown', False, 'protected config content changed at ' + path)
    tags = _tag_sequence(path)
    if tags[-2:] == ('Identification', 'VmUptimeInSeconds'):
        if not _nonneg_int(old) or not _nonneg_int(new):
            return ('uptime', False, 'VmUptimeInSeconds is not a non-negative integer at ' + path)
        if int(new) < int(old):
            return ('uptime', False, 'VmUptimeInSeconds went backwards at ' + path)
        return ('uptime', True, None)
    if len(tags) >= 3 and tags[-3:] == ('UsbConnectHistory', 'USBPort', 'Timestamp'):
        if not _nonneg_int(old) or not _nonneg_int(new):
            return ('usb_timestamp', False, 'UsbConnectHistory timestamp is not a non-negative integer at ' + path)
        if int(new) < int(old):
            return ('usb_timestamp', False, 'UsbConnectHistory timestamp went backwards at ' + path)
        return ('usb_timestamp', True, None)
    if tags[-2:] == ('NetworkAdapter', 'VMNetUuid'):
        return ('vmnet_uuid', False, VMNET_UUID_MISSING_FACT + ' at ' + path)
    return ('unknown', False, 'protected config content changed at ' + path)


def config_transitions(before, after):
    """Walk both trees. Structure, order, attributes and tails stay exact. Classify leaf text.

    Raises on missing, malformed, duplicate-length or structural drift. Byte-identical
    originals still hash separately; permitted leaf changes keep both streams.
    """
    if not before or not after:
        raise observations.Refusal('both original config byte streams are required')
    changes = []

    def compare(a, b, path):
        if a.tag != b.tag or a.attrib != b.attrib or a.tail != b.tail or len(a) != len(b):
            raise observations.Refusal('config structure or attributes changed at ' + path)
        if (a.text or '') != (b.text or ''):
            kind, admitted, reason = _classify_leaf(path, a.text or '', b.text or '', len(a) > 0)
            changes.append({'path': path, 'kind': kind, 'admitted': admitted, 'reason': reason})
        counts = {}
        for child_a, child_b in zip(a, b):
            counts[child_a.tag] = counts.get(child_a.tag, 0) + 1
            compare(child_a, child_b, path + '/' + child_a.tag + '[' + str(counts[child_a.tag]) + ']')

    try:
        root_a, root_b = ET.fromstring(before), ET.fromstring(after)
        compare(root_a, root_b, '/' + root_a.tag + '[1]')
    except ET.ParseError as exc:
        raise observations.Refusal('invalid config XML') from exc
    except (LookupError, UnicodeError, ValueError) as exc:
        raise observations.Refusal('malformed config XML') from exc
    return {'before_sha256': hashlib.sha256(before).hexdigest(),
            'after_sha256': hashlib.sha256(after).hexdigest(),
            'changes': changes,
            'changed_volatile_paths': [item['path'] for item in changes if item['admitted']]}


def compare_protected_config(before, after):
    """Admit only implemented justified leaf rules. Unknown or VMNetUuid drift refuses."""
    result = config_transitions(before, after)
    refusals = [item['reason'] for item in result['changes'] if not item['admitted'] and item['reason']]
    if refusals:
        raise observations.Refusal('; '.join(refusals))
    return result


def binding_reasons(fresh, anchors, trust, head, vm, growth_cap):
    s1 = fresh.get('stage1')
    if not isinstance(s1, dict) or any(not isinstance(s1.get(k), dict)
                                        for k in ('before', 'post', 'retained', 'occurrence')):
        return ['Stage1 originals and authenticated post-state not bound']
    before, post, retained, occurrence = (s1[k] for k in ('before', 'post', 'retained', 'occurrence'))
    reasons = []
    for key, name, expected in (
            ('config', 'original config', before.get('config_sha256')),
            ('descriptor', 'original descriptor', before.get('descriptor_sha256')),
            ('post_config', 'post config', post.get('post_config_sha256')),
            ('post_descriptor', 'post descriptor', post.get('post_descriptor_sha256'))):
        try:
            actual = hashlib.sha256(Path(retained[key]).read_bytes()).hexdigest()
        except (KeyError, TypeError, OSError):
            actual = None
        if not expected or actual != expected:
            reasons.append('Stage1 retained ' + name + ' does not match the observation')
    if post.get('config_sha256') != before.get('config_sha256'):
        reasons.append('Stage1 post observation original hash differs from the before observation')
    try:
        transition = compare_protected_config(Path(retained['config']).read_bytes(),
                                              Path(retained['post_config']).read_bytes())
        if post.get('post_config_sha256') != transition['after_sha256']:
            reasons.append('Stage1 post-run config hash differs from retained post bytes')
    except (observations.Refusal, OSError, KeyError, TypeError, ValueError, LookupError, UnicodeError) as exc:
        reasons.append('Stage1 post-run config transition: ' + str(exc))
    if post.get('post_descriptor_sha256') != before.get('descriptor_sha256'):
        reasons.append('Stage1 post-run descriptor differs from the original')
    try:
        growth = observations.compare_extents(before.get('extents'), post.get('extents'), before.get('chain'), cap=growth_cap)
        if (not isinstance(post.get('growth_bytes'), int) or isinstance(post.get('growth_bytes'), bool)
                or growth != post.get('growth_bytes') or post.get('chain') != before.get('chain')):
            reasons.append('Stage1 post-run inventory or measured growth differs')
    except (observations.Refusal, TypeError, AttributeError):
        reasons.append('Stage1 post-run extent evidence is invalid')
    if not isinstance(anchors, dict) or not anchors.get('stage1_nonce'):
        reasons.append('Stage1 fresh occurrence anchor unbound')
    elif occurrence.get('nonce') != anchors['stage1_nonce']:
        reasons.append('Stage1 occurrence is stale or differs from activation')
    start, end = occurrence.get('started_at'), occurrence.get('finished_at')
    low, high = (anchors or {}).get('stage1_not_before'), (anchors or {}).get('stage1_not_after')
    if any(not isinstance(n, int) or isinstance(n, bool) for n in (start, end, low, high)) or not low <= start <= end <= high or end - start > 600:
        reasons.append('Stage1 occurrence is outside the fresh activation window')
    if (occurrence.get('vm') != vm or occurrence.get('start_count') != 1
            or occurrence.get('start_waited_exit') != 0 or occurrence.get('stop_waited_exit') != 0
            or occurrence.get('terminal_state') != 'stopped' or occurrence.get('forced_stop') is not False
            or occurrence.get('snapshot_changed') is not False):
        reasons.append('Stage1 occurrence lacks the bounded waited start and graceful stopped observation')
    if occurrence.get('probe_sha256') != (anchors or {}).get('probe_sha256') or occurrence.get('identity') != fresh.get('identity'):
        reasons.append('Stage1 occurrence probe or guest identity differs')
    ok, reason = linux_receipts.verify_signed_receipt(s1.get('receipt'), 'Stage1-observed', observation_digest(s1), trust, head)
    if not ok:
        reasons.append('Stage1 observation authentication: ' + reason)
    return reasons
