"""#524 r17 qualification explicit synthetic-fixture inventory and fd-relative cleanup."""
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
PACKET=DIR/'colima-qualification-5f466914-r17.md'
# Frozen candidate subject: a detached, clean scratch checkout pinned to the
# exact source commit; its quick/dev build produced the manifest's binary.
HEAD='5f4669141d8dabd91b7fbfef28d2bce21bc42ba2'
WORKTREE=Path('/Volumes/Builds/scratch/effigy/524/candidate-src-5f466914')

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

def invoke(args, *, env=None, timeout=30):
    r=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=timeout,env=env)
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
    manifest=DIR/'r17-artifacts.json'
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
    if p!=BASE/'r17-go.txt' or p.is_symlink(): raise RuntimeError('exact operator-copied GO receipt missing')
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
        r=subprocess.run(['/opt/homebrew/bin/colima',*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=30,env=env)
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

def validate_deletable(root, expected):
    root=Path(root)
    if snapshot(root)!=expected: raise RuntimeError('fixture tree differs from exact manifest; no deletion')
    # Reject symlink ancestors and any host or guest mountpoint at or below root.
    current=Path('/')
    for part in root.parts[1:]:
        current=current/part
        if stat.S_ISLNK(os.lstat(current).st_mode):raise RuntimeError('symlink in fixture ancestry')
    host=invoke(['/sbin/mount'],env=scrubbed_env())
    host_mounts=[]
    for line in host.splitlines():
        if ' on ' not in line or ' (' not in line: raise RuntimeError('host mount output ambiguous')
        host_mounts.append(line.split(' on ',1)[1].rsplit(' (',1)[0])
    guest=invoke(['/opt/homebrew/bin/colima','ssh','-p','effigy','--','cat','/proc/self/mountinfo'],env=scrubbed_env(with_home=True),timeout=30)
    guest_mounts=[]
    for line in guest.splitlines():
        fields=line.split()
        if len(fields)<10 or '-' not in fields: raise RuntimeError('guest mountinfo ambiguous')
        value=re.sub(r'\\([0-7]{3})',lambda m:chr(int(m.group(1),8)),fields[4])
        if '\\' in value:raise RuntimeError('guest mountpoint escape ambiguous')
        guest_mounts.append(value)
    if any(m==str(root) or m.startswith(str(root)+'/') for m in host_mounts+guest_mounts):
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
