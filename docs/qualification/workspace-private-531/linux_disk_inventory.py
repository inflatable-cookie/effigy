"""Fixed Parallels host-inventory helper, its boot-bound reader and owned output grammar.

Production dispatch is exactly:

  /usr/bin/python3 -I -B THIS inventory

No other arguments. Tests inject transport at read_inventory; this file never opens a
VM, never dlopens the SDK, and never calls PrlApi_InitEx. Library hash and Info.plist
are local metadata.

Exact external limitation (SOURCE_GAP). The numeric PARALLELS_API_VER and the
PRL_HANDLE ABI are not in the permitted sources; the installed 27.1.0-58756 bundle
ships no SDK headers. enumerate_inventory therefore verifies the installed library,
then refuses Init instead of guessing, and reader_executable() reports that honestly so
a caller does not spend a native occurrence on an entry known to refuse. The one
concrete supported next read is named in SOURCE_GAP['next_read']; no SDK payload is
downloaded, installed or executed here. Session lifecycle APIs are required to reach
getters and are not sourced as read-only. GetVmListEx read-only is inferred.
LoginLocal state effect is not fetched. IsConnected's config-or-running sense is not
stated.

read_inventory is the production reader entry. It binds the helper bytes to the
caller's independent pin (never to a digest this file computes from itself), runs the
one bounded dispatch, and accepts the result only inside the caller's already
authenticated same-VM boot bracket: the caller's read_boot_id() is called once
immediately before and once immediately after the dispatch and both must equal the
boot identity that was authenticated for this VM. The selected VM UUID, home and
library identity are derived from the helper's own output, so no caller constant or
manually appended vm/boot field is a reading.

Allowlisted C API v17 names below are the only operations a later genuine read may
call. Set*/Connect/Disconnect/Reg*/Unreg*/PrlVm_RegEx are excluded. The allowlist
is not a claim that every name is a read.
"""
import hashlib
import plistlib
import re
import sys
from pathlib import Path

HELPER = Path(__file__).resolve()
PYTHON = '/usr/bin/python3'
INVENTORY_ARG = 'inventory'
INVENTORY_TIMEOUT = 30
MAX_BYTES = 64 * 1024
MAX_DEVICES = 32
MAX_STRING = 4096

# Installed 27.1.0 build 58756. Hash is of the framework executable; libprl_sdk.11.dylib
# is a symlink to the same bytes.
LIBRARY_PATH = ('/Applications/Parallels Desktop.app/Contents/Frameworks/'
                'ParallelsVirtualizationSDK.framework/Versions/11/ParallelsVirtualizationSDK')
LIBRARY_SHA256 = '464af6b9ec6260d77a849d54c55cd1b74fdb9d1d64684fb5564a4aa54ff00123'
INFO_PLIST = ('/Applications/Parallels Desktop.app/Contents/Frameworks/'
              'ParallelsVirtualizationSDK.framework/Versions/11/Resources/Info.plist')
LIBRARY_VERSION = '27.1.0'
LIBRARY_BUILD = '58756'

# C API v17 PrlApi_InitEx(nVersion=PARALLELS_API_VER, ...). Numeric value not sourced.
PARALLELS_API_VER = None
# C API v17 PRL_HANDLE. The ABI size and the InitEx/PrlSrv_*/PrlVmCfg_* prototypes are not sourced
# either, so no binding is written. None is "not sourced", never a guessed size.
PRL_HANDLE_SIZE = None

# The exact external limitation, and the one concrete supported read that would close it. Nothing
# here downloads, installs or executes an SDK: a later, separately briefed host-custodied step
# reads the vendor archive and pins it by sha256 before this file binds it.
SOURCE_GAP = {
    'phase': 'parallels-api-ver-unsourced',
    'missing': ('numeric PARALLELS_API_VER for PrlApi_InitEx',
                'PRL_HANDLE ABI size and the PrlApi_InitEx/PrlSrv_*/PrlVmCfg_* prototypes',
                'the reviewed job-wait, exit-code and PRL_HANDLE ownership sequence for one read'),
    'next_read': ('the Parallels Virtualization SDK archive for Desktop 27.1.0-58756 (the PrlApi.h '
                  'and PrlApiVersion.h constants plus the PRL_HANDLE and InitEx/Srv/Cfg '
                  'prototypes), read once in a separately briefed host-custodied collection step '
                  'and pinned by archive sha256 before use'),
    'not_here': 'no SDK payload download, installation or execution in this source task',
}

# Every phase read_inventory refuses with, so a caller names the outcome exactly.
READ_PHASES = (
    'pin-missing', 'helper-bytes-differ', 'boot-bracket-missing', 'boot-bracket-malformed',
    'boot-bracket-moved', 'inventory-output-missing', 'inventory-not-exited', 'inventory-exit-nonzero',
    'inventory-truncated', 'inventory-output-encoding', 'inventory-output-grammar',
    'inventory-vm-differs',
)

# C API v17 PRL_MASS_STORAGE_INTERFACE_TYPE / PRL_DEVICE_TYPE / disk and emulation enums.
PMS_IDE, PMS_SCSI, PMS_SATA, PMS_NVME = 0, 1, 2, 3
PDE_GENERIC_DEVICE = 0
PDE_CLUSTERED_DEVICE = 1
PDE_STORAGE_DEVICE = 2
PDE_FLOPPY_DISK = 3
PDE_MASSSTORAGE_DEVICE = 4
PDE_OPTICAL_DISK = 5
PDE_HARD_DISK = 6
PDE_GENERIC_NETWORK_ADAPTER = 8
PDE_GENERIC_PORT = 9
PDE_SERIAL_PORT = 10
PDE_PARALLEL_PORT = 11
PDE_SOUND_DEVICE = 12
PDE_MIXER_DEVICE = 13
PDE_USB_DEVICE = 15
PDE_PRINTER = 16
PDE_GENERIC_PCI_DEVICE = 17
PDE_GENERIC_SCSI_DEVICE = 18
PDE_VIRTUAL_SNAPSHOT_DEVICE = 19
PDE_PCI_VIDEO_ADAPTER = 20
PDE_VIRTUAL_SHARED_FOLDERS_DEVICE = 21
PDE_CAMERA = 22
PDE_SMART_CARD_READER = 23
PDE_GENERIC_NVME_DEVICE = 24
PDE_HID_DEVICE = 25
PDE_TPM_DEVICE = 26
PHD_EXPANDING_HARD_DISK = 1
PDT_USE_REAL_DEVICE = 0
PDT_USE_IMAGE_FILE = 1
PDT_USE_DIRECT_ASSIGN = 4
PAM_DESKTOP = 1
PSL_HIGH_SECURITY = 2
PRL_ERR_SUCCESS = 0
# C API v17 _VIRTUAL_MACHINE_STATE: VMS_RUNNING = 0x30000004.
VMS_RUNNING = 0x30000004

# Non-disk types a complete inventory may carry. Anything else is unknown storage.
ALLOWED_NON_DISK_TYPES = frozenset({
    PDE_OPTICAL_DISK, PDE_GENERIC_NETWORK_ADAPTER, PDE_GENERIC_PORT, PDE_SERIAL_PORT,
    PDE_PARALLEL_PORT, PDE_SOUND_DEVICE, PDE_MIXER_DEVICE, PDE_USB_DEVICE, PDE_PRINTER,
    PDE_VIRTUAL_SNAPSHOT_DEVICE, PDE_PCI_VIDEO_ADAPTER, PDE_VIRTUAL_SHARED_FOLDERS_DEVICE,
    PDE_CAMERA, PDE_SMART_CARD_READER, PDE_HID_DEVICE, PDE_TPM_DEVICE,
})
# Extra/passthrough/ambiguous block sources. One PDE_HARD_DISK is checked on its own fields.
FORBIDDEN_BLOCK_TYPES = frozenset({
    PDE_GENERIC_DEVICE, PDE_CLUSTERED_DEVICE, PDE_STORAGE_DEVICE, PDE_FLOPPY_DISK,
    PDE_MASSSTORAGE_DEVICE, PDE_GENERIC_PCI_DEVICE, PDE_GENERIC_SCSI_DEVICE,
    PDE_GENERIC_NVME_DEVICE,
})

# Session lifecycle. Required to reach getters. Not sourced as read-only.
# Remaining native premises include LoginLocal's state effect.
SESSION_APIS = (
    'PrlApi_InitEx', 'PrlApi_Deinit', 'PrlSrv_Create', 'PrlSrv_LoginLocal', 'PrlSrv_Logoff',
)
# Remaining native premise: GetVmListEx read-only is inferred, not fetched from C API v17.
INFERRED_LIST_APIS = ('PrlSrv_GetVmListEx',)
# Job and handle plumbing used by both session setup and getters.
JOB_APIS = (
    'PrlJob_Wait', 'PrlJob_GetRetCode', 'PrlJob_GetResult',
    'PrlResult_GetParam', 'PrlHndlList_GetItemsCount', 'PrlHndlList_GetItem', 'PrlHandle_Free',
)
# Configuration getters sourced as reads (C API v17). IsConnected's config-or-running sense
# is not stated and remains a native premise.
CONFIG_READ_APIS = (
    'PrlVm_GetConfig', 'PrlVm_GetState', 'PrlVmInfo_GetState', 'PrlVmCfg_GetUuid',
    'PrlVmCfg_GetHomePath', 'PrlVmCfg_GetAllDevices', 'PrlVmCfg_GetDevsCount',
    'PrlVmDev_GetType', 'PrlVmDev_GetIfaceType', 'PrlVmDev_GetIndex', 'PrlVmDev_GetSysName',
    'PrlVmDev_GetImagePath', 'PrlVmDev_IsEnabled', 'PrlVmDev_IsConnected', 'PrlVmDev_IsRemote',
    'PrlVmDev_IsPassthrough', 'PrlVmDevHd_GetDiskType', 'PrlVmDev_GetEmulatedType',
)
# Union of names a later genuine occurrence may call. Not a claim the whole sequence is a read.
ALLOWLISTED_APIS = SESSION_APIS + INFERRED_LIST_APIS + JOB_APIS + CONFIG_READ_APIS
REMAINING_NATIVE_PREMISES = (
    'PARALLELS_API_VER',
    'PRL_HANDLE sizeof',
    'PrlSrv_LoginLocal state effect',
    'PrlSrv_GetVmListEx read-only inferred',
    'PrlVmDev_IsConnected config-or-running',
)

SCALAR_KEYS = (
    'vm', 'home', 'library_path', 'library_sha256', 'library_version', 'library_build',
    'state_before', 'state_after', 'device_count',
)
DEVICE_KEYS = (
    'type', 'index', 'iface', 'sys_name', 'image_path',
    'enabled', 'connected', 'remote', 'passthrough', 'disk_type', 'emulated_type',
)
INT_DEVICE_KEYS = (
    'type', 'index', 'iface', 'enabled', 'connected', 'remote', 'passthrough',
    'disk_type', 'emulated_type',
)
OPTIONAL_KEYS = ('serial',)
KEY_RE = re.compile(r'^[a-z][a-z0-9_]*$')
INT_RE = re.compile(r'^(?:0|-1|[1-9][0-9]*)$')
SHA_RE = re.compile(r'^[0-9a-f]{64}$')
UUID_RE = re.compile(r'^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$')


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


def missing_sources():
    """The ABI facts this reader needs and does not have. None of them is guessed."""
    missing = []
    if PARALLELS_API_VER is None:
        missing.append(SOURCE_GAP['missing'][0])
    if PRL_HANDLE_SIZE is None:
        missing.append(SOURCE_GAP['missing'][1])
    return tuple(missing)


def reader_executable():
    """True only when every sourced ABI fact the one read needs is present."""
    return not missing_sources()


def reader_limitation():
    """The exact external limitation in one line, for a caller that must not spend an occurrence."""
    missing = missing_sources()
    if not missing:
        return 'none'
    return ('host inventory reader is not executable: unsourced ' + '; '.join(missing)
            + '; next fixed read: ' + SOURCE_GAP['next_read'])


def inventory_argv(helper=HELPER):
    return [PYTHON, '-I', '-B', str(helper), INVENTORY_ARG]


def _require(cond, message):
    if not cond:
        raise GrammarError(message)


def parse_output(text, pin):
    """Parse the owned key=value grammar. pin supplies max_devices and library/home pins."""
    _require(isinstance(text, str) and text.endswith('\n'),
             'host read output is not newline-terminated')
    fields = {}
    for line in text[:-1].split('\n'):
        key, sep, value = line.partition('=')
        if not sep or key in fields or not KEY_RE.fullmatch(key) or not value or value != value.strip():
            raise GrammarError('host read output line is malformed or repeated')
        fields[key] = value
    extra = set(fields) - set(SCALAR_KEYS) - set(OPTIONAL_KEYS)
    serial = fields.get('serial')
    try:
        count = int(fields['device_count'])
    except (KeyError, ValueError) as exc:
        raise GrammarError('host read device_count is missing or malformed') from exc
    _require(INT_RE.fullmatch(fields.get('device_count', '')) and 1 <= count <= pin['max_devices'],
             'host read device_count is missing or over its bound')
    expected = set(SCALAR_KEYS)
    if serial is not None:
        expected.add('serial')
    for index in range(count):
        for name in DEVICE_KEYS:
            expected.add('dev_%d_%s' % (index, name))
    if set(fields) != expected:
        raise GrammarError('host read output fields differ from the pinned grammar')
    extra_dev = [k for k in extra if k.startswith('dev_') and k not in expected]
    _require(not extra_dev, 'host read output fields differ from the pinned grammar')
    for key in ('state_before', 'state_after', 'library_build'):
        _require(INT_RE.fullmatch(fields[key]) and not fields[key].startswith('-'),
                 'host read %s is malformed' % key)
    _require(UUID_RE.fullmatch(fields['vm']), 'host read vm is malformed')
    _require(fields['home'] == pin['home'], 'host read names another VM home')
    _require(fields['library_path'] == pin['library_path'], 'host read library is not the pinned bytes')
    _require(SHA_RE.fullmatch(fields['library_sha256']) and fields['library_sha256'] == pin['library_sha256'],
             'host read library is not the pinned bytes')
    _require(fields['library_version'] == pin['version'] and fields['library_build'] == pin['library_build'],
             'host read tool version or Desktop build differs from the pin')
    devices = []
    for index in range(count):
        device = {}
        for name in DEVICE_KEYS:
            value = fields['dev_%d_%s' % (index, name)]
            if name in INT_DEVICE_KEYS:
                _require(INT_RE.fullmatch(value), 'host read device field %s is malformed' % name)
                device[name] = int(value)
            else:
                device[name] = value
        devices.append(device)
    return {
        'vm': fields['vm'], 'home': fields['home'],
        'library_path': fields['library_path'], 'library_sha256': fields['library_sha256'],
        'library_version': fields['library_version'], 'library_build': int(fields['library_build']),
        'state_before': int(fields['state_before']), 'state_after': int(fields['state_after']),
        'devices': devices, 'serial': serial,
    }


def render_output(vm, home, library_path, library_sha256, library_version, library_build,
                  state_before, state_after, devices, serial=None):
    """Owned grammar renderer. Used by controls; the helper prints this after a genuine read."""
    lines = [
        'vm=' + vm,
        'home=' + home,
        'library_path=' + library_path,
        'library_sha256=' + library_sha256,
        'library_version=' + library_version,
        'library_build=' + str(library_build),
        'state_before=' + str(state_before),
        'state_after=' + str(state_after),
        'device_count=' + str(len(devices)),
    ]
    for index, device in enumerate(devices):
        for name in DEVICE_KEYS:
            lines.append('dev_%d_%s=%s' % (index, name, device[name]))
    if serial:
        lines.append('serial=' + serial)
    return '\n'.join(lines) + '\n'


def verify_installed_library(path=LIBRARY_PATH, plist_path=INFO_PLIST,
                             expected_sha=LIBRARY_SHA256, expected_version=LIBRARY_VERSION,
                             expected_build=LIBRARY_BUILD):
    """Local metadata only: content hash and Info.plist. Does not dlopen or Init."""
    try:
        data = Path(path).read_bytes()
    except OSError as exc:
        raise Refused('library-unavailable') from exc
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected_sha:
        raise Refused('library-hash-mismatch')
    try:
        info = plistlib.loads(Path(plist_path).read_bytes())
    except (OSError, plistlib.InvalidFileException) as exc:
        raise Refused('library-plist-unavailable') from exc
    version = str(info.get('CFBundleShortVersionString', ''))
    build = str(info.get('CFBundleVersion', ''))
    prl_build = str(info.get('PrlBuild', ''))
    if version != expected_version or build != expected_build or prl_build != expected_build:
        raise Refused('library-version-mismatch')
    return {'path': path, 'sha256': digest, 'version': version, 'build': build}


def enumerate_inventory(verify_library=verify_installed_library, api_ver=PARALLELS_API_VER,
                        handle_size=PRL_HANDLE_SIZE):
    """Authenticate the installed library, then refuse Init until the ABI is sourced."""
    verify_library()
    if api_ver is None:
        raise Refused('parallels-api-ver-unsourced')
    if handle_size is None:
        raise Refused('prl-handle-abi-unsourced')
    raise Refused('sdk-init-not-reached')


def _boot_identity(value, when):
    if not isinstance(value, str) or not UUID_RE.fullmatch(value):
        raise Refused('boot-bracket-malformed', 'the %s guest boot original is not a boot identity' % when)
    return value


def read_inventory(transport=None, pin=None, boot_id=None, read_boot_id=None):
    """The production host-inventory read, bound to its pin and to the guest boot bracket.

    The helper bytes at pin['tool_path'] must equal pin['sha256']: the pin is the caller's reviewed
    literal, never a digest this module computes from itself. boot_id is the boot identity the
    caller already authenticated for this VM, and read_boot_id() returns fresh guest boot originals.
    It is called once immediately before the one dispatch and once immediately after it, and both
    originals must equal boot_id; a changed or malformed bracket refuses instead of being recorded.

    The record's VM UUID, home and library identity are read from the helper's own output under the
    pinned grammar. Every other outcome raises Refused with an exact phase (READ_PHASES); a
    non-observation never becomes a host record. A passing parse of fixture bytes is not a native
    receipt. The helper is not executed when transport is injected.
    """
    import linux_transfer as transfers
    if not isinstance(pin, dict) or set(('tool_path', 'sha256', 'argv', 'api', 'home', 'vm', 'version',
                                         'library_path', 'library_sha256', 'library_build',
                                         'max_devices')) - set(pin):
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
    if transport is None:
        transport = transfers.run_transfer
    result = transport(argv, None, INVENTORY_TIMEOUT, MAX_BYTES)
    if not isinstance(result, dict):
        raise Refused('inventory-output-missing', 'the dispatch returned no process result')
    output = result.get('stdout')
    stderr = result.get('stderr')
    exit_code = result.get('waited_exit')
    if not isinstance(output, (bytes, bytearray)):
        raise Refused('inventory-output-missing')
    if result.get('disposition') != 'exited':
        raise Refused('inventory-not-exited', 'disposition %r' % (result.get('disposition'),))
    if exit_code != 0:
        raise Refused('inventory-exit-nonzero', 'waited exit %r' % (exit_code,))
    if len(output) > MAX_BYTES:
        raise Refused('inventory-truncated', 'stdout is over the %d byte bound' % MAX_BYTES)
    after = _boot_identity(read_boot_id(), 'after')
    if after != boot_id:
        raise Refused('boot-bracket-moved', 'the boot identity after the host read is not the authenticated one')
    try:
        text = bytes(output).decode('ascii')
    except UnicodeDecodeError as exc:
        raise Refused('inventory-output-encoding', 'stdout is not ASCII') from exc
    try:
        parsed = parse_output(text, pin)
    except GrammarError as exc:
        raise Refused('inventory-output-grammar', str(exc)) from exc
    if parsed['vm'] != pin['vm']:
        raise Refused('inventory-vm-differs', 'the read names another VM')
    return {
        'tool_path': pin['tool_path'], 'tool_sha256': pin['sha256'],
        'tool_version': parsed['library_version'],
        'desktop_build': parsed['library_version'] + '-' + str(parsed['library_build']),
        # Observation that this dispatch is the inventory accessor, not a Set*/Connect.
        # Not a claim that LoginLocal or InitEx are sourced as read-only.
        'api': pin['api'], 'readonly': True, 'argv': argv,
        'status': 'ok', 'exit': 0, 'truncated': False,
        'output': bytes(output), 'stderr': b'' if not isinstance(stderr, (bytes, bytearray)) else bytes(stderr),
        'vm': parsed['vm'], 'home': parsed['home'], 'boot_ids': [before, after],
        'library_path': parsed['library_path'], 'library_sha256': parsed['library_sha256'],
        'library_version': parsed['library_version'],
    }


def main(argv):
    if argv != [INVENTORY_ARG]:
        print('status=refused phase=usage', file=sys.stderr)
        return 2
    try:
        enumerate_inventory()
    except Refused as exc:
        print('status=refused phase=%s' % exc.phase)
        return 3
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
