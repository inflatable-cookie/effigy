"""One source-immutable Nucleus physical-preparation occurrence after freeze.

A request or an environment variable is not activation. This entry is for the
separately activated Nucleus proof-only run; ordinary workers use native-smoke.
It emits preparation facts and artifacts, never full-session qualification.
"""
import hashlib
import json
from pathlib import Path
import re
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
import common
import linux_capacity_contract as capacity
import linux_diagnostic_proof as diagnostic
import linux_native_assets as assets
import linux_native_build as build
import linux_native_contract as contract
import linux_native_run as native
import linux_native_smoke as smoke

DURATION=5400
SNAPSHOT='d1602e5a-964a-4375-9eec-df6ed4350305'


def request(root, seed, head, activation):
    common.require(re.fullmatch(r'[0-9a-f]{40}',head) and re.fullmatch(r'[A-Za-z0-9_-]{8,64}',activation),
                   'exact source and fresh activation required')
    diagnostic.private_root(str(root))
    common.require(seed.is_absolute() and seed.is_relative_to(root) and seed.resolve(strict=True)==seed,
                   'seed must be a retained owned-root artifact')
    programs={path.name:common.digest(path) for path in Path(__file__).parent.glob('linux_native*.py')}
    programs.update({name:common.digest(Path(__file__).parent/name) for name in
                     ('common.py','runtime.py','probe.py','linux_loop_bound.py','linux_capacity_contract.py')})
    return dict(schema='effigy.linux-physical-preparation-request.v1',source_head=head,root=str(root),
                activation_id=activation,seed=build.file_input(seed),duration_seconds=DURATION,
                vm_uuid=capacity.VM,owned_snapshot=SNAPSHOT,programs=programs)


def execute(path, expected):
    common.require(path.is_absolute() and path.is_relative_to(common.SCRATCH)
                   and path.resolve(strict=True)==path and common.digest(path)==expected,
                   'bound preparation request differs')
    value=common.load(path)
    root,seed=Path(value['root']),Path(value['seed']['path'])
    common.require(value==request(root,seed,value['source_head'],value['activation_id']),
                   'preparation source, resources or request changed')
    common.require(common.source_head(Path(__file__).resolve().parents[3])==value['source_head'],
                   'preparation is not the frozen candidate')
    clean=common.run([diagnostic.GIT,'status','--porcelain']).stdout
    common.require(not clean,'proof preparation requires an immutable clean checkout')
    common.exclusive(root/('physical-preparation-'+value['activation_id']+'.claim'),
                     dict(request_sha256=expected,source_head=value['source_head']))
    attempt=native.Attempt(root,label='physical-preparation-proof')
    deadline=time.monotonic()+DURATION
    def budget(seconds=45):common.require(time.monotonic()+seconds<deadline-120,'preparation budget reserve reached')
    raw=attempt.command([diagnostic.PRLCTL,'list','--info',capacity.VM,'--json'],read_only=True)
    diagnostic.check_vm(raw,'running')
    snapshots=json.loads(attempt.command([diagnostic.PRLCTL,'snapshot-list',capacity.VM,'--json'],read_only=True))
    owned=snapshots.get('{'+SNAPSHOT+'}') if isinstance(snapshots,dict) else None
    common.require(isinstance(owned,dict) and owned.get('name')=='effigy-explore-2026-11-09',
                   'owned rollback snapshot is absent or differs')
    # Stage1 account, private SSH files and installed tools were prepared under
    # the preceding approved exploration. Adopt only their actual exact state;
    # no absent account/tool/config is fabricated or silently installed here.
    ssh=smoke.ssh_prefix(root,attempt)
    budget();smoke.root_resume(root,attempt,ssh)
    native.authorize_channel(root,attempt)
    ssh=smoke.ssh_prefix(root,attempt)
    budget();stage1=smoke.stage1_observation(attempt)
    budget(240);smoke.stop_engine(attempt)
    controls={'engine':smoke.capacity_probe(attempt,ssh,'/var/lib/effigy-829/rootful/client/tmp',3221225472)}
    smoke.root_resume(root,attempt,ssh)
    budget(240);controls['preparation']=smoke.capacity_probe(attempt,ssh,build.BASE+'/artifact-tmp',42949672960)
    budget(1900);binding=build.build(root,'HEAD')
    initial=common.load(common.artifact(value['seed']))
    initial['cell']['source_head']=binding['source_pin']
    budget(2300);prepared=assets.prepare(root,binding,initial)
    budget(240);controls['workspace']=smoke.capacity_probe(attempt,ssh,build.BASE+'/qualification',1073741824,
                                                         image=common.load(prepared)['cell']['runtime_image'])
    budget(1950)
    common.require(native.execute(root,prepared,'acquire')==0,'ordinary native acquisition failed')
    budget(240);cell=common.load(prepared)['cell']
    packet=contract.prepare(root,attempt,ssh,cell,evidence_kind='proof')
    cell['linux_capacity']=common.load(packet)
    driver=str(contract.resources.WORKSPACE.parent/('proof-driver-'+value['activation_id']))
    native.channel_directory(attempt,driver)
    for name in native.FILES:
        native.channel_receive(attempt,ssh,driver+'/'+name,(Path(__file__).parent/name).read_bytes())
    qualified=str(contract.resources.WORKSPACE/('sealed-'+__import__('uuid').uuid4().hex))
    made=native.channel_directory(attempt,qualified)
    cell['qualification_root']=dict(path=qualified,device=made['device'],inode=made['inode'],uid=made['uid'])
    metadata=contract.resources.WORKSPACE.parent/'metadata'
    cell['remote']=dict(packet=driver,inputs=str(metadata/('full-'+value['activation_id']+'.inputs.json')),
                        go_bindings=str(metadata/('full-'+value['activation_id']+'.go-index.json')),
                        selected_go=str(metadata/('full-'+value['activation_id']+'.go.json')))

    cell['native_writer_gate']=cell['linux_capacity']['plan']['phases']['live']['origin']['nonce']
    cell.pop('linux_capacity_preparation',None)
    result=dict(schema='effigy.linux-physical-preparation-complete.v1',source_head=value['source_head'],
                request_sha256=expected,cell=cell,build=binding,preparation=build.file_input(packet),controls=controls,stage1=stage1,
                vm_terminal='running',boot_id=cell['linux_capacity']['plan']['phases']['activation']['boot_id'],
                qualification_credit=False)
    common.exclusive(attempt.root/'complete.json',result)
    print(json.dumps({'preparation':str(attempt.root/'complete.json'),'sha256':common.digest(attempt.root/'complete.json'),
                      'qualification_credit':False}))
    return 0


if __name__=='__main__':
    if len(sys.argv)==6 and sys.argv[1]=='plan':
        print(json.dumps(request(Path(sys.argv[2]),Path(sys.argv[3]),sys.argv[4],sys.argv[5]),indent=2))
    elif len(sys.argv)==3:
        sys.exit(execute(Path(sys.argv[1]),sys.argv[2]))
    else:
        raise SystemExit('usage: physical preparation proof-only REQUEST SHA256 | plan ROOT SEED HEAD ACTIVATION')
