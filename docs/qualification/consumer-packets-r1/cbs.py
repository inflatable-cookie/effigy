"""CBS consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. The CBS checkout is `/Users/tom/Dev/legacy/sites/cbs`
with `infra/dev/effigy.container.toml` included by its root manifest. `main`
refuses every invocation before any read or child. The armed entry is
`proof_entry.py cbs`, which Nucleus launches with the request's SHA-256 under the
same-sequence precondition stated in consumer_core. While the committed manifest
holds pending pins, `run` refuses at `route-pins-pending` before the request is read.

Armed sequence, each child waited and run from the pinned CBS checkout:
  0. shape, frozen pins, provenance, bindings, window, budgets, resources,
     pinned checkout, selected clean head and closure blobs (all before effect)
  1. engine identity: the database container and the engine volume
  2. protected capture; named volumes must all be present
  3. prior running state of the `dev` system, which must match the request
  4. detached app-shell bring-up: `container up --detach --system dev`
  5. Level 1 forward: `container status --system dev --json` (non-destructive)
  6. named cbs and cbs-mortcalc counts, counts only, declared quiet window
  7. rollback: `container down --system dev` only if this window started it
  8. named volumes still present

No seed, deploy, sync, schema refresh or data command is in the route. The
tracked `[tasks.seed]` and `[tasks.schema:refresh]` stay out of authority.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core  # noqa: E402

ROUTE = 'cbs'
SYSTEM = 'dev'
TAILS = frozenset({
    ('container', 'up', '--detach', '--system', SYSTEM),
    ('container', 'status', '--system', SYSTEM, '--json'),
    ('container', 'down', '--system', SYSTEM),
})
FORWARD_TAIL = ('container', 'status', '--system', SYSTEM, '--json')
DATABASES = ('cbs', 'cbs-mortcalc')
TABLE_RE = re.compile(r'[a-z_]+')
EVIDENCE_ROOTS = ('/Volumes/Builds/scratch/effigy',)


def count_argv(engine, db_id, database, table):
    """One counts-only probe: pinned count tail, named database and table only."""
    if database not in DATABASES or not TABLE_RE.fullmatch(table):
        raise core.Refused('counts-plan', 'database or table is not allowlisted')
    return core.engine_argv(engine, 'exec', db_id, *engine['count_tail'],
                            'SELECT COUNT(*) FROM `%s`.`%s`' % (database, table))


def expected_counts(manifest):
    return {'%s.%s' % (db, table): pin['baseline'] + pin['delta']
            for db, tables in sorted(manifest['routes'][ROUTE]['counts'].items())
            for table, pin in sorted(tables.items())}


def observe_counts(session, engine, manifest, db_id):
    counts = {}
    for key in sorted(expected_counts(manifest)):
        database, table = key.split('.', 1)
        record = session.engine_call(count_argv(engine, db_id, database, table),
                                     'count-%s-%s' % (database, table))
        text = core.stdout_text(record).strip()
        if not text.isdigit():
            raise core.Refused('unexpected-counts', 'non-numeric output: ' + key)
        counts[key] = int(text)
    return counts


def read_dev_status(session, label):
    record = session.effigy(FORWARD_TAIL, label)
    return core.parse_running(core.stdout_text(record))


def execute(session, engine, manifest, request):
    identity = manifest['routes'][ROUTE]['identity']
    intent = request['prior_intent']
    if set(intent) != {SYSTEM}:
        raise core.Refused('prior-intent', 'intent must name exactly the dev system')
    db_id = core.identity_check(session, engine, identity, request,
                                'database-container', 'engine-volume')
    baseline = core.protected_capture(session, engine)
    observed = {SYSTEM: read_dev_status(session, 'pre-dev-status')}
    core.check_prior_intent(request, observed)
    started = {SYSTEM} if intent[SYSTEM] is False else set()
    counts = {}
    try:
        # Unknown until read: a failed up may still have started the system.
        observed[SYSTEM] = None
        session.effigy(('container', 'up', '--detach', '--system', SYSTEM), 'app-shell-up')
        observed[SYSTEM] = read_dev_status(session, 'app-shell-status')
        session.effigy(FORWARD_TAIL, 'level1-forward')
        counts = observe_counts(session, engine, manifest, db_id)
        core.counts_match(counts, expected_counts(manifest))
    finally:
        for name in core.rollback_plan(intent, observed, started):
            session.effigy(('container', 'down', '--system', SYSTEM), 'rollback-' + name)
    after = core.protected_capture(session, engine)
    core.volumes_kept(baseline, after)
    return {'status': 'passed', 'counts': counts, 'started': sorted(started)}


def run(request_path, request_sha256, evidence_dir, **kwargs):
    """The armed route. Reached only through `proof_entry.py`; see consumer_core.arm."""
    return core.arm(ROUTE, TAILS, execute, request_path, request_sha256, evidence_dir, **kwargs)


def main(argv):
    """Unarmed ordinary CLI: refuses every invocation before any read or child.

    The armed entry is `proof_entry.py cbs`, which Nucleus launches with the
    request's SHA-256. This module has no launch path of its own.
    """
    print(json.dumps({'status': 'refused', 'code': 'unarmed',
                      'detail': 'use proof_entry.py cbs; ordinary invocation is unarmed'}))
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
