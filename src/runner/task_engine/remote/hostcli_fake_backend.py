"""Strict owned HostCli backend for the task-engine source fixture only."""
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

root = Path(__file__).resolve().parent.parent
state_path = root / 'guest.json'
args = sys.argv[1:]
fd = os.open(root / 'backend-argv.jsonl', os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
with os.fdopen(fd, 'a') as log:
    log.write(json.dumps(args, separators=(',', ':')) + '\n')
if args[:2] != ['--context', 'fixture']:
    raise SystemExit(90)
args = args[2:]
full_id = 'a' * 64


def identity(pid):
    proc = Path('/proc/%d/stat' % pid)
    if proc.is_file():
        text = proc.read_text()
        fields = text[text.rfind(')') + 2:].split()
        return fields[19], fields[0]
    result = subprocess.run(['/bin/ps', '-p', str(pid), '-o', 'lstart=', '-o', 'stat='],
                            stdout=subprocess.PIPE, check=False, text=True)
    if result.returncode:
        return None, None
    parts = result.stdout.strip().rsplit(None, 1)
    return (parts[0], parts[1][0]) if len(parts) == 2 else (None, None)


def live(record, pid_key='pid', start_key='host_start'):
    start, state = identity(record[pid_key])
    return start == record[start_key] and state not in (None, 'Z', 'X')


if args == ['inspect', full_id]:
    print(json.dumps([{'Id': full_id, 'Created': 'fixture-created', 'State': {'Running': True}}]))
    raise SystemExit(0)
if not args or args[0] != 'exec':
    raise SystemExit(91)
if len(args) >= 10 and args[:7] == ['exec', '-i', '-u', '0', full_id, 'sh', '-c'] and args[8] == 'effigy-custody':
    if args[7] != (root / 'expected-helper').read_text():
        raise SystemExit(98)
    op = args[9:]
    if os.environ.get('EFFIGY_SIGNAL_FIXTURE_RECOVERY') == '1':
        with (root / 'recovery-calls').open('a') as log:
            log.write((op[0] if op else 'missing') + '\n')
    nonce = sys.stdin.readline().strip()
    if op == ['boot']:
        print('fixture-boot')
        raise SystemExit(0)
    record = json.loads(state_path.read_text()) if state_path.exists() else None
    active = record is not None and live(record)
    grandchild = record is not None and live(record, 'grandchild_pid', 'grandchild_start')
    if op == ['snap']:
        print('boot fixture-boot\nns pid:[401]')
        if active:
            mark = 'y' if nonce == record['nonce'] else 'n'
            pid = record['pid']
            print('p %d S 1 %d %d 100 %s' % (pid, pid, pid, mark))
        if grandchild:
            mark = 'y' if nonce == record['nonce'] else 'n'
            print('p %d S %d %d %d 101 %s' % (record['grandchild_pid'], record['pid'],
                                               record['pid'], record['pid'], mark))
        raise SystemExit(0)
    if len(op) == 8 and op[:3] == ['kill', 'fixture-boot', 'pid:[401]']:
        if not active:
            print('gone')
        elif (nonce != record['nonce'] or op[4] != 'session'
              or op[5:8] != [str(record['pid']), str(record['pid']), '100']):
            print('moved')
        else:
            os.killpg(record['pid'], {'TERM': signal.SIGTERM, 'CONT': signal.SIGCONT,
                                      'KILL': signal.SIGKILL}[op[3]])
            print('sent')
        raise SystemExit(0)
    if len(op) == 7 and op[:3] == ['kill', 'fixture-boot', 'pid:[401]'] and op[4] == 'member':
        if not grandchild:
            print('gone')
        elif nonce != record['nonce'] or op[5:7] != [str(record['grandchild_pid']), '101']:
            print('moved')
        else:
            os.kill(record['grandchild_pid'], {'TERM': signal.SIGTERM, 'CONT': signal.SIGCONT,
                                               'KILL': signal.SIGKILL}[op[3]])
            print('sent')
        raise SystemExit(0)
    raise SystemExit(92)
if (len(args) < 14 or args[:8] != ['exec', '-w', '/workspace', '--env-file', '/dev/fd/4', full_id, 'sh', '-c']
        or args[8] != (root / 'expected-wrapper').read_text() or args[9] != 'sh'
        or args[10] != (root / 'expected-register').read_text()
        or args[-3:] != ['sh', '-lc', 'sleep 300']):
    raise SystemExit(93)
with os.fdopen(4) as envfile:
    variables = dict(line.rstrip('\n').split('=', 1) for line in envfile if '=' in line)
nonce = variables.get('EFFIGY_CUSTODY_NONCE')
if not nonce or len(nonce) != 64:
    raise SystemExit(94)
guest_code = """import json,os,signal,subprocess,sys,time
from pathlib import Path
root=Path(sys.argv[1]); pid=os.getpid()
def start_of(target):
    proc=Path('/proc/%d/stat' % target)
    if proc.is_file():
        text=proc.read_text(); return text[text.rfind(')')+2:].split()[19]
    result=subprocess.run(['/bin/ps','-p',str(target),'-o','lstart='],stdout=subprocess.PIPE,check=True,text=True)
    return result.stdout.strip()
start=start_of(pid)
def finish(_sig,_frame):
    time.sleep(0.25)
    (root/'guest-exit').write_text('exit=0\\n')
    raise SystemExit(0)
signal.signal(signal.SIGTERM,finish)
fd=os.open(root/'leader.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'w') as stream: json.dump({'pid':pid,'host_start':start},stream)
grandchild_code='''import json,os,signal,subprocess,sys
from pathlib import Path
root=Path(sys.argv[1]); pid=os.getpid(); proc=Path('/proc/%d/stat' % pid)
if proc.is_file():
    text=proc.read_text(); start=text[text.rfind(')')+2:].split()[19]
else:
    start=subprocess.run(['/bin/ps','-p',str(pid),'-o','lstart='],stdout=subprocess.PIPE,check=True,text=True).stdout.strip()
def finish(_sig,_frame):
    (root/'grandchild-exit').write_text('exit=0\\\\n')
    raise SystemExit(0)
signal.signal(signal.SIGTERM,finish)
fd=os.open(root/'grandchild.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'w') as stream: json.dump({'pid':pid,'host_start':start},stream)
(root/'grandchild-ready').write_text('ready\\\\n')
while True: signal.pause()
'''
grandchild=subprocess.Popen([sys.executable,'-c',grandchild_code,str(root)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
until=time.monotonic()+5
while not (root/'grandchild-ready').exists():
    if grandchild.poll() is not None or time.monotonic()>=until: raise SystemExit(97)
    time.sleep(0.01)
grandchild_start=start_of(grandchild.pid)
fd=os.open(root/'guest.pending',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'w') as stream:
    json.dump({'pid':pid,'host_start':start,'grandchild_pid':grandchild.pid,'grandchild_start':grandchild_start,'nonce':os.environ['EFFIGY_CUSTODY_NONCE']},stream)
os.replace(root/'guest.pending',root/'guest.json')
(root/'guest-ready').write_text('ready\\n')
while True: time.sleep(1)
"""
guest_env = os.environ.copy()
guest_env.update(variables)
guest = subprocess.Popen([sys.executable, '-c', guest_code, str(root)], env=guest_env, start_new_session=True,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL)
deadline = time.monotonic() + 5
while not (root / 'guest-ready').exists():
    if guest.poll() is not None or time.monotonic() >= deadline:
        raise SystemExit(95)
    time.sleep(0.01)
record = json.loads(state_path.read_text())
if record['pid'] != guest.pid or not live(record):
    raise SystemExit(96)
sys.stderr.write('\x1eEFFIGY-CUSTODY v1 %s %d %d 100 fixture-boot 401\n' %
                 (nonce, guest.pid, guest.pid))
sys.stderr.flush()
while live(record) or live(record, 'grandchild_pid', 'grandchild_start'):
    time.sleep(0.02)
