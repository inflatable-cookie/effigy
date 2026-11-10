"""Explicitly unsealed single-Linux-cell use of the full-session assertions.

Task 829 owns this exploratory entry. It accepts neither a sealed GO nor a
three-backend qualification packet and emits no consumable qualification receipt.
"""
import json
import os
from pathlib import Path
import sys
import threading

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
from linux_native_window import Window

TASK = '06fa2bbb-55f5-4f02-84c9-069e3a179946'
BASE = Path('/Volumes/Stash/scratch/effigy/linux-native-829')
OLD = '823d859dace2cbaa848c8ad5a0b5bdfd8959fdf1'


class Peak:
    """Sample allocation headroom, independently of the hard physical ceiling."""
    def __init__(self):
        self.paths = (Path('/var/lib/effigy-829/rootful'), BASE / 'qualification')
        self.devices = [path.stat().st_dev for path in self.paths]
        self.peak = [0, 0]
        self.samples = 0
        self.error = None
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.sample, daemon=False)

    def sample(self):
        try:
            while True:
                for index, path in enumerate(self.paths):
                    common.require(path.stat().st_dev == self.devices[index], 'sampled filesystem changed')
                    fs = os.statvfs(path)
                    self.peak[index] = max(self.peak[index], (fs.f_blocks - fs.f_bfree) * fs.f_frsize)
                self.samples += 1
                if self.stop.wait(.05):
                    return
        except BaseException as error:
            self.error = error

    def finish(self):
        self.stop.set()
        self.thread.join()
        if self.error:
            raise self.error
        return {'sample_period_seconds': .05, 'samples': self.samples,
                'sampled_peak_bytes': dict(zip(map(str, self.paths), self.peak)),
                'engine_capacity_bytes': 2147483648,
                'engine_has_twice_sampled_peak_headroom': 2 * self.peak[0] <= 2147483648,
                'exploratory': True, 'qualification_credit': False}


def validate(value):
    common.require(set(value) == {'schema', 'task_id', 'exploratory', 'qualification_credit', 'cell'},
                   'explicit exploratory input required')
    common.require(value['schema'] == 'effigy.linux-native-session-exploration.v1'
                   and value['task_id'] == TASK and value['exploratory'] is True
                   and value['qualification_credit'] is False, 'not the task-owned exploratory schema')
    cell = value['cell']
    common.require(cell['backend'] == 'linux' and cell['engine_kind'] == 'docker'
                   and 0 < cell['budget_seconds'] <= 1800, 'unsupported native exploration cell')
    common.require(cell.get('native_engine_root') == '/var/lib/effigy-829/rootful'
                   and cell['docker_config'] == '/var/lib/effigy-829/rootful/client/config',
                   'dedicated rootful engine required; rootless experiments are not qualification candidates')
    root = common.qualification_path(cell['qualification_root'])
    common.require(root.is_relative_to(BASE / 'qualification') and root != BASE / 'qualification',
                   'fresh task-owned session directory required')
    common.require(Path(cell['source_root']).is_relative_to(BASE / 'builds')
                   and common.source_head(cell['source_root']) == cell['source_head'],
                   'native current-main source differs')
    common.require(cell['old_source_head'] == OLD
                   and common.source_head(cell['old_source_root']) == OLD,
                   'genuine old source differs')
    common.require(len({cell[key]['sha256'] for key in ('candidate', 'production', 'old_binary')}) == 3,
                   'ordinary, qualification and old binaries must be distinct')
    receipt = common.artifact(cell['guest_producer_receipt']).read_text()
    common.require('source_commit=' + cell['source_head'] + '\n' in receipt
                   and 'sha256=' + cell['guest_artifact']['sha256'] + '\n' in receipt
                   and 'build_kind=native-zig-workspace-artifact\n' in receipt,
                   'actual native producer record differs')
    return cell


def execute(path, expected, mode):
    common.require(sys.platform == 'linux' and mode in ('acquire', 'preflight', 'full'), 'native exploration only')
    common.require(common.digest(path) == expected, 'exploratory inputs changed')
    cell = validate(common.load(path))
    window = Window(cell, expected, {'decision_id': 'task-829-exploration'}, exploratory=True)
    peak = Peak()
    peak.thread.start()
    try:
        info = json.loads(window.engine_command('info', '--format', '{{json .}}').stdout)
        common.require(info['DockerRootDir'] == cell['native_engine_root'] + '/data'
                       and not any('rootless' in option for option in info['SecurityOptions'])
                       and any('seccomp' in option for option in info['SecurityOptions']),
                       'native engine placement, mode or default seccomp differs')
        if mode == 'acquire':
            window.prepare()
            # Ordinary host preparation owns acquisition and identity binding.
            # Even if a later publication refuses, keep its actual exit and
            # independently verify the producer/consumer output after down.
            result = window.command([window.production, 'container', 'up', '--detach', '--json'],
                                    check=False, timeout=1500)
            window.command([window.production, 'container', 'web', 'down', '--json'])
            window.command([window.production, 'container', 'retire', '--yes', '--json'])
            common.require(window.inventory() == cell['baseline'], 'acquisition inventory did not settle')
            identity = window.guest_artifact.with_suffix('.install-identity')
            common.require(identity.is_file() and identity.read_text().startswith('local:v'),
                           'ordinary consumer did not bind the produced artifact')
            common.exclusive('exploratory-acquire.json', {'exploratory': True,
                             'qualification_credit': False, 'ordinary_up_exit': result.returncode,
                             'consumer_identity_sha256': common.digest(identity)}, dir_fd=window.root_fd)
        elif mode == 'preflight':
            window.prepare()
            window.up()
            window.down()
            common.require(window.inventory() == cell['baseline'], 'preflight inventory did not settle')
            common.exclusive('exploratory-preflight.json', {'exploratory': True,
                             'qualification_credit': False, 'cells': window.events}, dir_fd=window.root_fd)
        else:
            window.execute()
    except BaseException as error:
        common.exclusive('exploratory-failed.json', {'error_type': type(error).__name__,
                         'message': str(error), 'cells_completed': len(window.events),
                         'qualification_credit': False}, dir_fd=window.root_fd)
        raise
    finally:
        try:
            allocation = peak.finish()
            common.exclusive('exploratory-allocation.json', allocation, dir_fd=window.root_fd)
            common.require(allocation['engine_has_twice_sampled_peak_headroom'],
                           'engine capacity has less than twice the sampled peak; resize before freeze')
        finally:
            window.close()


if __name__ == '__main__':
    if len(sys.argv) != 4:
        raise SystemExit('usage: linux_native_session.py INPUTS SHA256 acquire|preflight|full')
    os.umask(0o077)
    execute(Path(sys.argv[1]), sys.argv[2], sys.argv[3])
