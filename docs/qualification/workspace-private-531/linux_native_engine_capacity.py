"""Unsealed promotion of the owned engine pilot into a new fixed-size image.

The old image remains byte-identical and unmounted. Only the exclusively created
copy is resized; no existing image is formatted, enlarged, deleted or reverted.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
from linux_native_initialize import rewrite_same, unassociated
from linux_native_resume import ROOT, IMAGES, INODES
from linux_loop_bound import verify_image

FINAL = ROOT / 'engine-final.ext4'
CAPACITY = 3 * 1024 ** 3
UUID = 'ba08b2af-40d2-4da5-8f67-f5e48e02a4d6'


def copy_image(source, destination, capacity):
    """Copy every byte, initialize the extension, then verify the original prefix."""
    original = source.stat()
    if capacity <= original.st_size:
        raise ValueError('new image must have a larger fixed capacity')
    before = hashlib.sha256()
    new = os.open(destination, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with source.open('rb') as stream:
            offset = 0
            while offset < capacity:
                block = stream.read(min(1048576, original.st_size - offset)) if offset < original.st_size else bytes(min(1048576, capacity-offset))
                if not block:
                    raise ValueError('original image read was incomplete')
                if offset < original.st_size:
                    before.update(block)
                done = 0
                while done < len(block):
                    wrote = os.pwrite(new, block[done:], offset + done)
                    if wrote <= 0: raise ValueError('new image write was incomplete')
                    done += wrote
                offset += len(block)
        os.fsync(new)
        copied, retained = hashlib.sha256(), hashlib.sha256()
        with source.open('rb') as stream:
            offset = 0
            while offset < original.st_size:
                block = stream.read(min(1048576, original.st_size-offset))
                if not block: raise ValueError('original image changed')
                retained.update(block)
                copied.update(os.pread(new, len(block), offset))
                offset += len(block)
        if not before.hexdigest() == retained.hexdigest() == copied.hexdigest():
            raise ValueError('copied prefix or retained original differs')
        if any(getattr(source.stat(), key) != getattr(original, key) for key in
               ('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns')):
            raise ValueError('original image identity changed')
        return {'original_sha256': before.hexdigest(), 'copied_prefix_sha256': copied.hexdigest(),
                'original_bytes': original.st_size, 'new_bytes': capacity, 'new_inode': os.fstat(new).st_ino}
    finally:
        os.close(new)


def promote():
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise ValueError('native root promotion required')
    old, target, size, identifier = IMAGES[0]
    if size != 2147483648 or old.name != 'engine-rootful.ext4':
        raise ValueError('only the owned two-GiB pilot can be promoted')
    raw = subprocess.check_output(['systemctl','show','--all','effigy-829-rootful.service',
                                  '--property=ActiveState,MainPID,ControlGroup'],text=True)
    state = dict(line.split('=',1) for line in raw.splitlines())
    if state != {'ActiveState':'inactive','MainPID':'0','ControlGroup':''}:
        raise ValueError('engine must be independently stopped')
    observed = storage.observe(old,target,size,project_quota=False)
    info = old.stat()
    pin = dict(image=str(old),image_device=info.st_dev,image_inode=INODES[old.name],image_bytes=size,
               filesystem_uuid=identifier,mountpoint=str(target))
    verify_image(observed,pin)
    if FINAL.exists() or os.statvfs(ROOT).f_bavail * os.statvfs(ROOT).f_frsize < CAPACITY + 16 * 1024 ** 3:
        raise ValueError('fresh final image or native root reserve unavailable')
    subprocess.run(['umount',str(target)],check=True)
    unassociated(old)
    result = copy_image(old,FINAL,CAPACITY)
    journal = ROOT / 'helpers' / ('engine-promotion-' + str(result['new_inode']) + '.json')
    fd = os.open(journal,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    try:
        data = json.dumps(result,sort_keys=True).encode()
        if os.write(fd,data) != len(data): raise ValueError('promotion journal was incomplete')
        os.fsync(fd)
        os.fchmod(fd,0o400)
    finally:
        os.close(fd)
    unassociated(FINAL)
    commands = []
    for argv in (['tune2fs','-U',UUID,str(FINAL)], ['e2fsck','-f','-y',str(FINAL)], ['resize2fs',str(FINAL)]):
        done = subprocess.run(argv,capture_output=True,timeout=120)
        commands.append(dict(argv=argv,exit=done.returncode,stdout=done.stdout.decode(),stderr=done.stderr.decode()))
        if done.returncode:
            raise ValueError('new-copy filesystem operation failed: '+repr(commands[-1]))
    fd = os.open(FINAL,os.O_RDWR|os.O_NOFOLLOW)
    try:
        initialized = rewrite_same(fd,CAPACITY)
    finally:
        os.close(fd)
    subprocess.run(['mount','-o','loop,nodev,nosuid',str(FINAL),str(target)],check=True)
    storage.disable_discard(FINAL,target,CAPACITY)
    observed = storage.observe(FINAL,target,CAPACITY,project_quota=False)
    pin.update(image=str(FINAL),image_inode=FINAL.stat().st_ino,image_bytes=CAPACITY,filesystem_uuid=UUID)
    bound = verify_image(observed,pin)
    result.update(exploratory=True,qualification_credit=False,commands=commands,initialized=initialized,
                  final=bound,retained_original=str(old),accounting=storage.accounting(target))
    return result


if __name__ == '__main__':
    print(json.dumps(promote(),indent=2))
