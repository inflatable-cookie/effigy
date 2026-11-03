"""Fixed #531 recovery admission and owned-child exit observation. No runtime CLI.

Default preparation refuses. The separate administrative route requires an
explicit bound operator GO acknowledging that numeric-PID signaling is not race-free.
"""
from dataclasses import dataclass
import datetime
import select
import re
import time

PROFILE = 'effigy-531-qualification'
CONTROL = '/Volumes/Builds/scratch/effigy/531c'
SHARED = '/Volumes/Builds/scratch/effigy/531-colima-qualification'
RESIDUAL = ('Darwin kqueue NOTE_EXIT observes termination but does not bind '
            'kill(pid, SIGINT) to that process identity; administrative signaling requires '
            'a separately bound operator GO explicitly accepting that residual risk')
RESUME = ('/opt/homebrew/Cellar/colima/0.10.3/bin/colima', 'start', '-p', PROFILE,
          '--runtime', 'containerd', '--arch', 'aarch64', '--vm-type', 'vz',
          '--cpus', '2', '--memory', '2', '--disk', '20', '--root-disk', '20',
          '--mount', SHARED + ':w', '--activate=false', '--ssh-agent=false', '--ssh-config=false')


class Refused(RuntimeError):
    pass


def require(value, message):
    if not value:
        raise Refused(message)


@dataclass(frozen=True)
class Deadline:
    utc: datetime.datetime
    monotonic: float

    def remaining(self):
        require(self.utc.tzinfo is not None, 'timezone required')
        wall = (self.utc - datetime.datetime.now(datetime.timezone.utc)).total_seconds()
        monotonic = self.monotonic - time.monotonic()
        require(0 < wall <= 600 and 0 < monotonic <= 600,
                'expired or oversized recovery deadline')
        return min(wall, monotonic)


# Recorded metadata only: this function never observes a profile, PID or disk.
def admit_recorded(plan, observed, deadline, *, agent_alive=True):
    deadline.remaining()
    require(plan['profile'] == PROFILE and plan['control']['path'] == CONTROL
            and plan['shared']['path'] == SHARED, 'wrong private profile/root')
    require(type(plan['budget_seconds']) is int and 0 < plan['budget_seconds'] <= 600,
            'future recovery budget required')
    require(re.fullmatch(r'[0-9a-f]{40}', plan['source_head']), 'exact source head required')
    for key in ('control', 'shared', 'root_disk', 'data_disk', 'agent', 'executables'):
        require(observed[key] == plan[key], 'changed or wrong ' + key + ' identity')
    for key in ('control', 'shared'):
        root = observed[key]
        require(root['uid'] == 501 and root['mode'] == 0o700
                and root['canonical'] is True and root['symlink'] is False,
                'unverified owned root')
    for key, path, inode in (('control', CONTROL, 141767334), ('shared', SHARED, 140815661)):
        require(observed[key]['path'] == path and observed[key]['device'] == 16777240
                and observed[key]['inode'] == inode, 'retained root pin differs')
    for key, path, inode in (
            ('root_disk', CONTROL + '/c/_lima/colima-' + PROFILE + '/disk', 141768542),
            ('data_disk', CONTROL + '/c/_lima/_disks/colima-' + PROFILE + '/datadisk', 141767381)):
        disk = observed[key]
        require(disk == {'path': path, 'device': 16777240, 'inode': inode, 'uid': 501,
                         'regular': True, 'canonical': True, 'symlink': False,
                         'logical_bytes': 21474836480}, 'retained disk pin differs')
    require(observed['executables'] == {
        'colima': {'path': '/opt/homebrew/Cellar/colima/0.10.3/bin/colima',
                   'sha256': '55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf'},
        'lima': {'path': '/opt/homebrew/Cellar/lima/2.2.1/bin/limactl',
                 'sha256': 'e79b14815dc7e51629e064dba950120c4c9323e2b8ba6a52fe816b15f7f9803d'},
    }, 'retained executable pin differs')
    require(observed['shared_empty'] is True, 'shared root not empty')
    agent = observed['agent']
    require(agent['pid'] == 94093 and agent['uid'] == 501 and agent['ppid'] == 1
            and agent['started_utc'] == '2026-11-02T23:37:10Z'
            and agent['executable'] == '/opt/homebrew/Cellar/lima/2.2.1/bin/limactl'
            and agent['profile'] == PROFILE and observed['agent_alive'] is agent_alive,
            'retained agent identity not verified')
    require(observed['configuration'] == {
        'runtime': 'containerd', 'arch': 'aarch64', 'vm_type': 'vz', 'guest_os': 'Linux',
        'cpus': 2, 'memory_gib': 2, 'root_disk_gib': 20, 'data_disk_gib': 20,
        'mounts': [{'path': SHARED, 'writable': True}],
        'activate': False, 'ssh_agent': False, 'ssh_config': False,
    }, 'resource/mount configuration differs')
    require(observed['created'] is True and observed['store'] == {
        'disk_formatted': True, 'runtime': 'containerd',
        'resize_required': False, 'format_required': False, 'replacement_required': False,
    }, 'no-format/no-resize resume not established')
    deadline.remaining()


def prepare_retained(plan, observed, deadline):
    """Executable pre-effect refusal. Caller data/watches cannot override it.

    An exit observation is not an identity-bound signal capability. The separate
    administrative route can accept that residual only through a bound operator GO.
    """
    admit_recorded(plan, observed, deadline)
    return {'status': 'refused', 'reason': RESIDUAL, 'signals': 0, 'resumes': 0,
            'runtime_observed': False, 'authority_granted': False}


class ProcessExitWatch:
    """Darwin NOTE_EXIT observation; caller must already hold observation authority.

    Registration and the exit event are observations. They grant no signal
    capability, exit status, VZ closure or permission to resume a backend.
    """
    def __init__(self, pid, deadline):
        deadline.remaining()
        require(type(pid) is int and pid > 1, 'positive process identity required')
        require(hasattr(select, 'kqueue'), 'Darwin kqueue unavailable')
        self.pid = pid
        self.deadline = deadline
        self.queue = select.kqueue()
        self.closed = False
        try:
            change = select.kevent(self.pid, filter=select.KQ_FILTER_PROC,
                                   flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT,
                                   fflags=select.KQ_NOTE_EXIT)
            self.queue.control([change], 0, 0)
        except BaseException:
            self.close()
            raise

    def wait(self):
        require(not self.closed, 'watch already closed')
        left = self.deadline.remaining()
        events = self.queue.control(None, 1, left)
        require(bool(events), 'exit observation timed out')
        self.deadline.remaining()
        event = events[0]
        require(not event.flags & select.KQ_EV_ERROR, 'exit watcher error')
        require(event.ident == self.pid and event.filter == select.KQ_FILTER_PROC
                and event.fflags & select.KQ_NOTE_EXIT, 'wrong exit event')
        return {'pid': self.pid, 'note_exit': True, 'signal_identity_bound': False,
                'exit_status_verified': False, 'vz_closed': False}

    def close(self):
        if not self.closed:
            self.closed = True
            self.queue.close()


class ChildExitWatch(ProcessExitWatch):
    """Local proof adapter; never accepts the retained historical PID."""
    def __init__(self, child, deadline):
        require(child.pid != 94093, 'owned child required')
        super().__init__(child.pid, deadline)


class RecordedIntents:
    """Pure transcript consumer for labeled fixture proofs, never an executor.

    There is deliberately no callback/subprocess/signal interface. A recorded
    intent is not native capability, closure evidence or an armed GO.
    """
    def __init__(self, deadline):
        self.deadline = deadline
        self.signals = 0
        self.resumes = 0

    def signal(self, registration, same_identity, fixture_owned):
        self.deadline.remaining()
        require(registration is True and same_identity is True and fixture_owned is True,
                'fixture signal admission refused')
        require(self.signals == 0 and self.resumes == 0, 'single signal already consumed')
        self.signals += 1
        return ('owned-child-fixture-only', 'SIGINT')

    def resume(self, closure):
        self.deadline.remaining()
        require(self.signals == 1 and self.resumes == 0, 'resume ordering/limit refused')
        require(closure == {'fixture_only': True, 'watched_exit': True,
                           'exit_status_verified': True, 'vz_closed': True,
                           'profile_stopped': True, 'holders_settled': True,
                           'disk_identity_unchanged': True}, 'closure unknown')
        self.resumes += 1
        return {'fixture_only': True, 'argv': RESUME, 'execute': False}
