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


def checked_request(path, expected):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(SCRATCH),
            'exploration request must be canonical under Stash scratch')
    raw = executor.read_checked_file(str(path), must_be_in=str(SCRATCH), read_only=False, max_bytes=1024 * 1024)
    require(len(raw) <= 1024 * 1024 and hashlib.sha256(raw).hexdigest() == expected,
            'exploration input hash or size differs')
    request = json.loads(raw, object_pairs_hook=proof.unique_object)
    require(set(request) == {'control', 'identity', 'cell', 'exploration_authority'}, 'unsupported exploration request')
    control, cell = request['control'], request['cell']
    require(isinstance(control, str) and str(Path(control).resolve(strict=True)) == control
            and Path(control).is_relative_to(SCRATCH) and Path(control) != SCRATCH,
            'owned control must be canonical below Stash scratch')
    require(list(executor.Pins.observe(control)) == request['identity'], 'owned control identity changed')
    require(cell['backend'] == 'colima' and cell['engine_kind'] == 'colima' and cell['driver'] == 'local',
            'exploration supports only local Colima')
    require(type(cell['budget_seconds']) is int and 0 < cell['budget_seconds'] <= 1800,
            'full-session exploration must keep the 1800-second cell bound')
    require(cell['colima_home'] == control + '/c' and cell['docker_config'] == control + '/docker-config',
            'Colima home and config must belong to the named control')
    root = Path(cell['qualification_root']['path'])
    require(root.is_relative_to(Path(control) / 's'), 'session root must be below the owned qualification share')
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
    parser.add_argument('--request', required=True)
    parser.add_argument('--sha256', required=True)
    args = parser.parse_args()
    request = checked_request(args.request, args.sha256)
    cell, control = request['cell'], request['control']
    readiness = sqlite_readiness(os.environ['TMPDIR'])
    window = runtime.Window(cell, args.sha256, None,
                            exploration_authority=request['exploration_authority'])
    window.env.update(COLIMA_CACHE_HOME=control + '/cache', DOCKER_CONFIG=cell['docker_config'],
                      TMP=str(window.temp), TEMP=str(window.temp))
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
        window.close()


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print('exploration failed: ' + str(exc), file=sys.stderr)
        sys.exit(1)
