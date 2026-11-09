"""Controller-to-guest disk association: the fail-closed verifier and its premises.

Source controls only. This module checks the facts it is given: the registered config bytes, the
descriptor bytes, one host read and the guest block inventory. It opens no VM, image, host service
or guest, and it runs no tool. A mechanism qualifies only when PINS names it: the exact tool bytes,
version, Desktop build, accessor, and the output grammar this module parses. PINS is empty, so every
association refuses.

Premises, all required. premise_table reports each one on its own inputs, so a partial fact set shows
which premises it establishes:
  mechanism     the mechanism is supported and pinned
  host_read     one pinned read-only accessor, bounded, exit 0, bracketed by the same boot identity,
                output in the pinned grammar; yields the serial and the image UID it names
  config        the registered VM has exactly one hard disk: hdd0, SATA index 0, the reviewed device
                Uuid, enabled and connected, image-backed (not passthrough, not remote); every Hardware
                child is a reviewed tag, and no element outside Hdd and CdRom carries a block-capable name
                at any depth; USB holds exactly one device, the retained camera, every field exact; the connect history names only the reviewed virtual devices; CD-ROM devices are counted and not
                treated as disks
  descriptor    the descriptor bytes select the reviewed disk UID, the top, the chain and every extent
  guest_blocks  the guest has exactly one whole disk, /dev/sda, non-removable and on the platform ATA
                controller, and the reviewed root is its partition; loop and ROM devices are classified out
                by type, any other type refuses
  serial        the host read serial and the guest parent serial are equal and non-empty

Not established by any premise: that the config device Uuid and the descriptor disk UID name the same
image (the retained facts carry different identifiers and no documented rule links them); that guest
ata1 is SATA position 0; the origin of the bytes given here (admission of retained receipts is not in
this module); image bodies and encryption. Size agreement between descriptor and guest is not evidence.
"""
import hashlib
import re
import xml.etree.ElementTree as ET

import linux_disk_identity as identity
from linux_supervisor import Refusal

# The reviewed identity. linux_setup's constants must equal these; linux_disk_association_tests checks it.
REVIEWED = {
    'vm': '433c9e5b-c7b9-469f-9633-24b4addcd3b5',
    'disk_uid': '{44b5d35e-b053-4aae-9140-266a00bbef9a}',
    'device_uuid': '{9447f202-8d43-4ec2-80c9-ce527b34bd9d}',
    'system_name': 'harddisk1.hdd',
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
# Remote and Enabled/Connected meanings are observed, not sourced (see linux-setup.md). A physical or remote
# disk can match the count and the name and is still not this disk.
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
PREMISES = ('mechanism', 'host_read', 'config', 'descriptor', 'guest_blocks', 'serial')
UNSUPPORTED = ('unsupported controller-to-guest disk association: no source-verified host read of the '
               'disk serial and image UID is pinned; the guest chain is established, the link is not')
NOT_ESTABLISHED = (
    'config device Uuid and descriptor disk UID are different identifiers and no documented rule '
    'links them; the pinned host read must name the image UID itself; ata1 is not mapped to SATA '
    'position 0; the origin of the given bytes is not admitted here; image bodies not read; the guest '
    'presents no disk the registered config does not declare (the primary Parallels sources pinned for '
    'this module do not state it, so a count of one on both sides is not a bijection)')

# Source-verified host reads. Empty: no accessor is pinned. An entry names the exact tool bytes and
# version, the Desktop build, the read-only accessor, the largest output, and the output fields.
PINS = {}


def _lower(value):
    return value.lower() if isinstance(value, str) else value


def _text(node, tag, where):
    """The one text value of a field. A repeated field refuses: a later duplicate must not hide behind the first."""
    children = node.findall(tag)
    if len(children) != 1:
        raise Refusal('%s %s is missing or repeated (%d occurrences)' % (where, tag, len(children)))
    child = children[0]
    if not child.text or child.text != child.text.strip():
        raise Refusal('%s %s is missing, empty or padded' % (where, tag))
    return child.text


def _check_mechanism(association, supported, pins):
    mechanism = association.get('mechanism') if isinstance(association, dict) else None
    if mechanism not in supported or mechanism not in pins:
        raise Refusal(UNSUPPORTED)
    return pins[mechanism]


def _check_host_read(association, pin):
    """One pinned read: tool bytes, accessor, bounds, exit and boot bracket. Returns the parsed fields."""
    if pin is None:
        raise Refusal('no pinned host read, so no serial or image UID is read')
    read = association.get('host_read')
    if not isinstance(read, dict):
        raise Refusal('host read is missing')
    if read.get('tool_path') != pin['tool_path'] or read.get('tool_sha256') != pin['sha256']:
        raise Refusal('host read tool is not the pinned bytes')
    if read.get('tool_version') != pin['version'] or read.get('desktop_build') != pin['desktop_build']:
        raise Refusal('host read tool version or Desktop build differs from the pin')
    if read.get('api') != pin['api'] or read.get('readonly') is not True:
        raise Refusal('host read is not the pinned read-only accessor')
    if read.get('status') != 'ok' or read.get('exit') != 0 or read.get('truncated') is not False:
        raise Refusal('host read is truncated, failed or unknown')
    output = read.get('output')
    if not isinstance(output, bytes) or not output or len(output) > pin['max_bytes']:
        raise Refusal('host read output is missing or over its bound')
    boot = association.get('boot_id')
    if not isinstance(boot, str) or not boot or read.get('boot_ids') != [boot, boot]:
        raise Refusal('host read is not bracketed by the guest boot identity')
    if read.get('vm') != REVIEWED['vm']:
        raise Refusal('host read names another VM')
    try:
        text = output.decode('ascii')
    except UnicodeDecodeError as exc:
        raise Refusal('host read output is not ASCII') from exc
    if not text.endswith('\n'):
        raise Refusal('host read output is not newline-terminated')
    fields = {}
    for line in text[:-1].split('\n'):
        key, sep, value = line.partition('=')
        if not sep or key in fields or not re.fullmatch(r'[a-z_]+', key) or not value or value != value.strip():
            raise Refusal('host read output line is malformed or repeated')
        fields[key] = value
    if set(fields) != set(pin['fields']):
        raise Refusal('host read output fields differ from the pinned grammar')
    if _lower(fields['image_uid']) != _lower(REVIEWED['disk_uid']):
        raise Refusal('host read names another image UID')
    return {'serial': fields['serial'], 'image_uid': fields['image_uid'],
            'sha256': hashlib.sha256(output).hexdigest()}


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
    """Exactly one whole disk, /dev/sda, the reviewed root as its partition, one boot, one serial source."""
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
    serial = guest.get('parent_serial')
    sysfs = guest.get('sysfs_serial')
    if not serial:
        raise Refusal('guest parent serial is missing')
    if sysfs is not None and sysfs != serial:
        raise Refusal('guest serial sources disagree')
    return serial


def _check_serial(host_serial, guest_serial):
    if not host_serial or not guest_serial:
        raise Refusal('host or guest serial is missing')
    if host_serial != guest_serial:
        raise Refusal('host serial does not match the guest parent serial')
    return host_serial


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
    step('config', _check_config, association)
    step('descriptor', _check_descriptor, association)
    guest_serial = step('guest_blocks', _check_guest, association)
    step('serial', _check_serial, read['serial'] if read else None, guest_serial)
    if any(status != 'pass' for _, status, _ in table):
        return table, None
    config = values['config']
    selection = values['descriptor']
    return table, {'mechanism': association['mechanism'], 'vm': REVIEWED['vm'],
                   'disk_uid': REVIEWED['disk_uid'], 'device_uuid': config['device_uuid'],
                   'serial': read['serial'], 'top_guid': selection['top_guid'],
                   'chain': list(selection['chain']), 'extents': sorted(selection['images']),
                   'boot_id': association['boot_id'], 'read_sha256': read['sha256'],
                   'premises': PREMISES, 'not_established': NOT_ESTABLISHED}


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
