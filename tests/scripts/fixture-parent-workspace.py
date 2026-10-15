#!/usr/bin/env python3
"""Execute maintained workspace selectors under the existing private parent.

The integration harness supplies commands by walking the real TOML composition.
Nextest supplies exact compiled membership and retains each terminal case result.
Ordinary assertion failure (100) does not hide subsequent family members. Other
runner failures stop: unknown child effects cannot authorize continued execution.
"""
import json
import os
import re
from pathlib import Path
import shlex
import subprocess
import sys

assert os.environ.get('HOST_RUN_TOKEN'), 'authenticated parent required'
assert os.environ.get('HOST_RUN_ID'), 'authenticated run required'
specification = Path(sys.argv[1]).resolve(strict=True)
evidence = specification.parent
commands = json.loads(specification.read_text())
results = []
failed = False
for index, command in enumerate(commands):
    args = shlex.split(command)
    if args[:3] == ['cargo', 'nextest', 'run']:
        # Keep package/binary/filter selection exactly as maintained. Discovery
        # and execution use the same compiled build and the same nextest profile.
        selection = args[3:args.index('--no-capture')]
        listing = subprocess.run(['cargo', 'nextest', 'list'] + selection +
                                 ['--message-format', 'json'], capture_output=True)
        (evidence / f'{index:02d}-inventory.json').write_bytes(listing.stdout)
        (evidence / f'{index:02d}-inventory.stderr').write_bytes(listing.stderr)
        if listing.returncode:
            raise SystemExit('workspace compiled membership could not be enumerated')
        inventory = json.loads(listing.stdout)
        cases = [(binary, name) for binary, suite in inventory['rust-suites'].items()
                 for name, case in suite['testcases'].items()
                 if case['filter-match']['status'] == 'matches' and not case['ignored']]
        assert cases, ('empty required workspace family', command)
        assert '--no-fail-fast' in args, 'workspace family must not hide later cases'
    else:
        assert args[:2] == ['python3', 'tests/scripts/workspace-catalog-handoff.py'], command
        listing = subprocess.run(args + ['--list'], capture_output=True, check=True)
        (evidence / f'{index:02d}-inventory.json').write_bytes(listing.stdout)
        (evidence / f'{index:02d}-inventory.stderr').write_bytes(listing.stderr)
        cases = [('catalog-handoff', name) for name in json.loads(listing.stdout)]
        assert cases, 'empty required catalog handoff matrix'
    log_path = evidence / f'{index:02d}-execution.log'
    print(f'workspace member {index}: {command}; {len(cases)} compiled cases; log={log_path}', flush=True)
    with log_path.open('wb') as log:
        result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
    terminals_by_case = {}
    for line in log_path.read_text().splitlines():
        catalog = re.match(r'^(PASS|FAIL) catalog handoff case: (.+)$', line)
        if catalog:
            outcome, name = catalog.groups()
            terminals_by_case[('catalog-handoff', name)] = dict(binary='catalog-handoff', test=name, outcome=outcome)
            continue
        match = re.match(r'\s*(PASS|FAIL|TIMEOUT|LEAK|ABORT|XFAIL|XPASS)\s+\[[^]]*\] \(\s*\d+/\d+\) (\S+) (\S+)$', line)
        if match:
            outcome, binary, name = match.groups()
            # Nextest repeats failed cases in its final summary. Retain one
            # result per exact case; the full stream remains in the raw log.
            terminals_by_case[(binary, name)] = dict(binary=binary, test=name, outcome=outcome)
    terminals = list(terminals_by_case.values())
    expected = set(cases)
    observed = {(case['binary'], case['test']) for case in terminals}
    complete = expected == observed and len(terminals) == len(cases)
    results.append(dict(command=command, cases=cases, results=terminals,
                        complete=complete, exit_code=result.returncode, log=log_path.name))
    (evidence / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'workspace member {index}: terminal exit={result.returncode}', flush=True)
    failed |= (result.returncode != 0 or not complete or
               any(case['outcome'] != 'PASS' for case in terminals))
    if result.returncode not in (0, 100):
        break
assert len(results) == len(commands), 'workspace execution stopped with an unsafe or unavailable runner'
raise SystemExit(1 if failed else 0)
