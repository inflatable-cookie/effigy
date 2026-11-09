"""Acow consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. `main` refuses every invocation before any read or
child. The armed entry is `proof_entry.py acow`, which Nucleus launches with the
request's SHA-256 under the same-sequence precondition stated in consumer_core.
While the committed manifest holds pending pins, `run` refuses at
`route-pins-pending` before the request is read.

Armed sequence, each child waited and run from the pinned Acow checkout:
  0. shape, frozen pins, provenance, bindings, window, budgets, resources,
     pinned checkout, selected clean head and closure blobs (all before effect)
  1. engine identity: the postgres container and its volume (read-only probes)
  2. protected capture; shared MariaDB and named volumes must be present
  3. prior running state of hybrid and stack, which must match the request
  4. hybrid bring-up, stack bring-up, hybrid bring-up, each observed
  5. Level 1 forward: `container stack status --json`
  6. eight named-table counts, counts only, against baseline plus declared delta
  7. rollback: stop only environments this window started and that are not
     observed not running; anything running before the window stays running
  8. protected identities still present, no volume lost

`dev:test` is declared, not armed: its fresh environment is not pinned in
`infra/workspace.toml`. `dev` and `dev:container` are TUI and need a PTY, which
is Level 2, so this route uses the detached container forms instead.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core  # noqa: E402

ROUTE = 'acow'
ENVIRONMENTS = frozenset({'hybrid', 'stack'})
PHASES = (
    ('hybrid-1', 'hybrid'),
    ('container', 'stack'),
    ('hybrid-2', 'hybrid'),
)
TAILS = frozenset({
    ('container', 'hybrid', 'up', '--detach'),
    ('container', 'stack', 'up', '--detach'),
    ('container', 'hybrid', 'status', '--json'),
    ('container', 'stack', 'status', '--json'),
    ('container', 'hybrid', 'down'),
    ('container', 'stack', 'down'),
})
FORWARD_TAIL = ('container', 'stack', 'status', '--json')
PSQL_TAIL = ('-U', 'postgres', '-d', 'acowtancy', '--no-psqlrc',
             '-v', 'ON_ERROR_STOP=1', '-Atc')
TABLE_RE = re.compile(r'[a-z_]+\.[a-z_]+')
EVIDENCE_ROOTS = ('/Volumes/Builds/scratch/effigy',)


def count_argv(engine, pg_id, table):
    """One counts-only probe: fixed exec shape through the pinned profile."""
    if not TABLE_RE.fullmatch(table):
        raise core.Refused('counts-plan', 'table name is not allowlisted')
    return core.engine_argv(engine, 'exec', pg_id, 'psql', *PSQL_TAIL,
                            'SELECT count(*) FROM %s' % table)


def expected_counts(manifest):
    """Owner baseline plus the declared quiet-window delta, per table."""
    return {table: pin['baseline'] + pin['delta']
            for table, pin in manifest['routes'][ROUTE]['counts'].items()}


def observe_counts(session, engine, manifest, pg_id):
    counts = {}
    for table in sorted(expected_counts(manifest)):
        record = session.engine_call(count_argv(engine, pg_id, table),
                                     'count-' + table.replace('.', '-'))
        text = core.stdout_text(record).strip()
        if not text.isdigit():
            raise core.Refused('unexpected-counts', 'non-numeric output: ' + table)
        counts[table] = int(text)
    return counts


def execute(session, engine, manifest, request):
    """The armed sequence. Identity and prior intent are bound before any effect."""
    identity = manifest['routes'][ROUTE]['identity']
    intent = request['prior_intent']
    if set(intent) != ENVIRONMENTS:
        raise core.Refused('prior-intent', 'intent must name exactly hybrid and stack')
    pg_id = core.identity_check(session, engine, identity, request,
                                'postgres-container', 'postgres-volume')
    baseline = core.protected_capture(session, engine)
    core.protected_check(baseline, baseline, identity['shared_container'],
                         identity['shared_volume'], identity['never_touch_volumes'])
    observed = {env: core.read_running(session, env, 'pre-' + env) for env in sorted(intent)}
    core.check_prior_intent(request, observed)
    started = {env for env, running in intent.items() if running is False}
    counts = {}
    try:
        for phase, env in PHASES:
            # Unknown until read: a failed up may still have started the environment.
            observed[env] = None
            session.effigy(('container', env, 'up', '--detach'), phase + '-up')
            observed[env] = core.read_running(session, env, phase + '-status')
        session.effigy(FORWARD_TAIL, 'level1-forward')
        counts = observe_counts(session, engine, manifest, pg_id)
        core.counts_match(counts, expected_counts(manifest))
    finally:
        for name in core.rollback_plan(intent, observed, started):
            session.effigy(('container', name, 'down'), 'rollback-' + name)
    after = core.protected_capture(session, engine)
    core.protected_check(baseline, after, identity['shared_container'],
                         identity['shared_volume'], identity['never_touch_volumes'])
    return {'status': 'passed', 'counts': counts, 'started': sorted(started)}


def run(request_path, request_sha256, evidence_dir, **kwargs):
    """The armed route. Reached only through `proof_entry.py`; see consumer_core.arm."""
    return core.arm(ROUTE, TAILS, execute, request_path, request_sha256, evidence_dir, **kwargs)


def main(argv):
    """Unarmed ordinary CLI: refuses every invocation before any read or child.

    The armed entry is `proof_entry.py acow`, which Nucleus launches with the
    request's SHA-256. This module has no launch path of its own.
    """
    print(json.dumps({'status': 'refused', 'code': 'unarmed',
                      'detail': 'use proof_entry.py acow; ordinary invocation is unarmed'}))
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
