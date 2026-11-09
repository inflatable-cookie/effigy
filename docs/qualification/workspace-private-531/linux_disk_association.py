"""Controller-to-guest disk association: the fail-closed verifier and its premises.

Source controls only. This module checks the facts it is given: the registered config bytes, the
descriptor bytes, one host inventory read and the guest block inventory. It opens no VM, image, host
service or guest, and it runs no tool. A mechanism qualifies only when PINS names it: the exact helper
bytes, version, Desktop build, allowlisted accessor, and the output grammar this module parses.

parallels-host-inventory-trust-v1 is the accepted narrow Parallels trust premise (decision
64b1fe6f v2): for pinned Desktop 27.1.0-58756 and registered VM 433c9e5b-…, a complete freshly
authenticated source-validated host inventory is trusted to describe every block device presented to
that same VM/guest boot. It assumes faithful hypervisor presentation and cannot detect unreported or
stale/misreported presentation. That is not independent presentation proof.

Premises, all required. premise_table reports each one on its own inputs, so a partial fact set shows
which premises it establishes:
  mechanism     the mechanism is supported and pinned
  host_read     one pinned inventory dispatch, bounded, exit 0, bracketed by the same boot identity,
                output in the owned grammar; exactly one enabled connected image-backed data disk,
                canonical SystemName-to-image path, exactly one optical device matching the registered
                config CdRom count, no extra/unknown/passthrough/ambiguous sources
  config        the registered VM has exactly one hard disk: hdd0, SATA index 0, the reviewed device
                Uuid, enabled and connected, image-backed (not passthrough, not remote); every Hardware
                child is a reviewed tag, and no element outside Hdd and CdRom carries a block-capable name
                at any depth; USB holds exactly one device, the retained camera, every field exact; the connect history names only the reviewed virtual devices; CD-ROM devices are counted and not
                treated as disks
  descriptor    the descriptor bytes select the reviewed disk UID, the top, the chain and every extent
  guest_blocks  the guest has exactly one whole disk, /dev/sda, non-removable and on the platform ATA
                controller, and the reviewed root is its partition; loop and ROM devices are classified out
                by type, any other type refuses

Serial is an informative observation when both sides carry it. It is not a required premise and does
not require PrlVmDevHd_GetSerialNumber.

Not established by any premise: independent presentation proof; that the config device Uuid and the
descriptor disk UID name the same image (different namespaces, no documented join); that guest ata1
is SATA position 0; the origin of the bytes given here; image bodies and encryption. Size/model/name
agreement is not evidence. Source admission of this logic is not evidence a fresh read ran.

The production handoff is association_facts: one boot-bound host inventory read (read_inventory),
the current registered-config and descriptor originals, the extent inventory the stage observed and
the guest block observations of that same boot. It returns the complete record and the caller feeds
it to setup.verify_disk_association. The reader is dispatched only when its ABI sources are complete
(linux_disk_inventory.reader_executable), so a native occurrence is not spent on an entry that is
known to refuse; the exact limitation and the one supported next read are in
linux_disk_inventory.SOURCE_GAP.
"""
import hashlib
import re
import xml.etree.ElementTree as ET

import linux_disk_identity as identity
import linux_disk_inventory as inventory
from linux_supervisor import Refusal

# The reviewed identity. linux_setup's constants must equal these; linux_disk_association_tests checks it.
REVIEWED = {
    'vm': '433c9e5b-c7b9-469f-9633-24b4addcd3b5',
    'disk_uid': '{44b5d35e-b053-4aae-9140-266a00bbef9a}',
    'device_uuid': '{9447f202-8d43-4ec2-80c9-ce527b34bd9d}',
    'system_name': 'harddisk1.hdd',
    'home': '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm',
    'image_path': '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm/harddisk1.hdd',
    'interface_sata': '2',
    'root_size': 137438953472,
    'root_device': '/dev/sda2',
    'root_fstype': 'ext4',
    'root_uuid': 'd488be7b-682f-41ac-b79b-57bbf495a01d',
    'root_parent': 'sda',
    'disk_path': '/dev/sda',
    'disk_controller_prefix': '/sys/devices/platform/PRL4010:00/ata',
}
# Every Hardware child the registered config carries (the 747 retained config, before and after the
# observation). A child outside this set is an unreviewed device and refuses: it may be a block source.
HARDWARE_TAGS = frozenset({
    'Cpu', 'Chipset', 'Clock', 'Memory', 'Video', 'HibernateState', 'VirtIO', 'VirtIOSerial',
    'VirtIOInput', 'VirtIOVsock', 'CdRom', 'Hdd', 'NetworkAdapter', 'Sound', 'USB',
    'UsbConnectHistory', 'TpmChip',
})
# The reviewed values of the one hard disk: enabled, connected, image-backed (not passthrough, not remote),
# DiskType 1. The C API v17 enum has 1 = PHD_EXPANDING_HARD_DISK; the XML-to-enum mapping is inferred.
# InterfaceType 2 is PMS_SATA. Remote and Enabled/Connected meanings are observed, not sourced
# (see linux-setup.md). A physical or remote disk can match the count and the name and is still not this disk.
HDD_REVIEWED_FIELDS = (('Enabled', '1'), ('Connected', '1'), ('Remote', '0'), ('Passthrough', '0'),
                       ('DiskType', '1'))
# A block device can hide under an allowed tag (VirtIOBlk, a SCSI or SATA controller, a disk under USB), so
# every element outside Hdd and CdRom is checked at any depth. 'ide' is a prefix only: 'Video' contains it.
BLOCK_WORDS = ('hdd', 'cdrom', 'floppy', 'disk', 'scsi', 'sata', 'nvme', 'storage', 'optical', 'block', 'blk',
               'mass', 'ata')
BLOCK_PREFIXES = ('ide',)
# The reviewed USB device: the retained virtual camera, every field exact. Its SystemName carries a host camera
# identifier, so another suffix is a different device and refuses. Taken from the 747 retained registered config.
USB_CAMERA_RECORD = {
    'Enabled': '1', 'Connected': '1', 'EmulatedType': '3', 'UsbType': '2',
    'SystemName': 'VIRTUAL@UVC@|203a|fff9|high|--|6C707041-05AC-0011-0007-000000000001',
    'UserFriendlyName': 'Virtual Video Camera', 'Remote': '0', 'AutoConnect': '1', 'ConnectReason': '0',
    'DeviceDescription': '',
}
# The connect-history names the retained config carries. Only these virtual devices may appear; Location and
# Timestamp vary and are not identity.
USB_HISTORY_NAMES = frozenset({
    'VIRTUAL@MOUSE@|203a|fffc|full|--|PW3.0',
    'VIRTUAL@KEYBOARD@|203a|fffb|full|--|PW3.0',
    'VIRTUAL@UVC@|203a|fff9|high|--|98BA9681-B85F-4A7F-A15E-9AB9CDBF5350',
    'VIRTUAL@UVC@|203a|fff9|high|--|769D7766-AD2D-4FBA-9418-1F1A00000001',
    'VIRTUAL@UVC@|203a|fff9|high|--|6C707041-05AC-0011-0007-000000000001',
})
MAX_CONFIG_BYTES = 1024 * 1024
BOOT_ID = re.compile(r'^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$')
PREMISES = ('mechanism', 'host_read', 'config', 'descriptor', 'guest_blocks')
MECHANISM = 'parallels-host-inventory-trust-v1'
UNSUPPORTED = ('unsupported controller-to-guest disk association: the mechanism is not the pinned '
               'parallels-host-inventory-trust-v1 read')
NOT_ESTABLISHED = (
    'accepted narrow Parallels trust is not independent presentation proof: the inventory cannot '
    'detect unreported or stale or misreported hypervisor presentation; config device Uuid and '
    'descriptor disk UID are different identifiers and no documented rule links them; ata1 is not '
    'mapped to SATA position 0; the origin of the given bytes is not admitted here; image bodies '
    'not read; a passing source control is not evidence a fresh host inventory ran')

# Bound to the helper bytes, the installed SDK library, Desktop 27.1.0-58756, the reviewed VM/home,
# and the owned inventory grammar. Other mechanisms stay unsupported.
# The helper digest is a committed literal, not inventory.helper_digest() at import: a modified helper
# cannot re-pin itself, and changing the helper without changing this literal refuses in
# read_inventory('helper-bytes-differ').
HELPER_SHA256 = 'c03c2bcdf2bd41b4ed3caa6325f9ef6a1c6c63db38bca57154f01171cdb328eb'
PINS = {
    MECHANISM: {
        'tool_path': str(inventory.HELPER),
        'sha256': HELPER_SHA256,
        'version': inventory.LIBRARY_VERSION,
        'desktop_build': inventory.LIBRARY_VERSION + '-' + inventory.LIBRARY_BUILD,
        'api': 'PrlVmCfg_GetAllDevices',
        'argv': inventory.inventory_argv(),
        'max_bytes': inventory.MAX_BYTES,
        'max_devices': inventory.MAX_DEVICES,
        'library_path': inventory.LIBRARY_PATH,
        'library_sha256': inventory.LIBRARY_SHA256,
        'library_build': inventory.LIBRARY_BUILD,
        'home': REVIEWED['home'],
        'vm': REVIEWED['vm'],
        'trust': 'narrow-parallels-host-inventory',
    },
}


def _lower(value):
    return value.lower() if isinstance(value, str) else value


def _require_boot(value, when):
    """A boot identity this stage authenticated; another principal's string is not one."""
    if not isinstance(value, str) or not BOOT_ID.fullmatch(value):
        raise Refusal('the guest boot identity %s is missing or malformed' % when)
    return value


def _leaf(child, where):
    """A field the premises read is a leaf: nested elements or attributes would carry text the reader never sees."""
    if len(child) or child.attrib:
        raise Refusal('%s %s carries nested elements or attributes the reviewed record does not allow' % (where, child.tag))


def _text(node, tag, where):
    """The one text value of a field. A repeated field refuses: a later duplicate must not hide behind the first."""
    children = node.findall(tag)
    if len(children) != 1:
        raise Refusal('%s %s is missing or repeated (%d occurrences)' % (where, tag, len(children)))
    child = children[0]
    _leaf(child, where)
    if not child.text or child.text != child.text.strip():
        raise Refusal('%s %s is missing, empty or padded' % (where, tag))
    return child.text


def _check_mechanism(association, supported, pins):
    mechanism = association.get('mechanism') if isinstance(association, dict) else None
    if mechanism not in supported or mechanism not in pins:
        raise Refusal(UNSUPPORTED)
    return pins[mechanism]


def _check_inventory_devices(devices):
    """Exactly one enabled connected image-backed HDD; exactly one optical; no extra/unknown/passthrough sources."""
    hdds = [d for d in devices if d['type'] == inventory.PDE_HARD_DISK]
    if len(hdds) != 1:
        raise Refusal('host inventory has %d hard disks; the association needs exactly one' % len(hdds))
    hdd = hdds[0]
    if (hdd['index'] != 0 or hdd['iface'] != inventory.PMS_SATA
            or hdd['sys_name'] != REVIEWED['system_name']):
        raise Refusal('host inventory hard disk is not hdd0 at SATA position 0 of the reviewed bundle')
    if hdd['image_path'] != REVIEWED['image_path']:
        raise Refusal('host inventory image path is not the reviewed bundle')
    if (hdd['enabled'] != 1 or hdd['connected'] != 1 or hdd['remote'] != 0 or hdd['passthrough'] != 0
            or hdd['disk_type'] != inventory.PHD_EXPANDING_HARD_DISK
            or hdd['emulated_type'] != inventory.PDT_USE_IMAGE_FILE):
        raise Refusal('host inventory hard disk is not the reviewed image-backed disk')
    opticals = [d for d in devices if d['type'] == inventory.PDE_OPTICAL_DISK]
    names = [d['sys_name'] for d in opticals]
    if len(set(names)) != len(names):
        raise Refusal('host inventory has duplicate optical entries')
    if len(opticals) != 1:
        raise Refusal('host inventory has %d optical devices; the reviewed VM has exactly one' % len(opticals))
    cameras = [d for d in devices
               if d['sys_name'] == USB_CAMERA_RECORD['SystemName'] or d['type'] == inventory.PDE_CAMERA]
    if len(cameras) != 1:
        raise Refusal('host inventory has %d matching cameras; the reviewed VM has exactly one' % len(cameras))
    usbs = [d for d in devices if d['type'] == inventory.PDE_USB_DEVICE]
    if len(usbs) != 1 or usbs[0]['sys_name'] != USB_CAMERA_RECORD['SystemName']:
        raise Refusal('host inventory USB storage is extra or is not the reviewed camera')
    for device in devices:
        if device['type'] in inventory.FORBIDDEN_BLOCK_TYPES:
            raise Refusal('host inventory carries unknown or passthrough storage type %s' % device['type'])
        if device['type'] not in inventory.ALLOWED_NON_DISK_TYPES and device['type'] != inventory.PDE_HARD_DISK:
            raise Refusal('host inventory carries unknown or passthrough storage type %s' % device['type'])
        if device['emulated_type'] in (inventory.PDT_USE_REAL_DEVICE, inventory.PDT_USE_DIRECT_ASSIGN):
            raise Refusal('host inventory carries a real or passthrough block source')
    return {'hdd': hdd, 'optical_count': len(opticals)}


def _check_host_read(association, pin):
    """One pinned inventory: helper bytes, accessor, bounds, exit, boot bracket, owned grammar."""
    if pin is None:
        raise Refusal('no pinned host read, so no inventory is read')
    read = association.get('host_read')
    if not isinstance(read, dict):
        raise Refusal('host read is missing')
    if read.get('tool_path') != pin['tool_path'] or read.get('tool_sha256') != pin['sha256']:
        raise Refusal('host read tool is not the pinned bytes')
    if read.get('tool_version') != pin['version'] or read.get('desktop_build') != pin['desktop_build']:
        raise Refusal('host read tool version or Desktop build differs from the pin')
    if read.get('api') != pin['api'] or read.get('readonly') is not True:
        raise Refusal('host read is not the pinned read-only accessor')
    if read.get('argv') != pin['argv']:
        raise Refusal('host read argv is not the pinned inventory argv')
    if read.get('status') != 'ok' or read.get('exit') != 0 or read.get('truncated') is not False:
        raise Refusal('host read is truncated, failed or unknown')
    output = read.get('output')
    if not isinstance(output, bytes) or not output or len(output) > pin['max_bytes']:
        raise Refusal('host read output is missing or over its bound')
    boot = association.get('boot_id')
    if not isinstance(boot, str) or not boot or read.get('boot_ids') != [boot, boot]:
        raise Refusal('host read is not bracketed by the guest boot identity')
    if read.get('vm') != pin['vm']:
        raise Refusal('host read names another VM')
    try:
        text = output.decode('ascii')
    except UnicodeDecodeError as exc:
        raise Refusal('host read output is not ASCII') from exc
    try:
        parsed = inventory.parse_output(text, pin)
    except inventory.GrammarError as exc:
        raise Refusal(str(exc)) from exc
    if parsed['vm'] != REVIEWED['vm']:
        raise Refusal('host read names another VM')
    if parsed['home'] != REVIEWED['home']:
        raise Refusal('host read names another VM home')
    if parsed['state_before'] != parsed['state_after']:
        raise Refusal('host read VM state changed during the inventory')
    if parsed['state_before'] != inventory.VMS_RUNNING:
        raise Refusal('host read is not of the running VM')
    checked = _check_inventory_devices(parsed['devices'])
    hdd = checked['hdd']
    host_serial = parsed.get('serial')
    guest = association.get('guest') or {}
    guest_serial = guest.get('parent_serial') or ''
    if host_serial and guest_serial and host_serial != guest_serial:
        raise Refusal('host serial observation does not match the guest parent serial')
    return {'sys_name': hdd['sys_name'], 'image_path': hdd['image_path'],
            'serial': host_serial, 'sha256': hashlib.sha256(output).hexdigest(),
            'optical_count': checked['optical_count']}


def _check_hardware_content(hardware):
    """Block-capable content may not hide under an allowed tag. Hdd and CdRom are checked on their own fields."""
    for child in hardware:
        if child.tag in ('Hdd', 'CdRom'):
            continue
        for node in child.iter():
            low = node.tag.lower()
            if any(word in low for word in BLOCK_WORDS) or low.startswith(BLOCK_PREFIXES):
                raise Refusal('config %s carries a block-capable element %s' % (child.tag, node.tag))
    usb = hardware.findall('USB')
    if len(usb) != 1:
        raise Refusal('config has %d USB devices; the reviewed VM has exactly one, the virtual camera' % len(usb))
    _check_exact_record(usb[0], USB_CAMERA_RECORD, 'USB')
    if len(hardware.findall('UsbConnectHistory')) > 1:
        raise Refusal('config has more than one USB connect history section')
    for port in hardware.findall('UsbConnectHistory/USBPort'):
        if set(child.tag for child in port) - {'Location', 'SystemName', 'Timestamp'}:
            raise Refusal('config USB connect history entry carries an unreviewed field')
        if _text(port, 'SystemName', 'USBPort') not in USB_HISTORY_NAMES:
            raise Refusal('config USB connect history names a device outside the reviewed set')
    if len(hardware.findall('CdRom')) != 1:
        raise Refusal('config has %d optical devices; the reviewed VM has exactly one' % len(hardware.findall('CdRom')))


def _check_exact_record(node, record, where):
    """Every field of the node is in the record, once, with the reviewed value; nothing extra and nothing missing."""
    if sorted(child.tag for child in node) != sorted(record):
        raise Refusal('config %s fields differ from the reviewed record (extra, missing or repeated)' % where)
    for child in node:
        _leaf(child, where)
    for tag, value in record.items():
        if (node.findtext(tag) or '') != value:
            raise Refusal('config %s %s is not the reviewed value' % (where, tag))


def _check_config(association):
    """The registered VM: one hdd at SATA index 0, the reviewed device Uuid. CD-ROMs are counted only."""
    data = association.get('config')
    if not isinstance(data, (bytes, bytearray)) or not data or len(data) > MAX_CONFIG_BYTES:
        raise Refusal('registered config bytes are missing or over the bound')
    lowered = bytes(data).lower()
    if b'<!doctype' in lowered or b'<!entity' in lowered:
        raise Refusal('config DTD or entity declarations are outside the shape')
    try:
        root = ET.fromstring(bytes(data))
    except ET.ParseError as exc:
        raise Refusal('registered config is not well-formed XML') from exc
    if root.tag != 'ParallelsVirtualMachine':
        raise Refusal('config root is not ParallelsVirtualMachine')
    # One section of each kind: a second Hardware or Identification would be a hiding place that the first-match
    # reads never see, so a repeated section refuses before any field is read.
    for section in ('Identification', 'Hardware'):
        count = len(root.findall(section))
        if count != 1:
            raise Refusal('config has %d %s sections; the association needs exactly one' % (count, section))
    identification = root.find('Identification')
    if _text(identification, 'VmUuid', 'Identification').strip('{}').lower() != REVIEWED['vm']:
        raise Refusal('config VM is not the reviewed VM')
    hardware = root.find('Hardware')
    unknown = sorted({child.tag for child in hardware} - HARDWARE_TAGS)
    if unknown:
        raise Refusal('config Hardware has unreviewed device elements: %s' % ', '.join(unknown))
    _check_hardware_content(hardware)
    hdds = root.findall('Hardware/Hdd')
    if len(hdds) != 1:
        raise Refusal('config has %d hard disks; the association needs exactly one' % len(hdds))
    hdd = hdds[0]
    if (_text(hdd, 'Index', 'Hdd') != '0' or _text(hdd, 'InterfaceType', 'Hdd') != REVIEWED['interface_sata']
            or _text(hdd, 'SystemName', 'Hdd') != REVIEWED['system_name']):
        raise Refusal('config hard disk is not hdd0 at SATA position 0 of the reviewed bundle')
    for field, value in HDD_REVIEWED_FIELDS:
        if _text(hdd, field, 'Hdd') != value:
            raise Refusal('config hard disk %s is not the reviewed image-backed value %s' % (field, value))
    device_uuid = _text(hdd, 'Uuid', 'Hdd')
    if _lower(device_uuid) != _lower(REVIEWED['device_uuid']):
        raise Refusal('config device Uuid is not the reviewed device')
    return {'device_uuid': device_uuid, 'cdrom_count': len(root.findall('Hardware/CdRom'))}


def _check_descriptor(association):
    """The descriptor bytes select the reviewed top; the host record names that top, its chain and every extent."""
    try:
        selection = identity.select_top(association.get('descriptor'))
    except identity.Refusal as exc:
        raise Refusal('descriptor: ' + str(exc)) from exc
    if _lower(selection['disk_uid']) != _lower(REVIEWED['disk_uid']):
        raise Refusal('descriptor disk UID is not the reviewed disk')
    host = association.get('host') or {}
    if _lower(host.get('top_guid')) != _lower(selection['top_guid']):
        raise Refusal('association top differs from the descriptor top')
    if [_lower(g) for g in host.get('chain') or []] != [_lower(g) for g in selection['chain']]:
        raise Refusal('association chain differs from the descriptor chain')
    extents = host.get('extents')
    expected = {_lower(g): v['file'] for g, v in selection['images'].items()}
    if not isinstance(extents, dict) or {_lower(g): f for g, f in extents.items()} != expected:
        raise Refusal('association extents differ from every image of the descriptor')
    return selection


def _check_guest(association):
    """Exactly one whole disk, /dev/sda, the reviewed root as its partition, one boot."""
    guest = association.get('guest') or {}
    boot = association.get('boot_id')
    if not isinstance(boot, str) or not boot or guest.get('boot_id') != boot:
        raise Refusal('guest boot identity differs from the read bracket')
    blocks = guest.get('blocks')
    if not isinstance(blocks, list) or not blocks or not all(isinstance(b, dict) for b in blocks):
        raise Refusal('guest block inventory is missing')
    disks = []
    for block in blocks:
        kind = block.get('type')
        if kind == 'disk':
            disks.append(block)
        elif kind not in ('loop', 'rom', 'part'):
            raise Refusal('unclassified guest block device: %s' % block.get('name'))
    names = [d.get('name') for d in disks]
    if names != [REVIEWED['root_parent']]:
        raise Refusal('guest whole disks are %s; the association needs exactly /dev/sda' % names)
    disk = disks[0]
    if disk.get('path') != REVIEWED['disk_path'] or disk.get('size') != REVIEWED['root_size']:
        raise Refusal('guest /dev/sda path or size differs from the reviewed disk')
    if disk.get('removable') not in (0, False):
        raise Refusal('guest /dev/sda is removable or its removable flag is missing; not a fixed data disk')
    controller = disk.get('controller') or ''
    if not controller.startswith(REVIEWED['disk_controller_prefix']) or '/virtual/' in controller:
        raise Refusal('guest /dev/sda is not on the reviewed platform ATA controller: %r' % controller)
    root = guest.get('root') or {}
    if (root.get('device') != REVIEWED['root_device'] or root.get('fstype') != REVIEWED['root_fstype']
            or root.get('uuid') != REVIEWED['root_uuid'] or root.get('parent') != REVIEWED['root_parent']):
        raise Refusal('guest root differs from the reviewed root chain')
    if not any(b.get('name') == 'sda2' and b.get('type') == 'part' and b.get('pkname') == 'sda' for b in blocks):
        raise Refusal('guest root is not a partition of /dev/sda')
    serial = guest.get('parent_serial') or ''
    sysfs = guest.get('sysfs_serial')
    if serial and sysfs is not None and sysfs != serial:
        raise Refusal('guest serial sources disagree')
    return serial


def _run(association, supported, pins):
    """Each premise on its own inputs. Returns (table, result); result is None unless every premise passes."""
    association = association if isinstance(association, dict) else {}
    table = []
    values = {}

    def step(name, fn, *args):
        try:
            values[name] = fn(*args)
            table.append((name, 'pass', ''))
        except Refusal as exc:
            values[name] = None
            table.append((name, 'refuse', str(exc)))
        return values[name]

    pin = step('mechanism', _check_mechanism, association, supported, pins)
    read = step('host_read', _check_host_read, association, pin)
    config = step('config', _check_config, association)
    step('descriptor', _check_descriptor, association)
    guest_serial = step('guest_blocks', _check_guest, association)
    if (read is not None and config is not None
            and read['optical_count'] != config['cdrom_count']):
        table = [(name, 'refuse', 'host inventory optical count differs from the config CdRom count')
                 if name == 'host_read' else (name, status, reason)
                 for name, status, reason in table]
        return table, None
    if any(status != 'pass' for _, status, _ in table):
        return table, None
    selection = values['descriptor']
    return table, {'mechanism': association['mechanism'], 'vm': REVIEWED['vm'],
                   'disk_uid': REVIEWED['disk_uid'], 'device_uuid': config['device_uuid'],
                   'image_path': read['image_path'], 'system_name': read['sys_name'],
                   'serial': read.get('serial') or guest_serial or '',
                   'top_guid': selection['top_guid'],
                   'chain': list(selection['chain']), 'extents': sorted(selection['images']),
                   'boot_id': association['boot_id'], 'read_sha256': read['sha256'],
                   'premises': PREMISES, 'not_established': NOT_ESTABLISHED,
                   'trust': pin['trust']}


def premise_table(association, supported=frozenset(), pins=None):
    """Each premise and its own outcome: ('pass'|'refuse', reason). Use it to show what facts establish."""
    table, _ = _run(association, supported, PINS if pins is None else pins)
    return table


def verify(association, supported=frozenset(), pins=None):
    """The association if every premise passes; otherwise the first refusal in premise order."""
    table, result = _run(association, supported, PINS if pins is None else pins)
    if result is None:
        first = next(reason for _, status, reason in table if status == 'refuse')
        raise Refusal(first)
    return result


def read_inventory(transport=None, pin=None, boot_id=None, read_boot_id=None):
    """Production inventory dispatch: the pinned helper, inside the caller's authenticated boot
    bracket. Tests inject fixture transport; never a native receipt."""
    return inventory.read_inventory(transport=transport, pin=PINS[MECHANISM] if pin is None else pin,
                                    boot_id=boot_id, read_boot_id=read_boot_id)


def reader_executable():
    """True only when the pinned reader's own ABI sources are complete. Never a guess."""
    return inventory.reader_executable()


def reader_limitation():
    """The exact external limitation and the one supported next read, in one line."""
    return inventory.reader_limitation()


def host_facts(descriptor, extents):
    """The host-side top, chain and extent record for the descriptor premise, from the descriptor
    bytes this stage observed and its own extent inventory. A chain image the observed inventory does
    not carry refuses: the record may not name an extent the stage did not see."""
    try:
        selection = identity.select_top(descriptor)
    except identity.Refusal as exc:
        raise Refusal('descriptor: ' + str(exc)) from exc
    if not isinstance(extents, dict) or not extents:
        raise Refusal('the observed extent inventory is missing')
    files = {guid: value['file'] for guid, value in selection['images'].items()}
    if set(files.values()) - set(extents):
        raise Refusal('the descriptor chain names an extent the observed inventory does not carry')
    return {'top_guid': selection['top_guid'], 'chain': list(selection['chain']), 'extents': files}


def _guest_type(node, name):
    """The documented lsblk TYPE field is the only source of a device class here."""
    kind = node.get('type') if isinstance(node, dict) else None
    if not isinstance(kind, str) or not kind:
        raise Refusal('guest block %s has no documented lsblk type' % name)
    return kind


def guest_facts(boot_id, block, lsblk, root):
    """The guest side of one association, from the writer-bound observations of that same boot.

    The fixed sysfs read supplies the device size, removable flag, parent and controller path; the
    documented lsblk read supplies the device type. Both describe this one boot, so a device named by
    one inventory and not the other, a differing major:minor, or a differing size refuses instead of
    being dropped: a mapper may not hide a device from the verifier. Unclassified device types are
    forwarded, so guest_blocks refuses them rather than this mapper deciding they are harmless.
    """
    boot_id = _require_boot(boot_id, 'of the guest facts')
    blocks = block.get('blocks') if isinstance(block, dict) else None
    nodes = lsblk.get('nodes') if isinstance(lsblk, dict) else None
    if not isinstance(blocks, dict) or not isinstance(nodes, dict) or not blocks or not nodes:
        raise Refusal('the guest block observations are missing')
    if set(blocks) != set(nodes):
        missing = sorted(set(blocks) ^ set(nodes))
        raise Refusal('the two guest block inventories name different devices: %s' % ', '.join(missing[:8]))
    rows = []
    for name in sorted(blocks):
        sysfs, node = blocks[name], nodes[name]
        if sysfs.get('maj_min') != node.get('maj_min'):
            raise Refusal('guest block %s major:minor differs between the two inventories' % name)
        sectors = sysfs.get('sectors')
        if type(sectors) is not int or sectors < 0 or sectors * 512 != node.get('bytes'):
            raise Refusal('guest block %s size differs between the two inventories' % name)
        removable = sysfs.get('removable')
        rows.append({'name': name, 'type': _guest_type(node, name), 'path': '/dev/' + name,
                     'size': sectors * 512,
                     'removable': removable if type(removable) is int else removable,
                     'pkname': sysfs.get('parent'), 'controller': sysfs.get('controller_path')})
    facts_root = root if isinstance(root, dict) else {}
    device = facts_root.get('device')
    if not isinstance(device, str) or device not in blocks:
        raise Refusal('the guest root device is not in the observed block inventory')
    whole = blocks.get(blocks[device].get('parent'))
    serial = whole.get('serial') if isinstance(whole, dict) else None
    serial = serial.get('text') if isinstance(serial, dict) and serial.get('status') == 'observed' else None
    return {'boot_id': boot_id, 'parent_serial': serial or '', 'sysfs_serial': serial,
            'root': {'device': '/dev/' + device, 'fstype': facts_root.get('fstype'),
                     'uuid': facts_root.get('filesystem_uuid'),
                     'parent': blocks[device].get('parent')},
            'blocks': rows}


def association_facts(boot_id, read_boot_id, config, descriptor, extents, block, lsblk, root,
                      transport=None, pin=None):
    """The complete association record for one observation stage, not a receipt.

    One boot-bound host inventory read, the current registered-config and descriptor originals, the
    extent inventory the stage observed and the guest block observations of the same boot. The
    caller feeds the returned record to setup.verify_disk_association; nothing here decides that the
    association holds, and a caller constant is never a reading.
    """
    if not isinstance(config, (bytes, bytearray)) or not config:
        raise Refusal('the registered config original is missing')
    if not isinstance(descriptor, (bytes, bytearray)) or not descriptor:
        raise Refusal('the descriptor original is missing')
    read = read_inventory(transport=transport, pin=pin, boot_id=boot_id, read_boot_id=read_boot_id)
    return {'mechanism': MECHANISM, 'boot_id': _require_boot(boot_id, 'of the association'),
            'config': bytes(config), 'descriptor': bytes(descriptor), 'host_read': read,
            'host': host_facts(bytes(descriptor), extents),
            'guest': guest_facts(boot_id, block, lsblk, root)}
