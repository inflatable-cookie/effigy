"""Fixed Parallels host-inventory helper, its boot-bound reader and the supported CLI grammar.

Production dispatch is exactly:

  /usr/bin/python3 -I -B THIS inventory

No other arguments. The helper authenticates the installed Parallels Desktop command-line tool by
content, runs the one bounded fixed read below, parses its exact output grammar and passes the
original bytes through on success; every other outcome refuses with an exact phase. Tests inject
transport at read_inventory and at the helper's dispatch seam; this file never opens a VM, never
dlopens the SDK, and never calls PrlApi_InitEx.

Supported route. The Parallels Virtualization SDK is unsupported from Desktop 19.3.0 (Parallels KB
130091, retained original sha256 9319861841e1c454…); Parallels directs automation to its
command-line interface. The one fixed read is the documented configuration read

  prlctl list --info <vm_uuid> --json

("Displays the VM configuration information" for the VM selected by ID, JSON output; Parallels
Desktop developers guide, list-virtual-machines, retained original sha256 79fc5b3cb70d05ffe…).
The exact tool is pinned: bundle CLI
`/Applications/Parallels Desktop.app/Contents/MacOS/prlctl`, sha256 2f61a1c60db2d5159…, the binary
the genuine retained 747 occurrence ran, inside Desktop 27.1.0 build 58756 (the app Info.plist
carries CFBundleShortVersionString 27.1.0 and CFBundleVersion 58756). The environment is the
retained one (LANG=C, PATH=/usr/bin:/bin). `/usr/local/bin/prlctl` is a symlink to the vendor
`parallels_wrapper`, which selects its tool from basename($0) and exits 9 for its own name; the
canonical bundle path only, and an aliased or drifted binary refuses before any dispatch.

The output grammar is the genuine retained 747 grammar (offline retained originals; no query ran
to learn it): one JSON object in a one-element array naming the reviewed VM ID, `Home` with its
trailing slash, `Home path` …/config.pvs, `State`, and a `Hardware` map whose keys are exactly the
reviewed closed set cpu/memory/video/memory_quota/hdd0/cdrom0/usb/net0/sound0. Inside it, `hdd0`,
`cdrom0` and `usb` must equal the reviewed shapes exactly; every other reviewed section must be a
flat map of scalar leaves, so no storage device can hide under an allowed section. Exact schema
checks stop at the inventory premises: other top-level metadata (Name, OS, Uptime, sharing
sections, ...) is not pinned. A duplicate JSON key at any depth, an unknown Hardware section, a
second hddN/cdromN, an extra field, a changed value or a non-scalar leaf refuses.

read_inventory is the production reader entry. It binds the helper bytes to the caller's
independent pin (never to a digest this file computes from itself), runs the one bounded dispatch
of this helper, and accepts the result only inside the caller's already authenticated same-VM boot
bracket: the caller's read_boot_id() is called once immediately before and once immediately after
the dispatch and both must equal the boot identity that was authenticated for this VM. The
selected VM UUID and home are derived from the helper's own output, so no caller constant or
manually appended vm/boot field is a reading. The record keeps the exact bounded original
stdout/stderr of the dispatch and its waited exit. Its argv field is the fixed helper dispatch
route; its api field names the accessor — the documented `prlctl list --info --json` configuration
read. Neither is a checked boolean: none of them asserts more than the api name, the argv, the
waited exit and the bounded original streams establish. Configuration evidence is the accepted
narrow Parallels trust (decision 64b1fe6f v2), not independently observed runtime presentation.

SDK history, kept honest: the earlier SDK route (PrlApi_InitEx/LoginLocal/PrlVmCfg_GetAllDevices)
was source-qualified but never executable — its numeric PARALLELS_API_VER and PRL_HANDLE ABI were
never sourced, reader_executable() was False, no Init, Login or SDK read ever ran, and no receipt
claims otherwise. The supported CLI route above replaces it on the required dispatch path; the
SDK surface survey remains recorded in linux-setup.md as history.
"""
import hashlib
import json
import os
import plistlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_support as transfers

HELPER = Path(__file__).resolve()
PYTHON = '/usr/bin/python3'
INVENTORY_ARG = 'inventory'
INVENTORY_TIMEOUT = 30
MAX_BYTES = 64 * 1024

# The exact supported tool. Retained 747 occurrence input pin; matches the installed bundle today.
PRLCTL = '/Applications/Parallels Desktop.app/Contents/MacOS/prlctl'
PRLCTL_SHA256 = '2f61a1c60db2d51591fb8479a248a982a81658f136f1501a94733b7a81731c57'
DESKTOP_INFO_PLIST = '/Applications/Parallels Desktop.app/Contents/Info.plist'
DESKTOP_VERSION = '27.1.0'
DESKTOP_BUILD = '58756'
# The retained 747 occurrence environment: no locale or PATH variation in the fixed read.
CLI_ENV = {'LANG': 'C', 'PATH': '/usr/bin:/bin'}
ACCESSOR = 'prlctl list --info --json'

# The reviewed identity, from the retained 747 originals. linux_setup's constants must equal the
# home; linux_disk_association carries the same reviewed literals and its tests check the equality.
REVIEWED_VM = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
REVIEWED_HOME = '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm'
REVIEWED_DISK_IMAGE = REVIEWED_HOME + '/harddisk1.hdd'
# State of a running VM in the CLI grammar, as the retained originals carry it.
STATE_RUNNING = 'running'

# The reviewed Hardware sections: the complete configured-device inventory of the retained grammar.
HARDWARE_SECTIONS = frozenset({
    'cpu', 'memory', 'video', 'memory_quota', 'hdd0', 'cdrom0', 'usb', 'net0', 'sound0',
})
# The reviewed data disk: the sole image-backed internal disk, SATA position 0, expanding image.
# Values are the retained 747 grammar values, exact, except online-compact: the planner
# reverted hdd0 to SATA with online compaction off on 2026-11-11 (recorded in the
# task-897 closing notes), so the reviewed value moved from 'on' to 'off'. The retained
# 747 bytes still carry 'on' and are kept byte-pinned as grammar history only.
HDD0_REVIEWED = {
    'enabled': True, 'port': 'sata:0', 'image': REVIEWED_DISK_IMAGE,
    'type': 'expanded', 'size': '131072Mb', 'online-compact': 'off',
}
# The reviewed optical device: counted, never treated as a disk.
CDROM0_REVIEWED = {
    'enabled': True, 'port': 'sata:1', 'image': '', 'state': 'disconnected',
}
# The reviewed USB section: no USB device is configured into this VM's inventory; the reviewed
# virtual camera lives in the registered XML config, not in this CLI view.
USB_REVIEWED = {'enabled': True}
# The non-storage sections are checked for closure, not for their irrelevant values: each must be
# a flat map of scalar leaves so nothing can hide under an allowed section.
SCALAR_SECTIONS = frozenset({'cpu', 'memory', 'video', 'memory_quota', 'net0', 'sound0'})

# Every phase read_inventory refuses with, so a caller names the outcome exactly.
READ_PHASES = (
    'pin-missing', 'helper-bytes-differ', 'boot-bracket-missing', 'boot-bracket-malformed',
    'boot-bracket-moved', 'inventory-output-missing', 'inventory-not-exited', 'inventory-exit-nonzero',
    'inventory-truncated', 'inventory-output-encoding', 'inventory-output-grammar',
    'inventory-vm-differs',
)
# The dispatch failures a bounded retry may ride out: the helper never ran to its own
# exact refusal, so no premise was decided and nothing is a finding. Every other phase
# is a decided outcome (a premise refusal, a grammar break, a moved boot bracket) and
# is never retried as transport.
TRANSPORT_PHASES = frozenset({
    'inventory-output-missing', 'inventory-not-exited', 'inventory-exit-nonzero',
})
# Helper-side phases: installed-tool custody and the inner dispatch, printed as status=refused.
TOOL_PHASES = (
    'tool-alias', 'tool-unavailable', 'tool-hash-mismatch', 'tool-plist-unavailable',
    'tool-version-mismatch',
)

UUID_HEX = frozenset('0123456789abcdef')


def _is_uuid(value):
    """Exact lowercase 8-4-4-4-12 hexadecimal VM identity, as the retained grammar carries it."""
    if not isinstance(value, str) or len(value) != 36:
        return False
    parts = value.split('-')
    if [len(part) for part in parts] != [8, 4, 4, 4, 12]:
        return False
    return all(char in UUID_HEX for part in parts for char in part)


class GrammarError(ValueError):
    pass


class Refused(Exception):
    def __init__(self, phase, reason=None):
        super().__init__(phase if reason is None else phase + ': ' + reason)
        self.phase = phase
        self.reason = reason


def helper_digest(path=HELPER):
    """The content digest of the helper bytes. Not authority: the reviewed pin is a literal in the
    consumer (linux_disk_association.HELPER_SHA256), so a modified helper cannot re-pin itself."""
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory_argv(helper=HELPER):
    return [PYTHON, '-I', '-B', str(helper), INVENTORY_ARG]


def cli_argv(vm=REVIEWED_VM, tool=PRLCTL):
    """The one fixed read: the documented list --info configuration read, exact VM ID, JSON."""
    return [tool, 'list', '--info', vm, '--json']


def reader_executable():
    """True: every fact the one supported CLI read needs is sourced and implemented. The earlier
    SDK route returned False here because its ABI was unsourced; that history is not this route."""
    return True


def reader_limitation():
    """'none': the supported CLI read needs no further external source. Kept for the callers that
    must not spend an occurrence on an entry known to refuse; no entry is known to refuse here."""
    return 'none'


def _unique_pairs(pairs):
    """A JSON object decoder that refuses a repeated key at any depth."""
    value = {}
    for key, item in pairs:
        if key in value:
            raise GrammarError('duplicate JSON key %r' % key)
        value[key] = item
    return value


def _scalar(value):
    return value is None or isinstance(value, (str, bool, int, float))


def _flat_section(section, name):
    if not isinstance(section, dict) or not section:
        raise GrammarError('host read Hardware %s is not a reviewed flat section' % name)
    for key, value in section.items():
        if not isinstance(key, str) or not _scalar(value):
            raise GrammarError('host read Hardware %s carries a non-scalar leaf %r' % (name, key))


def _exact_section(section, reviewed, name):
    if not isinstance(section, dict):
        raise GrammarError('host read Hardware %s is not the reviewed section' % name)
    if set(section) != set(reviewed):
        raise GrammarError('host read Hardware %s fields differ from the reviewed shape' % name)
    for key, expected in reviewed.items():
        actual = section[key]
        # Type-exact: JSON true is a Python bool and 1 is an int, so 1 can never stand in for the
        # reviewed enabled flag (and 1.0 for an int) under Python's equal-value comparison.
        if type(actual) is not type(expected) or actual != expected:
            raise GrammarError('host read Hardware %s %s is not the reviewed value' % (name, key))


def _expected_hdd0(pin):
    """The reviewed data-disk shape with the image path bound to the pin's registered home."""
    return dict(HDD0_REVIEWED, image=pin['home'] + '/harddisk1.hdd')


def parse_output(text, pin):
    """Parse the retained CLI grammar exactly. pin supplies the reviewed vm and home.

    Returns the closed configured-device inventory: the VM identity, its home and home_path, the
    state, and the Hardware map with hdd0/cdrom0/usb in their reviewed shapes and the remaining
    reviewed sections as flat scalar maps. Anything outside the grammar refuses."""
    if not isinstance(text, str):
        raise GrammarError('host read output is not text')
    try:
        data = json.loads(text, object_pairs_hook=_unique_pairs)
    except ValueError as exc:
        raise GrammarError('host read output is not exact JSON: %s' % exc) from exc
    if not isinstance(data, list) or len(data) != 1 or not isinstance(data[0], dict):
        raise GrammarError('host read output is not a one-VM JSON array')
    vm = data[0]
    if not _is_uuid(vm.get('ID')):
        raise GrammarError('host read carries no VM identity')  # identity equality is the caller's phase
    home = vm.get('Home')
    if home != pin['home'] + '/':
        raise GrammarError('host read names another VM home')
    if vm.get('Home path') != pin['home'] + '/config.pvs':
        raise GrammarError('host read home path is not the registered config.pvs')
    state = vm.get('State')
    if not isinstance(state, str) or not state:
        raise GrammarError('host read carries no VM state')
    hardware = vm.get('Hardware')
    if not isinstance(hardware, dict) or set(hardware) != HARDWARE_SECTIONS:
        raise GrammarError('host read Hardware is not the reviewed closed section set')
    _exact_section(hardware.get('hdd0'), _expected_hdd0(pin), 'hdd0')
    _exact_section(hardware.get('cdrom0'), CDROM0_REVIEWED, 'cdrom0')
    _exact_section(hardware.get('usb'), USB_REVIEWED, 'usb')
    for name in sorted(SCALAR_SECTIONS):
        _flat_section(hardware.get(name), name)
    return {'vm': vm['ID'], 'home': home, 'home_path': vm['Home path'], 'state': state,
            'hardware': dict(hardware)}


def verify_installed_cli(path=PRLCTL, plist_path=DESKTOP_INFO_PLIST, expected_sha=PRLCTL_SHA256,
                         expected_version=DESKTOP_VERSION, expected_build=DESKTOP_BUILD):
    """Local metadata only: the canonical bundle path, the content hash of the pinned CLI and the
    app Info.plist version. Does not execute the tool."""
    if os.path.realpath(path) != path or not os.path.isabs(path):
        raise Refused('tool-alias', 'the CLI is not at its canonical bundle path')
    try:
        data = Path(path).read_bytes()
    except OSError as exc:
        raise Refused('tool-unavailable') from exc
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected_sha:
        raise Refused('tool-hash-mismatch')
    try:
        info = plistlib.loads(Path(plist_path).read_bytes())
    except (OSError, plistlib.InvalidFileException) as exc:
        raise Refused('tool-plist-unavailable') from exc
    version = str(info.get('CFBundleShortVersionString', ''))
    build = str(info.get('CFBundleVersion', ''))
    if version != expected_version or build != expected_build:
        raise Refused('tool-version-mismatch')
    return {'path': path, 'sha256': digest, 'version': version, 'build': build}


def _helper_refusal(output):
    """The helper's own exact refusal, if its stdout carries one.

    The helper prints exactly one line, `status=refused phase=<phase>[ reason=<reason>]`,
    and exits nonzero when it refuses. Propagating that phase (instead of masking every
    failed dispatch as inventory-not-exited) is what lets the caller tell a decided
    premise refusal from a transport failure that never reached one. Anything else —
    empty, non-ASCII, multi-line or an unknown phase — is not the helper's exact
    refusal and stays a dispatch-shaped outcome."""
    if not isinstance(output, (bytes, bytearray)):
        return None
    try:
        text = bytes(output).decode('ascii')
    except UnicodeDecodeError:
        return None
    lines = text.splitlines()
    if len(lines) != 1 or not lines[0].startswith('status=refused phase='):
        return None
    rest = lines[0][len('status=refused phase='):]
    phase, separator, reason = rest.partition(' reason=')
    if phase not in READ_PHASES:
        return None
    return phase, reason if separator else None


def _checked_dispatch(argv, transport):
    """One bounded waited dispatch; returns (text, raw bytes, stderr). Original streams, waited
    exit, deadline and output cap are the evidence; anything else is refused with an exact phase.
    An injected transport keeps the four-argument fixture contract."""
    result = (transfers.run_transfer if transport is None else transport)(
        argv, None, INVENTORY_TIMEOUT, MAX_BYTES)
    if not isinstance(result, dict):
        raise Refused('inventory-output-missing', 'the dispatch returned no process result')
    output = result.get('stdout')
    stderr = result.get('stderr')
    exit_code = result.get('waited_exit')
    if not isinstance(output, (bytes, bytearray)):
        raise Refused('inventory-output-missing')
    if result.get('disposition') != 'exited' or exit_code != 0:
        refusal = _helper_refusal(output)
        if refusal is not None:
            raise Refused(*refusal)
    if result.get('disposition') != 'exited':
        raise Refused('inventory-not-exited', 'disposition %r' % (result.get('disposition'),))
    if exit_code != 0:
        raise Refused('inventory-exit-nonzero', 'waited exit %r' % (exit_code,))
    if len(output) > MAX_BYTES:
        raise Refused('inventory-truncated', 'stdout is over the %d byte bound' % MAX_BYTES)
    try:
        text = bytes(output).decode('ascii')
    except UnicodeDecodeError as exc:
        raise Refused('inventory-output-encoding', 'stdout is not ASCII') from exc
    return text, bytes(output), stderr


def capture_inventory(transport=None):
    """The helper's own production read: verify the installed CLI, run the one bounded fixed
    dispatch in the retained environment, and parse its output under the reviewed grammar.

    The transport seam is injectable for offline controls; the production transport is the same
    supervised bounded process route the caller uses for this helper, carrying the retained CLI
    environment. Returns (parsed, raw bytes); raw is the exact original stdout the helper passes
    through on success."""
    verify_installed_cli()
    if transport is None:
        transport = lambda argv, stdin_path, timeout, cap: transfers.run_transfer(
            argv, stdin_path, timeout, cap, environment=CLI_ENV)
    try:
        text, raw, _ = _checked_dispatch(cli_argv(), transport)
        parsed = parse_output(text, {'vm': REVIEWED_VM, 'home': REVIEWED_HOME})
    except GrammarError as exc:
        raise Refused('inventory-output-grammar', str(exc)) from exc
    if parsed['vm'] != REVIEWED_VM:
        raise Refused('inventory-vm-differs', 'the read names another VM')
    return parsed, raw


def _boot_identity(value, when):
    if not isinstance(value, str) or not _is_uuid(value):
        raise Refused('boot-bracket-malformed', 'the %s guest boot original is not a boot identity' % when)
    return value


def read_inventory(transport=None, pin=None, boot_id=None, read_boot_id=None):
    """The production host-inventory read, bound to its pin and to the guest boot bracket.

    The helper bytes at pin['tool_path'] must equal pin['sha256']: the pin is the caller's reviewed
    literal, never a digest this module computes from itself. boot_id is the boot identity the
    caller already authenticated for this VM, and read_boot_id() returns fresh guest boot originals.
    It is called once immediately before the one dispatch and once immediately after it, and both
    originals must equal boot_id; a changed or malformed bracket refuses instead of being recorded.

    The record's VM UUID and home are read from the helper's own output under the pinned grammar.
    Every other outcome raises Refused with an exact phase (READ_PHASES); a non-observation never
    becomes a host record. A passing parse of fixture bytes is not a native receipt. The helper is
    not executed when transport is injected. tool_version and desktop_build in the record are the
    pinned build: the helper's own bytes are hash-pinned, and those bytes refuse any installed CLI
    that is not the pinned tool, so a successful dispatch is bound to that identity by the argv,
    the waited exit and the exact original streams.
    """
    if not isinstance(pin, dict) or set(('tool_path', 'sha256', 'argv', 'api', 'home', 'vm',
                                         'version', 'desktop_build', 'max_bytes')) - set(pin):
        raise Refused('pin-missing')
    if not callable(read_boot_id):
        raise Refused('boot-bracket-missing', 'no guest boot identity reader is bound')
    boot_id = _boot_identity(boot_id, 'authenticated')
    if helper_digest(pin['tool_path']) != pin['sha256']:
        raise Refused('helper-bytes-differ', 'the helper at the pinned path is not the pinned bytes')
    argv = inventory_argv(pin['tool_path'])
    if argv != list(pin['argv']):
        raise Refused('pin-missing', 'the pin does not name the fixed inventory argv')
    before = _boot_identity(read_boot_id(), 'before')
    if before != boot_id:
        raise Refused('boot-bracket-moved', 'the boot identity before the host read is not the authenticated one')
    text, output, stderr = _checked_dispatch(argv, transport)
    after = _boot_identity(read_boot_id(), 'after')
    if after != boot_id:
        raise Refused('boot-bracket-moved', 'the boot identity after the host read is not the authenticated one')
    try:
        parsed = parse_output(text, pin)
    except GrammarError as exc:
        raise Refused('inventory-output-grammar', str(exc)) from exc
    if parsed['vm'] != pin['vm']:
        raise Refused('inventory-vm-differs', 'the read names another VM')
    return {
        'tool_path': pin['tool_path'], 'tool_sha256': pin['sha256'],
        'tool_version': pin['version'], 'desktop_build': pin['desktop_build'],
        # argv is the fixed helper dispatch route; api names the documented read-only accessor
        # (prlctl list --info --json). No boolean asserts more than the api name, the argv, the
        # waited exit and the bounded original streams establish.
        'api': pin['api'], 'argv': argv,
        'status': 'ok', 'exit': 0, 'truncated': False,
        'output': output, 'stderr': b'' if not isinstance(stderr, (bytes, bytearray)) else bytes(stderr),
        'vm': parsed['vm'], 'home': parsed['home'], 'boot_ids': [before, after],
    }


def main(argv):
    if argv != [INVENTORY_ARG]:
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    try:
        _, raw = capture_inventory()
    except Refused as exc:
        print('status=refused phase=%s' % (exc.phase + (' reason=' + exc.reason if exc.reason else ''),))
        return 3
    sys.stdout.flush()
    sys.stdout.buffer.write(raw)
    sys.stdout.buffer.flush()
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
