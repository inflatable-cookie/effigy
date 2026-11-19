#!/usr/bin/env python3
"""Linux syscall accounting for private foreground, named and detached runs."""
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='q010-write-budget-') as temporary:
    base = Path(temporary)
    home, project = base / 'home', base / 'checkout'
    home.mkdir(); project.mkdir(); (project / '.git').mkdir()
    (project / 'effigy.toml').write_text('''[tasks.short]
run = "true"
scratch = false
[tasks.sequence]
run = [{task = "short"}, {run = "true"}]
[test.suites.local]
run = "true"
[qa.groups.local]
lifecycle = "maintained"
purpose = "Private physical write proof"
scope_policy = "advisory"
proof_limits = ["Only this fixture"]
members = [{id = "member", kind = "proof", surface = "published", task = "short", args = [], targets = ["workspace:root"], limits = ["Only this fixture"]}]
[tasks.hold]
mode = "tui"
scratch = false
concurrent = [{name = "job", run = "touch ready; sleep 60"}]
''')
    env = {key: value for key, value in os.environ.items() if not key.startswith('EFFIGY_')}
    env.update(HOME=str(home), NO_COLOR='1')
    def cli(*args, check=True):
        result = subprocess.run([str(binary), *args], cwd=project, env=env,
                                capture_output=True, text=True, timeout=45)
        if check: assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result
    def journal_rows():
        journal = project / '.effigy/state/kernel.db'
        with closing(sqlite3.connect(f'{journal.as_uri()}?mode=ro', uri=True)) as db:
            tables = ('foreground_terminals', 'runs', 'logs', 'objects')
            return tuple(db.execute(f'SELECT count(*) FROM {table}').fetchone()[0]
                         for table in tables)
    def measure(label, args, detached=False):
        prefix = base / label
        argv = ['strace', '-ff', '-yy', '-s', '32', '-o', str(prefix), '-e',
                'trace=openat,openat2,pwrite64,write,writev,fdatasync,fsync,ftruncate,truncate,unlink,unlinkat,rename,renameat,renameat2',
                str(binary), *args]
        if detached:
            marker = project / 'ready'
            marker.unlink(missing_ok=True)
            trace = subprocess.Popen(argv, cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 30
                while not marker.exists():
                    assert trace.poll() is None, trace.communicate()
                    assert time.monotonic() < deadline, 'detached readiness timeout'
                    time.sleep(.02)
                # Include the separate controller's durable stop request in
                # the same lifecycle budget, alongside traced supervisor IO.
                stop_argv = argv[:]
                stop_argv[stop_argv.index('-o') + 1] = str(prefix) + '.stop'
                stop_argv = stop_argv[:stop_argv.index(str(binary)) + 1] + ['hold', 'stop']
                stopped = subprocess.run(stop_argv, cwd=project, env=env, capture_output=True, timeout=45)
                assert stopped.returncode == 0, (stopped.stdout, stopped.stderr)
                out, err = trace.communicate(timeout=45)
                assert trace.returncode == 0, (out, err)
            finally:
                cli('hold', 'stop', check=False)
                if trace.poll() is None:
                    trace.terminate(); trace.communicate(timeout=45)
        else:
            result = subprocess.run(argv, cwd=project, env=env, capture_output=True, timeout=60)
            assert result.returncode == 0, (result.stdout, result.stderr)
        counts = dict(database_open_calls=0, database_mutation_open_calls=0,
                      database_write_calls=0, database_write_bytes=0, database_fsync_calls=0,
                      database_other_mutations=0, total_fsync_calls=0,
                      gate_write_calls=0, gate_write_bytes=0)
        traces = list(base.glob(label + '.*'))
        assert traces, 'strace produced no receipts'
        for file in traces:
            for line in file.read_text().splitlines():
                match = re.search(r'\b(openat2?|pwrite64|writev?|fdatasync|fsync|ftruncate|truncate|unlinkat?|renameat2?|rename)\(', line)
                returned = re.search(r'=\s+(-?\d+)(?:<[^>]*>)?\s*$', line)
                if not match or not returned or int(returned[1]) < 0: continue
                call, result = match[1], int(returned[1])
                if call in ['fsync', 'fdatasync']: counts['total_fsync_calls'] += 1
                if call in ['write', 'writev', 'pwrite64'] and '/gates/' in line:
                    counts['gate_write_calls'] += 1
                    counts['gate_write_bytes'] += result
                if not re.search(r'/(?:kernel|machine)\.db(?:-(?:wal|shm|journal))?\b', line): continue
                if call.startswith('openat'):
                    counts['database_open_calls'] += 1
                    if any(flag in line for flag in ['O_RDWR', 'O_WRONLY', 'O_CREAT', 'O_TRUNC']):
                        counts['database_mutation_open_calls'] += 1
                elif call in ['write', 'writev', 'pwrite64']:
                    counts['database_write_calls'] += 1
                    counts['database_write_bytes'] += result
                elif call in ['fsync', 'fdatasync']: counts['database_fsync_calls'] += 1
                else: counts['database_other_mutations'] += 1
        counts['trace_files'] = len(traces)
        return counts
    results = {}
    for count, label in enumerate(['foreground_fresh', 'foreground_repeat'], start=1):
        results[label] = measure(label, ['short'])
        assert journal_rows() == (count, 0, 0, 0)
    for label, args in [('foreground_sequence', ['sequence']), ('foreground_suite', ['test', 'local'])]:
        before = journal_rows()
        results[label] = measure(label, args)
        after = journal_rows()
        assert after[0] > before[0] and after[1:] == (0, 0, 0), (label, before, after)
    results['named_fresh'] = measure('named_fresh', ['--run-id', 'named', 'short'])
    before = journal_rows()
    results['foreground_warm'] = measure('foreground_warm', ['short'])
    assert journal_rows() == (before[0] + 1, *before[1:]), before
    results['named_warm'] = measure('named_warm', ['--run-id', 'named-warm', 'short'])
    results['qa_group_warm'] = measure('qa_group_warm', ['tasks', 'qa-group', 'run', 'local'])
    assert json.loads(cli('--json', 'tasks', 'status', 'short').stdout)['result']['latest'] is not None
    results['detached_warm'] = measure('detached_warm', ['hold', '--detach'], detached=True)
    for label, row in results.items():
        assert row['database_write_bytes'] > 0, (label, row)
    assert not list(home.rglob('*.db*')), 'checkout runs wrote a machine or legacy database'
    print(json.dumps({'schema': 'effigy.proof.runtime-write-budget.v1',
                      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'syscalls': results}, sort_keys=True))
