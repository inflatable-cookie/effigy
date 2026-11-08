"""Authenticate one fresh Stage1 occurrence; never run or infer a VM observation.

The proof-entry activation supplies the occurrence nonce, time window and signer hash
outside caller facts. Its trusted observation producer signs the complete evidence digest
using the existing receipt format. A signed GO for setup is not an observation receipt.
"""
import hashlib
import json
from pathlib import Path

import linux_receipts
import preparation_observations as observations


def digest_value(stage1):
    """All executable-admission evidence enters the setup plan, including post-state."""
    if not isinstance(stage1, dict):
        return None
    return {key: stage1.get(key) for key in ('before', 'post', 'retained', 'occurrence', 'receipt')}


def observation_digest(stage1):
    body = {key: stage1.get(key) for key in ('before', 'post', 'retained', 'occurrence')}
    return hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()


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
    if post.get('config_sha256') != before.get('config_sha256') or post.get('post_config_sha256') != before.get('config_sha256'):
        reasons.append('Stage1 post-run config differs from the original')
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
