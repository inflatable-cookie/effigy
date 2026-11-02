#!/usr/bin/env python3
"""One exact-GO diagnostic sequence; derives SHA from packet and copied GO.

Task effigy#542 revision r18: the launcher itself performs the complete
read-only entry sweep - packet/manifest/helper/GO identities, profile and
image pins, the exact 15-container baseline, absence of every synthetic
container/network/fixture root, the full 36-network and 255-volume identity
baselines pinned from the immutable recovery evidence, protected shared
identities, fixture-ancestor mount/symlink safety and prior-evidence absence -
BEFORE prepare can create any fixture or any native mutation can start. The
exact Queue proof selector stays
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
sys.path.insert(0,str(DIR))
import common
REVISION='r18'
HELPERS={'launch.py','common.py','prepare.py','preflight.py','runtime.py','source_estimate.py'}

def load_bundle():
    packet=DIR/('colima-qualification-5f466914-'+REVISION+'.md');go=BASE/(REVISION+'-go.txt')
    actual=hashlib.sha256(packet.read_bytes()).hexdigest();text=go.read_text();match=re.search(r'packet ([0-9a-f]{64})',text)
    if not match or actual!=match.group(1):raise RuntimeError('packet hash differs from copied GO')
    packet_text=packet.read_text();mh=re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`',packet_text)
    manifest=DIR/(REVISION+'-artifacts.json')
    if not mh or hashlib.sha256(manifest.read_bytes()).hexdigest()!=mh.group(1):raise RuntimeError('manifest hash differs')
    artifacts=json.loads(manifest.read_text());expected=HELPERS|{'candidate-src-5f466914/target/debug/effigy','r3-baseline-container-ids.txt'}
    if set(artifacts)!=expected:raise RuntimeError('artifact set differs')
    for name,want in artifacts.items():
        # Helper scripts are verified as committed in this task head; the frozen
        # candidate binary and the container baseline stay in scratch.
        path=(DIR/name) if name in HELPERS else (BASE/name)
        if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest()!=want:raise RuntimeError('artifact hash differs')
    return actual,match.group(1)

def main():
    actual,go_packet=load_bundle()
    print('computed packet SHA:',actual,'GO packet SHA:',go_packet,flush=True)
    # Complete read-only entry sweep; refuses before any fixture creation,
    # receipt write or native mutation if any predicate fails.
    bundle,go_sha,sweep=common.entry_sweep(actual,BASE/(REVISION+'-go.txt'))
    commands=[[sys.executable,'-I','-B',str(DIR/(step+'.py')),'--packet-sha',actual,'--go-receipt',str(BASE/(REVISION+'-go.txt'))] for step in ('prepare','preflight','runtime')]
    fd=os.open(BASE/(REVISION+'-invocation-receipt.json'),os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
    with os.fdopen(fd,'w') as f:json.dump({'revision':REVISION,'computed_packet_sha':actual,'go_packet_sha':go_packet,'go_receipt_sha256':go_sha,'entry_sweep':sweep,'commands':commands},f,indent=2);f.write('\n')
    for step,cmd in zip(('prepare','preflight','runtime'),commands):
        fd=os.open(BASE/(REVISION+'-'+step+'-console.log'),os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
        with os.fdopen(fd,'w') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT)
        print(step,'exit',r.returncode,flush=True)
        if r.returncode:raise SystemExit(r.returncode)

if __name__=='__main__':
    main()
