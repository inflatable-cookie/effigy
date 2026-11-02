"""#524 r18 qualification explicit synthetic-fixture inventory and fd-relative cleanup.

Task effigy#542: this revision adds the complete read-only admission sweep
(`entry_sweep`) that the launcher performs BEFORE any synthetic fixture
creation or native mutation: exact packet/manifest/helper/GO/binary/tool
identities, the 15-container baseline, absence of every exact-owned synthetic
container, network and fixture root, the full 36-network and 255-volume
identity baselines pinned from the immutable 2026-11-02 Colima recovery
evidence, the protected shared MariaDB and Acow dev identities, and bounded
fixture-ancestor mount/symlink safety. The network/volume snapshot helpers
moved here from runtime.py so the entry sweep shares their exact parsing
without importing runtime.py's executable main block.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess

BASE=Path('/Volumes/Builds/scratch/effigy/524')
# Task effigy#537 packet-only PR: this module runs from the committed
# qualification directory of the reviewed task head; scratch-only evidence and
# frozen artifacts (candidate binary, baseline IDs, GO receipt, receipts) stay
# under BASE and are pinned by hash.
DIR=Path(__file__).resolve().parent
PACKET=DIR/'colima-qualification-5f466914-r18.md'
# Frozen candidate subject: a detached, clean scratch checkout pinned to the
# exact source commit; its quick/dev build produced the manifest's binary.
HEAD='5f4669141d8dabd91b7fbfef28d2bce21bc42ba2'
WORKTREE=Path('/Volumes/Builds/scratch/effigy/524/candidate-src-5f466914')
# r18 entry baseline, pinned from the immutable recovery evidence only (Tom
# decision2bba81cd, closure.md and recovery-receipt.json of 2026-11-02); never
# recomputed from live state. The canonical digest serialization is exactly the
# r13/r17 receipt serialization: sha256 over json.dumps(map, sort_keys=True,
# separators=(',',':')) of the complete name->identity maps.
RECOVERY_DIR=Path('/Volumes/Builds/scratch/effigy/colima-recovery-20261102')
RECOVERY_RECEIPT='recovery-receipt.json'
RECOVERY_INVENTORY='post-start-readonly-inventory.json'
RECOVERY_RECEIPT_SHA256='5d05e690db01b08765421267b9d7c629dabc2f5a48ad85f3ed3b38ce94226145'
RECOVERY_INVENTORY_SHA256='33239d9583325cdc87e4b60cbae2d6089fd15dc4c7ed8378b13aea77e73cb49b'
EXPECTED_NETWORK_COUNT=36
EXPECTED_VOLUME_COUNT=255
NETWORK_BASELINE_SHA256='6e8fe0fedf99ac009d10ddb3e6115c049079cf5ed83cafebfb89eae98e42e5b6'
VOLUME_BASELINE_SHA256='7168fb1f6ca9735ea801c6977b767124bfc308f7c76d0176e678bf26b5a7eed9'
MARIADB_CONTAINER_ID='c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9'
PROTECTED_VOLUMES={
 'effigy-shared-mariadb-0721de3e-mysql-data':['/var/lib/nerdctl/1935db59/volumes/default/effigy-shared-mariadb-0721de3e-mysql-data/_data','64769:3932411:999:999:755:directory'],
 'acowtancy-dev-postgres-data':['/var/lib/nerdctl/1935db59/volumes/default/acowtancy-dev-postgres-data/_data','64769:1835021:70:70:700:directory'],
 'acowtancy-dev-silo-data':['/var/lib/nerdctl/1935db59/volumes/default/acowtancy-dev-silo-data/_data','64769:3934136:0:0:700:directory'],
 'acowtancy-dev-dbgate-data':['/var/lib/nerdctl/1935db59/volumes/default/acowtancy-dev-dbgate-data/_data','64769:1835015:0:0:700:directory'],
}
# Exact synthetic fixture paths this packet may create; entry checks run
# before any of them exists, and a mount at or inside them refuses entry.
FIXTURE_PARENT=Path('/Users/tom/.effigy-scratch')
FIXTURE_ROOTS=[Path('/Users/tom/.effigy-scratch/524-qual-20261101-lvfx'),Path('/Users/tom/.effigy-scratch/524-qual-20261101-mixx')]
# r18 execution evidence whose prior existence stops the one permitted run.
PRIOR_EVIDENCE=('r18-under-render.json','r18-over-render.json','r18-preparation-receipt.json','r18-under-preflight.log','r18-over-preflight.log','r18-preflight-receipt.json','r18-runtime-up.log','r18-runtime-receipt.json','r18-structure-only.json')

def scrubbed_env(with_home=False):
    env={'PATH':'/opt/homebrew/bin:/usr/bin:/bin','LANG':'C'}
    if with_home:
        # Preserve the real HOME solely for Colima's existing profile/SSH state;
        # never forward arbitrary ambient variables or change its value.
        if os.environ.get('HOME')!='/Users/tom': raise RuntimeError('unexpected real HOME')
        env['HOME']=os.environ['HOME']
        for key in ('USER','LOGNAME'):
            if key in os.environ: env[key]=os.environ[key]
    return env

def _spawn(args,*,env=None,timeout=30):
    # Single bounded-subprocess seam for every read-only native call in this
    # module; only the fake-only fixture tests replace it, production never does.
    return subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=timeout,env=env)

def invoke(args, *, env=None, timeout=30):
    r=_spawn(args,env=env,timeout=timeout)
    if r.returncode: raise RuntimeError('required check failed: '+Path(args[0]).name+' status '+str(r.returncode))
    return r.stdout

def exact_ps_ids(output):
    """All nonempty rows must be canonical, distinct full container IDs."""
    rows=[line for line in output.splitlines() if line!='']
    if any(not re.fullmatch(r'[0-9a-f]{64}',line) for line in rows) or len(rows)!=len(set(rows)):
        raise RuntimeError('container inventory contains malformed or duplicate ID')
    return sorted(rows)

def verify_bundle(packet_sha):
    if not re.fullmatch(r'[0-9a-f]{64}',packet_sha) or digest(PACKET)!=packet_sha:
        raise RuntimeError('packet SHA mismatch')
    text=PACKET.read_text()
    match=re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`',text)
    if not match: raise RuntimeError('packet lacks artifact manifest hash')
    manifest=DIR/'r18-artifacts.json'
    if digest(manifest)!=match.group(1): raise RuntimeError('artifact manifest SHA mismatch')
    obj=json.loads(manifest.read_text())
    helpers={'launch.py','common.py','prepare.py','preflight.py','runtime.py','source_estimate.py'}
    expected=helpers|{'candidate-src-5f466914/target/debug/effigy','r3-baseline-container-ids.txt'}
    if set(obj)!=expected: raise RuntimeError('artifact manifest set mismatch')
    for name,want in obj.items():
        # Helper scripts are verified as committed in this task head; the
        # frozen candidate binary and the container baseline stay in scratch.
        path=(DIR/name) if name in helpers else (BASE/name)
        if path.is_symlink() or digest(path)!=want: raise RuntimeError('artifact SHA mismatch: '+name)
    git_env={'PATH':'/usr/bin:/bin','LANG':'C','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'/dev/null','GIT_TERMINAL_PROMPT':'0'}
    head=invoke(['/usr/bin/git','-C',str(WORKTREE),'rev-parse','HEAD'],env=git_env).strip()
    dirty=invoke(['/usr/bin/git','-C',str(WORKTREE),'status','--porcelain=v1'],env=git_env).strip()
    # Task effigy#537 guard change (Queue planner a6994b17): the r17
    # origin/main ls-remote equality predicate is replaced by content
    # verification of the frozen candidate — exact commit, clean source, the
    # binary hash and the packet/manifest/helper hashes checked above. Main has
    # moved past the candidate since pinning; that movement is disclosed in the
    # packet and does not invalidate the intentionally frozen subject. No
    # remote is consulted at proof time.
    if head!=HEAD or dirty:
        raise RuntimeError('candidate source head changed or worktree dirty')
    return {'packet_sha256':packet_sha,'artifact_manifest_sha256':match.group(1),'artifact_hashes':obj,'candidate_sha256':obj['candidate-src-5f466914/target/debug/effigy'],'source_head':head}

def verify_go(go_path,packet_sha):
    p=Path(go_path)
    if p!=BASE/'r18-go.txt' or p.is_symlink(): raise RuntimeError('exact operator-copied GO receipt missing')
    fd=os.open(p,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_CLOEXEC',0))
    try:
        st=os.fstat(fd)
        if not stat.S_ISREG(st.st_mode) or st.st_uid!=os.getuid() or st.st_nlink!=1 or st.st_mode&0o077 or st.st_size>8192:
            raise RuntimeError('GO receipt mode/owner/type invalid')
        data=os.read(fd,8193)
    finally: os.close(fd)
    text=data.decode('utf-8')
    if packet_sha not in text or 'GO' not in text or 'Supervisor' not in text:
        raise RuntimeError('GO receipt does not name supervisor GO and exact packet SHA')
    # This is only a hash of the operator-copied record, not authentication.
    return hashlib.sha256(data).hexdigest()

def verify_environment():
    env=scrubbed_env(with_home=True)
    def colima(*args, both=False):
        r=_spawn(['/opt/homebrew/bin/colima',*args],env=env,timeout=30)
        if r.returncode: raise RuntimeError('Colima read-only preflight failed: '+str(r.returncode))
        return r.stdout+'\n'+r.stderr if both else r.stdout
    host=colima('version')
    status=colima('status','-p','effigy',both=True)
    ver=colima('nerdctl','-p','effigy','--','version')
    compose=colima('nerdctl','-p','effigy','--','compose','version')
    image=colima('nerdctl','-p','effigy','--','image','inspect','sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1','--format','{{.Id}} {{.Architecture}}').strip()
    shared=colima('ssh','-p','effigy','--','test','-d','/Users/tom')
    del shared
    if '0.10.3' not in host or 'is running' not in status or 'containerd' not in status or 'v2.3.1' not in ver or 'v2.3.1' not in compose or image!='sha256:6cf065f724d522cbb3e1898559cd88f015db8a1a4210d2d048a0853b69b57ba9 arm64':
        raise RuntimeError('profile/version/image preflight mismatch')
    ids=exact_ps_ids(colima('nerdctl','-p','effigy','--','ps','-a','--no-trunc','--format','{{.ID}}'))
    baseline=exact_ps_ids((BASE/'r3-baseline-container-ids.txt').read_text())
    if len(ids)!=15 or ids!=baseline: raise RuntimeError('15-container baseline drift')
    projects=('effigy524-synthetic-lvfjv9','effigy524-synthetic-9wogvj')
    for project in projects:
        output=colima('nerdctl','-p','effigy','--','ps','-a','--no-trunc','--filter','label=com.docker.compose.project='+project,'--format','{{.ID}}')
        if exact_ps_ids(output):
            raise RuntimeError('synthetic project already has containers')
    networks=set(colima('nerdctl','-p','effigy','--','network','ls','--format','{{.Name}}').splitlines())
    if any(project+'_default' in networks for project in projects):
        raise RuntimeError('synthetic project network already exists')
    return {'profile':'effigy','colima_version':'0.10.3','nerdctl_version':'2.3.1','image_reference':'sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1','image_id':'sha256:6cf065f724d522cbb3e1898559cd88f015db8a1a4210d2d048a0853b69b57ba9','architecture':'arm64','shared_parent':'/Users/tom','synthetic_projects_absent':True,'container_baseline_sha256':digest(BASE/'r3-baseline-container-ids.txt')}

def digest(path):
    h=hashlib.sha256()
    with open(path,'rb') as f:
        for data in iter(lambda:f.read(65536),b''): h.update(data)
    return h.hexdigest()

def meta(path, rel):
    s=os.lstat(path)
    if s.st_uid!=os.getuid(): raise RuntimeError('foreign-owned fixture entry: '+rel)
    row={'rel':rel,'dev':s.st_dev,'ino':s.st_ino,'mode':stat.S_IMODE(s.st_mode),'uid':s.st_uid}
    if stat.S_ISDIR(s.st_mode): row['kind']='dir'
    elif stat.S_ISREG(s.st_mode):
        if s.st_nlink!=1: raise RuntimeError('hard-linked fixture file: '+rel)
        fd=os.open(path,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_CLOEXEC',0))
        try:
            fst=os.fstat(fd)
            if (fst.st_dev,fst.st_ino)!=(s.st_dev,s.st_ino) or not stat.S_ISREG(fst.st_mode):
                raise RuntimeError('fixture file replaced: '+rel)
            h=hashlib.sha256()
            while True:
                chunk=os.read(fd,65536)
                if not chunk: break
                h.update(chunk)
            after=os.fstat(fd)
            if (after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns)!=(fst.st_dev,fst.st_ino,fst.st_size,fst.st_mtime_ns):
                raise RuntimeError('fixture file changed while reading: '+rel)
            row.update(kind='file',size=fst.st_size,sha256=h.hexdigest())
        finally: os.close(fd)
    else: raise RuntimeError('unexpected fixture entry type: '+rel)
    return row

def snapshot(root):
    root=Path(root)
    rows=[meta(root,'.')]
    for here,dirs,files in os.walk(root,topdown=True,followlinks=False):
        for name in sorted(dirs+files):
            p=Path(here)/name
            rel=p.relative_to(root).as_posix()
            rows.append(meta(p,rel))
    return sorted(rows,key=lambda r:r['rel'])

def host_mounts():
    """Bounded host mount table; every row must be unambiguous."""
    host=invoke(['/sbin/mount'],env=scrubbed_env())
    rows=[]
    for line in host.splitlines():
        if ' on ' not in line or ' (' not in line: raise RuntimeError('host mount output ambiguous')
        rows.append(line.split(' on ',1)[1].rsplit(' (',1)[0])
    return rows

def guest_mounts():
    """Bounded guest mountinfo; every row must be unambiguous."""
    guest=invoke(['/opt/homebrew/bin/colima','ssh','-p','effigy','--','cat','/proc/self/mountinfo'],env=scrubbed_env(with_home=True),timeout=30)
    rows=[]
    for line in guest.splitlines():
        fields=line.split()
        if len(fields)<10 or '-' not in fields: raise RuntimeError('guest mountinfo ambiguous')
        value=re.sub(r'\\([0-7]{3})',lambda m:chr(int(m.group(1),8)),fields[4])
        if '\\' in value:raise RuntimeError('guest mountpoint escape ambiguous')
        rows.append(value)
    return rows

def validate_deletable(root, expected):
    root=Path(root)
    if snapshot(root)!=expected: raise RuntimeError('fixture tree differs from exact manifest; no deletion')
    # Reject symlink ancestors and any host or guest mountpoint at or below root.
    current=Path('/')
    for part in root.parts[1:]:
        current=current/part
        if stat.S_ISLNK(os.lstat(current).st_mode):raise RuntimeError('symlink in fixture ancestry')
    if any(m==str(root) or m.startswith(str(root)+'/') for m in host_mounts()+guest_mounts()):
        raise RuntimeError('fixture contains a mountpoint')

def safe_delete(root, expected):
    root=Path(root)
    validate_deletable(root,expected)
    byrel={r['rel']:r for r in expected}
    flags=os.O_RDONLY|os.O_DIRECTORY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_CLOEXEC',0)
    pfd=os.open(root.parent,flags)
    rootfd=None
    try:
        before=os.stat(root.name,dir_fd=pfd,follow_symlinks=False)
        rr=byrel['.']
        if (before.st_dev,before.st_ino)!=(rr['dev'],rr['ino']): raise RuntimeError('fixture root changed')
        rootfd=os.open(root.name,flags,dir_fd=pfd)
        if (os.fstat(rootfd).st_dev,os.fstat(rootfd).st_ino)!=(rr['dev'],rr['ino']): raise RuntimeError('fixture root identity mismatch')
        def parent_fd(rel):
            pieces=rel.split('/')
            fd=os.dup(rootfd)
            try:
                for i,part in enumerate(pieces[:-1]):
                    nfd=os.open(part,flags,dir_fd=fd)
                    os.close(fd); fd=nfd
                    row=byrel['/'.join(pieces[:i+1])]
                    st=os.fstat(fd)
                    if (st.st_dev,st.st_ino)!=(row['dev'],row['ino']): raise RuntimeError('fixture parent changed')
                return fd,pieces[-1]
            except BaseException:
                os.close(fd); raise
        for row in sorted((r for r in expected if r['kind']=='file'),key=lambda r:r['rel'],reverse=True):
            fd,name=parent_fd(row['rel'])
            try:
                st=os.stat(name,dir_fd=fd,follow_symlinks=False)
                if not stat.S_ISREG(st.st_mode) or st.st_nlink!=1 or (st.st_dev,st.st_ino)!=(row['dev'],row['ino']): raise RuntimeError('fixture file changed')
                os.unlink(name,dir_fd=fd)
            finally: os.close(fd)
        for row in sorted((r for r in expected if r['kind']=='dir' and r['rel']!='.'),key=lambda r:(r['rel'].count('/'),r['rel']),reverse=True):
            fd,name=parent_fd(row['rel'])
            try:
                st=os.stat(name,dir_fd=fd,follow_symlinks=False)
                if not stat.S_ISDIR(st.st_mode) or (st.st_dev,st.st_ino)!=(row['dev'],row['ino']): raise RuntimeError('fixture dir changed')
                os.rmdir(name,dir_fd=fd)
            finally: os.close(fd)
        after=os.stat(root.name,dir_fd=pfd,follow_symlinks=False)
        if (after.st_dev,after.st_ino)!=(rr['dev'],rr['ino']): raise RuntimeError('fixture root replaced')
        os.rmdir(root.name,dir_fd=pfd)
    finally:
        if rootfd is not None: os.close(rootfd)
        os.close(pfd)

def snap_run(args,timeout=20):
    try:
        r=_spawn(args,env=scrubbed_env(with_home=True),timeout=timeout)
    except (OSError,subprocess.TimeoutExpired):
        raise RuntimeError('read-only or exact cleanup command unavailable or timed out') from None
    if r.returncode: raise RuntimeError('read-only or exact cleanup command failed, status='+str(r.returncode))
    return r

def nerdctl(*args): return ['/opt/homebrew/bin/colima','nerdctl','-p','effigy','--',*args]

def resource_names(kind):
    return sorted(x for x in snap_run(nerdctl(kind,'ls','--format','{{.Name}}')).stdout.splitlines() if x)

def network_snapshot():
    lines=snap_run(nerdctl('network','ls','--format','{{.Name}}\t{{.ID}}')).stdout.splitlines()
    listed={}
    for line in lines:
        cols=line.split('\t')
        if len(cols)!=2 or not re.fullmatch(r'[A-Za-z0-9_.-]+',cols[0]) or cols[0] in listed:
            raise RuntimeError('network list row ambiguous')
        name,ident=cols
        if name in ('host','none'):
            if ident: raise RuntimeError('builtin network unexpectedly has identity')
        elif not re.fullmatch(r'[0-9a-f]{12}',ident):
            raise RuntimeError('network list ID not canonical')
        listed[name]=ident
    if not {'host','none'} <= set(listed): raise RuntimeError('expected builtin network missing')
    inspectable=sorted(set(listed)-{'host','none'})
    raw=json.loads(snap_run(nerdctl('network','inspect',*inspectable),timeout=45).stdout) if inspectable else []
    result={'host':'builtin:host','none':'builtin:none'}
    seen=set()
    for row in raw:
        name=row.get('Name'); ident=row.get('Id',row.get('ID'))
        if name not in inspectable or name in seen or not isinstance(ident,str) or not re.fullmatch(r'[0-9a-f]{64}',ident) or not ident.startswith(listed[name]):
            raise RuntimeError('network list/inspect identity mismatch')
        seen.add(name); result[name]=ident
    if seen!=set(inspectable): raise RuntimeError('network inspect incomplete')
    if len(seen)!=len({result[n] for n in seen}): raise RuntimeError('duplicate network identity')
    return result

def volume_snapshot():
    vs=resource_names('volume')
    if len(vs)!=len(set(vs)) or any(not re.fullmatch(r'[A-Za-z0-9_.-]+',v) for v in vs):
        raise RuntimeError('volume name set ambiguous')
    if not vs: return {}
    raw=json.loads(snap_run(nerdctl('volume','inspect',*vs),timeout=45).stdout)
    rows={}
    for item in raw:
        name=item.get('Name'); path=item.get('Mountpoint')
        if name not in vs or name in rows or not isinstance(path,str) or not path.startswith('/var/lib/nerdctl/') or '..' in Path(path).parts or not re.fullmatch(r'/[A-Za-z0-9_./-]+',path):
            raise RuntimeError('volume backing identity unavailable')
        rows[name]=path
    if set(rows)!=set(vs): raise RuntimeError('volume inspect incomplete')
    ordered=sorted(rows)
    values=snap_run(['/opt/homebrew/bin/colima','ssh','-p','effigy','--','sudo','-n','stat','--format=%d:%i:%u:%g:%a:%F','--',*[rows[n] for n in ordered]],timeout=45).stdout.splitlines()
    if len(values)!=len(ordered) or any(not re.fullmatch(r'[0-9]+:[0-9]+:[0-9]+:[0-9]+:[0-7]+:directory',x) for x in values):
        raise RuntimeError('volume backing device/inode/owner/mode unavailable')
    if len({':'.join(x.split(':')[:2]) for x in values})!=len(values): raise RuntimeError('duplicate volume backing identity')
    return {name:[rows[name],value] for name,value in zip(ordered,values)}

def canonical_digest(value):
    """Exact digest serialization shared with the r13/r17 runtime receipts and
    the immutable recovery inventory; the pinned baselines use it verbatim."""
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()

def verify_historical_baseline():
    """Reconstruct the pinned identity baseline from the immutable recovery
    evidence only; never from live state, VM or image contents."""
    receipt_path=RECOVERY_DIR/RECOVERY_RECEIPT
    if receipt_path.is_symlink() or digest(receipt_path)!=RECOVERY_RECEIPT_SHA256:
        raise RuntimeError('recovery receipt hash mismatch')
    try:
        receipt=json.loads(receipt_path.read_text())
    except (ValueError,OSError) as error:
        raise RuntimeError('recovery receipt unreadable or malformed: '+str(error)) from None
    if receipt.get('network_digest')!=NETWORK_BASELINE_SHA256 or receipt.get('volume_digest')!=VOLUME_BASELINE_SHA256:
        raise RuntimeError('recovery receipt does not pin the expected identity digests')
    if receipt.get('inventory_counts')!={'containers':15,'networks':EXPECTED_NETWORK_COUNT,'volumes':EXPECTED_VOLUME_COUNT}:
        raise RuntimeError('recovery receipt inventory counts mismatch')
    if receipt.get('historical_identity_digest_equality')!={'network':True,'volume':True}:
        raise RuntimeError('recovery receipt does not confirm historical digest equality')
    if receipt.get('artifact_sha256',{}).get(RECOVERY_INVENTORY)!=RECOVERY_INVENTORY_SHA256:
        raise RuntimeError('recovery receipt does not pin the inventory hash')
    inventory_path=RECOVERY_DIR/RECOVERY_INVENTORY
    if inventory_path.is_symlink() or digest(inventory_path)!=RECOVERY_INVENTORY_SHA256:
        raise RuntimeError('recovery inventory hash mismatch')
    try:
        inventory=json.loads(inventory_path.read_text())
    except (ValueError,OSError) as error:
        raise RuntimeError('recovery inventory unreadable or malformed: '+str(error)) from None
    networks=inventory.get('networks'); volumes=inventory.get('volumes')
    if not isinstance(networks,dict) or not isinstance(volumes,dict):
        raise RuntimeError('recovery inventory identity maps missing')
    if canonical_digest(networks)!=NETWORK_BASELINE_SHA256 or canonical_digest(volumes)!=VOLUME_BASELINE_SHA256:
        raise RuntimeError('recovered identity maps do not reproduce the pinned canonical digests')
    if len(networks)!=EXPECTED_NETWORK_COUNT or len(volumes)!=EXPECTED_VOLUME_COUNT:
        raise RuntimeError('recovered identity counts mismatch')
    for name,ident in PROTECTED_VOLUMES.items():
        if volumes.get(name)!=ident:
            raise RuntimeError('recovered protected volume identity mismatch: '+name)
    return {'receipt_sha256':RECOVERY_RECEIPT_SHA256,'inventory_sha256':RECOVERY_INVENTORY_SHA256,'network_identity_sha256':NETWORK_BASELINE_SHA256,'volume_identity_sha256':VOLUME_BASELINE_SHA256,'network_count':EXPECTED_NETWORK_COUNT,'volume_count':EXPECTED_VOLUME_COUNT,'protected_volume_identities_verified':True}

def verify_resource_baseline():
    """Full read-only network and volume identity snapshots must equal the
    pinned baseline digests exactly; counts or names alone never suffice."""
    networks=network_snapshot(); volumes=volume_snapshot()
    if len(networks)!=EXPECTED_NETWORK_COUNT or canonical_digest(networks)!=NETWORK_BASELINE_SHA256:
        raise RuntimeError('live network identities differ from the pinned baseline')
    if len(volumes)!=EXPECTED_VOLUME_COUNT or canonical_digest(volumes)!=VOLUME_BASELINE_SHA256:
        raise RuntimeError('live volume identities differ from the pinned baseline')
    for name,ident in PROTECTED_VOLUMES.items():
        if volumes.get(name)!=ident:
            raise RuntimeError('protected volume identity mismatch: '+name)
    baseline=exact_ps_ids((BASE/'r3-baseline-container-ids.txt').read_text())
    if MARIADB_CONTAINER_ID not in baseline:
        raise RuntimeError('protected MariaDB container missing from the 15-container baseline')
    return {'network_identity_sha256':canonical_digest(networks),'volume_identity_sha256':canonical_digest(volumes),'network_count':len(networks),'volume_count':len(volumes),'protected_volume_identities_verified':True,'protected_container_verified':True}

def verify_fixture_roots_absent():
    for root in FIXTURE_ROOTS:
        if os.path.lexists(root):
            raise RuntimeError('an exact fixture path exists; do not adopt or delete it: '+str(root))
    if FIXTURE_PARENT.is_symlink(): raise RuntimeError('fixture parent is a symlink')
    return {'fixture_roots_absent':True}

def verify_prior_artifacts_absent():
    for name in PRIOR_EVIDENCE:
        if os.path.lexists(BASE/name):
            raise RuntimeError('prior r18 evidence exists; no retry: '+name)
    return {'prior_artifacts_absent':True}

def verify_fixture_ancestry():
    """Bounded pre-creation fixture-ancestor safety, before any synthetic path
    exists: every existing ancestor of the fixture parent is a real, host-owned
    (root or invoking user) directory and no host or guest mountpoint sits at
    the fixture parent, below it, or at/inside either exact fixture root.
    Shared mounts strictly above the fixture parent stay acceptable."""
    mounts=host_mounts()+guest_mounts()
    uid=os.getuid()
    current=Path('/')
    for part in FIXTURE_PARENT.parts[1:]:
        current=current/part
        try:
            st=os.lstat(current)
        except FileNotFoundError:
            # A missing component (normally the fixture parent itself) is the
            # normal pre-creation state; nothing below it can exist either.
            break
        if stat.S_ISLNK(st.st_mode): raise RuntimeError('symlink in fixture ancestry: '+str(current))
        if not stat.S_ISDIR(st.st_mode): raise RuntimeError('fixture ancestor is not a directory: '+str(current))
        if st.st_uid not in (0,uid): raise RuntimeError('foreign-owned fixture ancestor: '+str(current))
    for root in FIXTURE_ROOTS:
        if root.parent!=FIXTURE_PARENT:
            raise RuntimeError('fixture root is not an exact child of the fixture parent: '+str(root))
        if os.path.lexists(root) and stat.S_ISLNK(os.lstat(root).st_mode):
            raise RuntimeError('fixture root path is a symlink: '+str(root))
    if any(m==str(FIXTURE_PARENT) or m.startswith(str(FIXTURE_PARENT)+'/') for m in mounts):
        raise RuntimeError('mountpoint at or inside the fixture parent: '+str(FIXTURE_PARENT))
    for root in FIXTURE_ROOTS:
        if any(m==str(root) or m.startswith(str(root)+'/') for m in mounts):
            raise RuntimeError('mountpoint at or inside fixture root: '+str(root))
    return {'fixture_ancestors_safe':True,'mount_tables_parsed':True}

def entry_sweep(packet_sha,go_path):
    """The complete read-only admission gate; the launcher runs it BEFORE
    prepare's first mkdir/write and before any native mutation. It writes
    nothing, creates nothing and mutates no backend resource."""
    bundle=verify_bundle(packet_sha)
    go_sha=verify_go(go_path,packet_sha)
    environment=verify_environment()
    historical=verify_historical_baseline()
    resources=verify_resource_baseline()
    roots=verify_fixture_roots_absent()
    ancestry=verify_fixture_ancestry()
    prior=verify_prior_artifacts_absent()
    return bundle,go_sha,{'environment':environment,'historical_baseline':historical,'resource_baseline':resources,'fixture_roots_absent':roots,'fixture_ancestors_safe':ancestry,'prior_artifacts_absent':prior}
