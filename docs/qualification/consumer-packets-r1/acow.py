"""Acow consumer packet r1 (task effigy#757): the installed-candidate route.

Source only and UNARMED. The committed manifest pins no candidate, window or
protected baseline, so `main` refuses before any child runs. A later exact
Nucleus proof-only occurrence binds the frozen candidate C, positive624, the
actual window and resources, then this same reviewed code runs.

Armed sequence (prior intent observed first, every child waited):
  0. pre-observe running state of each environment named in the prior intent
  1. hybrid bring-up, observe            (phase hybrid-1)
  2. stack bring-up, observe             (phase container)
  3. hybrid bring-up, observe            (phase hybrid-2)
  4. Level 1 forward: stack status       (non-interactive, captured)
  5. eight-table counts, counts only     (declared quiet window)
  6. scoped rollback to prior intent     (only environments this window started)

`dev:test` is declared, not armed: its fresh environment is not pinned in
`infra/workspace.toml`. `dev:container` and `dev` are TUI and need a PTY,
which is Level 2, so this route uses the detached container forms instead.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core

ROUTE = 'acow'
ALLOWED_TAILS = frozenset({
    ('container', 'hybrid', 'up', '--detach'),
    ('container', 'stack', 'up', '--detach'),
    ('container', 'hybrid', 'status', '--json'),
    ('container', 'stack', 'status', '--json'),
    ('container', 'hybrid', 'down'),
    ('container', 'stack', 'down'),
})
PHASES = (
    {'phase': 'hybrid-1', 'environment': 'hybrid'},
    {'phase': 'container', 'environment': 'stack'},
    {'phase': 'hybrid-2', 'environment': 'hybrid'},
)
ENVIRONMENTS = frozenset(phase['environment'] for phase in PHASES)
DEV_TEST_DECLARATION = {
    'phase': 'dev:test', 'status': 'declared-unarmed',
    'reason': 'fresh test environment not pinned in infra/workspace.toml',
}
LEVEL1_FORWARD = ('container', 'stack', 'status', '--json')
PSQL_TAIL = ('-U', 'postgres', '-d', 'acowtancy', '--no-psqlrc',
             '-v', 'ON_ERROR_STOP=1', '-Atc')
TABLE_RE = re.compile(r'[a-z_]+\.[a-z_]+')
PG_RESOURCE = 'acowtancy-dev-postgres-container'
EVIDENCE_ROOTS = ('/Volumes/Builds/scratch/effigy',)


def count_argv(engine, pg_id, table):
    """One counts-only probe: fixed nerdctl exec shape, table allowlisted."""
    if not TABLE_RE.fullmatch(table):
        raise core.Refused('counts-plan', 'table name is not allowlisted')
    return [engine['colima_binary'], 'nerdctl', '-p', engine['profile'], '--',
            'exec', pg_id, 'psql', *PSQL_TAIL,
            'SELECT count(*) FROM %s' % table]


def expected_counts(manifest):
    """Owner baseline plus the declared quiet-window delta, per table."""
    table_pins = manifest['routes'][ROUTE]['counts']
    return {table: pin['baseline'] + pin['delta']
            for table, pin in table_pins.items()}


def parse_running(output):
    """The status must name its running state as a bool, or refuse."""
    try:
        body = json.loads(output)
    except ValueError:
        raise core.Refused('status-unparsed', 'status is not JSON')
    if not isinstance(body, dict) or not isinstance(body.get('running'), bool):
        raise core.Refused('status-unparsed', 'running state is not a bool')
    return body['running']


def plan(prior_intent, binary):
    """The static step list (argv only). Tests read it for allowlist purity."""
    steps = [('pre-%s-status' % name, [binary, 'container', name, 'status', '--json'])
             for name in sorted(prior_intent)]
    for phase in PHASES:
        steps.append((phase['phase'] + '-up', [binary, 'container',
                      phase['environment'], 'up', '--detach']))
        steps.append((phase['phase'] + '-status', [binary, 'container',
                      phase['environment'], 'status', '--json']))
    steps.append(('level1-forward', [binary, *LEVEL1_FORWARD]))
    return steps


def read_running(name, binary, transport, label):
    argv = [binary, 'container', name, 'status', '--json']
    core.check_argv(argv, ALLOWED_TAILS, binary)
    record = transport(argv, label)
    return parse_running(Path(record['streams']['stdout']['path']).read_text())


def observe_counts(manifest, request, transport, binary):
    """Counts only, through the engine route the manifest names."""
    engine = manifest['routes'][ROUTE]['engine']
    pg_id = manifest['routes'][ROUTE]['resources'][PG_RESOURCE]['identity']
    if not core.HEX64.fullmatch(str(pg_id)):
        raise core.Refused('counts-plan', 'postgres container id not pinned')
    counts = {}
    for table in sorted(expected_counts(manifest)):
        record = transport(count_argv(engine, pg_id, table),
                           'count-' + table.replace('.', '-'))
        text = Path(record['streams']['stdout']['path']).read_text().strip()
        if not text.isdigit():
            raise core.Refused('unexpected-counts', 'non-numeric output: ' + table)
        counts[table] = int(text)
    return counts


def rollback(prior_intent, now_running, started, binary, transport):
    """Stop only what this window started, restoring the recorded intent."""
    for name in core.rollback_plan(prior_intent, now_running, started):
        argv = [binary, 'container', name, 'down']
        core.check_argv(argv, ALLOWED_TAILS, binary)
        transport(argv, 'rollback-' + name)


def execute(manifest, request, binary, transport):
    """The armed sequence through `transport(argv, label) -> record`."""
    intent = request['prior_intent']
    if not set(intent) <= ENVIRONMENTS:
        raise core.Refused('prior-intent', 'environment outside the route')
    observed = {}
    started = set()
    try:
        for name in sorted(intent):
            observed[name] = read_running(name, binary, transport, 'pre-' + name)
        core.check_prior_intent(request, observed)
        started = {name for name in intent if intent[name] is False}
        for phase in PHASES:
            env = phase['environment']
            up = [binary, 'container', env, 'up', '--detach']
            core.check_argv(up, ALLOWED_TAILS, binary)
            transport(up, phase['phase'] + '-up')
            if env in observed:
                observed[env] = read_running(env, binary, transport,
                                             phase['phase'] + '-status')
        forward = [binary, *LEVEL1_FORWARD]
        core.check_argv(forward, ALLOWED_TAILS, binary)
        transport(forward, 'level1-forward')
        counts = observe_counts(manifest, request, transport, binary)
        core.counts_match(counts, expected_counts(manifest))
    finally:
        rollback(intent, dict(observed), started, binary, transport)
    return {'status': 'passed', 'counts': counts, 'started': sorted(started)}


def make_transport(request, evidence):
    """The production transport: fixed argv, captured streams, owned cleanup."""
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
    """Entry. Every gate runs before the first child; refusals are typed."""
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
