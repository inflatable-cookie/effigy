"""CBS consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. The CBS checkout is `/Users/tom/Dev/legacy/sites/cbs`
with `infra/dev/effigy.container.toml` included by its root manifest. `main`
refuses before any child runs. The armed entry is `run`, which needs an
authenticated Nucleus proof-entry record for the exact request bytes. Source-only
code has no such channel, so `run` refuses too.

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


def run(request_path, evidence_dir, *, authenticate=None, transport=None, manifest=None,
        git=None, now_ns=None, roots=EVIDENCE_ROOTS):
    manifest = manifest if manifest is not None else core.load_manifest()
    request, digest = core.load_request(request_path)
    core.validate_request_shape(request, ROUTE)
    core.require_route_frozen(manifest, ROUTE)
    record = core.authenticate_provenance(request, digest, ROUTE, authenticate)
    core.bind_record(request, record)
    now = core.monotonic_now() if now_ns is None else now_ns
    core.check_window(request, manifest, now, ROUTE)
    core.check_budgets(request, manifest, ROUTE)
    core.check_resources(request, manifest, ROUTE)
    checkout = core.resolve_checkout(manifest['routes'][ROUTE]['checkout'])
    reads = []
    core.verify_consumer(core.recording_git(git or core.default_git, reads),
                         request, manifest, ROUTE, checkout)
    evidence = core.claim_evidence(evidence_dir, roots)
    core.write_binding(evidence, digest, record, core.module_hashes())
    core.write_closure(evidence, reads)
    engine = manifest['routes'][ROUTE]['engine']
    run_transport = transport or core.make_transport(request['budgets'], evidence / 'children')
    session = core.Session(run_transport, record['candidate']['install_identity']['path'],
                           checkout, TAILS, engine)
    try:
        result = execute(session, engine, manifest, request)
    except core.Refused as exc:
        core.write_occurrence(evidence, exc.code, exc.detail)
        raise
    core.write_occurrence(evidence, 'passed', 'armed sequence settled')
    return result


def main(argv):
    """Unarmed ordinary CLI: every invocation refuses before any effect."""
    if len(argv) != 4 or argv[0] != '--request' or argv[2] != '--evidence-dir':
        print(json.dumps({'status': 'refused', 'code': 'unarmed',
                          'detail': 'ordinary invocation has no proof-entry'}))
        return 2
    try:
        result = run(argv[1], argv[3])
    except core.Refused as exc:
        print(json.dumps({'status': 'refused', 'code': exc.code, 'detail': exc.detail}))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
