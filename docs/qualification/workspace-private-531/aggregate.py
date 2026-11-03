#!/usr/bin/env python3
"""Read-only aggregate qualification verification; never a native action.

One backend success, fake fixture, diagnostic or refusal cannot qualify the
packet: every required positive backend receipt must be authentic, complete and
match the SAME final source/feature/common contract, with platform-specific
artifacts explicit. This verifier writes nothing, starts no backend and mutates
no resource; its only remote access is a read-only sha256sum over the Linux
receipt through that backend's own pinned private SSH artifacts.
"""
import argparse
import json
import os
from pathlib import Path
import re
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BACKENDS, SCRATCH, digest, load, require, run, ssh_prefix,
                    open_qualification_root)

CONTRACT = 'effigy.workspace-private-proof.v1'
INVOCATION = 'effigy.workspace-private-backend-invocation.v1'
QUALIFICATION_FEATURES = ['workspace-private-qualification']


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
    binding = record['binding']
    return (binding['candidate']['sha256'], tuple(binding['candidate_receipt']['features']),
            binding['production']['sha256'], tuple(binding['production_receipt']['features']),
            binding['old_source_head'], binding['old_binary']['sha256'], binding['contract'],
            json.dumps(binding['programs'], sort_keys=True))


def verify_original_receipt(record):
    # The original complete receipt is authenticated against the pinned root
    # itself: locally by held descriptor and identity, remotely by a read-only
    # hash over the exact pinned SSH artifacts. No replacement receipt is read.
    binding = record['binding']
    root = binding['qualification_root']
    expected = Path(root['path']) / 'complete.json'
    require(Path(record['receipt']) == expected,
            'the recorded receipt must be the pinned root\u2019s complete.json')
    if binding['driver'] == 'ssh':
        found = run(ssh_prefix(binding) + ['sha256sum', '--', str(expected)]).stdout.decode().split()[0]
    else:
        _, fd = open_qualification_root(root)
        try:
            try:
                found = digest('complete.json', dir_fd=fd)
            except OSError as error:
                raise RuntimeError('original complete receipt is missing') from error
        finally:
            os.close(fd)
    require(found == record['receipt_sha256'], 'original receipt differs from the recorded hash')


def verify_record(backend, record, head):
    require(record['schema'] == INVOCATION and record['backend'] == backend
            and record['binding']['backend'] == backend, 'unknown or misbound invocation receipt')
    require(record['exit_code'] == 0, 'a failed, refused or diagnostic backend cannot qualify the packet')
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
    require((binding['driver'] == 'ssh') == (backend == 'linux')
            and (binding['engine_kind'] == 'colima') == (backend == 'colima'),
            'backend driver or engine identity differs from its platform')
    verify_original_receipt(record)


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
            'receipts do not match the SAME final candidate contract')
    first = records['colima']['binding']
    return {'schema': 'effigy.workspace-private-aggregate.v1', 'qualified': True,
            'source_head': args.source_head, 'contract': CONTRACT,
            'candidate': {'sha256': first['candidate']['sha256'],
                          'features': first['candidate_receipt']['features']},
            'production': {'sha256': first['production']['sha256'],
                           'features': first['production_receipt']['features']},
            'old_baseline': {'source_head': first['old_source_head'],
                             'sha256': first['old_binary']['sha256']},
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
