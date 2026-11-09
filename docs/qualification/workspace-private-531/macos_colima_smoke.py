"""Unsealed native Colima exploration on a fresh, privately owned chain.

No Nucleus proof occurrence is consumed and these observations give no proof
credit. Keep all evidence; stop only the profile this invocation created.
"""
import json
import os
from pathlib import Path
import sys
import socket
import shutil
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor  # noqa: E402
import macos_colima_chain as chain
import macos_stage_proof as proof  # noqa: E402


def main():
    scratch = os.environ['TMPDIR']
    if (os.path.realpath(scratch) != scratch or not os.path.isdir(scratch)
            or not scratch.startswith(executor.prep.STASH + '/')):
        raise executor.Refusal('TMPDIR must be an existing canonical Stash scratch directory')
    root = tempfile.mkdtemp(prefix='colima-smoke-', dir=scratch)
    for name in ('activations', 'claims', 'evidence'):
        os.mkdir(root + '/' + name, 0o700)
    # Invocation-private records: never use the sealed chain's activation or claims directories.
    executor.ACTIVATIONS_DIR = root + '/activations'
    executor.CLAIMS_DIR = root + '/claims'
    token = os.urandom(4).hex()
    paths = executor.prep.control_paths(token)
    executor.prep.check_paths(paths, set())
    if os.path.lexists(paths['control']):
        raise executor.Refusal('fresh control root collided; nothing adopted')
    bindings = {key: value for key, value in paths.items() if key in executor.prep.SLOTS}
    bindings.update(token=token, desktop_host=executor.prep.DESKTOP_HOST,
                    check_source=executor.prep.CHECK_SOURCE, route_echo=executor.prep.ROUTE_ECHO)
    executor.write_json_new(root, 'exploration.json', dict(proof=False, bindings=bindings))
    print(root, flush=True)
    prior = None
    started = False
    try:
        for stage in ('C0', 'C1'):
            options = dict(activation_id='smoke-' + token + '-' + stage.lower(),
                           duration_seconds=executor.stage_of(executor.load_manifest(), stage)['seconds'],
                           evidence_root=root + '/evidence')
            if prior is not None:
                options['prior_receipt'] = prior
            request = proof.window_recipe(stage, bindings, options)
            executor.write_json_new(root, stage + '.request.json', request)
            if stage == 'C1':
                capacity = max(row['bytes'] for row in executor.prep.check_socket_paths(bindings['colima_home']))
                parent = bindings['control'] + '/t'
                probe = parent + '/' + ('p' * (capacity - len(parent.encode()) - 1))
                bound = False
                try:
                    with socket.socket(socket.AF_UNIX) as endpoint:
                        endpoint.bind(probe)
                        bound = True
                    executor.write_json_new(root, 'socket-capacity.json', dict(bytes=capacity, bound=True))
                finally:
                    if bound:
                        os.unlink(probe)
                started = True  # even an unknown start belongs to this invocation
            receipt = proof.execute_window(request, executor.canon_sha(request), executor.load_manifest(),
                                           request['inputs_sha256'])
            print(stage + ': ' + receipt['closure'], flush=True)
            if receipt['closure'] != 'closed':
                return 1
            path = root + '/evidence/' + stage + '-' + options['activation_id'] + '/stage-receipt.json'
            prior = dict(stage=stage, path=path, sha256=executor.sha_file(path))
        request = chain.recipe(bindings, dict(activation_id='smoke-' + token,
                               evidence_root=root + '/evidence', prior_receipt=prior))
        executor.write_json_new(root, 'chain.request.json', request)
        result = chain.execute(request, executor.canon_sha(request))
        print('C2-C6: ' + result['closure'], flush=True)
        return 0 if result['closure'] == 'closed' else 1
    finally:
        if started:
            # The root was absent at entry and created by our C0. No default-home or other profile action.
            step = executor.stage_of(executor.load_manifest(), 'C1')['steps'][-1]
            cleanup = root + '/stop'
            os.mkdir(cleanup, 0o700)
            result = executor.primitive.run_stage(
                cleanup, [executor.prep.COLIMA, 'stop', '-p', executor.prep.PROFILE], 120, 262144,
                {executor.prep.COLIMA: executor.sha_file(executor.prep.COLIMA)},
                environment=executor.step_environment(step, bindings))
            executor.write_json_new(root, 'stop.json', result)
            retained = paths['control'] + '/exploration-evidence'
            shutil.copytree(root, retained)
            print('retained: ' + retained, flush=True)
            if result.get('waited_exit') != 0:
                raise executor.Refusal('owned profile stop did not close; retained at ' + paths['control'])


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as exc:
        print('exploration failed: ' + str(exc), file=sys.stderr)
        sys.exit(1)
