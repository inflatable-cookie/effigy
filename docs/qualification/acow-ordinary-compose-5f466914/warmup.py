#!/usr/bin/env python3
"""Acow ordinary-Compose worktree warm-up route — PREPARED, NOT EXECUTED.

Queue task effigy#540 packet route (amended by effigy#543: the disposable
worktree materializes at the SELECTED current clean consumer head, and the
bundle sync is verified as the Acow-owner-approved source-identity transition
before attach/up). The exact future proof selector, run once
from the reviewed task head only after the full GO chain recorded in
packet-r1.md:

    python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/warmup.py

Every precondition is machine-checked and refuses closed BEFORE the worktree
is created or any native effect happens: missing GO, wrong packet SHA, missing
token, absent or non-accepted #544 r18 proof receipt, installed-identity drift, moved
consumer pin or bundle lock, dirty primary checkout, an existing fixture path,
ambiguous engine inventory or an already-active warm-up scope. The one
authorized sequence is exactly the owner-approved order:

    effigy bundle sync
    effigy worktree:container:attach   (infra/tasks.toml:16; #795 pointer and
                                        hash-verified local-dev copy authority)
    effigy container up --detach       (plain; NEVER --system)

Acceptance, teardown and the preserve-all-volumes rule are recorded in
packet-r1.md; ambiguous ownership or unsupported teardown stops and reports
instead of guessing.
"""
import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from shutil import which

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    ACOW_PRIMARY, ATTACH_RECORD_REL, BUNDLE_LOCK_REL, COLIMA_BIN,
    COLIMA_PROFILE, COPIED_VAULT_REL, DIR, GO_WARMUP_TOKENS, MOUNTS_LABEL,
    MOUNTS_LABEL_BUDGET, SCRATCH, SHARED_MARIADB_VOLUME, WARMUP_PARENT,
    NEVER_TOUCH_VOLUMES, ConsoleLog, Refused,
    assert_no_scoped_containers, assert_no_warmup_scope, assert_packet_dir,
    capture_inventory, container_state, ensure_owned_dir, expect_absent,
    git_args, judge_teardown_inventory, label_length_only,
    queue_shape_components, read_owner_marker, require_mariadb_unchanged,
    run_quiet, scoped_container_map, session_identity, validate_attach_record,
    verify_544_proof_receipt, verify_bundle_transition_after,
    verify_bundle_transition_before, verify_consumer_pins,
    verify_go_receipt, verify_installed_identity, verify_packet_bundle,
    volume_mount_identity, write_owner_marker, write_receipt,
)

# The scoped compose project owns exactly these services; the shared MariaDB
# stays the machine-wide container and is never part of the scope.
WARMUP_SERVICES = ('workspace', 'postgres', 'silo', 'mailpit', 'dbgate')

# Exact authorized command surface inside the worktree (never --system).
AUTHORIZED_COMMANDS = {
    ('bundle', 'sync'),
    ('bundle', 'inspect', '--json'),
    ('worktree:container:attach',),
    ('container', 'up', '--detach'),
    ('container', 'down'),
}

# Read-only preflight inspection of the consumer checkout's bundle lock
# (inspection never fetches or writes; it targets the pinned Acow primary).
PREFLIGHT_INSPECT = ('bundle', 'inspect', '--repo', str(ACOW_PRIMARY), '--json')


def colima_runner():
    """Real read-only engine runner over the pinned Colima profile."""
    def runner(args):
        r = run_quiet([COLIMA_BIN, 'nerdctl', '-p', COLIMA_PROFILE, '--', *args],
                      env=None, timeout=90)
        return r.returncode, r.stdout, r.stderr
    return runner


def make_effigy_runner(worktree, console, effigy_path):
    """Authorized-sequence runner for the installed candidate in the worktree.

    Asserts the exact command allowlist (plain `container up --detach`, never
    --system), records exit codes and child output into the 0600 console log,
    and refuses nonzero exits.
    """
    def run(args, timeout=900):
        key = tuple(args)
        if key not in AUTHORIZED_COMMANDS:
            raise Refused('command outside the authorized sequence: ' + ' '.join(args))
        if '--system' in args:
            raise Refused('--system is never part of this route')
        console.write('\n$ effigy %s\n' % ' '.join(args))
        console.flush()
        r = run_quiet([effigy_path, *args], cwd=str(worktree), timeout=timeout)
        console.write(r.stdout)
        console.write(r.stderr)
        console.write('\n[exit %d]\n' % r.returncode)
        console.flush()
        if r.returncode:
            raise Refused('authorized sequence step failed: effigy %s (exit %d)'
                          % (' '.join(args), r.returncode))
        return r.stdout
    return run


def bundle_inspect_reader(run):
    """Machine-readable bundle lock reader (`effigy bundle inspect --json`)."""
    def inspect_fn():
        raw = run(['bundle', 'inspect', '--json'])
        try:
            obj = json.loads(raw)
        except ValueError:
            raise Refused('bundle inspect produced invalid JSON') from None
        if obj.get('ok') is not True or obj.get('error') is not None:
            raise Refused('bundle inspect did not succeed')
        result = obj.get('result')
        if not isinstance(result, dict):
            # Some installed builds return the inner inspect value directly.
            result = obj if isinstance(obj.get('source'), dict) else None
        if not isinstance(result, dict):
            raise Refused('bundle inspect result block missing')
        return result
    return inspect_fn


def preflight_effigy_reader(console):
    """Read-only bundle-inspect reader for the pre-worktree preconditions."""
    resolved = which('effigy')
    if not resolved:
        raise Refused('no effigy on PATH for the bundle lock preflight')

    def run(args, timeout=90):
        key = tuple(args)
        if key != PREFLIGHT_INSPECT:
            raise Refused('command outside the authorized preflight: ' + ' '.join(args))
        console.write('\n$ effigy %s\n' % ' '.join(args))
        r = run_quiet([resolved, *args], timeout=timeout)
        console.write(r.stdout)
        console.write(r.stderr)
        if r.returncode:
            raise Refused('bundle inspect failed (exit %d)' % r.returncode)
        return r.stdout
    return run


def preconditions(console):
    """All machine-readable preconditions; refuses before any effect."""
    assert_packet_dir(DIR)
    bundle = verify_packet_bundle()
    packet_sha = bundle['packet_sha256']
    go = verify_go_receipt(SCRATCH / 'acow-warmup-go.txt', packet_sha,
                           GO_WARMUP_TOKENS, 'warmup')
    receipt544 = verify_544_proof_receipt(SCRATCH / '544-proof-receipt.json',
                                          go['receipt544_sha256'])

    def version_fn(path):
        r = run_quiet([str(path), '--version'], timeout=30)
        if r.returncode:
            raise Refused('installed effigy --version failed')
        return r.stdout.strip().splitlines()[0]

    def hash_fn(path):
        h = hashlib.sha256()
        with open(path, 'rb') as f:
            for chunk in iter(lambda: f.read(65536), b''):
                h.update(chunk)
        return h.hexdigest()

    identity = verify_installed_identity(which, version_fn,
                                         go['installed_binary_path'], hash_fn,
                                         go['installed_version_line'])

    def git_fn(args):
        r = run_quiet(['/usr/bin/git', '-C', '/Users/tom/Dev/projects/acowtancy', *args],
                      env=git_args(), timeout=30)
        return r.returncode, r.stdout, r.stderr

    pins = verify_consumer_pins(git_fn)
    lock = verify_bundle_transition_before(
        bundle_inspect_reader(preflight_effigy_reader(console)),
        ACOW_PRIMARY / BUNDLE_LOCK_REL)
    shape = queue_shape_components(WARMUP_PARENT, packet_sha)
    ensure_owned_dir(WARMUP_PARENT)
    expect_absent(shape['path'])

    runner = colima_runner()
    baseline = capture_inventory(runner)
    require_mariadb_unchanged(runner)
    protected = {}
    for name in NEVER_TOUCH_VOLUMES:
        protected[name] = volume_mount_identity(runner, name) \
            if name in set(baseline['volumes']) else None
    if protected[SHARED_MARIADB_VOLUME] is None:
        raise Refused('shared MariaDB data volume missing from the engine inventory')
    # Busy-fence: no warm-up scope may already exist on the engine.
    assert_no_warmup_scope(baseline['networks'])

    record = {
        'route': 'warmup',
        'task': 'effigy#540',
        'packet_sha256': packet_sha,
        'manifest_sha256': bundle['manifest_sha256'],
        'go': go,
        'receipt544': receipt544,
        'installed_identity': identity,
        'consumer_pins': pins,
        'bundle_lock': lock,
        'fixture': shape,
        'baseline': {
            'inventory': baseline,
            'protected_volume_identity': protected,
        },
    }
    return record, runner, shape


def primary_git_runner():
    """Read-only git runner against the pinned Acow primary checkout."""
    def run_git(args, timeout=120, cwd=None):
        return run_quiet(['/usr/bin/git', '-C', cwd or str(ACOW_PRIMARY), *args],
                         env=git_args(), timeout=timeout)
    return run_git


def create_warmup_worktree(record, shape, console, state, run_git=None, git_fn=None):
    """Effect 1: the disposable linked worktree at the SELECTED current head.

    The selected consumer head is re-verified immediately before this first
    effect and the exact same commit is used for the worktree add, the
    ownership marker and the recorded receipt identities; a silently
    substituted checkout (the old anchor pin, a stale worktree, a moved head)
    refuses. `run_git` is the injectable effect runner and `git_fn` the
    injectable read-only pin verifier (tests pass fakes; the route uses the
    real primary runners for both).
    """
    if run_git is None:
        run_git = primary_git_runner()
    selected = record['consumer_pins']['selected_commit']
    if git_fn is None:
        reader = primary_git_runner()
        def git_fn(args):
            result = reader(args)
            return result.returncode, result.stdout, result.stderr
    reverified = verify_consumer_pins(git_fn)
    if reverified['selected_commit'] != selected \
            or reverified['selected_tree'] != record['consumer_pins']['selected_tree']:
        raise Refused('selected consumer head moved between preflight and the first effect')
    record['consumer_pins_reverified'] = {
        'selected_commit': reverified['selected_commit'],
        'selected_tree': reverified['selected_tree'],
    }
    worktree = Path(shape['path'])
    ident = session_identity(worktree)
    state['project'] = ident['project']
    r = run_git(['worktree', 'add', '--detach', str(worktree), selected])
    console.write('\n$ git worktree add --detach %s %s\n[exit %d]\n'
                  % (worktree, selected[:12], r.returncode))
    if r.returncode:
        raise Refused('disposable worktree creation failed (exit %d)' % r.returncode)
    state['worktree'] = worktree
    marker = {
        'task': 'effigy#540', 'route': 'warmup', 'kind': 'disposable-linked-worktree',
        'packet_sha256': record['packet_sha256'], 'selected_commit': selected,
        'root': str(worktree), 'session_expectation': ident['session_id'],
    }
    write_owner_marker(worktree, marker)
    head = run_git(['rev-parse', 'HEAD'], cwd=str(worktree))
    if head.returncode or head.stdout.strip() != selected:
        raise Refused('worktree head does not match the selected consumer commit')
    record['activation'] = {'worktree': str(worktree), 'worktree_head': selected,
                            'session': ident, 'project': ident['project'], 'steps': []}
    return worktree, ident


def worktree_lock_before(worktree):
    """Record the fresh worktree's actual lock state before sync."""
    lock_path = worktree / BUNDLE_LOCK_REL
    if os.path.lexists(lock_path):
        raise Refused('disposable worktree bundle lock unexpectedly exists before sync')
    return {'mode': 'absent', 'path': str(lock_path)}


def activate(record, runner, shape, console, state):
    """The exact owner-approved sequence under the verified GO chain.

    `state` records how far activation got so a mid-window refusal can be
    rolled back exactly (owned worktree only) instead of guessed.
    """
    effigy_path = which('effigy')
    if not effigy_path:
        raise Refused('no effigy on PATH at activation')
    worktree, ident = create_warmup_worktree(record, shape, console, state)
    project = ident['project']
    run = make_effigy_runner(worktree, console, effigy_path)

    # Exact sequence step 1: effigy bundle sync; afterwards the lock must
    # name exactly the approved target state (typed revision + digest) with
    # no residual staleness; any unexpected post-sync source-origin, revision
    # or content drift refuses BEFORE attach/up.
    lock_before = worktree_lock_before(worktree)
    run(['bundle', 'sync'])
    lock_after = verify_bundle_transition_after(
        bundle_inspect_reader(run), worktree / BUNDLE_LOCK_REL)
    record['activation']['steps'].append({
        'step': 'bundle sync',
        'lock_before': lock_before,
        'lock_after': {key: lock_after[key]
                       for key in ('mode', 'revision', 'digest', 'stale')},
    })

    # Exact sequence step 2: effigy worktree:container:attach (#795 authority).
    attach_out = run(['worktree:container:attach'])
    record_obj = validate_attach_record(worktree / ATTACH_RECORD_REL, ident, worktree)
    m = re.search(r'secrets vault: ([^\n]+?) \(', attach_out)
    if not m:
        raise Refused('attach output did not classify the vault transfer')
    vault_action = m.group(1).strip()
    if vault_action.startswith('none found'):
        raise Refused('attach reports no vault in the primary checkout; refusing before up')
    if not (worktree / COPIED_VAULT_REL).is_file():
        raise Refused('attach did not leave a vault document in the worktree')
    record['activation']['steps'].append({
        'step': 'worktree:container:attach', 'session_id': record_obj['session_id'],
        'vault_action': vault_action,
    })

    # Exact sequence step 3: plain effigy container up --detach, NEVER --system.
    run(['container', 'up', '--detach'])
    record['activation']['steps'].append({'step': 'container up --detach'})

    # Acceptance: scoped project shape, service health, label length only.
    services = scoped_container_map(runner, project)
    if set(services) != set(WARMUP_SERVICES):
        raise Refused('scoped service set differs from the accepted shape: %s'
                      % ','.join(sorted(services)))
    health = {s: container_state(runner, cid) for s, cid in sorted(services.items())}
    unhealthy = {s: st for s, st in health.items() if st != 'running'}
    if unhealthy:
        raise Refused('scoped services not healthy: %s' % json.dumps(unhealthy, sort_keys=True))
    workspace_id = services['workspace']
    label_len = label_length_only(runner, workspace_id, MOUNTS_LABEL)
    if label_len >= MOUNTS_LABEL_BUDGET:
        raise Refused('nerdctl/mounts serialized label length %d >= %d'
                      % (label_len, MOUNTS_LABEL_BUDGET))
    require_mariadb_unchanged(runner)
    record['acceptance'] = {
        'scoped_services': {s: {'id': cid, 'state': health[s]}
                            for s, cid in sorted(services.items())},
        'mounts_label_length': label_len,
        'mounts_label_budget': MOUNTS_LABEL_BUDGET,
    }
    write_receipt('acow-warmup-activation-receipt.json', record)
    return worktree, project


def teardown(record, runner, worktree, project, console, failure=None):
    """Down, exact owned-worktree removal, preserve ALL volumes.

    Volumes are never deleted here — including the window's own scoped
    volumes and every older scoped volume. Ambiguous ownership or an
    unsupported teardown stops and reports instead of guessing.
    """
    marker = {
        'task': 'effigy#540', 'route': 'warmup', 'kind': 'disposable-linked-worktree',
        'packet_sha256': record['packet_sha256'],
        'selected_commit': record['activation']['worktree_head'],
        'root': str(worktree),
        'session_expectation': record['activation']['session']['session_id'],
    }
    read_owner_marker(worktree, marker)  # refuses on any ownership ambiguity
    result = {'failure': failure, 'steps': []}
    stop = None
    effigy_path = which('effigy')
    if not effigy_path:
        stop = 'no effigy on PATH during teardown'
    else:
        console.write('\n$ effigy container down\n')
        console.flush()
        r = run_quiet([effigy_path, 'container', 'down'], cwd=str(worktree), timeout=600)
        console.write(r.stdout)
        console.write(r.stderr)
        console.write('\n[exit %d]\n' % r.returncode)
        result['steps'].append({'step': 'container down', 'exit': r.returncode})
        if r.returncode:
            stop = 'container down failed (exit %d); not guessing further' % r.returncode
    if stop is None:
        try:
            assert_no_scoped_containers(runner, project)
            result['steps'].append({'step': 'scoped containers absent', 'project': project})
        except Refused as e:
            stop = str(e)
    if stop is None:
        try:
            live = capture_inventory(runner)
            baseline = record['baseline']['inventory']
            added_volumes = judge_teardown_inventory(
                live, baseline, project,
                record['baseline']['protected_volume_identity'],
                mount_of=lambda name: volume_mount_identity(runner, name))
            result['volumes_preserved'] = added_volumes
            result['steps'].append({'step': 'inventory restored; scoped volumes preserved'})
        except Refused as e:
            stop = str(e)
    if stop is None:
        vault = Path(worktree) / COPIED_VAULT_REL
        r = run_quiet(['/usr/bin/git', '-C', '/Users/tom/Dev/projects/acowtancy',
                       'worktree', 'remove', '--force', str(worktree)],
                      env=git_args(), timeout=120)
        result['steps'].append({'step': 'git worktree remove --force', 'exit': r.returncode})
        if r.returncode:
            stop = 'owned worktree removal failed (exit %d); not guessing further' % r.returncode
        elif os.path.lexists(worktree):
            stop = 'owned worktree path still exists after removal; not guessing further'
        else:
            result['copied_vault_confirmed_gone'] = True
    result['status'] = 'teardown-complete' if stop is None else 'ambiguous-stopped'
    result['stop_reason'] = stop
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packet-sha', required=False, default=None,
                        help='optional cross-check; the committed packet is authoritative')
    args = parser.parse_args()
    console = ConsoleLog('acow-warmup-console.log')
    state = {}
    record = runner = None
    failure = None
    exit_code = 0
    try:
        record, runner, shape = preconditions(console)
        if args.packet_sha is not None and args.packet_sha != record['packet_sha256']:
            raise Refused('--packet-sha argument differs from the committed packet')
        write_receipt('acow-warmup-invocation-receipt.json', {
            'route': 'warmup', 'phase': 'preconditions-passed',
            'packet_sha256': record['packet_sha256'],
            'go_receipt_sha256': record['go']['sha256'],
            'fixture': shape,
        })
        worktree, project = activate(record, runner, shape, console, state)
    except Refused as e:
        console.write('\nREFUSED: %s\n' % e)
        print('REFUSED: %s' % e, file=sys.stderr)
        failure = str(e)
        exit_code = 3
    if state.get('worktree') is not None:
        result = teardown(record, runner, state['worktree'], state['project'],
                          console, failure=failure)
        write_receipt('acow-warmup-teardown-receipt.json', result)
        if result['status'] != 'teardown-complete':
            print('TEARDOWN AMBIGUOUS: %s' % result['stop_reason'], file=sys.stderr)
            exit_code = max(exit_code, 4)
    console.close()
    if exit_code == 0:
        print('warm-up window complete; teardown verified; volumes preserved')
    return exit_code


if __name__ == '__main__':
    sys.exit(main())
