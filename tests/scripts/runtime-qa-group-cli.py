#!/usr/bin/env python3
"""Private native proof of retained group members and honest owner loss."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='q010-group-') as temporary:
    base = Path(temporary)
    root, home = base / 'checkout', base / 'home'
    root.mkdir(); home.mkdir(); (root / '.git').mkdir()
    env = {key: value for key, value in os.environ.items() if not key.startswith('EFFIGY_')}
    env.update(HOME=str(home), NO_COLOR='1')
    workload = 'exec ' + shlex.quote(sys.executable) + ' -c ' + shlex.quote(
        "import os,signal,time; from pathlib import Path; "
        "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
        "Path('job-pid').write_text(str(os.getpid())); time.sleep(60)")
    text = '[tasks.ok]\nrun = "printf group-ok"\n[tasks.hold]\nrun = ' + json.dumps(workload) + '\n'
    for group, task in [('pass', 'ok'), ('hold', 'hold')]:
        text += f'''[qa.groups.{group}]
lifecycle = "maintained"
purpose = "Private custody proof"
scope_policy = "advisory"
proof_limits = ["Only this fixture"]
members = [{{id = "member", kind = "proof", surface = "published", task = "{task}", args = [], targets = ["workspace:root"], limits = ["Only this fixture"]}}]
'''
    (root / 'effigy.toml').write_text(text)
    def cli(*args):
        result = subprocess.run([str(binary), '--json', *args], cwd=root, env=env,
                                capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return json.loads(result.stdout)['result']
    passed = cli('tasks', 'qa-group', 'run', 'pass')
    assert passed['outcome'] == 'passed'
    member = cli('tasks', 'status', 'ok')
    assert member['latest'] is not None, 'group member downgraded to memory'
    assert (root / passed['members'][0]['log_ref']).read_text().find('group-ok') >= 0
    assert cli('tasks', 'qa-group', 'status', passed['run_id'])['run']['outcome'] == 'passed'
    child = subprocess.Popen([str(binary), '--json', 'tasks', 'qa-group', 'run', 'hold'],
                             cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    job_pid = None
    started = time.monotonic()
    try:
        deadline = time.monotonic() + 15
        while not (root / 'job-pid').exists():
            assert child.poll() is None, child.communicate()
            assert time.monotonic() < deadline
            time.sleep(.02)
        job_pid = int((root / 'job-pid').read_text())
        records = list((root / '.effigy/runtime/qa-groups').glob('*.json'))
        assert len(records) == 1
        run_id = json.loads(records[0].read_text())['run_id']
        os.kill(child.pid, signal.SIGKILL)  # The exact CLI process started above.
        child.communicate(timeout=10)
        deadline = time.monotonic() + 20
        while True:
            try: os.kill(job_pid, 0)
            except ProcessLookupError: break
            assert time.monotonic() < deadline, 'group member survived owning-client death'
            time.sleep(.03)
        job_pid = None
        status = cli('tasks', 'qa-group', 'status', run_id)
        assert status['run']['state'] == 'unknown', status
        assert status['run'].get('outcome') != 'passed', status
        assert not list(home.rglob('*.db*')), 'group created a central journal'
        print(json.dumps({'schema': 'effigy.proof.runtime-qa-group.v1', 'member_history_retained': True,
                          'group_logs_retained': True, 'owner_loss_state': 'unknown',
                          'member_cleanup_seconds': round(time.monotonic() - started, 3),
                          'central_databases': 0}))
    finally:
        if child.poll() is None:
            child.terminate(); child.communicate(timeout=30)
        if job_pid is not None:
            # The fixture has a finite lifetime; never signal an unverified PID
            # from a file when a proof fails before native cleanup is observed.
            deadline = time.monotonic() + 65
            while True:
                try: os.kill(job_pid, 0)
                except ProcessLookupError: break
                assert time.monotonic() < deadline, 'bounded fixture did not finish'
                time.sleep(.05)
