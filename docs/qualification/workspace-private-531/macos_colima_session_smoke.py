"""Unsealed Colima full-session exploration using the real #621 cell driver.

Artifacts and native resources must already exist. This entry never activates a
sealed occurrence, builds, starts a VM, or supplies a qualification receipt.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import sqlite3
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor
import macos_stage_proof as proof
import runtime
from common import SCRATCH, digest, exclusive, require


def checked_request(path, expected, *, backend='colima'):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(SCRATCH),
            'exploration request must be canonical under Stash scratch')
    raw = executor.read_checked_file(str(path), must_be_in=str(SCRATCH), read_only=False, max_bytes=1024 * 1024)
    require(len(raw) <= 1024 * 1024 and hashlib.sha256(raw).hexdigest() == expected,
            'exploration input hash or size differs')
    request = json.loads(raw, object_pairs_hook=proof.unique_object)
    keys = {'control', 'identity', 'cell', 'exploration_authority'}
    require(backend in ('colima', 'desktop'), 'unsupported exploration backend')
    require(set(request) == keys | ({'preparation_receipt'} if backend == 'desktop' else set())
            | ({'external_checkout'} if backend == 'desktop' and 'external_checkout' in request else set()),
            'unsupported exploration request')
    control, cell = request['control'], request['cell']
    require(isinstance(control, str) and str(Path(control).resolve(strict=True)) == control
            and Path(control).is_relative_to(SCRATCH) and Path(control) != SCRATCH,
            'owned control must be canonical below Stash scratch')
    require(list(executor.Pins.observe(control)) == request['identity'], 'owned control identity changed')
    require(cell['backend'] == backend and cell['engine_kind'] == ('docker' if backend == 'desktop' else 'colima')
            and cell['driver'] == 'local', 'exploration supports only the selected local backend')
    require(type(cell['budget_seconds']) is int and 0 < cell['budget_seconds'] <= 1800,
            'full-session exploration must keep the 1800-second cell bound')
    if backend == 'colima':
        require(cell['colima_home'] == control + '/c' and cell['docker_config'] == control + '/docker-config',
                'Colima home and config must belong to the named control')
    else:
        require(cell['engine_identity'] == executor.prep.DESKTOP_HOST
                and cell['docker_config'] == control + '/session-docker-config',
                'Desktop endpoint and private config differ')
        ref = request['preparation_receipt']
        parent = Path(control).parent
        require(Path(ref['path']).is_relative_to(parent / 'evidence'),
                'Desktop predecessor must belong to the owned preparation evidence')
        executor.ACTIVATIONS_DIR, executor.CLAIMS_DIR = str(parent / 'activations'), str(parent / 'claims')
        body = executor.read_receipt(ref)
        bindings = dict(executor.prep.control_paths(body['token'], parent=str(Path(control).parent)),
                        token=body['token'], desktop_host=executor.prep.DESKTOP_HOST,
                        check_source=executor.prep.CHECK_SOURCE, route_echo=executor.prep.ROUTE_ECHO)
        require(body['control'] == control and ref['stage'] == 'D3', 'Desktop preparation belongs to another control')
        proof.checked_prior(ref, bindings, executor.load_manifest(), 'D3')
        images = [row['id'] for row in body['receipts'] if row['kind'] == 'image']
        require(images == [cell['runtime_image']], 'session image differs from receipted preparation')
    if 'external_checkout' in request:
        external = request['external_checkout']
        require(isinstance(external, dict) and set(external) == {'path', 'device', 'inode', 'uid'},
                'external checkout must pin its complete directory identity')
        path = Path(external['path'])
        require(path.parent == Path('/Users/tom/.effigy-qual-831')
                and path.resolve(strict=True) == path and not path.is_symlink(),
                'only the planner-approved owned Users fixture may be mounted')
        st = path.stat()
        require((st.st_dev, st.st_ino, st.st_uid) == (external['device'], external['inode'], external['uid'])
                and st.st_uid == os.getuid() and not list(path.iterdir()),
                'external checkout identity changed or is not fresh')
    root = Path(cell['qualification_root']['path'])
    if backend == 'colima':
        require(root.is_relative_to(Path(control) / 's'), 'session root must be below the owned qualification share')
    else:
        scratch = Path(os.environ['TMPDIR']).resolve(strict=True)
        require(scratch.is_relative_to(SCRATCH) and root.is_relative_to(scratch) and root != scratch,
                'Desktop evidence must be below this run’s owned scratch')
    require(isinstance(request['exploration_authority'], str) and request['exploration_authority'],
            'record the actual exploration authority')
    return request



def sqlite_readiness(scratch):
    directory = Path(tempfile.mkdtemp(prefix='session-sqlite-', dir=scratch))
    path = directory / 'readiness.db'
    try:
        writer = sqlite3.connect(path)
        try:
            writer.execute('CREATE TABLE readiness(value INTEGER)')
            writer.execute('INSERT INTO readiness VALUES (1)')
            writer.commit()
        finally:
            writer.close()
        reader = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True)
        try:
            require(reader.execute('SELECT value FROM readiness').fetchall() == [(1,)],
                    'read-only SQLite observation differs')
            try:
                reader.execute('INSERT INTO readiness VALUES (2)')
            except sqlite3.OperationalError:
                pass
            else:
                raise RuntimeError('read-only SQLite observation permits writes')
        finally:
            reader.close()
    except sqlite3.Error as exc:
        raise RuntimeError('selected Python cannot observe scratch SQLite read-only; select a capable Python on PATH') from exc
    interpreter = Path(sys.executable).resolve(strict=True)
    return dict(interpreter=str(interpreter), interpreter_sha256=digest(interpreter),
                sqlite_version=sqlite3.sqlite_version, fixture=str(directory), read_only=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--backend', choices=('colima', 'desktop'), default='colima')
    parser.add_argument('--request', required=True)
    parser.add_argument('--sha256', required=True)
    args = parser.parse_args()
    request = checked_request(args.request, args.sha256, backend=args.backend)
    cell, control = request['cell'], request['control']
    readiness = sqlite_readiness(os.environ['TMPDIR'])
    window = runtime.Window(cell, args.sha256, None,
                            exploration_authority=request['exploration_authority'],
                            exploration_checkout=request.get('external_checkout'))
    window.env.update(DOCKER_CONFIG=cell['docker_config'], TMP=str(window.temp), TEMP=str(window.temp))
    if args.backend == 'colima':
        window.env['COLIMA_CACHE_HOME'] = control + '/cache'
    desktop_inventory = None
    if args.backend == 'desktop':
        from macos_desktop_smoke import inventory
        desktop_inventory = inventory()
        exclusive('inventory-before.json', desktop_inventory, dir_fd=window.root_fd)
    try:
        exclusive('exploration.json', dict(proof=False, request_sha256=args.sha256,
                  exploration_authority=request['exploration_authority'], sqlite_readiness=readiness,
                  phase_bounds_seconds=dict(activation=300, command=60, backend=cell['budget_seconds'])), dir_fd=window.root_fd)
        window.execute()
    except BaseException as exc:
        exclusive('exploration-failed.json', dict(proof=False, exploratory=True, error=str(exc),
                  cells=window.events), dir_fd=window.root_fd)
        raise
    finally:
        try:
            if window.external_checkout is not None:
                window.verify_root()
                exclusive('external-checkout.json', dict(binding=window.external_checkout,
                          peak_observed_bytes=window.external_checkout_peak, cap_bytes=16 * 1024 * 1024,
                          ruling='planner 0796c1788c789665ffc81b2ecd6c4051d58d91889814e1cd273ff6c36572f190'),
                          dir_fd=window.root_fd)
            if desktop_inventory is not None:
                after = inventory()
                exclusive('inventory-after.json', after, dir_fd=window.root_fd)
                difference = {kind: dict(removed=sorted(set(desktop_inventory[kind]) - set(after[kind])),
                                         added=sorted(set(after[kind]) - set(desktop_inventory[kind])))
                              for kind in desktop_inventory}
                exclusive('inventory-difference.json', difference, dir_fd=window.root_fd)
                print('inventory differences: ' + json.dumps(difference), flush=True)
        finally:
            window.close()


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print('exploration failed: ' + str(exc), file=sys.stderr)
        sys.exit(1)
