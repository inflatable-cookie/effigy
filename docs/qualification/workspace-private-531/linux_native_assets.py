"""Build native roles and a GNU guest artifact, then bind their actual bytes."""
import json
from pathlib import Path
import shlex
import sys
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import linux_native_build as build
import linux_native_run as native_run
import linux_native_session as session


def script(binding, roles, seed):
    source = binding['guest_source']
    # All paths are produced by our fixed-root builder, never arbitrary shell
    # text from a caller. Quote every value nevertheless.
    qsource, qroles, qbase = map(shlex.quote, (source, roles, build.BASE))
    commands = {
        'fixture': 'build --locked -p effigy-kernel --features fault-injection,workspace-private-qualification --bin kernel-custody-fixture',
        'test-inputs': 'nextest list --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection,workspace-private-qualification --lib --test custody --message-format json --list-type binaries-only > ' + qroles + '/binaries.original.json',
        'cargo-metadata': 'metadata --format-version 1 --no-deps > ' + qroles + '/cargo.json',
    }
    catalog = '\n'.join('[tasks.' + name + ']\nrun = ' + json.dumps(
        'cd ' + qsource + ' && sh scripts/oxide-selector.sh ' + command)
        + '\ntimeout_ms = 900000\n' for name, command in commands.items())
    catalog += ('\n[tasks.runtime-image]\nrun = ' + json.dumps('python3 -I -B ' + qsource
                + '/docs/qualification/workspace-private-531/linux_native_engine.py --build-resident '
                + qroles + '/runtime.iid') + '\ntimeout_ms = 240000\n')
    wrapper = ('#!/bin/sh\nset -eu\nbase=' + qbase + '\n'
               'export RUSTUP_HOME="$base/toolchain/rustup" CARGO_HOME="$base/toolchain/cargo"\n'
               'export PATH="$base/tools/bin:$base/tools/zig-tools/cargo-zigbuild-aarch64-unknown-linux-gnu:$base/tools/zig-tools/zig-aarch64-linux-0.15.2:$CARGO_HOME/bin:/usr/bin:/bin"\n'
               'export EFFIGY_OXIDE_ESCAPE=cargo CARGO_BUILD_JOBS=4 ZIG_GLOBAL_CACHE_DIR="$base/toolchain/zig-cache"\n'
               'exec python3 -I -B '
               + qsource + '/docs/qualification/workspace-private-531/linux_native_artifact.py '
               + qsource + ' ' + qroles + '/effigy-production\n')
    overlay = '[tasks."workspace:linux:artifact"]\nrun = ' + json.dumps(
        'exec ' + shlex.quote(roles + '/native-artifact.sh')) + '\ntimeout_ms = 1800000\n'
    finish = '''import hashlib,json,shutil,sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd()/'docs/qualification/workspace-private-531'))
import common
from runtime import restarted_inventory
source=Path(SOURCE)
roles=Path(ROLES)
value=json.loads(SEED)
cell=value['cell']
metadata=json.loads((roles/'binaries.original.json').read_text())
def visit(row):
 if isinstance(row,dict):
  for key,child in list(row.items()):
   if key in ('binary-path','binary_path'):
    role='custody-tests' if Path(child).name.startswith('custody-') else 'kernel-tests'
    shutil.copy2(child,roles/role);row[key]=str(roles/role)
   else:visit(child)
 elif isinstance(row,list):
  for child in row:visit(child)
visit(metadata)
(roles/'binaries.json').write_text(json.dumps(metadata))
def bind(path):
 return {'path':str(path),'sha256':common.digest(path)}
cell.update(source_root=str(source),source_head=common.source_head(source))
cell['runtime_image']=(roles/'runtime.iid').read_text().strip()
cell['runtime_image_receipt']=bind(roles/'runtime.json')
for key,name in {'candidate':'effigy-candidate','production':'effigy-production',
 'utility':'workspace-qualification-fixture','kernel_fixture':'kernel-custody-fixture',
 'kernel_tests':'kernel-tests','custody_tests':'custody-tests',
 'binaries_metadata':'binaries.json','cargo_metadata':'cargo.json'}.items():
 cell[key]=bind(roles/name)
cell['nextest_config']=bind(source/'tests/config/gateway-nextest.toml')
artifact=source/'.effigy/linux-release/artifacts/effigy-aarch64-unknown-linux-gnu'
for key,path in {'guest_artifact':artifact,'guest_producer_receipt':artifact.with_suffix('.receipt'),
 'guest_active_version':artifact.with_suffix('.active-version'),
 'guest_install_identity':artifact.with_suffix('.install-identity')}.items():cell[key]=bind(path)
for key in ('old_binary','nextest_binary','engine_binary'):common.artifact(cell[key])
env={'PATH':'/usr/bin:/bin','HOME':'/var/lib/effigy-829/rootful/client/home','DOCKER_CONFIG':cell['docker_config']}
def engine(*args):return common.run([cell['engine_binary']['path'],'--context',cell['engine_identity'],*args],env=env,timeout=30)
prior=cell['baseline']
observed=restarted_inventory(engine,prior)
# Docker recreates its default bridge identity on daemon restart. This is a
# new, unsealed window after the recorded resume, not a changed baseline in
# an existing window. Keep both complete observations; no missing resource is
# excused, and Window still requires exact equality throughout the new run.
(roles/'prior-baseline.json').write_text(json.dumps(prior,indent=2))
cell['baseline']=observed
image=json.loads(engine('image','inspect',cell['runtime_image']).stdout)
if len(image)!=1 or image[0]['Id']!=cell['runtime_image']:raise RuntimeError('runtime image differs')
(roles/'seed.json').write_text(json.dumps(value,indent=2))
print(json.dumps({'seed':str(roles/'seed.json'),'sha256':common.digest(roles/'seed.json'),'exploratory':True,'qualification_credit':False}))
'''.replace('SOURCE', repr(source)).replace('ROLES', repr(roles)).replace('SEED', repr(json.dumps(seed)))
    return f'''set -eu
umask 077
base={qbase}
source={qsource}
roles={qroles}
export RUSTUP_HOME="$base/toolchain/rustup" CARGO_HOME="$base/toolchain/cargo"
export PATH="$base/tools/bin:$base/tools/zig-tools/cargo-zigbuild-aarch64-unknown-linux-gnu:$base/tools/zig-tools/zig-aarch64-linux-0.15.2:$CARGO_HOME/bin:/usr/bin:/bin"
export EFFIGY_OXIDE_ESCAPE=cargo CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false
export CARGO_TARGET_DIR="$base/build-cache"
export ZIG_GLOBAL_CACHE_DIR="$base/toolchain/zig-cache" TMPDIR="$base/artifact-tmp" TMP="$base/artifact-tmp" TEMP="$base/artifact-tmp"
test -d "$roles" && test "$(stat -c %a \"$roles\")" = 700
install -m 700 "$source/target/debug/effigy" "$roles/effigy-production"
cd "$source"
"$roles/effigy-production" build:workspace:private-qualification
install -m 700 "$CARGO_TARGET_DIR/debug/effigy" "$roles/effigy-candidate"
install -m 700 "$CARGO_TARGET_DIR/debug/workspace-qualification-fixture" "$roles/workspace-qualification-fixture"
mkdir -m 700 "$roles/driver"
cat > "$roles/driver/effigy.toml" <<'CATALOG'
{catalog}CATALOG
"$roles/effigy-production" skill run --path "$roles/driver" fixture
"$roles/effigy-production" skill run --path "$roles/driver" test-inputs
"$roles/effigy-production" skill run --path "$roles/driver" cargo-metadata
"$roles/effigy-production" skill run --path "$roles/driver" runtime-image
install -m 700 "$CARGO_TARGET_DIR/debug/kernel-custody-fixture" "$roles/kernel-custody-fixture"
cat > "$roles/native-artifact.sh" <<'WRAPPER'
{wrapper}WRAPPER
chmod 700 "$roles/native-artifact.sh"
cat > "$source/effigy.local.toml" <<'OVERLAY'
{overlay}OVERLAY
"$roles/effigy-production" workspace:linux:artifact
python3 -I -B - <<'FINISH'
{finish}FINISH
'''


def prepare(root, binding, seed):
    common.require(binding.get('complete') is True and binding['source_pin'] == seed['cell']['source_head'],
                   'exact native build source pin required')
    source = Path(binding['guest_source'])
    common.require(source.is_relative_to(Path(build.BASE) / 'builds') and source.name == 'source',
                   'owned native source required')
    attempt = native_run.Attempt(root)
    control = root / 'control'
    ssh = common.isolated_ssh_prefix('effigy-531-vault', build.file_input(control / 'ssh_config'),
                                    build.file_input(control / 'ssh_known_hosts'), control, attempt.root)
    roles = build.BASE + '/artifacts/native-' + uuid.uuid4().hex
    payload = script(binding, roles, seed).encode()
    common.exclusive_bytes(attempt.root / 'build-roles.sh', payload)
    native_run.channel_directory(attempt, roles)
    build.run_script(attempt, root, ssh, roles, payload, seconds=2100)
    data = native_run.channel_read(attempt, ssh, roles + '/seed.json', limit=1024 * 1024)
    target = attempt.root / 'assets.json'
    common.exclusive_bytes(target, data)
    return target
