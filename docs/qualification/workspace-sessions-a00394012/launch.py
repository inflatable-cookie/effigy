#!/usr/bin/env python3
"""One exact-GO diagnostic sequence for one backend packet; derives SHA from packet and copied GO.

Task effigy#530 committed proof route. The exact Queue proof selectors are
`python3 -I -B docs/qualification/workspace-sessions-a00394012/launch.py --backend colima|desktop|linux`,
run from the reviewed task-head checkout. One execution per backend packet;
evidence receipts stay in the scratch qualification directory named by the
packet. A missing or mismatched GO receipt stops before any step.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

os.umask(0o077)
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import DIR, BASE, BACKENDS, packet_path, go_path  # noqa: E402

parser = __import__('argparse').ArgumentParser()
parser.add_argument('--backend', required=True, choices=BACKENDS)
args = parser.parse_args()
backend = args.backend

if (DIR.name, DIR.parent.name, DIR.parent.parent.name) != \
        ('workspace-sessions-a00394012', 'qualification', 'docs'):
    raise RuntimeError('launcher must run from docs/qualification/workspace-sessions-a00394012')

packet = packet_path(backend)
go = go_path(backend)
actual = hashlib.sha256(packet.read_bytes()).hexdigest()
text = go.read_text()
match = re.search(r'packet ([0-9a-f]{64})', text)
if not match or actual != match.group(1):
    raise RuntimeError('packet hash differs from copied GO')

packet_text = packet.read_text()
mh = re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`', packet_text)
manifest = DIR / backend / 'manifest-r1.json'
if not mh or hashlib.sha256(manifest.read_bytes()).hexdigest() != mh.group(1):
    raise RuntimeError('manifest hash differs')

print('computed packet SHA:', actual, 'GO packet SHA:', match.group(1), flush=True)
commands = [[sys.executable, '-I', '-B', str(DIR / (step + '.py')),
             '--backend', backend, '--packet-sha', actual, '--go-receipt', str(go)]
            for step in ('prepare', 'preflight', 'runtime')]
receipt = {'backend': backend, 'computed_packet_sha': actual, 'go_packet_sha': match.group(1),
           'commands': commands}
fd = os.open(BASE / ('530-' + backend + '-invocation-receipt.json'),
             os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
with os.fdopen(fd, 'w') as f:
    json.dump(receipt, f, indent=2, sort_keys=True)
    f.write('\n')
for step, cmd in zip(('prepare', 'preflight', 'runtime'), commands):
    fd = os.open(BASE / ('530-' + backend + '-' + step + '-console.log'),
                 os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'w') as f:
        r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT)
    print(step, 'exit', r.returncode, flush=True)
    if r.returncode:
        raise SystemExit(r.returncode)
