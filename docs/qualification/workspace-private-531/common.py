"""Reviewed full-session packet primitives. No engine action at import time."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

BACKENDS = ('colima', 'desktop', 'linux')
SCRATCH = Path('/Volumes/Builds/scratch/effigy')
PACKET = Path(__file__).resolve().parent
HEX = re.compile(r'[0-9a-f]{64}')


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def load(path):
    return json.loads(Path(path).read_bytes())


def artifact(item):
    path = Path(item['path'])
    require(path.is_absolute() and not path.is_symlink() and path.is_file(), 'artifact unavailable')
    require(HEX.fullmatch(item['sha256']) and digest(path) == item['sha256'], 'artifact hash mismatch')
    return path


def exclusive(path, value):
    with open(path, 'x', encoding='utf-8') as out:
        json.dump(value, out, sort_keys=True)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())


def run(argv, *, cwd=None, env=None, timeout=30, check=True, data=None):
    # Diagnostics are fixed: subprocess output can include a guest credential.
    result = subprocess.run([str(x) for x in argv], cwd=cwd, env=env, input=data,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    require(not check or result.returncode == 0, 'required command failed: ' + Path(argv[0]).name)
    return result


def source_head(root):
    head = run(['git', 'rev-parse', 'HEAD'], cwd=root).stdout.decode().strip()
    require(not run(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=root).stdout,
            'proof requires unchanged tracked source and index')
    return head


def validate_inputs(value, head):
    require(value['schema'] == 'effigy.workspace-private-proof.v1', 'unknown input schema')
    require(value['source_head'] == head, 'candidate is not the immutable proof head')
    require(tuple(cell['backend'] for cell in value['backends']) == BACKENDS,
            'all three native backends are required, in order')
    for cell in value['backends']:
        require(cell['source_head'] == head, 'backend candidate source differs')
        require(cell['old_source_head'] != head and re.fullmatch(r'[0-9a-f]{40}', cell['old_source_head']),
                'genuine old source identity required')
        require(cell['old_source_receipt']['source_head'] == cell['old_source_head'], 'old receipt source mismatch')
        require(cell['candidate_receipt']['source_head'] == head, 'candidate build receipt source mismatch')
        require(cell['candidate_receipt']['features'] == ['workspace-private-qualification'],
                'native candidate must exclude external-I/O doubles and fault injection')
        require(len({cell['candidate']['sha256'], cell['production']['sha256'], cell['old_binary']['sha256']}) == 3,
                'distinct qualification, ordinary and genuine old binaries required')
        require(cell['candidate_receipt']['sha256'] == cell['candidate']['sha256'], 'candidate receipt mismatch')
        require(cell['old_source_receipt']['sha256'] == cell['old_binary']['sha256'], 'old receipt mismatch')
        require(cell['production_receipt']['source_head'] == head and cell['production_receipt']['features'] == []
                and cell['production_receipt']['sha256'] == cell['production']['sha256'], 'ordinary production build receipt mismatch')
        require(not cell.get('environment'), 'ambient environment overrides prohibited')
        require(cell['runtime_image'].startswith('sha256:') and HEX.fullmatch(cell['runtime_image'][7:]),
                'resident image must be pinned by ID')
        require(cell['budget_seconds'] > 0 and cell['budget_seconds'] <= 1800,
                'packet cannot extend the existing 1800 second test window')
        require(cell['driver'] in ('local', 'ssh'), 'unknown native driver')
        require((cell['backend'] == 'linux') == (cell['driver'] == 'ssh'),
                'Linux must execute natively in its separately approved Linux host')
        require(cell['engine_kind'] in ('colima', 'docker'), 'unknown engine kind')
        require((cell['backend'] == 'colima') == (cell['engine_kind'] == 'colima'), 'backend engine mismatch')
        require(not any(key.startswith(('EFFIGY_TEST', 'EFFIGY_WORKSPACE_ACTIVATION_FAKE'))
                        for key in cell.get('environment', {})), 'native doubles are prohibited')
    return value


def validate_go(go, inputs_hash, cell, head):
    require(go['schema'] == 'effigy.workspace-private-native-go.v1', 'unknown GO schema')
    require(go['operator'] == 'Tom' and go['decision_id'] and go['kind'] == 'native', 'scoped native GO required')
    require(go['source_head'] == head and go['inputs_sha256'] == inputs_hash, 'GO candidate/input binding mismatch')
    require(go['backend'] == cell['backend'], 'GO belongs to another backend')
    require(go['budget_seconds'] == cell['budget_seconds'], 'GO budget mismatch')
    require(go['engine_identity'] == cell['engine_identity'], 'GO engine identity mismatch')
    require(go['effects'] == 'workspace-private-531-full-session-r1', 'GO does not cover this packet')
    # Expiry is checked on each machine immediately before effects.
    import datetime
    expires = datetime.datetime.fromisoformat(go['expires_at'].replace('Z', '+00:00'))
    require(expires.tzinfo is not None and expires > datetime.datetime.now(datetime.timezone.utc), 'GO expired')
