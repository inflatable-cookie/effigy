"""Materialise a reviewed stage request from hash-bound, passing gate receipts.

This is not an authenticator: the caller must independently authenticate each
receipt and retain it. Unknown or absent gates cannot produce a request.
"""
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from preparation_stage import exclusive


def bind(manifest, stage_id, stage_dir, environment, inputs, gates, verify_gate):
    rows = [row for row in manifest['stages'] if row['id'] == stage_id]
    if len(rows) != 1:
        raise ValueError('unknown or duplicate stage')
    row = rows[0]
    if 'E11-cargo-escape' in row['preconditions'] and environment.get('EFFIGY_OXIDE_ESCAPE') != 'cargo':
        raise ValueError('E11 escape not bound')
    if set(gates) != set(row['preconditions']):
        raise ValueError('missing or unexpected gate')
    bound = dict(inputs)
    for gate, path in gates.items():
        path = str(Path(path).resolve())
        data = Path(path).read_bytes()
        receipt = json.loads(data)
        if receipt.get('gate') != gate or receipt.get('status') != 'pass' or verify_gate(gate, receipt) is not True:
            raise ValueError('gate not verified: ' + gate)
        digest = hashlib.sha256(data).hexdigest()
        if path in bound and bound[path] != digest:
            raise ValueError('input collision')
        bound[path] = digest
    if row['timeout_seconds'] + manifest['admission_reserve_seconds'] + manifest['report_reserve_seconds'] > manifest['max_stage_seconds_including_admission_and_report']:
        raise ValueError('stage deadline exceeds total')
    return {'stage_dir': str(stage_dir), 'argv': row['argv'], 'cwd': row['cwd'],
            'timeout_seconds': row['timeout_seconds'], 'output_cap_bytes': row['output_cap_bytes'],
            'input_hashes': bound, 'guest': True, 'environment': environment}


def write_request(path, request):
    data = (json.dumps(request, sort_keys=True, indent=2) + '\n').encode()
    exclusive(path, data)
    return hashlib.sha256(data).hexdigest()
