"""One-time, unsealed migration of the owned preparation tree to a physical bound.

Requires the task-owned rollback and a stopped dedicated engine at the host
entry. Existing source/cache trees and every failed session remain retained.
No existing image is formatted, replaced or enlarged.
"""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
from linux_native_resume import ROOT, WORKSPACE, IMAGES
from linux_loop_bound import verify_image, image_custody
from linux_native_initialize import rewrite_same, unassociated

BASE = WORKSPACE.parent
IMAGE = ROOT / 'preparation.ext4'
CAPACITY = 40 * 1024 ** 3
UUID = '18f16a9f-a2cd-421e-8390-8f8d6d2bd829'


def manifest(root, *, filesystem_root=False):
    """No-follow identity/content census, excluding the separate workspace image."""
    if filesystem_root:
        lost = root / 'lost+found'
        info = lost.lstat()
        if (info.st_ino != 11 or info.st_uid != 0 or info.st_gid != 0
                or info.st_mode != 0o40700 or list(lost.iterdir())):
            raise ValueError('formatter-owned empty lost+found differs')
    digest = hashlib.sha256(b'[')
    entries, encoded_bytes = 0, 2
    def visit(path):
        nonlocal entries, encoded_bytes
        relative = path.relative_to(root).as_posix()
        if relative == 'qualification' or (filesystem_root and relative == 'lost+found'):
            return
        info = path.lstat()
        row = [relative, info.st_mode, info.st_uid, info.st_gid, info.st_mtime_ns]
        if stat.S_ISREG(info.st_mode):
            content = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1048576), b''):
                    content.update(block)
            row.extend([info.st_size, content.hexdigest()])
        elif stat.S_ISLNK(info.st_mode):
            row.append(os.readlink(path))
        elif stat.S_ISDIR(info.st_mode):
            for child in sorted(path.iterdir()):
                visit(child)
        else:
            raise ValueError('preparation tree contains an unexpected special file: ' + relative)
        raw = json.dumps(row, separators=(',', ':')).encode()
        if entries:
            digest.update(b',')
            encoded_bytes += 1
        digest.update(raw)
        entries += 1
        encoded_bytes += len(raw)
    visit(root)
    digest.update(b']')
    return {'sha256': digest.hexdigest(), 'entries': entries, 'bytes': encoded_bytes}


def migrate(stages, *, resume=False):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise ValueError('native root preparation role required')
    state = dict(line.split('=', 1) for line in subprocess.check_output([
        'systemctl', 'show', '--all', 'effigy-829-rootful.service',
        '--property=ActiveState,MainPID,ControlGroup'], text=True).splitlines())
    if state != {'ActiveState': 'inactive', 'MainPID': '0', 'ControlGroup': ''}:
        raise ValueError('dedicated engine must be independently stopped')
    if (BASE.resolve(strict=True) != BASE or BASE.stat().st_uid != 1001
            or BASE.stat().st_dev != os.stat('/').st_dev or (IMAGE.exists() and not resume)):
        raise ValueError('preparation base changed or image already exists; reconcile without replay')
    root_uuid = subprocess.check_output(['blkid', '-s', 'UUID', '-o', 'value', '/dev/sda2'], text=True).strip()
    if root_uuid != storage.ROOT_UUID:
        raise ValueError('fixed native root identity differs')
    available = os.statvfs(ROOT)
    if available.f_bavail * available.f_frsize < (0 if resume else CAPACITY) + 16 * 1024 ** 3:
        raise ValueError('preparation image would consume the native root reserve')
    mounts = storage.mount_rows(Path('/proc/self/mountinfo').read_text())
    if any(row['target'].startswith(str(BASE) + '/') and row['target'] != str(WORKSPACE) for row in mounts):
        raise ValueError('unexpected preparation submount')
    before = manifest(BASE)
    stages.append({'source_before': before})
    if resume:
        # Exact resource from the retained pre-cutover OOM. No directory scan,
        # replacement image or new allocation is permitted by this recovery.
        target = ROOT / 'preparation-copy-xcqmuork'
        info = IMAGE.lstat()
        if (info.st_ino != 2641184 or info.st_size != CAPACITY or info.st_uid != 0
                or info.st_mode != 0o100600 or info.st_nlink != 1):
            raise ValueError('owned partial preparation image differs')
        value = storage.observe(IMAGE, target, CAPACITY, project_quota=False)
        image_custody(value, dict(image=str(IMAGE), image_device=info.st_dev, image_inode=info.st_ino,
                     image_bytes=CAPACITY, filesystem_uuid=UUID, mountpoint=str(target)))
        stages.append({'resumed_owned_image': str(IMAGE), 'inode': info.st_ino})
    else:
        fd = os.open(IMAGE, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        try:
            os.posix_fallocate(fd, 0, CAPACITY)
            os.fsync(fd)
        finally:
            os.close(fd)
        stages.append({'new_image': str(IMAGE), 'bytes': CAPACITY, 'inode': IMAGE.stat().st_ino})
        subprocess.run(['mkfs.ext4', '-q', '-F', '-m', '0', '-U', UUID,
                        '-E', 'lazy_itable_init=0,lazy_journal_init=0,nodiscard', str(IMAGE)], check=True)
        target = Path(tempfile.mkdtemp(prefix='preparation-copy-', dir=ROOT))
        subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(IMAGE), str(target)], check=True)
        storage.disable_discard(IMAGE, target, CAPACITY)
    info = IMAGE.lstat()
    value = storage.observe(IMAGE, target, CAPACITY, project_quota=False)
    pin = dict(image=str(IMAGE), image_device=info.st_dev, image_inode=info.st_ino,
               image_bytes=CAPACITY, filesystem_uuid=UUID, mountpoint=str(target))
    if not image_custody(value, pin)['initialized']:
        subprocess.run(['umount', '--', str(target)], check=True)
        unassociated(IMAGE)
        fd = os.open(IMAGE, os.O_RDWR | os.O_NOFOLLOW)
        try:
            if os.fstat(fd).st_ino != IMAGE.lstat().st_ino or os.fstat(fd).st_size != CAPACITY:
                raise ValueError('offline preparation image changed')
            stages.append({'same_byte_initialization': rewrite_same(fd, CAPACITY)})
        finally:
            os.close(fd)
        subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(IMAGE), str(target)], check=True)
        storage.disable_discard(IMAGE, target, CAPACITY)
    subprocess.run(['cp', '-a', '--one-file-system', str(BASE) + '/.', str(target)], check=True)
    copied, after = manifest(target, filesystem_root=True), manifest(BASE)
    stages.append({'source_after': after, 'copied': copied})
    if before != copied or before != after:
        raise ValueError('preparation source or copied content/metadata changed; retained before cutover')
    stages.append({'source_after': after, 'copied': copied, 'staging_mount': str(target)})
    old = BASE.with_name(BASE.name + '.before-physical-18f16a9f')
    if old.exists():
        raise ValueError('retained preparation path already exists')
    subprocess.run(['umount', '--', str(WORKSPACE)], check=True)
    subprocess.run(['umount', '--', str(target)], check=True)
    BASE.rename(old)
    BASE.mkdir(mode=0o700)
    stages.append({'retained_original': str(old), 'cutover_started': True})
    subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(IMAGE), str(BASE)], check=True)
    value = storage.disable_discard(IMAGE, BASE, CAPACITY)
    workspace_image, _, workspace_bytes, _ = IMAGES[1]
    subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(workspace_image), str(WORKSPACE)], check=True)
    storage.disable_discard(workspace_image, WORKSPACE, workspace_bytes)
    pin = dict(image=str(IMAGE), image_device=IMAGE.stat().st_dev, image_inode=IMAGE.stat().st_ino,
               image_bytes=CAPACITY, filesystem_uuid=UUID, mountpoint=str(BASE))
    bound = verify_image(value, pin, child_mounts=(str(WORKSPACE),))
    if manifest(BASE, filesystem_root=True) != before:
        raise ValueError('cutover preparation content/metadata differs')
    stages.append({'cutover_complete': True, 'bound': bound, 'accounting': storage.accounting_view(BASE)})
    return {'schema': 'effigy.linux-preparation-storage.v1', 'stages': stages,
            'exploratory': True, 'qualification_credit': False}


if __name__ == '__main__':
    stages = []
    try:
        if sys.argv[1:] not in ([], ['--resume-copy']):
            raise ValueError('only the exact owned pre-cutover recovery is supported')
        print(json.dumps(migrate(stages, resume=sys.argv[1:] == ['--resume-copy']), indent=2))
    except BaseException as error:
        print(json.dumps({'error': str(error), 'stages': stages, 'exploratory': True,
                          'qualification_credit': False}, indent=2))
        raise
