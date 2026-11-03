#!/usr/bin/env python3
"""ONE Queue proof invocation; all backends, no partial-success result."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BACKENDS, PACKET, SCRATCH, artifact, digest, exclusive, exclusive_bytes, load, require, run,
                    open_qualification_root, qualification_path, source_head, ssh_prefix, validate_go, validate_inputs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--inputs', required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--go-bindings', required=True)
    parser.add_argument('--go-sha256', required=True)
    parser.add_argument('--backend', choices=list(BACKENDS),
                        help='independently execute exactly one backend under its own scoped GO')
    args = parser.parse_args()
    os.umask(0o077)
    root = PACKET.parents[2]
    head = source_head(root)
    inputs = Path(args.inputs)
    require(inputs.is_absolute() and inputs.is_relative_to(SCRATCH) and not inputs.is_symlink(),
            'input must be a scratch artifact')
    require(digest(inputs) == args.sha256, 'Queue-bound input hash mismatch')
    value = validate_inputs(load(inputs), head)
    # The selected entry validates exactly its own scoped authority; the single
    # all-backend invocation still validates every GO before any native window.
    go_bindings = Path(args.go_bindings)
    require(digest(go_bindings) == args.go_sha256, 'Queue-bound GO receipt index differs')
    go_index = load(go_bindings)
    selected = value['backends'] if args.backend is None else [
        cell for cell in value['backends'] if cell['backend'] == args.backend]
    goes = {}
    for cell in selected:
        go = artifact(go_index[cell['backend']])
        goes[cell['backend']] = load(go)
        validate_go(goes[cell['backend']], args.sha256, cell, head)
    # Every local config is pinned/private and checked before creating evidence or
    # starting any backend. -F avoids the user's global SSH configuration.
    for cell in selected:
        if cell['driver'] == 'ssh':
            ssh_prefix(cell)
    evidence = Path(value['evidence'])
    require(evidence.parent.resolve().is_relative_to(SCRATCH.resolve()) and not evidence.exists(),
            'evidence must be a fresh scratch directory')
    evidence.mkdir(mode=0o700)
    exclusive(evidence / 'started.json', {'source_head': head, 'inputs_sha256': args.sha256,
                                         'backends': [cell['backend'] for cell in selected]})
    results = []
    for cell in selected:
        command = ['python3', '-I', '-B', str(PACKET / 'runtime.py'), '--inputs', str(inputs),
                   '--sha256', args.sha256, '--go-bindings', str(go_bindings), '--go-sha256', args.go_sha256, '--backend', cell['backend']]
        if cell['driver'] == 'ssh':
            # Remote source/artifact preparation is separately GO-owned. No copy,
            # build, VM start or install is hidden inside proof execution.
            def ssh():
                return ssh_prefix(cell)  # Recheck pinned bytes/policy before every SSH command.
            remote = cell['remote']
            for name in ('common.py', 'runtime.py', 'probe.py'):
                found = run(ssh() + [
                             'sha256sum -- ' + shlex.quote(str(Path(remote['packet']) / name))]).stdout.decode().split()[0]
                require(found == digest(PACKET / name), 'remote reviewed driver differs')
            found = run(ssh() + [
                         'sha256sum -- ' + shlex.quote(remote['inputs'])]).stdout.decode().split()[0]
            require(found == args.sha256, 'remote exact input file differs')
            found = run(ssh() + [
                         'sha256sum -- ' + shlex.quote(remote['go_bindings'])]).stdout.decode().split()[0]
            require(found == args.go_sha256, 'remote GO receipt index differs')
            command = ssh() + [
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
            # The exact original receipt bytes travel only inside this already
            # authorized proof turn: bounded transport, then hash-bound against
            # the remote original. No transport or re-read ever happens later.
            transported = run(ssh() + ['cat', '--', str(expected)], timeout=60).stdout
            receipt_hash = run(ssh() + [
                                'sha256sum -- ' + shlex.quote(receipt_path)]).stdout.decode().split()[0]
        else:
            _, fd = open_qualification_root(cell['qualification_root'])
            try:
                with os.fdopen(os.open(expected.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd), 'rb') as original:
                    transported = original.read()
                receipt_hash = digest(expected.name, dir_fd=fd)
            finally:
                os.close(fd)
        require(hashlib.sha256(transported).hexdigest() == receipt_hash,
                'transported original receipt differs from its recorded hash')
        export = evidence / (cell['backend'] + '-receipt.json')
        exclusive_bytes(export, transported)
        require(digest(export) == receipt_hash, 'retained receipt copy differs from the original hash')
        # The binding record fixes the exact selected authority and immutable
        # context of this invocation: source, executed programs, contract,
        # candidate, inputs, pinned root, old baseline, window and one-use marker.
        # Digests are read after execution; the bytes on disk are what ran.
        programs = {name: digest(PACKET / name) for name in ('launch.py', 'runtime.py', 'common.py', 'probe.py')}
        binding = {'schema': 'effigy.workspace-private-backend-invocation.v1', 'backend': cell['backend'],
                   'source_head': head, 'programs': programs, 'contract': value['schema'],
                   'candidate': cell['candidate'], 'candidate_receipt': cell['candidate_receipt'],
                   'production': cell['production'], 'production_receipt': cell['production_receipt'],
                   'old_source_head': cell['old_source_head'], 'old_binary': cell['old_binary'],
                   'old_source_receipt': cell['old_source_receipt'],
                   'inputs': {'path': str(inputs), 'sha256': args.sha256},
                   'go_bindings': {'path': str(go_bindings), 'sha256': args.go_sha256},
                   'go_receipt': go_index[cell['backend']],
                   'go_decision_id': goes[cell['backend']]['decision_id'],
                   'go_expires_at': goes[cell['backend']]['expires_at'],
                   'qualification_root': cell['qualification_root'], 'driver': cell['driver'],
                   'engine_kind': cell['engine_kind'], 'engine_identity': cell['engine_identity'],
                   'runtime_image': cell['runtime_image'], 'budget_seconds': cell['budget_seconds'],
                   'one_shot_marker': str(SCRATCH / ('531-' + args.sha256 + '-' + cell['backend'] + '.started.json'))}
        if cell['driver'] == 'ssh':
            binding.update(ssh_alias=cell['ssh_alias'], ssh_config=cell['ssh_config'],
                           ssh_known_hosts=cell['ssh_known_hosts'])
        # The post-run source check gates positivity: a mid-window source edit
        # yields honest, nonqualifying evidence, never a positive receipt.
        unchanged = source_head(root) == head
        receipt = {'backend': cell['backend'], 'exit_code': result.returncode, 'receipt': receipt_path,
                   'receipt_sha256': receipt_hash,
                   'receipt_export': {'path': str(export), 'sha256': receipt_hash},
                   'source_unchanged': unchanged,
                   'positive': result.returncode == 0 and unchanged,
                   'host': cell.get('ssh_alias', 'local'),
                   'stdout_sha256': __import__('hashlib').sha256(result.stdout).hexdigest(),
                   'stderr_sha256': __import__('hashlib').sha256(result.stderr).hexdigest(),
                   'binding': binding}
        exclusive(evidence / (cell['backend'] + '.json'), receipt)
        results.append(receipt)
        require(result.returncode == 0, 'native backend failed; no retry or partial qualification credit')
        require(unchanged, 'proof source changed during execution')
    if args.backend is None:
        # Only the single all-backend invocation writes this aggregate record;
        # an independent backend entry never implies packet qualification. The
        # authoritative aggregate remains the read-only verifier's judgment.
        exclusive(evidence / 'complete.json', {'source_head': head, 'inputs_sha256': args.sha256,
                                             'backends': results, 'consumer': 'separate-owner-evidence-required',
                                             'production_enablement': False})
        print(json.dumps({'evidence': str(evidence), 'native_backends': results, 'production_enablement': False}))
    else:
        entry = evidence / (args.backend + '.json')
        print(json.dumps({'backend': args.backend, 'invocation': str(entry),
                          'aggregate': 'verify:workspace:private-qualification:aggregate',
                          'qualified': False, 'production_enablement': False}))


if __name__ == '__main__':
    main()
