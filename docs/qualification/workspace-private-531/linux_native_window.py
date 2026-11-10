"""Linux custody and physical writer checks around the shared session driver."""
import json
from pathlib import Path
import subprocess
import time

import runtime
import linux_native_storage as storage
from common import exclusive, exclusive_bytes, require, run


class Window(runtime.Window):
    def __init__(self, cell, inputs_hash, go, *, exploration_authority=None):
        self.command_count = 0
        super().__init__(cell, inputs_hash, go, exploration_authority=exploration_authority)

    def inventory(self):
        return runtime.engine_inventory(self.engine_command)

    def engine_command(self, *args, **kwargs):
        if 'inspect' in args:
            kwargs['protected_output'] = True
        return super().engine_command(*args, **kwargs)

    def guest(self, args, **kwargs):
        if args and args[0] in ('cat', 'printenv'):
            kwargs['protected_output'] = True
        return super().guest(args, **kwargs)

    def up(self):
        result = super().up()
        rows = json.loads(self.engine_command('inspect', self.container).stdout)
        require(len(rows) == 1 and rows[0]['Id'] == self.container, 'workload inspect differs')
        bound = storage.verify_workload(rows[0], self.root, self.cell['native_engine_root'])
        self.record('bounded-native-workload', **bound)
        gate = self.cell.get('native_writer_gate')
        if gate:
            self.command_count += 1
            name = 'writer-gate-%04d' % self.command_count
            exclusive(name + '.request.json', {'nonce': gate, 'container': self.container}, dir_fd=self.root_fd)
            acknowledgement = self.base / (name + '.observed.json')
            deadline = min(self.end, time.monotonic() + 90)
            while not acknowledgement.exists():
                require(time.monotonic() < deadline, 'live native writer observation did not return')
                time.sleep(.1)
            observed = json.loads(acknowledgement.read_bytes())
            require(observed == {'nonce': gate, 'complete': True}, 'live writer observation refused')
            self.record('bounded-native-writers', observation=name)
        return result

    def record_exploratory_client(self, label, status, stdout, stderr):
        child = getattr(self, '_last_child', None)
        if child is not None and child.poll() is not None and not getattr(child, 'retained', False):
            self.retain_child(child, stdout, stderr, True)
            child.retained = True
        super().record_exploratory_client(label, status, stdout, stderr)

    def command(self, args, *, check=True, data=None, cwd=None, env=None, timeout=None, activation=False, protected_output=False):
        self.verify_root()
        remaining = self.end - time.monotonic()
        require(remaining > 0, 'native window deadline expired')
        limit = min(timeout if timeout is not None else (300 if activation else 60), remaining)
        environment = self.env if env is None else env
        if not self.exploratory:
            return super().command(args,check=check,data=data,cwd=cwd,env=env,timeout=timeout,
                                   activation=activation,protected_output=protected_output)
        self.command_count += 1
        stem = 'explore-command-%04d' % self.command_count
        exclusive(stem + '.intent.json', {'argv': [str(arg) for arg in args],
                  'cwd': str(cwd or self.root), 'timeout_seconds': limit,
                  'protected_output_omitted': protected_output}, dir_fd=self.root_fd)
        try:
            result = run(args, cwd=cwd or self.root, env=environment, timeout=limit, check=False, data=data)
        except subprocess.TimeoutExpired as error:
            if not protected_output:
                for name in ('stdout', 'stderr'):
                    exclusive_bytes(stem + '.' + name, getattr(error, name) or b'', dir_fd=self.root_fd)
            exclusive(stem + '.exit.json', {'exit': None, 'timeout': True,
                      'remote_closure': 'unknown'}, dir_fd=self.root_fd)
            raise
        if not protected_output:
            for name in ('stdout', 'stderr'):
                exclusive_bytes(stem + '.' + name, getattr(result, name), dir_fd=self.root_fd)
        exclusive(stem + '.exit.json', {'exit': result.returncode}, dir_fd=self.root_fd)
        require(not check or result.returncode == 0, 'exploratory command failed: ' + stem)
        return result


    def spawn(self, args):
        self.verify_root()
        require(time.monotonic() < self.end, 'native window deadline expired')
        child = subprocess.Popen(args, cwd=self.root, env=self.env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if self.exploratory:
            self.command_count += 1
            child.exploratory_stem = 'explore-spawn-%04d' % self.command_count
            exclusive(child.exploratory_stem + '.intent.json',
                      {'argv': [str(arg) for arg in args], 'local_pid': child.pid,
                       'cwd': str(self.root), 'stdin': 'eof'}, dir_fd=self.root_fd)
        self._last_child = child
        return child


    def retain_child(self, child, stdout, stderr, complete):
        if self.exploratory:
            stem = child.exploratory_stem
            exclusive_bytes(stem + '.stdout', bytes(stdout), dir_fd=self.root_fd)
            exclusive_bytes(stem + '.stderr', bytes(stderr), dir_fd=self.root_fd)
            exclusive(stem + '.exit.json', {'local_pid': child.pid, 'waited_exit': child.returncode,
                      'streams_complete': complete, 'remote_closure': 'not established by client exit'},
                      dir_fd=self.root_fd)

