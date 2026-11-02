#!/usr/bin/env python3
"""Fail-closed candidate preflight controls. RUN ONLY after r18_prepare under GO."""
import hashlib
import argparse
import json
import os
os.umask(0o077)
from pathlib import Path
import subprocess
import sys
import tempfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
from common import (NETWORK_BASELINE_SHA256, RECOVERY_RECEIPT_SHA256, VOLUME_BASELINE_SHA256,
                    snapshot, verify_bundle, verify_go, verify_environment, verify_historical_baseline)

BASE=Path('/Volumes/Builds/scratch/effigy/524')
BINARY=BASE/'candidate-src-5f466914/target/debug/effigy'
CASES=[('over',Path('/Users/tom/.effigy-scratch/524-qual-20261101-mixx')),('under',Path('/Users/tom/.effigy-scratch/524-qual-20261101-lvfx'))]
parser=argparse.ArgumentParser()
parser.add_argument('--packet-sha',required=True)
parser.add_argument('--go-receipt',required=True)
args=parser.parse_args()
bundle=verify_bundle(args.packet_sha)
go_sha=verify_go(args.go_receipt,args.packet_sha)
environment=verify_environment()
if hashlib.sha256(BINARY.read_bytes()).hexdigest()!='cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1':
    raise RuntimeError('candidate hash mismatch')
receipt=json.loads((BASE/'r18-preparation-receipt.json').read_text())
sweep=receipt.get('entry_sweep') if isinstance(receipt,dict) else None
if (not isinstance(sweep,dict)
    or sweep.get('environment')!=environment
    or sweep.get('historical_baseline')!=verify_historical_baseline()
    or sweep.get('resource_baseline',{}).get('network_identity_sha256')!=NETWORK_BASELINE_SHA256
    or sweep.get('resource_baseline',{}).get('volume_identity_sha256')!=VOLUME_BASELINE_SHA256
    or sweep.get('fixture_roots_absent')!={'fixture_roots_absent':True}
    or sweep.get('fixture_ancestors_safe')!={'fixture_ancestors_safe':True,'mount_tables_parsed':True}
    or sweep.get('prior_artifacts_absent')!={'prior_artifacts_absent':True}):
    raise RuntimeError('preparation entry sweep missing or mismatched')
if receipt['bundle']!=bundle or receipt['go_receipt_sha256']!=go_sha: raise RuntimeError('preparation bundle/GO differs')
preparation_sha=hashlib.sha256((BASE/'r18-preparation-receipt.json').read_bytes()).hexdigest()
proof=[]
for kind,root in CASES:
    info=next(x for x in receipt['cases'] if x['kind']==kind)
    if info['root']!=str(root) or not (root/'.effigy-524-r18-owner.json').is_file():
        raise RuntimeError('fixture identity mismatch')
    if snapshot(root)!=info['fixture_manifest']: raise RuntimeError('fixture changed before preflight')
    if any(hashlib.sha256(Path(x['path']).read_bytes()).hexdigest()!=x['sha256'] for x in info['compose']):
        raise RuntimeError('candidate rendered input changed')
    with tempfile.TemporaryDirectory(prefix='r18-stub-',dir=BASE) as name:
        stub=Path(name)
        log=stub/'invocations'
        for program in ('colima','docker','nerdctl'):
            p=stub/program
            p.write_text('#!/bin/sh\nprintf "'+program+'\\n" >> '+str(log)+'\nexit 113\n')
            p.chmod(0o700)
        env={'PATH':str(stub),'LANG':'C','EFFIGY_COMPOSE_BACKEND':'containerd'}
        r=subprocess.run([str(BINARY),'container','fixture','shell','--command','exit 113'],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
        output=r.stdout.decode('utf-8','replace')
        calls=log.read_text().splitlines() if log.exists() else []
        if kind=='over':
            if r.returncode==0 or calls or 'estimated nerdctl/mounts label size of 4899 bytes plus a 2% reserve (98 bytes) = 4997 bytes across 21' not in output:
                raise RuntimeError('over-budget preflight did not fail closed')
        else:
            if r.returncode==0 or calls!=['colima'] or 'runtime is not available' not in output:
                raise RuntimeError('under-budget preflight did not reach only inert Colima')
        out=BASE/('r18-'+kind+'-preflight.log')
        with out.open('xb') as f: f.write(r.stdout)
        row={'case':kind,'exit':r.returncode,'inert_calls':calls,'output_sha256':hashlib.sha256(r.stdout).hexdigest()}
        if snapshot(root)!=info['fixture_manifest']: raise RuntimeError('fixture changed during preflight')
        proof.append(row)
        print(json.dumps(row))
with (BASE/'r18-preflight-receipt.json').open('x') as out:
    json.dump({'bundle':bundle,'go_receipt_sha256':go_sha,'environment':environment,'preparation_receipt_sha256':preparation_sha,'cases':proof},out,indent=2)
