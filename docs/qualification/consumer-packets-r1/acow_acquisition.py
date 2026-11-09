"""Ordinary Acow acquisition check (task effigy#785): source only and UNARMED.

One armed occurrence checks the reported ordinary Linux-artifact acquisition in
two Acow checkouts, legacy first and modern second. Each pass runs the detached
ordinary form `container stack up --detach` in its own checkout, from that
checkout's cwd, and passes only when this occurrence produced the artifact and
the stack is observed running. A failed or unknown legacy pass refuses before
modern starts, and nothing retries.

Not here: counts, seed, deploy, sync, SQL, secret reads, manual artifact builds,
`dev` or `dev:container` (TUI, Level 2), `--repo`, `--command`, and any stop of a
resource the window did not start. The stack must be down before its pass, so
the up is a first activation. A stack that is already running is refused before
any effect.

Trust precondition as in consumer_core: Nucleus launches `proof_entry.py
acow-acquisition` at the reviewed merged commit under already-recorded
same-sequence authority. This module adds no authority of its own.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core  # noqa: E402

ROUTE = 'acow-acquisition'
ENVIRONMENT = 'stack'
PASSES = ('legacy', 'modern')
TARGET = 'aarch64-unknown-linux-gnu'
ARTIFACT = Path('.effigy/linux-release/artifacts') / ('effigy-' + TARGET)
UP_TAIL = ('container', 'stack', 'up', '--detach')
STATUS_TAIL = ('container', 'stack', 'status', '--json')
DOWN_TAIL = ('container', 'stack', 'down')
TAILS = frozenset({UP_TAIL, STATUS_TAIL, DOWN_TAIL})


def artifact_facts(checkout):
    """Stat and hash the producer's output in one checkout. Reads no child.

    A symlink at any step of the artifact path refuses, so the producer's output
    is the only thing this reads.
    """
    base = Path(checkout)
    for step in (base / '.effigy', base / '.effigy' / 'linux-release',
                 base / '.effigy' / 'linux-release' / 'artifacts'):
        if step.is_symlink():
            raise core.Refused('artifact-symlink', str(step))
    path = base / ARTIFACT
    if path.is_symlink():
        raise core.Refused('artifact-symlink', str(path))
    if not path.is_file():
        return {'present': False}
    receipt = Path(str(path) + '.receipt')
    receipt_names_target = receipt.is_file() and not receipt.is_symlink() and any(
        line.strip() == 'release_triple=' + TARGET
        for line in receipt.read_text().splitlines())
    stat = path.stat()
    return {'present': True, 'mtime_ns': stat.st_mtime_ns, 'size': stat.st_size,
            'sha256': core.sha256_file(path), 'receipt_names_target': receipt_names_target}


def produced_in_window(before, after):
    """The producer wrote a receipted artifact during this occurrence.

    A build exit alone is not this. The artifact must be new, or its mtime must
    move, and its receipt must name the target.
    """
    if not after['present'] or not after['receipt_names_target']:
        return False
    return (not before['present']) or after['mtime_ns'] != before['mtime_ns']


def read_status(session, label, status):
    """The running state, read through the route's pinned status contract."""
    return core.read_running(session, ENVIRONMENT, label, status['field'], status['schema'])


def run_pass(session, engine, pin, request, name, status):
    """One consumer pass. Identity and prior state are bound before the up."""
    roles = pin['roles']
    intent = request['prior_intent'][name]
    core.identity_check(session, engine, pin['identity'], request, roles['container'],
                        roles['volume'])
    baseline = core.protected_capture(session, engine)
    identity = pin['identity']
    core.protected_check(baseline, baseline, identity['shared_container'],
                         identity['shared_volume'], identity['never_touch_volumes'])
    observed = {ENVIRONMENT: read_status(session, name + '-pre-stack', status)}
    core.check_prior_intent({'prior_intent': intent}, observed)
    if intent[ENVIRONMENT] is not False:
        raise core.Refused('prior-running', name + ': stack already running before the window')
    before = artifact_facts(session.checkout)
    started = {ENVIRONMENT}
    observed = {ENVIRONMENT: None}
    try:
        # Unknown until read: a failed up may still have started the stack.
        session.effigy(UP_TAIL, name + '-up')
        observed[ENVIRONMENT] = read_status(session, name + '-status', status)
    finally:
        for env in core.rollback_plan({ENVIRONMENT: False}, observed, started):
            session.effigy(DOWN_TAIL, name + '-rollback')
    if observed[ENVIRONMENT] is not True:
        raise core.Refused('not-running', name + ': stack not observed running after up')
    after = artifact_facts(session.checkout)
    if not produced_in_window(before, after):
        raise core.Refused('handoff-missing', name + ': up exited but no receipted artifact '
                           'was produced in this window')
    core.protected_check(baseline, core.protected_capture(session, engine),
                         identity['shared_container'], identity['shared_volume'],
                         identity['never_touch_volumes'])
    return {'artifact': {'before': before, 'after': after}, 'stack_running': True}


def execute(session, engine, manifest, request):
    """Legacy first. Modern runs only after legacy passed; a refusal from legacy stops here."""
    pins = manifest['routes'][ROUTE]
    intent = request['prior_intent']
    if set(intent) != set(PASSES) or any(set(intent[name]) != {ENVIRONMENT} for name in PASSES):
        raise core.Refused('prior-intent', 'intent must name stack for legacy and modern only')
    status = pins['status_contract']
    legacy = run_pass(session, engine, {'identity': pins['identity'], 'roles': pins['roles']},
                      request, 'legacy', status)
    modern_session = session.at(core.resolve_checkout(pins['modern']['checkout']))
    modern = run_pass(modern_session, engine, pins['modern'], request, 'modern', status)
    return {'status': 'passed', 'passes': {'legacy': legacy, 'modern': modern}}


def verify_modern(git, request, manifest):
    """The modern closure, read before any effect, with the occurrence's recording reader."""
    modern = manifest['routes'][ROUTE]['modern']
    core.verify_consumer_block(git, request['modern_consumer'], modern['consumer'],
                               core.resolve_checkout(modern['checkout']))


def run(request_path, request_sha256, evidence_dir, **kwargs):
    """The armed route. Reached only through `proof_entry.py`; see consumer_core.arm."""
    return core.arm(ROUTE, TAILS, execute, request_path, request_sha256, evidence_dir,
                    verify_more=verify_modern, **kwargs)


def main(argv):
    """Unarmed ordinary CLI: refuses every invocation before any read or child."""
    print(json.dumps({'status': 'refused', 'code': 'unarmed',
                      'detail': 'use proof_entry.py acow-acquisition; ordinary invocation is unarmed'}))
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
