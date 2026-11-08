"""CBS consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. The CBS checkout is `/Users/tom/Dev/legacy/sites/cbs`
with `infra/dev/effigy.container.toml` included by its root manifest. The
committed manifest pins no candidate, window, engine volume or counts, so
`main` refuses before any child runs.

Armed sequence (prior intent observed first, every child waited):
  0. pre-observe the `dev` system's running state (prior intent)
  1. detached app-shell bring-up        (`container up --detach --system dev`)
  2. Level 1 forward: system status     (non-destructive, captured)
  3. named cbs / cbs-mortcalc counts    (counts only, declared quiet window)
  4. scoped rollback to prior intent    (only if this window started it)

No seed, deploy, sync, schema refresh or data command is in the route. The
tracked `[tasks.seed]` and `[tasks.schema:refresh]` stay out of authority.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core

ROUTE = 'cbs'
SYSTEM = 'dev'
ALLOWED_TAILS = frozenset({
    ('container', 'up', '--detach', '--system', SYSTEM),
    ('container', 'status', '--system', SYSTEM, '--json'),
    ('container', 'down', '--system', SYSTEM),
})
FORWARD = ('container', 'status', '--system', SYSTEM, '--json')
TABLE_RE = re.compile(r'[a-z_]+')
DATABASES = ('cbs', 'cbs-mortcalc')
DB_RESOURCE = 'cbs-database-container'
EVIDENCE_ROOTS = ('/Volumes/Builds/scratch/effigy',)


def count_argv(engine, db_id, database, table):
    """One counts-only probe: pinned engine tail, named schema and table only."""
    if database not in DATABASES or not TABLE_RE.fullmatch(table):
        raise core.Refused('counts-plan', 'database or table is not allowlisted')
    return [engine['colima_binary'], 'nerdctl', '-p', engine['profile'], '--',
            'exec', db_id, *engine['count_tail'],
            'SELECT COUNT(*) FROM `%s`.`%s`' % (database, table)]


def expected_counts(manifest):
    table_pins = manifest['routes'][ROUTE]['counts']['tables']
    return {'%s.%s' % (db, table): pin['baseline'] + pin['delta']
            for db, tables in sorted(table_pins.items())
            for table, pin in sorted(tables.items())}


def parse_running(output):
    try:
        body = json.loads(output)
    except ValueError:
        raise core.Refused('status-unparsed', 'status is not JSON')
    if not isinstance(body, dict) or not isinstance(body.get('running'), bool):
        raise core.Refused('status-unparsed', 'running state is not a bool')
    return body['running']


def plan(prior_intent, binary):
    """The static step list (argv only). Tests read it for allowlist purity."""
    return [
        ('pre-dev-status', [binary, 'container', 'status', '--system', SYSTEM, '--json']),
        ('app-shell-up', [binary, 'container', 'up', '--detach', '--system', SYSTEM]),
        ('level1-forward', [binary, *FORWARD]),
    ]


def read_running(binary, transport, label):
    argv = [binary, *FORWARD]
    core.check_argv(argv, ALLOWED_TAILS, binary)
    record = transport(argv, label)
    return parse_running(Path(record['streams']['stdout']['path']).read_text())


def observe_counts(manifest, request, transport):
    engine = manifest['routes'][ROUTE]['counts']['engine']
    db_id = manifest['routes'][ROUTE]['resources'][DB_RESOURCE]['identity']
    if not core.HEX64.fullmatch(str(db_id)):
        raise core.Refused('counts-plan', 'database container id not pinned')
    counts = {}
    for key in sorted(expected_counts(manifest)):
        database, table = key.split('.', 1)
        record = transport(count_argv(engine, db_id, database, table),
                           'count-%s-%s' % (database, table))
        text = Path(record['streams']['stdout']['path']).read_text().strip()
        if not text.isdigit():
            raise core.Refused('unexpected-counts', 'non-numeric output: ' + key)
        counts[key] = int(text)
    return counts


def execute(manifest, request, binary, transport):
    intent = request['prior_intent']
    if set(intent) != {SYSTEM}:
        raise core.Refused('prior-intent', 'intent must name only the dev system')
    observed = {}
    started = set()
    try:
        observed[SYSTEM] = read_running(binary, transport, 'pre-dev-status')
        core.check_prior_intent(request, observed)
        started = {SYSTEM} if intent[SYSTEM] is False else set()
        up = [binary, 'container', 'up', '--detach', '--system', SYSTEM]
        core.check_argv(up, ALLOWED_TAILS, binary)
        transport(up, 'app-shell-up')
        observed[SYSTEM] = read_running(binary, transport, 'post-dev-status')
        forward = [binary, *FORWARD]
        core.check_argv(forward, ALLOWED_TAILS, binary)
        transport(forward, 'level1-forward')
        counts = observe_counts(manifest, request, transport)
        core.counts_match(counts, expected_counts(manifest))
    finally:
        for name in core.rollback_plan(intent, dict(observed), started):
            down = [binary, 'container', 'down', '--system', SYSTEM]
            core.check_argv(down, ALLOWED_TAILS, binary)
            transport(down, 'rollback-' + name)
    return {'status': 'passed', 'counts': counts, 'started': sorted(started)}


def make_transport(request, evidence):
    caps = request['budgets']
    children = Path(evidence) / 'children'

    def transport(argv, label):
        return core.run_child(
            argv, timeout_s=caps['step_timeout_s'],
            stdout_cap=caps['stdout_cap_bytes'],
            stderr_cap=caps['stderr_cap_bytes'],
            out_dir=children, label=label)
    return transport


def run(request_path, evidence_dir, *, manifest=None, transport=None,
        authenticate=None, now_ns=None, roots=EVIDENCE_ROOTS):
    manifest = manifest if manifest is not None else core.load_manifest()
    request = core.load_request(request_path)
    now = core.monotonic_now() if now_ns is None else now_ns
    core.validate_request(request, manifest, ROUTE, authenticate, now)
    evidence = core.within_root(evidence_dir, roots)
    binary = request['candidate']['install_identity']['path']
    run_transport = transport or make_transport(request, evidence)
    try:
        result = execute(manifest, request, binary, run_transport)
    except core.Refused as exc:
        core.write_occurrence(evidence, exc.code, exc.detail)
        raise
    core.write_occurrence(evidence, 'passed', 'armed sequence settled')
    return result


def main(argv):
    """Unarmed ordinary CLI: no proof-entry arguments refuse before any effect."""
    if len(argv) != 4 or argv[0] != '--request' or argv[2] != '--evidence-dir':
        print(json.dumps({'status': 'refused', 'code': 'unarmed',
                          'detail': 'ordinary invocation has no proof-entry'}))
        return 2
    try:
        result = run(argv[1], argv[3])
    except core.Refused as exc:
        print(json.dumps({'status': 'refused', 'code': exc.code,
                          'detail': exc.detail}))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
