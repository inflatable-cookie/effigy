#!/usr/bin/env python3
"""Linux syscall accounting for private foreground, named and detached runs."""
import hashlib
import json
import os
from pathlib import Path
import re
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
    def files():
        return {str(p.relative_to(base)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in base.rglob('*.db*') if p.is_file()}
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
    for label in ['foreground_fresh', 'foreground_repeat']:
        results[label] = measure(label, ['short'])
        assert not files(), 'foreground initialized a database'
    for label, args in [('foreground_sequence', ['sequence']), ('foreground_suite', ['test', 'local'])]:
        results[label] = measure(label, args)
        assert not files(), 'local composition initialized a database'
    results['named_fresh'] = measure('named_fresh', ['--run-id', 'named', 'short'])
    before = files()
    results['foreground_warm'] = measure('foreground_warm', ['short'])
    assert files() == before, 'warm foreground changed journal bytes'
    results['named_warm'] = measure('named_warm', ['--run-id', 'named-warm', 'short'])
    results['qa_group_warm'] = measure('qa_group_warm', ['tasks', 'qa-group', 'run', 'local'])
    assert json.loads(cli('--json', 'tasks', 'status', 'short').stdout)['result']['latest'] is not None
    results['detached_warm'] = measure('detached_warm', ['hold', '--detach'], detached=True)
    for label, row in results.items():
        if label.startswith('foreground'):
            assert all(row[key] == 0 for key in ['database_mutation_open_calls', 'database_write_calls',
                       'database_write_bytes', 'database_fsync_calls', 'database_other_mutations']), (label, row)
        else: assert row['database_write_bytes'] > 0, (label, row)
    assert not list(home.rglob('*.db*')), 'checkout runs wrote a machine or legacy database'
    print(json.dumps({'schema': 'effigy.proof.runtime-write-budget.v1',
                      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'syscalls': results}, sort_keys=True))
