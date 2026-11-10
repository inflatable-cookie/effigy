#!/usr/bin/env python3
"""Read-only OFFLINE aggregate qualification verification.

Aggregate judgment (#624) reads and hashes retained original-receipt exports
locally: no SSH, no runtime, no qualification-root reopen, no backend or VM
observation. The authorized backend proof turn retains the exact original
complete/failed receipt bytes as a bounded, hash-bound export inside its own
budget; this verifier consumes only those retained copies.

One backend success, fake fixture, diagnostic or refusal cannot qualify the
packet: every required positive backend receipt must be authentic, complete,
carry the fixed full-session cell contract, and match the SAME final source,
feature and common contract. Each platform's actual artifact bytes stay bound
to their own producer receipts and are compared per platform, never across
platforms. This verifier writes nothing and starts nothing; its module surface
deliberately excludes every SSH/runtime/native/root API.
"""
import argparse
import json
import os
from pathlib import Path
import re
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import BACKENDS, SCRATCH, digest, load, qualification_path, require

CONTRACT = 'effigy.workspace-private-proof.v1'
INVOCATION = 'effigy.workspace-private-backend-invocation.v1'
QUALIFICATION_FEATURES = ['workspace-private-qualification']
# The packet's fixed positive full-session cell contract, in first-occurrence
# order. down and positive-session legitimately recur; no other cell may appear.
REQUIRED_CELLS = ('production-refusal', 'positive-session', 'native-identity-and-tls', 'level-one-stream',
                  'real-signal', 'credential-refusal', 'caller-drop', 'gap-and-truncation', 'kernel-custody',
                  'down', 'session-crash-recovery', 'baseline-and-cleanup')
# The recorded positive facts each cell family must carry; None means a truthy
# recorded identity, any other value must compare exactly. No caller bool alone
# is authority: a wrong, false or absent fact refuses the receipt.
CELL_FACTS = {
    'positive-session': {'session': None},
    'native-identity-and-tls': {'wrong_pin_refused': True},
    'level-one-stream': {'real_exit': 23, 'no_pty_and_stdin_eof': True},
    'real-signal': {'signal': 15},
    'credential-refusal': {'invalid_new_client_refused': True},
    'caller-drop': {'actual_cancelled_and_settled': True, 'cause': 'OwnerLost'},
    'gap-and-truncation': {'unknown_refused': True},
    'kernel-custody': {'actual_helpers': True},
    'session-crash-recovery': {'killed_supervisor': None, 'fresh_session': None},
    'down': {'closed': True},
    'baseline-and-cleanup': {'fixture_retained': True},
}


def parse_pairs(values, label):
    pairs = {}
    for spec in values:
        backend, separator, rest = spec.partition('=')
        require(separator and rest and backend in BACKENDS and backend not in pairs,
                'exactly one ' + label + ' binding per backend is required')
        pairs[backend] = rest
    return pairs


def receipt_file(raw, label):
    path = Path(raw)
    require(path.is_absolute() and str(path) == raw and '..' not in path.parts
            and not path.is_symlink() and path.is_file() and path.is_relative_to(SCRATCH),
            label + ' must be an absolute retained scratch file')
    return path


def common_contract(record):
    # The shared contract is source, features, contract schema, executed driver
    # bytes and old source identity. Actual artifact bytes are per-platform and
    # are never compared across Darwin and native Linux.
    binding = record['binding']
    return (binding['source_head'], binding['contract'],
            tuple(binding['candidate_receipt']['features']), tuple(binding['production_receipt']['features']),
            binding['old_source_head'], json.dumps(binding['programs'], sort_keys=True))


def fact_recorded(cell, field, expected):
    # Identity facts (expected None) must be a non-bool, non-empty string or
    # integer; boolean facts compare by identity so 1 never passes for True;
    # any other expected value compares type-exactly. A wrong, false, absent
    # or type-confused fact refuses the receipt.
    if field not in cell:
        return False
    value = cell[field]
    if expected is None:
        if isinstance(value, bool):
            return False
        if isinstance(value, str):
            return value != ''
        return isinstance(value, int)
    if isinstance(expected, bool):
        return value is expected
    return type(value) == type(expected) and value == expected


def verify_receipt_semantics(record):
    # Offline original-receipt judgment: hash the retained export locally and
    # require the actual complete positive cell contract, recorded baseline,
    # cleanup and fixture-retention equality. A caller bool alone is never
    # authority; a copied, self-labelled or incomplete receipt refuses.
    export = record['receipt_export']
    path = receipt_file(export['path'], 'retained receipt export')
    require(digest(path) == export['sha256'] == record['receipt_sha256'],
            'retained original receipt bytes differ from the recorded hash')
    parsed = load(path)
    require(parsed.get('native') is True and parsed.get('production_enabled') is False,
            'not a native, production-disabled complete receipt')
    require(not parsed.get('exploratory') and parsed.get('proof') is not False,
            'exploratory receipts cannot qualify')
    cells = parsed.get('cells')
    require(isinstance(cells, list) and cells, 'complete receipt records no cells')
    all_names=[cell.get('cell') for cell in cells]
    if record['backend']=='linux':
        extras={'bounded-native-workload','bounded-native-writers'}
        activations=[index for index,name in enumerate(all_names) if name=='positive-session']
        require(len(activations)==3 and all(all_names[index+1:index+3]==['bounded-native-workload','bounded-native-writers']
                for index in activations),'Linux activations lack their workload and writer observations')
        require(all_names.count('bounded-native-workload')==all_names.count('bounded-native-writers')==3,
                'Linux receipt has missing or extra writer gates')
        names=[name for name in all_names if name not in extras]
    else:
        names=all_names
    require(all(name in REQUIRED_CELLS for name in names),
            'receipt records a cell outside the fixed full-session contract')
    require(list(dict.fromkeys(names)) == list(REQUIRED_CELLS),
            'receipt cells do not match the fixed full-session contract in order')
    require(names[0] == 'production-refusal' and names[-1] == 'baseline-and-cleanup',
            'receipt does not span the ordinary refusal through restored baseline')
    for cell in cells:
        for field, expected in CELL_FACTS.get(cell.get('cell'), {}).items():
            require(fact_recorded(cell, field, expected),
                    'receipt records a cell without its recorded positive facts')
    baseline = cells[0].get('inventory')
    require(isinstance(baseline, dict) and cells[-1].get('inventory') == baseline
            and cells[-1].get('fixture_retained') is True,
            'recorded baseline/cleanup/fixture-retention differs')


def verify_record(backend, record, head):
    require(record['schema'] == INVOCATION and record['backend'] == backend
            and record['binding']['backend'] == backend, 'unknown or misbound invocation receipt')
    require(record['exit_code'] == 0 and record['source_unchanged'] is True
            and record['positive'] is True,
            'a failed, refused, diagnostic or source-changed backend cannot qualify the packet')
    binding = record['binding']
    require(binding['contract'] == CONTRACT, 'receipt contract differs')
    require(binding['source_head'] == head, 'stale or mixed-source receipt')
    require(binding['candidate_receipt']['source_head'] == head
            and binding['candidate_receipt']['features'] == QUALIFICATION_FEATURES
            and binding['candidate_receipt']['sha256'] == binding['candidate']['sha256'],
            'receipt does not carry the exact qualification candidate contract')
    require(binding['production_receipt']['source_head'] == head
            and binding['production_receipt']['features'] == []
            and binding['production_receipt']['sha256'] == binding['production']['sha256'],
            'receipt does not carry the exact ordinary production contract')
    old = binding['old_source_head']
    require(old != head and re.fullmatch(r'[0-9a-f]{40}', old)
            and binding['old_source_receipt']['source_head'] == old
            and binding['old_source_receipt']['sha256'] == binding['old_binary']['sha256'],
            'receipt does not carry the genuine old-baseline contract')
    require(binding['driver'] in ('local', 'ssh') and binding['engine_kind'] in ('colima', 'docker')
            and binding['engine_identity'] and binding['runtime_image'].startswith('sha256:')
            and type(binding['budget_seconds']) is int and 0 < binding['budget_seconds'] <= 1800
            and binding['one_shot_marker'],
            'platform-specific artifacts must be explicit')
    require(re.fullmatch(r'[0-9a-f]{64}', binding['inputs']['sha256'])
            and binding['go_decision_id'] and binding['go_receipt'].get('sha256'),
            'invocation authority bindings must be explicit')
    require((binding['driver'] == 'ssh') == (backend == 'linux')
            and (binding['engine_kind'] == 'colima') == (backend == 'colima'),
            'backend driver or engine identity differs from its platform')
    root = qualification_path(binding['qualification_root'])
    require(Path(record['receipt']) == root / 'complete.json',
            'the recorded receipt must be the pinned root\u2019s complete.json')
    if backend == 'linux':
        import linux_capacity_contract as capacity
        preparation=binding['linux_preparation']
        prepared_path=receipt_file(preparation['path'],'retained Linux preparation')
        require(digest(prepared_path)==preparation['sha256'],'Linux preparation bytes changed')
        prepared=load(prepared_path)
        require(prepared['schema']=='effigy.linux-physical-preparation-complete.v1'
                and prepared['source_head']==head and prepared['cell']['linux_capacity']==binding['linux_capacity'],
                'Linux full receipt does not carry its frozen preparation')
        original = record['linux_capacity_originals']
        path = receipt_file(original['path'], 'Linux physical originals')
        require(digest(path) == original['sha256'], 'Linux physical originals changed')
        capacity.verify_full(binding['linux_capacity'], load(path), head)
        limits=binding['linux_capacity']['plan']['phases']['activation']['images']
        allocation=record['linux_allocation']
        peak=allocation['sampled_peak_bytes'][limits['engine']['mountpoint']]
        require(type(peak) is int and peak>0 and type(allocation['samples']) is int and allocation['samples']>0
                and allocation['sample_period_seconds']==.05
                and allocation['engine_capacity_bytes']==limits['engine']['image_bytes']
                and allocation['engine_has_twice_sampled_peak_headroom'] is True
                and peak*2<=limits['engine']['image_bytes'], 'Linux engine lacks recorded two-times peak headroom')
        claim=Path(limits['preparation']['mountpoint'])/'proof-claims'/('531-'+binding['inputs']['sha256']+'-linux.started.json')
        require(binding['one_shot_marker']==str(claim),'Linux one-use claim escapes its prepared role')
    verify_receipt_semantics(record)


def aggregate(args):
    receipts = parse_pairs(args.receipt, '--receipt')
    hashes = parse_pairs(args.receipt_sha256, '--receipt-sha256')
    require(re.fullmatch(r'[0-9a-f]{40}', args.source_head), 'final source head required')
    require(set(receipts) == set(BACKENDS) and set(hashes) == set(BACKENDS),
            'missing backend receipt: aggregate judgment needs every required positive backend')
    records = {}
    for backend in BACKENDS:
        path = receipt_file(receipts[backend], 'invocation receipt')
        require(re.fullmatch(r'[0-9a-f]{64}', hashes[backend]), 'bound invocation receipt hash required')
        require(digest(path) == hashes[backend], 'invocation receipt differs from its bound hash')
        records[backend] = load(path)
    for backend in BACKENDS:
        verify_record(backend, records[backend], args.source_head)
    require(len({common_contract(record) for record in records.values()}) == 1,
            'receipts do not match the SAME final source, features and common contract')
    first = records['colima']['binding']
    return {'schema': 'effigy.workspace-private-aggregate.v1', 'qualified': True,
            'source_head': args.source_head, 'contract': CONTRACT,
            'candidate_features': first['candidate_receipt']['features'],
            'production_features': first['production_receipt']['features'],
            'old_source_head': first['old_source_head'],
            'artifacts': {backend: {'candidate': records[backend]['binding']['candidate'],
                                    'production': records[backend]['binding']['production'],
                                    'old_binary': records[backend]['binding']['old_binary']}
                          for backend in BACKENDS},
            'backends': {backend: {'invocation_receipt': receipts[backend],
                                   'inputs': records[backend]['binding']['inputs'],
                                   'go_decision_id': records[backend]['binding']['go_decision_id'],
                                   'go_receipt': records[backend]['binding']['go_receipt'],
                                   'engine_kind': records[backend]['binding']['engine_kind'],
                                   'engine_identity': records[backend]['binding']['engine_identity'],
                                   'host': records[backend]['host'],
                                   'qualification_root': records[backend]['binding']['qualification_root'],
                                   'native_receipt': records[backend]['receipt'],
                                   'native_receipt_sha256': records[backend]['receipt_sha256'],
                                   'receipt_export': records[backend]['receipt_export'],
                                   'one_shot_marker': records[backend]['binding']['one_shot_marker']}
                         for backend in BACKENDS},
            'consumer': 'separate-owner-evidence-required', 'production_enablement': False}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-head', required=True)
    parser.add_argument('--receipt', action='append', required=True, metavar='BACKEND=PATH')
    parser.add_argument('--receipt-sha256', action='append', required=True, metavar='BACKEND=SHA256')
    args = parser.parse_args()
    os.umask(0o077)
    print(json.dumps(aggregate(args)))


if __name__ == '__main__':
    main()
