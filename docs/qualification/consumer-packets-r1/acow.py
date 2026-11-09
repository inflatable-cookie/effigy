"""Acow consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. `main` refuses before any child runs. The armed entry
is `run`, which needs an authenticated Nucleus proof-entry record for the exact
request bytes. Source-only code has no such channel, so `run` refuses too.

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


def run(request_path, evidence_dir, *, authenticate=None, transport=None, manifest=None,
        git=None, now_ns=None, roots=EVIDENCE_ROOTS):
    """The armed entry. Every gate runs before the first effect; refusals are typed."""
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
    """Unarmed ordinary CLI: every invocation refuses before any effect.

    The committed entry has no authenticated provenance channel, so even a
    well-formed proof-entry invocation refuses at `nucleus-provenance-unavailable`.
    """
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
