#!/usr/bin/env python3
"""ONE Queue proof invocation; all backends, no partial-success result."""
import argparse
import json
import os
from pathlib import Path
import shlex
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (PACKET, SCRATCH, artifact, digest, exclusive, load, require, run,
                    open_qualification_root, qualification_path, source_head, validate_go, validate_inputs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--inputs', required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--go-bindings', required=True)
    parser.add_argument('--go-sha256', required=True)
    args = parser.parse_args()
    os.umask(0o077)
    root = PACKET.parents[2]
    head = source_head(root)
    inputs = Path(args.inputs)
    require(inputs.is_absolute() and inputs.is_relative_to(SCRATCH) and not inputs.is_symlink(),
            'input must be a scratch artifact')
    require(digest(inputs) == args.sha256, 'Queue-bound input hash mismatch')
    value = validate_inputs(load(inputs), head)
    # Validate ALL GO receipts before consuming any native window.
    go_bindings = Path(args.go_bindings)
    require(digest(go_bindings) == args.go_sha256, 'Queue-bound GO receipt index differs')
    go_index = load(go_bindings)
    for cell in value['backends']:
        go = artifact(go_index[cell['backend']])
        validate_go(load(go), args.sha256, cell, head)
    evidence = Path(value['evidence'])
    require(evidence.parent.resolve().is_relative_to(SCRATCH.resolve()) and not evidence.exists(),
            'evidence must be a fresh scratch directory')
    evidence.mkdir(mode=0o700)
    exclusive(evidence / 'started.json', {'source_head': head, 'inputs_sha256': args.sha256})
    results = []
    for cell in value['backends']:
        command = ['python3', '-I', '-B', str(PACKET / 'runtime.py'), '--inputs', str(inputs),
                   '--sha256', args.sha256, '--go-bindings', str(go_bindings), '--go-sha256', args.go_sha256, '--backend', cell['backend']]
        if cell['driver'] == 'ssh':
            # Remote source/artifact preparation is separately GO-owned. No copy,
            # build, VM start or install is hidden inside proof execution.
            require(cell['ssh_alias'] and not cell['ssh_alias'].startswith('-'), 'invalid Linux host alias')
            remote = cell['remote']
            for name in ('common.py', 'runtime.py', 'probe.py'):
                found = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', cell['ssh_alias'],
                             'sha256sum -- ' + shlex.quote(str(Path(remote['packet']) / name))]).stdout.decode().split()[0]
                require(found == digest(PACKET / name), 'remote reviewed driver differs')
            found = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', cell['ssh_alias'],
                         'sha256sum -- ' + shlex.quote(remote['inputs'])]).stdout.decode().split()[0]
            require(found == args.sha256, 'remote exact input file differs')
            found = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', cell['ssh_alias'],
                         'sha256sum -- ' + shlex.quote(remote['go_bindings'])]).stdout.decode().split()[0]
            require(found == args.go_sha256, 'remote GO receipt index differs')
            command = ['/usr/bin/ssh', '-o', 'BatchMode=yes', cell['ssh_alias'],
                       shlex.join(['python3', '-I', '-B', str(Path(remote['packet']) / 'runtime.py'),
                                   '--inputs', remote['inputs'], '--sha256', args.sha256,
                                   '--go-bindings', remote['go_bindings'], '--go-sha256', args.go_sha256, '--backend', 'linux'])]
        result = run(command, cwd=root, timeout=cell['budget_seconds'], check=False)
        # Runtime emits only a bounded receipt path, never raw native output.
        returned = json.loads(result.stdout) if result.stdout else {}
        receipt_path = returned.get('receipt') if result.returncode == 0 else returned.get('failed_receipt')
        expected = qualification_path(cell['qualification_root']) / ('complete.json' if result.returncode == 0 else 'failed.json')
        require(receipt_path and Path(receipt_path) == expected,
                'runtime returned no bounded receipt; native result remains unverified')
        if cell['driver'] == 'ssh':
            receipt_hash = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', cell['ssh_alias'],
                                'sha256sum -- ' + shlex.quote(receipt_path)]).stdout.decode().split()[0]
        else:
            _, fd = open_qualification_root(cell['qualification_root'])
            try:
                receipt_hash = digest(expected.name, dir_fd=fd)
            finally:
                os.close(fd)
        receipt = {'backend': cell['backend'], 'exit_code': result.returncode, 'receipt': receipt_path,
                   'receipt_sha256': receipt_hash,
                   'host': cell.get('ssh_alias', 'local'),
                   'stdout_sha256': __import__('hashlib').sha256(result.stdout).hexdigest(),
                   'stderr_sha256': __import__('hashlib').sha256(result.stderr).hexdigest()}
        exclusive(evidence / (cell['backend'] + '.json'), receipt)
        results.append(receipt)
        require(result.returncode == 0, 'native backend failed; no retry or partial qualification credit')
        require(source_head(root) == head, 'proof source changed during execution')
    exclusive(evidence / 'complete.json', {'source_head': head, 'inputs_sha256': args.sha256,
                                         'backends': results, 'consumer': 'separate-owner-evidence-required',
                                         'production_enablement': False})
    print(json.dumps({'evidence': str(evidence), 'native_backends': results, 'production_enablement': False}))


if __name__ == '__main__':
    main()
