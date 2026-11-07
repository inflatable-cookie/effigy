"""Explicit local control entry points; transport and GO admission are separate."""
import hashlib
import json
import os
import stat
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_archive as archive
import preparation_nextest as nextest
import preparation_placement as placement
import preparation_metadata as metadata
from preparation_stage import exclusive


def read_json(path):
    with open(path, 'rb') as source:
        return json.load(source)


def main():
    if len(sys.argv) != 3:
        raise ValueError('usage: preparation_controls.py REQUEST EXPECTED_SHA256')
    raw = Path(sys.argv[1]).read_bytes()
    if len(raw) > 1024 * 1024 or hashlib.sha256(raw).hexdigest() != sys.argv[2]:
        raise ValueError('request hash mismatch')
    request = json.loads(raw)
    op = request['operation']
    if op == 'place-source' and set(request) == {'operation', 'archive', 'archive_sha256', 'archive_bytes', 'manifest', 'manifest_sha256', 'root', 'uid'}:
        manifest_data = Path(request['manifest']).read_bytes()
        if hashlib.sha256(manifest_data).hexdigest() != request['manifest_sha256']:
            raise ValueError('manifest hash mismatch')
        archive.place_archive(request['archive'], request['archive_sha256'], request['archive_bytes'],
                              json.loads(manifest_data), Path(request['root']), request['uid'])
        result = {'status': 'verified'}
    elif op == 'empty-root' and set(request) == {'operation', 'root', 'uid'}:
        result = placement.empty_root(request['root'], request['uid'])
    elif op == 'filesystem-controls' and set(request) == {'operation', 'root', 'uid'}:
        result = placement.filesystem_controls(request['root'], request['uid'])
    elif op == 'place-file' and set(request) == {'operation', 'directory', 'name', 'source', 'sha256', 'uid'}:
        data = Path(request['source']).read_bytes()
        if len(data) > 1024 * 1024:
            raise ValueError('placement exceeds one MiB')
        result = placement.place_file(request['directory'], request['name'], data, request['sha256'], request['uid'])
    elif op == 'nextest-offline' and set(request) == {'operation', 'archive', 'directory', 'uid'}:
        result = nextest.install_prepositioned(request['archive'], request['directory'], request['uid'])
    elif op == 'guest-reconcile' and set(request) == {'operation', 'process', 'process_sha256', 'receipt', 'receipt_sha256'}:
        process_bytes = Path(request['process']).read_bytes()
        receipt_bytes = Path(request['receipt']).read_bytes()
        if hashlib.sha256(process_bytes).hexdigest() != request['process_sha256'] or hashlib.sha256(receipt_bytes).hexdigest() != request['receipt_sha256']:
            raise ValueError('guest receipt hash mismatch')
        process, terminal = json.loads(process_bytes), json.loads(receipt_bytes)
        if terminal.get('process') != process:
            raise ValueError('guest process identity drift')
        result = {'guest_closure': 'unknown', 'disposition': terminal.get('disposition'),
                  'waited_exit': terminal.get('waited_exit')}
    elif op == 'nextest-metadata' and set(request) == {'operation', 'input', 'input_sha256', 'version', 'replacements', 'frozen_dir', 'frozen_hashes', 'uid', 'output'}:
        original = Path(request['input']).read_bytes()
        if hashlib.sha256(original).hexdigest() != request['input_sha256']:
            raise ValueError('metadata hash mismatch')
        frozen = Path(request['frozen_dir'])
        info = frozen.lstat()
        if not stat.S_ISDIR(info.st_mode) or frozen.resolve() != frozen or info.st_uid != request['uid'] or stat.S_IMODE(info.st_mode) != 0o700:
            raise ValueError('unsafe frozen directory')
        destinations = {pair[1] for pair in request['replacements'].values()}
        if destinations != set(request['frozen_hashes']):
            raise ValueError('frozen hash mapping mismatch')
        for path, expected in request['frozen_hashes'].items():
            target = Path(path)
            item = target.lstat()
            if target.parent != frozen or not stat.S_ISREG(item.st_mode) or item.st_uid != request['uid'] or item.st_nlink != 1 or stat.S_IMODE(item.st_mode) != 0o500:
                raise ValueError('unsafe frozen executable')
            fd = os.open(target, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
            with os.fdopen(fd, 'rb') as source:
                digest = hashlib.sha256()
                for block in iter(lambda: source.read(1024 * 1024), b''):
                    digest.update(block)
                if digest.hexdigest() != expected:
                    raise ValueError('frozen executable hash mismatch')
        derived = metadata.derive(json.loads(original), request['version'], request['replacements'])
        result_bytes = (json.dumps(derived, sort_keys=True, indent=2) + '\n').encode()
        exclusive(request['output'], result_bytes)
        result = {'sha256': hashlib.sha256(result_bytes).hexdigest(), 'bytes': len(result_bytes)}
    else:
        raise ValueError('unknown or invalid operation')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
