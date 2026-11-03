"""One reviewed image preparation; caller must supply Queue scope and PID supervision.

No CLI, start, install, alternate builder, cleanup or native qualification route.
"""
import hashlib
import datetime
import json
import os
from pathlib import Path
import re
import time

from common import artifact, digest, exclusive, open_qualification_root, require, verify_qualification_root
import image_context

PROFILE = 'effigy-531-qualification'
CONTROL = Path('/Volumes/Builds/scratch/effigy/531c')
SHARED = image_context.SHARED


def validate_plan(plan):
    require(plan['profile'] == PROFILE and plan['colima_home'] == str(CONTROL / 'c'),
            'exact isolated profile/home required')
    require(plan['control']['path'] == str(CONTROL) and plan['shared']['path'] == str(SHARED),
            'exact retained roots required')
    require(plan['engine_id'] == '9ea86682-b930-4da6-9901-73681bd7ba85'
            and plan['machine_id'] == '52aa90d932e84f0382bd2bbde03ba72a', 'retained runtime identity required')
    require(type(plan['budget_seconds']) is int and 0 < plan['budget_seconds'] <= 1058,
            'remaining 1058-second preparation cap required')
    started = datetime.datetime.fromisoformat(plan['started_at_utc'])
    deadline = datetime.datetime.fromisoformat(plan['deadline_utc'])
    require(started.tzinfo is not None and deadline.tzinfo is not None
            and 0 < (deadline - started).total_seconds() <= 1058
            and started <= datetime.datetime.now(datetime.timezone.utc) < deadline,
            'original bounded preparation deadline required')
    require(re.fullmatch(r'[0-9a-f]{32}', plan['token']), 'fresh context token required')
    require(plan['recipe']['sha256'] == image_context.RECIPE_SHA256
            and plan['context_program_sha256'] == digest(image_context.__file__)
            and plan['preparation_program_sha256'] == digest(__file__), 'reviewed recipe/program required')
    require(isinstance(plan['guest_parent'], dict) and set(plan['guest_parent']) == {'path', 'device', 'inode', 'uid'},
            'observed private guest parent required')
    require(not plan.get('environment'), 'ambient overrides prohibited')


def environment():
    value = {key: os.environ[key] for key in ('PATH', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'TMP', 'TEMP')
             if key in os.environ}
    value.update(COLIMA_HOME=str(CONTROL / 'c'), COLIMA_CACHE_HOME=str(CONTROL / 'cache'),
                 XDG_CONFIG_HOME=str(CONTROL / 'config'), DOCKER_CONFIG=str(CONTROL / 'docker-config'))
    return value  # No inherited LIMA_HOME or registry/agent credentials.


def guest_command(colima, seconds, *args):
    return [str(colima), 'ssh', '-p', PROFILE, '--', 'timeout', '-k', '2', str(seconds), *args]


def probe_command(colima):
    # Missing Python/findmnt refuses before any transfer. No install or guessed utility path.
    return guest_command(colima, 20, 'python3', '-I', '-B', '-c',
                         Path(image_context.__file__).read_text(), 'probe')


def admit(plan, status, engine_id, guest):
    require(status['display_name'] == 'colima [profile=' + PROFILE + ']'
            and status['runtime'] == 'containerd' and status['arch'] == 'aarch64'
            and status['driver'] == 'macOS Virtualization.Framework' and status['mount_type'] == 'virtiofs'
            and status['kubernetes'] is False
            and status['cpu'] == 2 and status['memory'] == 2147483648
            and status['disk'] == 21474836480
            and status['containerd_socket'] == 'unix://' + str(CONTROL / 'c' / PROFILE / 'containerd.sock')
            and status['buildkitd_socket'] == 'unix://' + str(CONTROL / 'c' / PROFILE / 'buildkitd.sock'),
            'running isolated runtime differs')
    require(engine_id == plan['engine_id'] and guest['machine_id'] == plan['machine_id']
            and guest['architecture'] == 'aarch64', 'actual engine/VM identity differs')
    require(guest['parent'] == plan['guest_parent'] and len(guest['storage']) == 1
            and guest['storage'][0]['fstype'] == 'ext4', 'private native guest storage differs')
    require(len(guest['shares']) == 1 and guest['shares'][0]['target'] == str(SHARED)
            and guest['shares'][0]['source'] == 'lima-4661d5013c960385'
            and guest['shares'][0]['fstype'] == 'virtiofs'
            and 'rw' in guest['shares'][0]['options'].split(','), 'actual sharing differs')


def prepare(plan, call):
    """Execute only inside a fresh owner-bound approved continuation.

    call(argv, env, timeout, data) must bound/wait/log each exact host PID and exit,
    return CompletedProcess, and retain Unknown. Tests double only this external I/O.
    Queue's actual GO is required by the caller; this helper cannot grant authority.
    """
    validate_plan(plan)
    owner = artifact(plan['owner_receipt'])
    require(owner == CONTROL / 'owner.json', 'exact retained owner receipt required')
    receipt = json.loads(owner.read_bytes())
    require(receipt['task'] == '12563bc2-e1fe-421d-94b5-6bb1df991eb5'
            and receipt['run'] == '06266c0c-08a4-4592-9dd3-e4240db42c77'
            and receipt['roots'] == {'control': plan['control'], 'shared': plan['shared']},
            'retained owner receipt binding differs')
    colima = artifact(plan['colima_binary'])
    require(plan['colima_binary']['sha256'] == '55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf',
            'reviewed Colima executable required')
    recipe = artifact(plan['recipe']).read_bytes()
    require(0 < len(recipe) <= 8192, 'bounded recipe required')
    deadline = datetime.datetime.fromisoformat(plan['deadline_utc'])
    end = time.monotonic() + min(plan['budget_seconds'],
        (deadline - datetime.datetime.now(datetime.timezone.utc)).total_seconds())
    _, control_fd = open_qualification_root(plan['control'])
    try:
        _, shared_fd = open_qualification_root(plan['shared'])
        try:
            def invoke(argv, seconds=30, data=None):
                require(digest(image_context.__file__) == plan['context_program_sha256']
                        and digest(__file__) == plan['preparation_program_sha256']
                        and digest(plan['recipe']['path']) == image_context.RECIPE_SHA256
                        and digest(owner) == plan['owner_receipt']['sha256'],
                        'reviewed preparation source changed')
                verify_qualification_root(plan['control'], control_fd)
                verify_qualification_root(plan['shared'], shared_fd)
                require(not os.listdir(shared_fd), 'qualification root must stay empty')
                require(time.monotonic() + seconds + 30 < end, 'insufficient original deadline')
                require(datetime.datetime.now(datetime.timezone.utc) < deadline, 'original deadline expired')
                result = call(argv, env=environment(), timeout=seconds, data=data)
                require(result.returncode == 0, 'preparation command failed; retain Unknown, no retry')
                return result.stdout
            def observe():
                status = json.loads(invoke([str(colima), 'status', '-p', PROFILE, '--json']))
                engine = invoke([str(colima), '-p', PROFILE, 'nerdctl', '--', 'info',
                                 '--format', '{{.ID}}']).decode().strip()
                guest = json.loads(invoke(probe_command(colima), 25))
                admit(plan, status, engine, guest)
            observe()  # Remount history makes previous observations insufficient.
            for args in (['ps', '-a', '--format', '{{json .}}'],
                         ['volume', 'ls', '--format', '{{json .}}'],
                         ['images', '--format', '{{json .}}']):
                require(not invoke([str(colima), '-p', PROFILE, 'nerdctl', '--'] + args).strip(),
                        'retained empty runtime inventory changed')
            exclusive('runtime-image.started.json', {'plan_sha256': hashlib.sha256(
                json.dumps(plan, sort_keys=True).encode()).hexdigest()}, dir_fd=control_fd)
            name = 'effigy-531-image-' + plan['token']
            spec = dict(parent=plan['guest_parent'], name=name, sha256=image_context.RECIPE_SHA256)
            code = Path(image_context.__file__).read_text()
            context = json.loads(invoke(guest_command(colima, 20, 'python3', '-I', '-B', '-c', code,
                                                     'stage', json.dumps(spec)), 25, recipe))
            spec['context'] = context
            require(context['path'] == str(Path(spec['parent']['path']) / name)
                    and context['sha256'] == image_context.RECIPE_SHA256, 'transfer receipt differs')
            exclusive('runtime-image-context.json', context, dir_fd=control_fd)
            observe()
            verified = json.loads(invoke(guest_command(colima, 20, 'python3', '-I', '-B', '-c', code,
                                                       'verify', json.dumps(spec)), 25))
            require(verified == context, 'context replaced before build')
            # Exactly one existing nerdctl builder; sudo is Colima's maintained containerd privilege.
            # Explicit nonexistent Docker-config child prevents reading a private registry config.
            builder = ['sudo', '-n', 'env', 'DOCKER_CONFIG=' + context['path'] + '/uncreated-registry-config', 'nerdctl']
            build_seconds = min(600, int(end - time.monotonic()) - 100)
            require(build_seconds > 0, 'no build budget remains')
            exclusive('runtime-image-build.started.json', {'context': context}, dir_fd=control_fd)
            invoke(guest_command(colima, build_seconds, *builder, 'build', '--platform', 'linux/arm64',
                                 '--provenance=mode=max', '--tag', 'effigy-531-runtime:r1', context['path']),
                   build_seconds + 5)
            image = json.loads(invoke([str(colima), '-p', PROFILE, 'nerdctl', '--', 'image', 'inspect',
                                      'effigy-531-runtime:r1']))[0]
            require(image['Architecture'] == 'arm64' and image['Os'] == 'linux'
                    and re.fullmatch(r'sha256:[0-9a-f]{64}', image['Id']), 'actual ARM64 image required')
            exclusive('runtime-image-inspect.json', image, dir_fd=control_fd)
            observe()
            require(json.loads(invoke(guest_command(colima, 20, 'python3', '-I', '-B', '-c', code,
                    'verify', json.dumps(spec)), 25)) == context, 'context changed after build')
            exclusive('runtime-image-tools.started.json', {'image': image['Id']}, dir_fd=control_fd)
            tools = invoke(guest_command(colima, 25, *builder, 'run', '--rm', '--pull=never', '--network', 'none',
                            '--name', 'effigy-531-tools-' + plan['token'], image['Id'],
                            '/usr/bin/timeout', '-k', '2', '20', '/bin/sh', '-ec',
                            'test -x /lib/ld-linux-aarch64.so.1; test "$(id -u)" = 0; '
                            'python3 -c "import ssl; print(ssl.OPENSSL_VERSION)"; python3 --version; '
                            '/lib/ld-linux-aarch64.so.1 --version; stat --version; printenv --version; '
                            'dpkg-query -W python3 coreutils libc6 ca-certificates'), 30)
            exclusive('runtime-image-tools.json', {'stdout_sha256': hashlib.sha256(tools).hexdigest()}, dir_fd=control_fd)
            return dict(context=context, image=image, tools=tools.decode(), qualification=False)
        finally:
            os.close(shared_fd)
    finally:
        os.close(control_fd)
