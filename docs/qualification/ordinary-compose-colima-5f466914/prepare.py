#!/usr/bin/env python3
"""Prepare exact #524 r17 qualification synthetic fixtures. RUN ONLY under the packet-specific GO."""
import hashlib
import argparse
import json
import os
os.umask(0o077)
from pathlib import Path
import stat
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from common import scrubbed_env, snapshot, verify_bundle, verify_go, verify_environment

BASE = Path('/Volumes/Builds/scratch/effigy/524')
PARENT = Path('/Users/tom/.effigy-scratch')
BINARY = BASE / 'candidate-src-5f466914/target/debug/effigy'
IMAGE = 'sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1'
PLACEHOLDER = 'example.invalid/effigy-qualification@sha256:' + '0' * 64
CASES = [
    ('under', BASE/'fixture.LVFJv9', PARENT/'524-qual-20261101-lvfx', 'effigy524-synthetic-lvfjv9', 3630),
    ('over', BASE/'fixture.9wOGVj', PARENT/'524-qual-20261101-mixx', 'effigy524-synthetic-9wogvj', 4899),
]
SOURCE_HASHES = {
    'under': {'effigy.toml':'2cc5cc6b7e80a3a28cd36c777d6fc453eb6d9ed63908fe4df7413a0821f6bdc6', 'infra/dev/catalog/fixture/compose.fragment.yml':'9cab3dd8aaa8129d20fe3ff392f28dc716c2af27ec6335dc03a9142dc43819e7', '.effigy/runtime/compose/compose.override.yml':'b93ba571ff7c3d79529d75294a48211becf87cbae8d0c7d32c088fdc8a7dbd7e'},
    'over': {'effigy.toml':'ef3b6ed7c4754e56e6cd074c7ddfdfada2c638a000e92438b22e6707499b138b', 'infra/dev/catalog/fixture/compose.fragment.yml':'34246f1dbb6d4b88d9cd00a8708a6b73f0a386b66b594f2129d4b22c99141951', '.effigy/runtime/compose/compose.override.yml':'2bcec12e354a22f427269f6024692335708af06772285ebefded436da6238ba1'},
}
SERVICE_SHA = '1afa0d2a71174b45a2b86ba97d0d3bd99f51a6d4cd13e137657e29e46b5ca38e'
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def owned_dir(path):
    s=os.lstat(path)
    if not stat.S_ISDIR(s.st_mode) or s.st_uid != os.getuid() or s.st_mode & 0o077:
        raise RuntimeError('unsafe private fixture directory: '+str(path))
def put(path, data):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd=os.open(path, os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
    with os.fdopen(fd,'wb') as out: out.write(data)

parser=argparse.ArgumentParser()
parser.add_argument('--packet-sha',required=True)
parser.add_argument('--go-receipt',required=True)
args=parser.parse_args()
bundle=verify_bundle(args.packet_sha)
go_sha=verify_go(args.go_receipt,args.packet_sha)
environment=verify_environment()
if sha(BINARY) != 'cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1':
    raise RuntimeError('candidate hash mismatch')
for name in ('r17-under-render.json','r17-over-render.json','r17-preparation-receipt.json','r17-under-preflight.log','r17-over-preflight.log','r17-preflight-receipt.json','r17-runtime-up.log','r17-runtime-receipt.json'):
    if os.path.lexists(BASE/name): raise RuntimeError('prior r17 artifact exists; no retry: '+name)
for kind,src,_,_,_ in CASES:
    if sha(src/'infra/dev/catalog/fixture/service.toml') != SERVICE_SHA:
        raise RuntimeError('service input hash mismatch: '+kind)
    for name,expected_sha in SOURCE_HASHES[kind].items():
        if sha(src/name)!=expected_sha: raise RuntimeError('source input hash mismatch: '+kind+'/'+name)
if PARENT.is_symlink(): raise RuntimeError('fixture parent is a symlink')
try:
    os.mkdir(PARENT,0o700)
    parent_created=True
except FileExistsError:
    parent_created=False
owned_dir(PARENT)
if any(os.path.lexists(dst) for _,_,dst,_,_ in CASES):
    raise RuntimeError('an exact fixture path exists; do not adopt or delete it')
report={'bundle':bundle,'go_receipt_sha256':go_sha,'environment':environment,'parent_created':parent_created,'cases':[]}
for kind,src,dst,project,expected in CASES:
    os.mkdir(dst,0o700)
    put(dst/'.effigy-524-r17-owner.json',json.dumps({'kind':kind,'project':project,'head':'5f4669141d8dabd91b7fbfef28d2bce21bc42ba2','root':str(dst)},sort_keys=True).encode()+b'\n')
    for i in range(1,22): (dst/'data'/f'bind-{i:02d}').mkdir(parents=True,exist_ok=True,mode=0o700)
    for name in ('effigy.toml','infra/dev/catalog/fixture/service.toml','infra/dev/catalog/fixture/compose.fragment.yml','.effigy/runtime/compose/compose.override.yml'):
        text=(src/name).read_text().replace(str(src),str(dst)).replace(PLACEHOLDER,IMAGE)
        if name.endswith('compose.fragment.yml'):
            text=text.replace('    image: '+IMAGE,'    network_mode: none\n    image: '+IMAGE)
        put(dst/name,text.encode())
    render_env={'PATH':'/usr/bin:/bin','LANG':'C','EFFIGY_COMPOSE_BACKEND':'containerd'}
    r=subprocess.run([str(BINARY),'container','render','fixture','--json'],cwd=dst,env=render_env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
    if r.returncode: raise RuntimeError('candidate render failed: '+kind)
    render=json.loads(r.stdout)
    if not render.get('ok'): raise RuntimeError('candidate render not ok: '+kind)
    raw=BASE/('r17-'+kind+'-render.json')
    put(raw,r.stdout)
    env=json.loads(next(x['content'] for x in render['result']['files'] if x['name']=='inputs/environment.json'))
    if env['project_name']!=project: raise RuntimeError('rendered project mismatch')
    by_path={x['path']:x['content'] for x in render['result']['files']}
    actual=[]
    for index,name in enumerate(render['result']['compose_files']):
        content=by_path[name]
        if PLACEHOLDER in content: raise RuntimeError('invalid image placeholder in render')
        if index == 0:
            if content.count('    image: '+IMAGE+'\n') != 1 or content.count('    network_mode: none\n') != 1:
                raise RuntimeError('rendered image/network identity mismatch')
        p=dst/f'candidate-rendered-{index}.yml'
        put(p,content.encode())
        actual.append({'path':str(p),'sha256':sha(p)})
    model=subprocess.run([sys.executable,'-I',str(Path(__file__).resolve().parent/'source_estimate.py'),str(raw)],env=scrubbed_env(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=10,check=True)
    estimate=json.loads(model.stdout)
    if estimate['mount_count']!=21 or estimate['estimated_bytes']!=expected:
        raise RuntimeError('unexpected rendered estimate: '+json.dumps(estimate))
    report['cases'].append({'kind':kind,'root':str(dst),'project':project,'estimate':estimate,'render_sha256':sha(raw),'compose':actual,'fixture_manifest':snapshot(dst)})
put(BASE/'r17-preparation-receipt.json',(json.dumps(report,indent=2,sort_keys=True)+'\n').encode())
print(json.dumps(report,indent=2,sort_keys=True))
