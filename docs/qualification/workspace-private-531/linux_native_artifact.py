"""Guest-native GNU artifact producer for unsealed Linux exploration.

The installed-identity sidecar belongs to Effigy's consumer. This producer never
writes it. Its receipt describes an actual Zig build, not the Colima recipe.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import struct
import subprocess
import sys
import tempfile

TRIPLE = 'aarch64-unknown-linux-gnu'
TARGET = TRIPLE + '.2.31'
FLOOR = (2, 31)
BUILD_HOME = Path('/Volumes/Stash/scratch/effigy/linux-native-829/toolchain/build-home')
TARGET_HOME = Path('/Volumes/Stash/scratch/effigy/linux-native-829/build-cache')


def validate_elf(header, program_headers, versions):
    if (len(header) < 64 or header[:7] != b'\x7fELF\x02\x01\x01'
            or struct.unpack_from('<H', header, 18)[0] != 183):
        raise ValueError('artifact is not an ARM64 little-endian ELF64 executable')
    interpreters = re.findall(r'\[Requesting program interpreter: ([^\]]+)\]', program_headers)
    if interpreters != ['/lib/ld-linux-aarch64.so.1']:
        raise ValueError('artifact does not use the ARM64 GNU loader')
    tags = set(re.findall(r'Name: (GLIBC_[^\s]+)', versions))
    if not tags:
        raise ValueError('GNU version requirements are missing')
    for tag in tags:
        match = re.fullmatch(r'GLIBC_(\d+)\.(\d+)(?:\.(\d+))?', tag)
        if not match or tuple(int(part or 0) for part in match.groups()) > (*FLOOR, 0):
            raise ValueError('artifact exceeds the admitted GNU floor: ' + tag)
    return sorted(tags)


def publish(path, data, mode):
    # One role at a time; the receipt is published last. An interrupted or
    # mismatched set is refused by the existing digest/version consumer.
    fd, temporary = tempfile.mkstemp(prefix='.native-', dir=path.parent)
    with os.fdopen(fd, 'wb') as output:
        os.fchmod(output.fileno(), mode)
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, path)


def produce(source, effigy):
    if sys.platform != 'linux' or platform.machine() not in ('aarch64', 'arm64'):
        raise RuntimeError('this producer requires the native ARM64 Linux guest')
    if os.geteuid() != 1001:
        raise RuntimeError('this producer requires the dedicated native build account')
    if os.environ.get('EFFIGY_LINUX_RELEASE_TRIPLE', TRIPLE) != TRIPLE:
        raise RuntimeError('unsupported requested artifact target')
    if os.environ.get('EFFIGY_OXIDE_ESCAPE') != 'cargo':
        raise RuntimeError('the named missing-Oxide guest route must be explicit')
    source, effigy = (path.resolve(strict=True) for path in (source, effigy))
    pin = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source).decode().strip()
    if not re.fullmatch('[0-9a-f]{40}', pin):
        raise RuntimeError('missing exact source pin')
    if subprocess.check_output(['git', 'diff', 'HEAD', '--'], cwd=source):
        raise RuntimeError('tracked source differs from the pinned commit')
    driver = Path(tempfile.mkdtemp(prefix='native-artifact-', dir=os.environ['TMPDIR']))
    command = ('cd ' + shlex.quote(str(source))
               + ' && sh scripts/oxide-selector.sh zigbuild --locked --release --bin effigy --target ' + TARGET)
    (driver / 'effigy.toml').write_text('[tasks.compile]\nrun = ' + json.dumps(command)
                                      + '\ntimeout_ms = 1800000\n')
    # cargo-zigbuild puts linker wrappers under HOME's cache. A new fixture
    # HOME otherwise changes rustc's linker path and rebuilds every dependency.
    # Keep it under the owned base: the acquisition service hides /home.
    for directory in (BUILD_HOME, TARGET_HOME):
        directory.mkdir(mode=0o700, exist_ok=True)
        info = directory.lstat()
        if (directory.resolve(strict=True) != directory or not directory.is_dir()
                or info.st_uid != os.geteuid() or info.st_mode & 0o777 != 0o700):
            raise RuntimeError('private stable build directory differs')
    build_env = dict(os.environ, HOME=str(BUILD_HOME), CARGO_TARGET_DIR=str(TARGET_HOME))
    subprocess.run([effigy, 'skill', 'run', '--path', driver, 'compile'], cwd=source,
                   env=build_env, check=True)
    binary = TARGET_HOME / TRIPLE / 'release/effigy'
    size = binary.stat().st_size
    if not 0 < size <= 200 * 1024 * 1024:
        raise RuntimeError('artifact exceeds its binary cap')
    data = binary.read_bytes()
    headers = subprocess.check_output(['readelf', '-W', '-l', binary], text=True)
    versions = subprocess.check_output(['readelf', '-W', '--version-info', binary], text=True)
    tags = validate_elf(data[:64], headers, versions)
    version = subprocess.check_output([binary, '--version'], text=True).strip()
    match = re.fullmatch(r'effigy v([^\s]+)', version)
    if not match:
        raise RuntimeError('artifact does not report an Effigy active version')
    if subprocess.check_output([effigy, '--version'], text=True).strip() != version:
        raise RuntimeError('producer host and GNU artifact versions differ')
    # The real publisher binds local artifacts to the host freshness anchor.
    # Match that contract from the actual prepared host's metadata; never stamp
    # a release identity or borrow a different checkout's install record.
    anchors = [source / '.local-install/bin/effigy', source / 'target/debug/effigy']
    anchors = [path for path in anchors if path.is_file()]
    if not anchors:
        raise RuntimeError('native host freshness anchor is unavailable')
    anchor = max(anchors, key=lambda path: path.stat().st_mtime_ns)
    if anchor.resolve(strict=True) != anchor:
        raise RuntimeError('native host freshness anchor is aliased')
    anchor_info = anchor.stat()
    identity = 'local:v%s:%d:%d\n' % (match[1], anchor_info.st_size, anchor_info.st_mtime_ns)
    destination = source / '.effigy/linux-release/artifacts'
    destination.mkdir(mode=0o700, parents=True, exist_ok=True)
    artifact = destination / ('effigy-' + TRIPLE)
    sha = hashlib.sha256(data).hexdigest()
    receipt = ('release_triple=' + TRIPLE + '\nsha256=' + sha
               + '\nbuild_kind=native-zig-workspace-artifact\nsource_commit=' + pin
               + '\ngnu_target=' + TARGET
               + '\nproducer_sha256=' + hashlib.sha256(Path(__file__).read_bytes()).hexdigest() + '\n').encode()
    publish(artifact, data, 0o700)
    publish(artifact.with_suffix('.active-version'), (match[1] + '\n').encode(), 0o600)
    publish(artifact.with_suffix('.receipt'), receipt, 0o600)
    if (anchor.stat().st_size, anchor.stat().st_mtime_ns) != (anchor_info.st_size, anchor_info.st_mtime_ns):
        raise RuntimeError('native host freshness anchor changed during publication')
    publish(artifact.with_suffix('.install-identity'), identity.encode(), 0o600)
    print(json.dumps({'artifact': str(artifact), 'sha256': sha, 'source_pin': pin,
                      'glibc_requirements': tags, 'exploratory': True,
                      'qualification_credit': False}, indent=2))


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('usage: linux_native_artifact.py SOURCE EFFIGY')
    produce(*(Path(value) for value in sys.argv[1:]))
