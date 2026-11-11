"""Unsealed Desktop exploration. Fresh labelled resources; no sealed proof credit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor
import macos_stage_proof as proof
import macos_colima_chain as chain
import macos_desktop_readiness as readiness


def inventory():
    """All shared-engine identities, without container environments or log payloads."""
    env = {k: os.environ[k] for k in ('PATH', 'HOME', 'TMPDIR', 'TMP', 'TEMP') if k in os.environ}
    env.update(DOCKER_HOST=executor.prep.DESKTOP_HOST, DOCKER_CONFIG='/dev/null')
    result = {}
    for kind, fields in [('container', '{{.ID}} {{.Names}} {{.Image}} {{.Labels}}'),
                         ('image', '{{.ID}} {{.Repository}}:{{.Tag}}'),
                         ('volume', '{{.Name}} {{.Labels}}'), ('network', '{{.ID}} {{.Name}} {{.Labels}}')]:
        argv = [executor.prep.DOCKER, kind, 'ls', '--format', fields]
        if kind in ('container', 'image', 'network'):
            argv += ['--no-trunc']
        if kind in ('container', 'image'):
            argv += ['--all']
        child = subprocess.run(argv, env=env, capture_output=True, timeout=60, check=True)
        result[kind] = sorted(child.stdout.decode().splitlines())
    return result


def cleanup_preparation(path, expected):
    path = Path(path)
    scratch = Path(os.environ['TMPDIR']).resolve(strict=True)
    if path.resolve(strict=True) != path or not path.is_relative_to(scratch):
        raise executor.Refusal('cleanup preparation must be canonical inside this run’s scratch')
    raw = executor.read_checked_file(str(path), must_be_in=str(scratch), read_only=False)
    if hashlib.sha256(raw).hexdigest() != expected:
        raise executor.Refusal('cleanup preparation hash differs')
    prepared = json.loads(raw, object_pairs_hook=proof.unique_object)
    if set(prepared) != {'bindings', 'prior_receipt', 'proof'} or prepared['proof'] is not False:
        raise executor.Refusal('cleanup requires an unsealed preparation receipt')
    bindings, root = prepared['bindings'], str(path.parent)
    if os.path.dirname(bindings['control']) != root:
        raise executor.Refusal('cleanup control belongs to another preparation')
    executor.ACTIVATIONS_DIR, executor.CLAIMS_DIR = root + '/activations', root + '/claims'
    options = dict(activation_id='cleanup-' + bindings['token'] + '-' + os.urandom(4).hex(),
                   duration_seconds=executor.stage_of(executor.load_manifest(), 'D4')['seconds'],
                   evidence_root=root + '/evidence', prior_receipt=prepared['prior_receipt'])
    request = proof.window_recipe('D4', bindings, options)
    executor.write_json_new(root, options['activation_id'] + '.request.json', request)
    result = proof.execute_window(request, executor.canon_sha(request), executor.load_manifest(), request['inputs_sha256'])
    print('D4: ' + result['closure'], flush=True)
    return 0 if result['closure'] == 'closed' else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cleanup-preparation')
    parser.add_argument('--sha256')
    parser.add_argument('--retain-runtime', action='store_true', help='Keep this run’s D3 runtime for a separate owned session; do not run D4 yet.')
    args = parser.parse_args()
    if args.cleanup_preparation:
        if args.retain_runtime or not args.sha256:
            raise executor.Refusal('cleanup needs its hash and cannot retain a new runtime')
        return cleanup_preparation(args.cleanup_preparation, args.sha256)
    if args.sha256:
        raise executor.Refusal('hash requires a cleanup preparation')
    scratch = os.environ['TMPDIR']
    if (os.path.realpath(scratch) != scratch or not os.path.isdir(scratch)
            or not scratch.startswith(executor.prep.STASH + '/')):
        raise executor.Refusal('TMPDIR must be an existing canonical Stash scratch directory')
    root = tempfile.mkdtemp(prefix='desktop-smoke-', dir=scratch)
    # Before D0 or any native resource, use the same isolated policy as the
    # session. Desktop's plugin directory is referenced read-only, never copied.
    home = Path(root) / 'preflight-home'
    home.mkdir(mode=0o700)
    env = {k: os.environ[k] for k in ('PATH', 'USER', 'LOGNAME', 'LANG', 'TMPDIR', 'TMP', 'TEMP') if k in os.environ}
    env.update(HOME=str(home), DOCKER_CONFIG=root + '/preflight-docker-config',
               DOCKER_HOST=executor.prep.DESKTOP_HOST)
    readiness.preflight(env, Path(root) / 'desktop-preflight.json', item=dict(
        path=executor.prep.DOCKER, sha256=readiness.digest(Path(executor.prep.DOCKER).resolve(strict=True))))
    for name in ('activations', 'claims', 'evidence'):
        os.mkdir(root + '/' + name, 0o700)
    executor.ACTIVATIONS_DIR, executor.CLAIMS_DIR = root + '/activations', root + '/claims'
    token = os.urandom(4).hex()
    paths = executor.prep.control_paths(token, parent=root)
    # D0 expects its own empty control; never adopt an existing one.
    os.mkdir(paths['control'], 0o700)
    bindings = {key: value for key, value in paths.items() if key in executor.prep.SLOTS}
    bindings.update(token=token, desktop_host=executor.prep.DESKTOP_HOST,
                    check_source=executor.prep.CHECK_SOURCE, route_echo=executor.prep.ROUTE_ECHO)
    executor.write_json_new(root, 'exploration.json', dict(proof=False, bindings=bindings))
    before = inventory()
    executor.write_json_new(root, 'inventory-before.json', before)
    print(root, flush=True)
    prior = None
    try:
        for stage in ('D0', 'D4'):
            if stage == 'D4':
                request = chain.recipe(bindings, dict(activation_id='smoke-' + token,
                                       evidence_root=root + '/evidence', prior_receipt=prior), desktop=True)
                executor.write_json_new(root, 'chain.request.json', request)
                result = chain.execute(request, executor.canon_sha(request))
                print('D1-D3: ' + result['closure'], flush=True)
                if result['closure'] != 'closed':
                    return 1
                prior = result['receipts'][-1]
                executor.write_json_new(root, 'preparation.json', dict(bindings=bindings, prior_receipt=prior, proof=False))
                if args.retain_runtime:
                    return 0
            options = dict(activation_id='smoke-' + token + '-' + stage.lower(),
                           duration_seconds=executor.stage_of(executor.load_manifest(), stage)['seconds'],
                           evidence_root=root + '/evidence')
            if prior is not None:
                options['prior_receipt'] = prior
            request = proof.window_recipe(stage, bindings, options)
            executor.write_json_new(root, stage + '.request.json', request)
            receipt = proof.execute_window(request, executor.canon_sha(request), executor.load_manifest(),
                                           request['inputs_sha256'])
            print(stage + ': ' + receipt['closure'], flush=True)
            if receipt['closure'] != 'closed':
                return 1
            path = root + '/evidence/' + stage + '-' + options['activation_id'] + '/stage-receipt.json'
            prior = dict(stage=stage, path=path, sha256=executor.sha_file(path))
        return 0
    finally:
        after = inventory()
        executor.write_json_new(root, 'inventory-after.json', after)
        difference = {kind: dict(removed=sorted(set(before[kind]) - set(after[kind])),
                                 added=sorted(set(after[kind]) - set(before[kind]))) for kind in before}
        executor.write_json_new(root, 'inventory-difference.json', difference)
        print('inventory differences: ' + json.dumps(difference), flush=True)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as exc:
        print('exploration failed: ' + str(exc), file=sys.stderr)
        sys.exit(1)
