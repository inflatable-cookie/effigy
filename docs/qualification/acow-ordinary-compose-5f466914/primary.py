#!/usr/bin/env python3
"""Acow primary-data counts route — PREPARED, NOT EXECUTED.

Queue task effigy#540 packet route, separately armed from the worktree
warm-up and independently invocable. The exact future proof selector, run
once from the reviewed task head only after its own operator GO:

    python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/primary.py

Window (owner-run around this selector, recorded in packet-r1.md):

    effigy dev --headless            (primary checkout; owner brings the stack up)
    <this selector>                  (counts-only observation through the
                                      existing postgres container)
    effigy dev:down                  (owner stops the stack; no remaining runs)

The selector never starts or stops the stack and refuses closed unless the
primary stack is already up with the exact expected postgres identity. It
records counts only — never row values, never raw child output — protects the
shared MariaDB by identity before and after, refuses the accidental
invocations listed in packet-r1.md, and grants no project.qa.run.
"""
import argparse
import json
import re
import sys
from pathlib import Path
from shutil import which

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    COLIMA_BIN, COLIMA_PROFILE, DIR, EXPECTED_PRIMARY_COUNTS, GO_PRIMARY_TOKENS,
    PRIMARY_DATABASE, PRIMARY_POSTGRES_VOLUME, PRIMARY_PROJECT, SCRATCH,
    SHARED_MARIADB_VOLUME, ConsoleLog, Refused, assert_no_warmup_scope,
    assert_packet_dir, capture_inventory, exact_ps_ids, git_args,
    require_mariadb_unchanged, run_quiet, verify_consumer_pins,
    verify_go_receipt, verify_packet_bundle, volume_mount_identity,
    write_receipt,
)

# The one authorized observation command shape, per qualified table. Counts
# only; any other query is outside this route's authority.
PSQL_ALLOWLIST = ('-U', 'postgres', '-d', PRIMARY_DATABASE, '--no-psqlrc',
                  '-v', 'ON_ERROR_STOP=1', '-Atc')


def colima_runner():
    def runner(args):
        r = run_quiet([COLIMA_BIN, 'nerdctl', '-p', COLIMA_PROFILE, '--', *args],
                      env=None, timeout=90)
        return r.returncode, r.stdout, r.stderr
    return runner


def pick_primary_postgres(runner):
    """Exactly one postgres container in the primary project, or refuse."""
    rc, out, _ = runner(['ps', '-a', '--no-trunc', '--filter',
                         'label=com.docker.compose.project=' + PRIMARY_PROJECT,
                         '--format', '{{.ID}}'])
    if rc:
        raise Refused('primary project container read failed')
    ids = exact_ps_ids(out)
    matches = []
    for cid in ids:
        rc, out, _ = runner(['inspect', '--format',
                             '{{index .Labels "com.docker.compose.service"}}', cid])
        if rc:
            raise Refused('primary project container inspect failed: ' + cid)
        if out.strip() == 'postgres':
            matches.append(cid)
    if not matches:
        raise Refused('primary dev stack is not up (no postgres container); '
                      'bring it up with effigy dev --headless first')
    if len(matches) > 1:
        raise Refused('ambiguous primary postgres identity: %s' % ','.join(matches))
    return matches[0]


def postgres_volume_identity(runner, pg_id):
    """The observed postgres must mount the primary data volume, or refuse."""
    rc, out, _ = runner(['inspect', '--format',
                         '{{range .Mounts}}{{.Type}} {{.Name}}\n{{end}}', pg_id])
    if rc:
        raise Refused('primary postgres mounts unreadable')
    names = [line.split(' ', 1)[1] for line in out.splitlines()
             if line.startswith('volume ') and len(line) > 7]
    if PRIMARY_POSTGRES_VOLUME not in names:
        raise Refused('primary postgres does not mount %s; refusing to observe'
                      % PRIMARY_POSTGRES_VOLUME)
    return volume_mount_identity(runner, PRIMARY_POSTGRES_VOLUME)


def observation_plan(pg_id):
    """The exact per-table command plans (machine-checkable allowlist)."""
    plans = {}
    for table in sorted(EXPECTED_PRIMARY_COUNTS):
        plans[table] = ['exec', pg_id, 'psql', *PSQL_ALLOWLIST,
                        'SELECT count(*) FROM %s' % table]
    return plans


def observe_counts(runner, pg_id, console):
    """Counts only. Refuses any non-numeric, extra or failed output."""
    plans = observation_plan(pg_id)
    observed = {}
    for table, args in plans.items():
        if args[:3] != ['exec', pg_id, 'psql'] or tuple(args[3:10]) != PSQL_ALLOWLIST \
                or args[10] != 'SELECT count(*) FROM %s' % table or len(args) != 11:
            raise Refused('observation plan drifted for ' + table)
        console.write('\n$ nerdctl exec <pg> psql -Atc "SELECT count(*) FROM %s"\n'
                      % table)
        console.flush()
        r = run_quiet([COLIMA_BIN, 'nerdctl', '-p', COLIMA_PROFILE, '--', *args],
                      env=None, timeout=90)
        console.write('[exit %d]\n' % r.returncode)
        console.flush()
        if r.returncode or r.stderr.strip():
            raise Refused('count query failed for %s (exit %d)' % (table, r.returncode))
        value = r.stdout.strip()
        if not re.fullmatch(r'[0-9]+', value):
            raise Refused('count output for %s is not a bare count' % table)
        observed[table] = int(value)
    return observed


def compare_counts(observed):
    mismatches = {t: {'expected': EXPECTED_PRIMARY_COUNTS[t][0], 'observed': n}
                  for t, n in observed.items()
                  if n != EXPECTED_PRIMARY_COUNTS[t][0]}
    return mismatches


def preconditions(console):
    assert_packet_dir(DIR)
    bundle = verify_packet_bundle()
    packet_sha = bundle['packet_sha256']
    go = verify_go_receipt(SCRATCH / 'acow-primary-go.txt', packet_sha,
                           GO_PRIMARY_TOKENS, 'primary')

    def git_fn(args):
        r = run_quiet(['/usr/bin/git', '-C', '/Users/tom/Dev/projects/acowtancy', *args],
                      env=git_args(), timeout=30)
        return r.returncode, r.stdout, r.stderr

    pins = verify_consumer_pins(git_fn)
    runner = colima_runner()
    baseline = capture_inventory(runner)
    require_mariadb_unchanged(runner)
    maria_identity = volume_mount_identity(runner, SHARED_MARIADB_VOLUME)
    assert_no_warmup_scope(baseline['networks'])
    pg_id = pick_primary_postgres(runner)
    pg_volume = postgres_volume_identity(runner, pg_id)
    record = {
        'route': 'primary-data',
        'task': 'effigy#540',
        'packet_sha256': packet_sha,
        'manifest_sha256': bundle['manifest_sha256'],
        'go': go,
        'consumer_pins': pins,
        'baseline': baseline,
        'shared_mariadb_volume_identity': maria_identity,
        'primary_postgres': {'id': pg_id, 'volume_identity': pg_volume},
        'expected_counts': {t: EXPECTED_PRIMARY_COUNTS[t][0]
                            for t in sorted(EXPECTED_PRIMARY_COUNTS)},
        'count_sources': {t: '%s:%d' % (EXPECTED_PRIMARY_COUNTS[t][1],
                                        EXPECTED_PRIMARY_COUNTS[t][2])
                          for t in sorted(EXPECTED_PRIMARY_COUNTS)},
    }
    return record, runner


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packet-sha', required=False, default=None,
                        help='optional cross-check; the committed packet is authoritative')
    args = parser.parse_args()
    console = ConsoleLog('acow-primary-console.log')
    exit_code = 0
    try:
        record, runner = preconditions(console)
        if args.packet_sha is not None and args.packet_sha != record['packet_sha256']:
            raise Refused('--packet-sha argument differs from the committed packet')
        pg_id = record['primary_postgres']['id']
        record['observed_counts'] = {}
        record['count_mismatches'] = {}
        record['status'] = 'observation-error'
        try:
            observed = observe_counts(runner, pg_id, console)
            mismatches = compare_counts(observed)
            require_mariadb_unchanged(runner)
            if volume_mount_identity(runner, SHARED_MARIADB_VOLUME) \
                    != record['shared_mariadb_volume_identity']:
                raise Refused('shared MariaDB volume identity changed during observation')
            if pick_primary_postgres(runner) != pg_id:
                raise Refused('primary postgres identity changed during observation')
            record['observed_counts'] = observed
            record['count_mismatches'] = mismatches
            record['status'] = 'counts-match' if not mismatches else 'count-mismatch'
        finally:
            # Counts observed so far are recorded even on a refused
            # observation; the receipt carries statuses and count results
            # only, never row values or raw child output.
            write_receipt('acow-primary-observation-receipt.json', record)
        if record['status'] == 'count-mismatch':
            raise Refused('observed counts differ from the owner baseline: %s'
                          % json.dumps(record['count_mismatches'], sort_keys=True))
    except Refused as e:
        console.write('\nREFUSED: %s\n' % e)
        print('REFUSED: %s' % e, file=sys.stderr)
        exit_code = 3
    console.close()
    if exit_code == 0:
        print('primary-data counts observation complete; counts only; '
              'shared MariaDB unchanged')
    return exit_code


if __name__ == '__main__':
    sys.exit(main())
