#!/usr/bin/env python3
"""ONE authorized #524 r18 qualification runtime cell; never execute without exact packet GO."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import time
from collections import Counter
os.umask(0o077)
sys.path.insert(0,str(Path(__file__).resolve().parent))
from common import (NETWORK_BASELINE_SHA256, VOLUME_BASELINE_SHA256, canonical_digest, exact_ps_ids,
                    network_snapshot, scrubbed_env, snapshot, validate_deletable, safe_delete,
                    verify_bundle, verify_go, verify_environment, verify_historical_baseline, volume_snapshot)

BASE=Path('/Volumes/Builds/scratch/effigy/524')
DIR=Path(__file__).resolve().parent
PACKET=DIR/'colima-qualification-5f466914-r18.md'
PROJECT='effigy524-synthetic-lvfjv9'
UNDER=Path('/Users/tom/.effigy-scratch/524-qual-20261101-lvfx')
OVER=Path('/Users/tom/.effigy-scratch/524-qual-20261101-mixx')
IMAGE='sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1'
IMAGE_ID='sha256:6cf065f724d522cbb3e1898559cd88f015db8a1a4210d2d048a0853b69b57ba9'
EXPECTED_BASELINE_SHA='d89c846cdb44c0d9669f4db50eb57347d2b322aee26b4b03c7fbd2e000663f10'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def run(args, timeout=20, check=True):
    try:
        r=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=timeout,env=scrubbed_env(with_home=True))
    except (OSError,subprocess.TimeoutExpired):
        raise RuntimeError('read-only or exact cleanup command unavailable or timed out') from None
    if check and r.returncode: raise RuntimeError('read-only or exact cleanup command failed, status='+str(r.returncode))
    return r
def nerd(*args): return ['/opt/homebrew/bin/colima','nerdctl','-p','effigy','--',*args]
def ids():
    return exact_ps_ids(run(nerd('ps','-a','--no-trunc','--format','{{.ID}}')).stdout)
def project_ids(project):
    return exact_ps_ids(run(nerd('ps','-a','--no-trunc','--filter','label=com.docker.compose.project='+project,'--format','{{.ID}}')).stdout)
# Full network/volume identity snapshots and the canonical digest moved to
# common.py (task effigy#542) so the launcher's entry sweep shares their exact
# parsing without importing this executable main block.
snapshot_hash=canonical_digest
def store_evidence(name,data):
    path=BASE/name
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0),0o600)
    with os.fdopen(fd,'wb') as out:
        out.write(data); out.flush(); os.fsync(out.fileno())
    return hashlib.sha256(data).hexdigest()
def json_type(v):
    if v is None:return 'null'
    if type(v) is bool:return 'boolean'
    if type(v) in (int,float):return 'number'
    if type(v) is str:return 'string'
    if type(v) is list:return 'array'
    if type(v) is dict:return 'object'
    return 'other'
def byte_length(v):
    return len(v.encode('utf-8')) if type(v) is str else len(json.dumps(v,ensure_ascii=True,separators=(',',':')).encode())
KNOWN_KEYS={'Type','Source','Target','Destination','Options','RW','Mode','Name','Driver','Propagation','nerdctl/mounts','nerdctl/name','nerdctl/namespace','nerdctl/managed','com.effigy.checkout','com.docker.compose.project','com.docker.compose.service','com.docker.compose.version','com.docker.compose.container-number','com.docker.compose.config-hash','com.docker.compose.oneoff','com.docker.compose.project.working_dir','com.docker.compose.project.config_files','com.docker.compose.image','com.docker.compose.depends_on','com.docker.compose.replace','com.docker.compose.project.environment_file'}
def field_name(key):
    return key if type(key) is str and key in KNOWN_KEYS else '<other-key>'
def reject_nonfinite(value):
    raise ValueError('non-finite JSON constant')
def path_class(value):
    if type(value) is not str:return {'class':'non-string','length_bytes':byte_length(value)}
    length=len(value.encode())
    if value.startswith(str(UNDER)+'/data/bind-'): c='synthetic-A'
    elif value.startswith('/workspace-root/synthetic/'): c='synthetic-B'
    elif value=='/etc' or value.startswith('/etc/'):c='etc'
    elif value=='/run' or value.startswith('/run/'):c='run'
    elif value.startswith('/var/lib/nerdctl/') and '/volumes/' in value:c='anonymous-volume-like'
    else:c='other'
    return {'class':c,'length_bytes':length}
def record_structure(row):
    if type(row) is not dict:return {'row_type':json_type(row),'row_bytes':byte_length(row),'length_basis':'utf8-value' if type(row) is str else 'reserialized-compact-json'}
    fields=[]
    for key,value in sorted(row.items(),key=lambda x:str(x[0])):
        out={'key':field_name(key),'key_length_bytes':len(key.encode()) if type(key) is str else None,'json_type':json_type(value),'length_bytes':byte_length(value),'length_basis':'utf8-value' if type(value) is str else 'reserialized-compact-json'}
        if key in ('Source','Target','Destination'):out['path']=path_class(value)
        elif key in ('Type','Mode'):
            vocab={'bind','tmpfs','volume'} if key=='Type' else {'','ro','rw'}
            out['closed_value']=value if type(value) is str and value in vocab else 'other'
        elif key=='RW':out['closed_value']=value if type(value) is bool else 'other'
        fields.append(out)
    return {'row_type':'object','unknown_key_count':sum(1 for field in fields if field['key']=='<other-key>'),'fields':fields}
def unique_pairs(pairs):
    result={}
    for key,value in pairs:
        if key in result:raise ValueError('duplicate JSON field')
        result[key]=value
    return result
def valid_label_shape(item):
    return type(item) is dict and all(type(item.get(key)) is str for key in ('Type','Source','Destination','Mode','Propagation')) and type(item.get('RW')) is bool
def valid_mount_shape(item):
    return type(item) is dict and all(type(item.get(key)) is str for key in ('Type','Source','Destination','Mode','Propagation')) and type(item.get('RW')) is bool
def label_mounts_parity(raw,rows):
    try:
        label=json.loads(raw,object_pairs_hook=unique_pairs,parse_constant=reject_nonfinite)
        if type(label) is not list or type(rows) is not list or not all(valid_label_shape(x) for x in label) or not all(valid_mount_shape(x) for x in rows):return False
        return Counter((row['Destination'],row['RW']) for row in label)==Counter((row['Destination'],row['RW']) for row in rows)
    except (ValueError,TypeError,OverflowError,RecursionError):return False
def mount_label_structure(raw):
    parsed=json.loads(raw,object_pairs_hook=unique_pairs,parse_constant=reject_nonfinite)
    if type(parsed) is not list:return {'status':'not-array','label_value_bytes':len(raw.encode()),'entries':[]}
    decoder=json.JSONDecoder(object_pairs_hook=unique_pairs,parse_constant=reject_nonfinite);i=0;n=len(raw);costs=[]
    def skip(j):
        while j<n and raw[j] in ' \t\r\n':j+=1
        return j
    i=skip(i)+1
    for _ in parsed:
        i=skip(i);start=i;_,i=decoder.raw_decode(raw,i);costs.append(len(raw[start:i].encode()));i=skip(i)
        if i<n and raw[i]==',':i+=1
    return {'status':'ok','label_value_bytes':len(raw.encode()),'label_key_bytes':len('nerdctl/mounts'),'entry_count':len(parsed),'typed_shape_valid':all(valid_label_shape(row) for row in parsed),'entries':[{'json_bytes':cost,'structure':record_structure(row)} for cost,row in zip(costs,parsed)]}
def structure_from_rows(rows):
    if type(rows) is not list:return {'status':'not-array','rows':[]}
    return {'status':'ok','row_count':len(rows),'rows':[record_structure(row) for row in rows]}
def mounts_match(mounts):
    expected=[f'/workspace-root/synthetic/{i:02d} '+('false' if i==9 else 'true') for i in range(1,22)]
    return len(mounts)==21 and Counter(mounts)==Counter(expected)
def classify_network_change(before,after,project):
    if any(after.get(name)!=ident for name,ident in before.items()):
        raise RuntimeError('existing network removed or identity replaced')
    added=sorted(set(after)-set(before))
    if added not in ([],[project+'_default']):
        raise RuntimeError('unrelated/unexpected new network')
    return added
def owned(root,kind):
    st=os.lstat(root)
    if not stat.S_ISDIR(st.st_mode) or st.st_uid!=os.getuid() or st.st_mode&0o077:
        raise RuntimeError('unsafe fixture root')
    m=json.loads((root/'.effigy-524-r18-owner.json').read_text())
    if m.get('root')!=str(root) or m.get('kind')!=kind or m.get('head')!='5f4669141d8dabd91b7fbfef28d2bce21bc42ba2':
        raise RuntimeError('fixture ownership marker mismatch')

parser=argparse.ArgumentParser()
parser.add_argument('--packet-sha',required=True)
parser.add_argument('--go-receipt',required=True)
args=parser.parse_args()
bundle=verify_bundle(args.packet_sha)
go_sha=verify_go(args.go_receipt,args.packet_sha)
environment=verify_environment()
out=BASE/'r18-runtime-receipt.json'
if out.exists() or (BASE/'r18-runtime-up.log').exists() or any(os.path.lexists(BASE/name) for name in ('r18-structure-only.json',)): raise RuntimeError('runtime cell already attempted or evidence exists; no retry')
if sha(BASE/'r3-baseline-container-ids.txt')!=EXPECTED_BASELINE_SHA: raise RuntimeError('baseline receipt changed')
baseline=exact_ps_ids((BASE/'r3-baseline-container-ids.txt').read_text())
if len(baseline)!=15 or ids()!=baseline: raise RuntimeError('15-container baseline drift')
owned(UNDER,'under'); owned(OVER,'over')
prep=json.loads((BASE/'r18-preparation-receipt.json').read_text())
proof=json.loads((BASE/'r18-preflight-receipt.json').read_text())
prep_sweep=prep.get('entry_sweep') if isinstance(prep,dict) else None
if (not isinstance(prep_sweep,dict)
    or prep_sweep.get('environment')!=environment
    or prep_sweep.get('historical_baseline')!=verify_historical_baseline()
    or prep_sweep.get('resource_baseline',{}).get('network_identity_sha256')!=NETWORK_BASELINE_SHA256
    or prep_sweep.get('resource_baseline',{}).get('volume_identity_sha256')!=VOLUME_BASELINE_SHA256
    or prep_sweep.get('fixture_roots_absent')!={'fixture_roots_absent':True}
    or prep_sweep.get('fixture_ancestors_safe')!={'fixture_ancestors_safe':True,'mount_tables_parsed':True}
    or prep_sweep.get('prior_artifacts_absent')!={'prior_artifacts_absent':True}):
    raise RuntimeError('missing or mismatched preparation entry sweep')
if prep['bundle']!=bundle or proof['bundle']!=bundle or prep['go_receipt_sha256']!=go_sha or proof['go_receipt_sha256']!=go_sha or proof['environment']!=environment or {x['case'] for x in proof['cases']}!={'under','over'}: raise RuntimeError('missing or mismatched preflight cells')
if sha(BASE/'r18-preparation-receipt.json')!=proof['preparation_receipt_sha256']: raise RuntimeError('preparation receipt changed')
for x in proof['cases']:
    if sha(BASE/('r18-'+x['case']+'-preflight.log'))!=x['output_sha256']: raise RuntimeError('preflight receipt mismatch')
under=next(x for x in prep['cases'] if x['kind']=='under')
over=next(x for x in prep['cases'] if x['kind']=='over')
if under['root']!=str(UNDER) or under['project']!=PROJECT or under['estimate']['estimated_bytes']!=3630 or under['estimate']['mount_count']!=21:
    raise RuntimeError('under fixture mismatch')
if snapshot(UNDER)!=under['fixture_manifest'] or snapshot(OVER)!=over['fixture_manifest']: raise RuntimeError('fixture manifest changed')
for x in under['compose']:
    p=Path(x['path'])
    if p.parent!=UNDER or p.is_symlink() or sha(p)!=x['sha256']: raise RuntimeError('rendered compose changed')
version=run(nerd('version')).stdout
compose_version=run(nerd('compose','version')).stdout
if 'v2.3.1' not in version or 'v2.3.1' not in compose_version: raise RuntimeError('nerdctl version changed')
host_version=run(['/opt/homebrew/bin/colima','version']).stdout
status=run(['/opt/homebrew/bin/colima','status','-p','effigy'])
if '0.10.3' not in host_version or 'is running' not in (status.stdout+status.stderr) or 'containerd' not in (status.stdout+status.stderr):
    raise RuntimeError('Colima version/profile not as pinned')
img=run(nerd('image','inspect',IMAGE,'--format','{{.Id}} {{.Architecture}}')).stdout.strip()
if img!=IMAGE_ID+' arm64': raise RuntimeError('offline image identity changed')
present=run(['/opt/homebrew/bin/colima','ssh','-p','effigy','--','test','-d',str(UNDER)],check=False)
if present.returncode: raise RuntimeError('fixture path not shared with guest')
if project_ids(PROJECT): raise RuntimeError('project already has containers')
net_before=network_snapshot(); vol_before=volume_snapshot()
if PROJECT+'_default' in net_before: raise RuntimeError('project network already exists')
# Entry-baseline chain: the launcher's sweep pinned these exact identities
# before any fixture creation; the mutation boundary re-asserts them before up.
if canonical_digest(net_before)!=NETWORK_BASELINE_SHA256 or canonical_digest(vol_before)!=VOLUME_BASELINE_SHA256:
    raise RuntimeError('network/volume identities differ from the pinned r18 entry baseline')
compose=[]
for x in under['compose']: compose += ['-f',x['path']]
cmd=nerd('compose',*compose,'-p',PROJECT,'up','--no-build','--pull','never','--no-recreate','-d')
# Recheck artifact/head/profile/GO and all identities immediately before spawn.
# These sequential observations are not an atomic global lock; the approved
# quiet-window coordination remains an entry requirement.
if verify_bundle(args.packet_sha)!=bundle or verify_go(args.go_receipt,args.packet_sha)!=go_sha or verify_environment()!=environment:
    raise RuntimeError('pre-exec packet/GO/environment changed')
if volume_snapshot()!=vol_before or network_snapshot()!=net_before or ids()!=baseline or project_ids(PROJECT):
    raise RuntimeError('pre-exec resource identity drift')
outcome={'supervisor_disposition':'quiet window by Acow confirmation (no stack/reservation), no other actor assumed; observation scope = Colima effigy guest root namespace via SSH + engine view; no claim that no other process holds our roots; consequences analysed: none destructive to third parties','command':cmd,'bundle':bundle,'go_receipt_sha256':go_sha,'go_authenticity':'operator must verify source; file is not cryptographic authorization','profile':'effigy','colima_version':'0.10.3','nerdctl_version':'2.3.1','image_reference':IMAGE,'image_id':IMAGE_ID,'architecture':'arm64','network_ids_before':sorted(net_before.values()),'network_identity_before_sha256':snapshot_hash(net_before),'volume_identity_before_sha256':snapshot_hash(vol_before),'started_at_epoch':time.time(),'baseline_count':15,'baseline_sha256':EXPECTED_BASELINE_SHA,'result':'started','container_id':None}
with (BASE/'r18-runtime-up.log').open('xb') as log:
    p=subprocess.Popen(cmd,stdout=log,stderr=subprocess.STDOUT,start_new_session=True,env=scrubbed_env(with_home=True))
    outcome['owned_pid']=p.pid
    outcome['owned_pgid']=os.getpgid(p.pid)
    out.write_text(json.dumps(outcome,indent=2)+'\n')
    try: code=p.wait(timeout=120)
    except subprocess.TimeoutExpired:
        # Leader is still our unreaped child, and start_new_session gives it a private group.
        if p.poll() is None and os.getpgid(p.pid)==p.pid:
            os.killpg(p.pid,signal.SIGTERM)
        try: p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            if p.poll() is None and os.getpgid(p.pid)==p.pid: os.killpg(p.pid,signal.SIGKILL)
            try: p.wait(timeout=5)
            except subprocess.TimeoutExpired: pass
        outcome['result']='unknown: timeout; do not assume VM operation stopped; inspect exact owned identities before cleanup'
        out.write_text(json.dumps(outcome,indent=2)+'\n')
        sys.exit(2)
outcome['exit_code']=code
if code:
    outcome['result']='failed: inspect log and project state; no automatic cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n')
    sys.exit(1)
found=project_ids(PROJECT)
if len(found)!=1 or found[0] in baseline:
    outcome['result']='unknown: project did not produce exactly one new identified container'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
cid=found[0]; outcome['container_id']=cid
outcome['result']='container identified; awaiting oracle and exact cleanup'
out.write_text(json.dumps(outcome,indent=2)+'\n')
# Only schema, types, lengths and tiny closed vocabulary are persisted.
# Raw native output remains in memory and is never written to a log or receipt.
diagnostic={'status':'complete','label':None,'mount_rows':None,'label_keys':None}
raw_rows=None; raw_label=None
try:
    raw_label=run(nerd('inspect','--mode','native','--format','{{index .Labels "nerdctl/mounts"}}',cid)).stdout.rstrip('\n')
    if len(raw_label.encode())>1048576:diagnostic['label']={'status':'too-large','length_bytes':len(raw_label.encode())}
    else:
        try: diagnostic['label']=mount_label_structure(raw_label)
        except (ValueError,TypeError,OverflowError,RecursionError):diagnostic['label']={'status':'invalid-json','length_bytes':len(raw_label.encode())}
except (RuntimeError,OSError,subprocess.TimeoutExpired):diagnostic['label']={'status':'unavailable'}
try:
    raw_rows=run(nerd('inspect','--format','{{json .Mounts}}',cid)).stdout.rstrip('\n')
    if len(raw_rows.encode())>1048576:diagnostic['mount_rows']={'status':'too-large','length_bytes':len(raw_rows.encode())};row_objects=None
    else:
        try:row_objects=json.loads(raw_rows,object_pairs_hook=unique_pairs,parse_constant=reject_nonfinite);diagnostic['mount_rows']=structure_from_rows(row_objects)
        except (ValueError,TypeError,OverflowError,RecursionError):row_objects=None;diagnostic['mount_rows']={'status':'invalid-json','length_bytes':len(raw_rows.encode())}
except (RuntimeError,OSError,subprocess.TimeoutExpired):row_objects=None;diagnostic['mount_rows']={'status':'unavailable'}
try:
    keys=run(nerd('inspect','--mode','native','--format','{{range $key, $value := .Labels}}{{$key}}{{"\\n"}}{{end}}',cid)).stdout.splitlines()
    diagnostic['label_keys']={'status':'ok','count':len(keys),'keys':sorted(field_name(k) for k in keys),'unknown_key_count':sum(field_name(k)=='<other-key>' for k in keys),'unknown_key_lengths_bytes':sorted(len(k.encode()) for k in keys if field_name(k)=='<other-key>')}
except (RuntimeError,OSError,subprocess.TimeoutExpired):diagnostic['label_keys']={'status':'unavailable'}
diagnostic['closed_vocabulary_counts']={}
for group in (diagnostic['label'].get('entries',[]),diagnostic['mount_rows'].get('rows',[])):
    for item in group:
        row=item.get('structure',item)
        for field in row.get('fields',[]):
            if field['key'] in ('Type','Mode','RW'):
                value=field.get('closed_value','other');value=str(value).lower() if type(value) is bool else value
                bucket=diagnostic['closed_vocabulary_counts'].setdefault(field['key'],{})
                bucket[value]=bucket.get(value,0)+1
if raw_label is None or type(row_objects) is not list or diagnostic['label'].get('status')!='ok' or diagnostic['label'].get('entry_count',0)<1 or diagnostic['label'].get('typed_shape_valid') is not True or diagnostic['mount_rows'].get('status')!='ok' or not all(valid_mount_shape(row) for row in row_objects) or diagnostic['label_keys'].get('status')!='ok' or 'nerdctl/mounts' not in diagnostic['label_keys'].get('keys',[]):diagnostic['status']='incomplete'
evidence_persisted=False
try:
    data=(json.dumps(diagnostic,indent=2,sort_keys=True)+'\n').encode()
    outcome['diagnostic_evidence_sha256']={'r18-structure-only.json':store_evidence('r18-structure-only.json',data)}
    evidence_persisted=True
except (OSError,RuntimeError,TypeError,ValueError,OverflowError,RecursionError):
    outcome['diagnostic_persistence']='failed; cleanup still required'
outcome['label_key_plus_value_bytes']=None if raw_label is None else len('nerdctl/mounts')+len(raw_label.encode())
mounts=[str(x.get('Destination',''))+' '+str(x.get('RW')).lower() for x in row_objects if type(x) is dict] if type(row_objects) is list else []
outcome['mount_count']=len(row_objects) if type(row_objects) is list else None
outcome['expected_mounts_preserved']=type(row_objects) is list and mounts_match(mounts)
outcome['qualification_scope']='synthetic Colima mount cell only; consumer verification and merge hold remain'
try:out.write_text(json.dumps(outcome,indent=2)+'\n')
except OSError:outcome['diagnostic_receipt_write']='failed; cleanup still required'
net_after=network_snapshot(); vol_after=volume_snapshot()
outcome['network_identity_during_sha256']=snapshot_hash(net_after)
outcome['volume_identity_during_sha256']=snapshot_hash(vol_after)
try:
    outcome['new_network_names']=classify_network_change(net_before,net_after,PROJECT)
except RuntimeError:
    outcome['result']='unknown: unrelated/replaced/removed network; no resource cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
outcome['volume_identities_unchanged']=vol_after==vol_before
outcome['owned_volumes_created']=[]  # 21 binds only; any volume delta prevents cleanup.
unchanged_existing_networks=all(net_after.get(name)==ident for name,ident in net_before.items())
outcome['diagnostic_complete_and_persisted']=diagnostic['status']=='complete' and evidence_persisted
outcome['label_mounts_destination_rw_parity']=label_mounts_parity(raw_label,row_objects) if raw_label is not None else False
outcome['model_estimated_bytes']=under['estimate']['estimated_bytes']
outcome['model_guarded_bytes']=under['estimate']['guarded_bytes']
outcome['model_estimate_covers_actual']=outcome['label_key_plus_value_bytes'] is not None and outcome['model_estimated_bytes']>=outcome['label_key_plus_value_bytes']
outcome['label_entry_count']=diagnostic['label'].get('entry_count')
outcome['oracle_pass']=outcome['label_mounts_destination_rw_parity'] and outcome['label_entry_count']==21 and outcome['diagnostic_complete_and_persisted'] and outcome['label_key_plus_value_bytes'] is not None and outcome['label_key_plus_value_bytes']<4096 and outcome['model_estimate_covers_actual'] and outcome['expected_mounts_preserved'] and outcome['volume_identities_unchanged'] and unchanged_existing_networks
# Classify EVERY new network before deleting even our own container. Only the
# unique project-default network is eligible, and only after full-ID/label proof.
if outcome['new_network_names']:
    detail=run(nerd('network','inspect',PROJECT+'_default','--format','{{.Id}} {{index .Labels "com.docker.compose.project"}}')).stdout.strip().split()
    if len(detail)!=2 or not re.fullmatch(r'[0-9a-f]{64}',detail[0]) or detail[1]!=PROJECT or net_after.get(PROJECT+'_default')!=detail[0]:
        outcome['result']='unknown: network identity/ownership unverified; no resource cleanup'
        out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
    outcome['owned_network_id']=detail[0]
owner=run(nerd('inspect','--format','{{index .Config.Labels "com.docker.compose.project"}}',cid)).stdout.strip()
if owner!=PROJECT:
    outcome['result']='unknown: container owner label mismatch; no cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
if ids()!=sorted(baseline+[cid]) or project_ids(PROJECT)!=[cid] or volume_snapshot()!=vol_before or not unchanged_existing_networks or network_snapshot()!=net_after:
    outcome['result']='unknown: resource identity changed before cleanup; no cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
run(nerd('rm','-f',cid))
if exact_ps_ids(run(nerd('ps','-a','--no-trunc','--filter','id='+cid,'--format','{{.ID}}')).stdout): raise RuntimeError('removed container ID still listed')
outcome['post_container_ids_unchanged']=ids()==baseline
if not outcome['post_container_ids_unchanged']:
    outcome['result']='unknown: baseline changed after exact-container cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
if outcome['new_network_names']:
    if ids()!=baseline or project_ids(PROJECT) or volume_snapshot()!=vol_before or network_snapshot()!=net_after:
        outcome['result']='unknown: resource identity changed before network cleanup; no network cleanup'
        out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
    detail=run(nerd('network','inspect',PROJECT+'_default','--format','{{.Id}} {{index .Labels "com.docker.compose.project"}}')).stdout.strip().split()
    if len(detail)!=2 or not re.fullmatch(r'[0-9a-f]{64}',detail[0]) or detail[1]!=PROJECT or outcome['owned_network_id']!=detail[0] or net_after.get(PROJECT+'_default')!=detail[0]:
        outcome['result']='unknown: network identity/ownership unverified; no network cleanup'
        out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
    outcome['owned_network_id']=detail[0]
    out.write_text(json.dumps(outcome,indent=2)+'\n')
    run(nerd('network','rm',detail[0]))
net_final=network_snapshot(); vol_final=volume_snapshot()
outcome['network_identity_after_sha256']=snapshot_hash(net_final)
outcome['volume_identity_after_sha256']=snapshot_hash(vol_final)
outcome['post_network_identities_unchanged']=net_final==net_before
outcome['post_volume_identities_unchanged']=vol_final==vol_before
if not outcome['post_network_identities_unchanged'] or not outcome['post_volume_identities_unchanged']:
    outcome['result']='unknown: network/volume identity changed; no broad cleanup'
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
try:
    for root,kind,info in ((UNDER,'under',under),(OVER,'over',over)):
        owned(root,kind)
        if root.is_symlink(): raise RuntimeError('fixture root replaced')
        validate_deletable(root,info['fixture_manifest'])
    for root,kind,info in ((UNDER,'under',under),(OVER,'over',over)):
        safe_delete(root,info['fixture_manifest'])
except (RuntimeError,OSError,subprocess.TimeoutExpired):
    outcome['result']='unknown: fixture manifest or mountpoint verification failed; partial fixture cleanup is possible; stop'
    outcome['fixture_roots_present']={str(root):os.path.lexists(root) for root in (UNDER,OVER)}
    out.write_text(json.dumps(outcome,indent=2)+'\n'); sys.exit(2)
if prep.get('parent_created'):
    try: os.rmdir(UNDER.parent)
    except OSError: pass
outcome['fixtures_absent']=not os.path.lexists(UNDER) and not os.path.lexists(OVER)
outcome['final_container_ids_unchanged']=ids()==baseline
outcome['final_network_identities_unchanged']=network_snapshot()==net_before
outcome['final_volume_identities_unchanged']=volume_snapshot()==vol_before
all_final=outcome['final_container_ids_unchanged'] and outcome['final_network_identities_unchanged'] and outcome['final_volume_identities_unchanged']
outcome['result']='pass' if outcome['oracle_pass'] and outcome['fixtures_absent'] and all_final else 'unknown: final resource drift or failed runtime oracle'
out.write_text(json.dumps(outcome,indent=2)+'\n')
print(json.dumps(outcome,indent=2))
sys.exit(0 if outcome['result']=='pass' else 1)
