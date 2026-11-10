"""Resume only task 829's existing physical images; never format or replace one."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
import linux_native_writers as writers
from linux_loop_bound import verify_image

ROOT = Path('/var/lib/effigy-829')
WORKSPACE = Path('/Volumes/Stash/scratch/effigy/linux-native-829/qualification')
IMAGES = ((ROOT / 'engine-rootful.ext4', ROOT / 'rootful', 2147483648,
           '958bd45f-83da-43e7-b15a-b498a79b34bf'),
          (ROOT / 'rows-final.ext4', WORKSPACE, 1073741824,
           '8d8707ea-dd3d-48ab-b408-47f66e5475dd'))
INODES = {'engine-rootful.ext4': 2641043, 'rows-final.ext4': 2640535}


def run(*argv):
    return subprocess.check_output(argv, text=True)


def configuration_originals():
    paths = [Path('/etc/systemd/system/effigy-829-rootful.service'),
             Path('/usr/local/libexec/effigy829-rootful.sh'),
             ROOT / 'rootful/containerd.toml', ROOT / 'rootful/dockerd.json']
    result = {}
    for path in paths:
        info = path.lstat()
        if path.resolve(strict=True) != path or info.st_uid != 0 or info.st_mode & 0o022:
            raise RuntimeError('daemon configuration is not root controlled')
        data = path.read_bytes()
        if len(data) > 65536:
            raise RuntimeError('daemon configuration exceeds its original bound')
        result[str(path)] = {'text': data.decode('utf-8'), 'uid': info.st_uid,
                            'mode': info.st_mode, 'device': info.st_dev, 'inode': info.st_ino,
                            'sha256': hashlib.sha256(data).hexdigest()}
    return result


def resume():
    if os.geteuid() != 0 or run('blkid', '-s', 'UUID', '-o', 'value', '/dev/sda2').strip() != storage.ROOT_UUID:
        raise RuntimeError('fixed native root observer required')
    observations = []
    for image, target, capacity, expected_uuid in IMAGES:
        if (image.resolve(strict=True) != image or target.resolve(strict=True) != target
                or image.stat().st_uid != 0 or image.stat().st_size != capacity
                or image.stat().st_mode & 0o777 != 0o600
                or run('blkid', '-s', 'UUID', '-o', 'value', str(image)).strip() != expected_uuid):
            raise RuntimeError('owned physical image differs')
        rows = storage.mount_rows(Path('/proc/self/mountinfo').read_text())
        if not any(row['target'] == str(target) for row in rows):
            if run('losetup', '-j', str(image)).strip():
                raise RuntimeError('unmounted image already has a loop association; reconcile explicitly')
            subprocess.run(['mount', '-o', 'loop,nodev,nosuid', str(image), str(target)], check=True)
        observation = storage.disable_discard(image, target, capacity)
        verify_image(observation, {'image': str(image), 'image_device': os.stat('/').st_dev,
            'image_inode': INODES[image.name], 'image_bytes': capacity,
            'filesystem_uuid': expected_uuid, 'mountpoint': str(target)})
        observations.append(observation)
    if (WORKSPACE.stat().st_uid != 1001 or WORKSPACE.stat().st_mode & 0o777 != 0o700
            or run('id', '-G', 'effigy531').strip() != '1001'):
        raise RuntimeError('dedicated native account or workspace ownership differs')
    bridge = Path('/sys/class/net/effigy829br0')
    if not bridge.exists():
        import ipaddress
        pools = [ipaddress.ip_network(value) for value in ('172.29.0.0/24', '172.30.0.0/16')]
        routes = json.loads(run('ip', '-j', 'route'))
        if any(any(ipaddress.ip_network(row['dst'], strict=False).overlaps(pool) for pool in pools)
               for row in routes if row.get('dst') not in (None, 'default')):
            raise RuntimeError('dedicated bridge address space overlaps a current route')
        subprocess.run(['ip', 'link', 'add', 'effigy829br0', 'type', 'bridge'], check=True)
        subprocess.run(['ip', 'addr', 'add', '172.29.0.1/24', 'dev', 'effigy829br0'], check=True)
        subprocess.run(['ip', 'link', 'set', 'effigy829br0', 'up'], check=True)
    addresses = json.loads(run('ip', '-j', '-4', 'addr', 'show', 'dev', 'effigy829br0'))
    if (len(addresses) != 1 or not (bridge / 'bridge').is_dir()
            or [(row['local'], row['prefixlen']) for row in addresses[0]['addr_info']] != [('172.29.0.1', 24)]):
        raise RuntimeError('existing bridge identity differs')
    configs = {path: item['sha256'] for path, item in configuration_originals().items()}
    subprocess.run(['systemctl', 'start', 'effigy-829-rootful.service'], check=True)
    # Type=exec acknowledges exec, not daemon readiness. In particular, stopping
    # immediately can interrupt the wrapper before it has exec'd dockerd.
    deadline = time.monotonic() + 30
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise RuntimeError('dedicated engine readiness deadline expired')
        try:
            output = subprocess.run(['/usr/bin/docker', '--config', str(ROOT / 'rootful/client/config'),
                                     '--context', 'effigy-829-rootful', 'info', '--format', '{{json .}}'],
                                    capture_output=True, text=True, timeout=min(2, remaining))
            if output.returncode == 0:
                engine = json.loads(output.stdout)
                if (engine['DockerRootDir'] != str(ROOT / 'rootful/data')
                        or engine['CgroupDriver'] != 'cgroupfs' or engine['CgroupVersion'] != '2'
                        or any('rootless' in option for option in engine['SecurityOptions'])):
                    raise RuntimeError('ready engine placement or mode differs')
                break
        except subprocess.TimeoutExpired:
            pass
        time.sleep(min(.2, max(0, deadline - time.monotonic())))
    service = run('systemctl', 'show', 'effigy-829-rootful.service',
                  '-p', 'MainPID', '-p', 'ActiveState', '-p', 'ControlGroup', '-p', 'Result')
    state = dict(line.split('=', 1) for line in service.splitlines())
    if state['ActiveState'] != 'active' or int(state['MainPID']) <= 0:
        raise RuntimeError('ready engine lost its service owner')
    if engine['Containers'] != 0:
        raise RuntimeError('resume requires the dedicated engine to have no container writers')
    devices = {item['mount']['device'] for item in observations}
    closure = writers.observe([state['ControlGroup']], devices, str(ROOT / 'rootful'), 3221225472)
    allocation = {str(target): storage.accounting(target) for _, target, _, _ in IMAGES}
    return {'exploratory': True, 'qualification_credit': False, 'images': observations,
            'daemon_config_sha256': configs, 'engine': engine, 'writer_closure': closure,
            'accounting': allocation, 'service': service}



if __name__ == '__main__':
    print(json.dumps(resume(), indent=2))
