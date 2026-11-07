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


def bind(manifest, stage_id, stage_dir, environment, inputs, gates, verify_gate, resource_accounting, manifest_path, nonce=None, guest_path=None):
    rows = [row for row in manifest['stages'] if row['id'] == stage_id]
    if len(rows) != 1:
        raise ValueError('unknown or duplicate stage')
    row = rows[0]
    if (not isinstance(resource_accounting, dict) or set(resource_accounting) != {'counter_files', 'limits'}
            or not resource_accounting['limits'] or set(resource_accounting['counter_files']) != set(resource_accounting['limits'])):
        raise ValueError('resource counters and limits are required')
    if 'network_bytes_cap' in row and (not isinstance(resource_accounting['limits'].get('network'), int)
                                        or not 0 < resource_accounting['limits']['network'] <= row['network_bytes_cap']):
        raise ValueError('network bound missing or exceeds stage cap')
    if 'E11-cargo-escape' in row['preconditions'] and environment.get('EFFIGY_OXIDE_ESCAPE') != 'cargo':
        raise ValueError('E11 escape not bound')
    if set(gates) != set(row['preconditions']):
        raise ValueError('missing or unexpected gate')
    bound = dict(inputs)
    manifest_file = Path(manifest_path).resolve()
    manifest_bytes = manifest_file.read_bytes()
    if json.loads(manifest_bytes) != manifest:
        raise ValueError('manifest bytes differ from reviewed manifest')
    own_dir = Path(__file__).resolve().parent
    modules = ('preparation_requests.py', 'preparation_stage.py', 'preparation_resources.py',
               'preparation_guest.py', 'preparation_controls.py', 'preparation_observations.py',
               'preparation_placement.py', 'preparation_archive.py', 'preparation_nextest.py',
               'preparation_metadata.py')
    for path in (manifest_file, *(own_dir / name for name in modules)):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if str(path) in bound and bound[str(path)] != digest:
            raise ValueError('source or manifest hash collision')
        bound[str(path)] = digest
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
    transport = row.get('transport', 'ssh')
    if transport not in ('host', 'ssh'):
        raise ValueError('unsupported transport')
    if transport == 'ssh':
        if not isinstance(nonce, str) or len(nonce) != 64 or any(c not in '0123456789abcdef' for c in nonce) or not callable(guest_path):
            raise ValueError('guest nonce and explicit path mapping are required')
        mapped = {}
        for path, digest in bound.items():
            remote = guest_path(path)
            if not isinstance(remote, str) or not remote.startswith('/') or str(Path(remote)) != remote or '..' in Path(remote).parts or remote in mapped:
                raise ValueError('unsafe or colliding guest path mapping')
            mapped[remote] = digest
        bound = mapped
    result = {'stage_dir': str(stage_dir), 'argv': row['argv'], 'cwd': row['cwd'],
              'timeout_seconds': row['timeout_seconds'], 'output_cap_bytes': row['output_cap_bytes'],
              'input_hashes': bound, 'environment': environment, 'resources': resource_accounting}
    if transport == 'ssh':
        result['nonce'] = nonce
    else:
        result['guest'] = False
    return result


def write_request(path, request):
    data = (json.dumps(request, sort_keys=True, indent=2) + '\n').encode()
    exclusive(path, data)
    return hashlib.sha256(data).hexdigest()
