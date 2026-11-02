#!/usr/bin/env python3
"""One exact-GO diagnostic sequence; derives SHA from packet and copied GO.

Task effigy#537 committed packet route. The exact Queue proof selector is
`python3 -I -B docs/qualification/ordinary-compose-colima-5f466914/launch.py`,
run from the reviewed task-head checkout. Evidence receipts stay in the
scratch qualification directory named by the packet.
"""
import hashlib,json,os,re,subprocess,sys
from pathlib import Path
os.umask(0o077)
DIR=Path(__file__).resolve().parent
BASE=Path('/Volumes/Builds/scratch/effigy/524')
if (DIR.name,DIR.parent.name,DIR.parent.parent.name)!=('ordinary-compose-colima-5f466914','qualification','docs'):
    raise RuntimeError('launcher must run from docs/qualification/ordinary-compose-colima-5f466914')
packet=DIR/'colima-qualification-5f466914-r17.md';go=BASE/'r17-go.txt'
actual=hashlib.sha256(packet.read_bytes()).hexdigest();text=go.read_text();match=re.search(r'packet ([0-9a-f]{64})',text)
if not match or actual!=match.group(1):raise RuntimeError('packet hash differs from copied GO')
packet_text=packet.read_text();mh=re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`',packet_text)
manifest=DIR/'r17-artifacts.json'
if not mh or hashlib.sha256(manifest.read_bytes()).hexdigest()!=mh.group(1):raise RuntimeError('manifest hash differs')
artifacts=json.loads(manifest.read_text());expected={'candidate-src-5f466914/target/debug/effigy','r3-baseline-container-ids.txt'}|{'launch.py','common.py','prepare.py','preflight.py','runtime.py','source_estimate.py'}
if set(artifacts)!=expected:raise RuntimeError('artifact set differs')
HELPERS={'launch.py','common.py','prepare.py','preflight.py','runtime.py','source_estimate.py'}
for name,want in artifacts.items():
 # Helper scripts are verified as committed in this task head; the frozen
 # candidate binary and the container baseline stay in scratch.
 path=(DIR/name) if name in HELPERS else (BASE/name)
 if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest()!=want:raise RuntimeError('artifact hash differs')
print('computed packet SHA:',actual,'GO packet SHA:',match.group(1),flush=True)
commands=[[sys.executable,'-I','-B',str(DIR/(step+'.py')),'--packet-sha',actual,'--go-receipt',str(go)] for step in ('prepare','preflight','runtime')]
fd=os.open(BASE/'r17-invocation-receipt.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
with os.fdopen(fd,'w') as f:json.dump({'computed_packet_sha':actual,'go_packet_sha':match.group(1),'commands':commands},f,indent=2);f.write('\n')
for step,cmd in zip(('prepare','preflight','runtime'),commands):
 fd=os.open(BASE/('r17-'+step+'-console.log'),os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
 with os.fdopen(fd,'w') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT)
 print(step,'exit',r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
