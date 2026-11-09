"""Controls for the controller-to-guest association verifier (linux_disk_association).

Portable controls use synthetic candidates shaped like the retained facts: a registered config excerpt,
the documented descriptor shape from linux_disk_identity_tests, one host inventory and a guest inventory.
TEST_PINS is a test-only mechanism used to isolate premise logic. Production PINS names
parallels-host-inventory-trust-v1. No VM, host service, image, disk or guest is touched. A pass of
the verifier on fixture bytes is source admission of the logic, not a fresh native inventory.
"""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_disk_association as dal
import linux_disk_identity as identity
import linux_disk_identity_tests as identity_tests
import linux_disk_inventory as inventory
import linux_setup as setup
from linux_supervisor import Refusal

VM = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = setup.DISK_UID
OTHER_UID = '{11111111-2222-3333-4444-555555555555}'
DEVICE_UUID = '{9447f202-8d43-4ec2-80c9-ce527b34bd9d}'
BOOT = '6de5909b-25da-444a-9610-731ee1247b11'
OTHER_BOOT = '18ad4b3d-6b39-4d23-a9bd-51e9ca4f3ed4'
OTHER_VM = '00000000-0000-0000-0000-000000000000'
ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'
SERIAL = 'BV9VAH2KP2Q4N4A04SN0'
# The retained 747 sysfs controller of sda (the platform ATA host), from the block-sysfs original.
CONTROLLER = '/sys/devices/platform/PRL4010:00/ata1/host0/target0:0:0/0:0:0:0'
# The Hardware children of the retained registered config, before and after the observation.
RETAINED_HARDWARE_TAGS = {
    'Cpu', 'Chipset', 'Clock', 'Memory', 'Video', 'HibernateState', 'VirtIO', 'VirtIOSerial',
    'VirtIOInput', 'VirtIOVsock', 'CdRom', 'Hdd', 'NetworkAdapter', 'Sound', 'USB',
    'UsbConnectHistory', 'TpmChip',
}
TOP = '{11111111-1111-1111-1111-111111111111}'
CHAIN = [TOP, '{22222222-2222-2222-2222-222222222222}', '{33333333-3333-3333-3333-333333333333}']
EXTENTS = {CHAIN[0]: 'a.hds', CHAIN[1]: 'b.hds', CHAIN[2]: 'r.hds'}
TEST_MECHANISM = 'test-disk'
TEST_HELPER = '/test/linux_disk_inventory.py'
TEST_LIBRARY = '/test/ParallelsVirtualizationSDK'
TEST_LIBRARY_SHA = 'b' * 64
TEST_ARGV = ['/usr/bin/python3', '-I', '-B', TEST_HELPER, 'inventory']
TEST_PINS = {TEST_MECHANISM: {
    'tool_path': TEST_HELPER, 'sha256': 'a' * 64, 'version': '27.1.0',
    'desktop_build': '27.1.0-58756', 'api': 'PrlVmCfg_GetAllDevices',
    'argv': TEST_ARGV, 'max_bytes': inventory.MAX_BYTES, 'max_devices': inventory.MAX_DEVICES,
    'library_path': TEST_LIBRARY, 'library_sha256': TEST_LIBRARY_SHA,
    'library_build': '58756', 'home': setup.PVM_HOME, 'vm': VM, 'trust': 'test-only',
}}
TEST_SUPPORTED = frozenset({TEST_MECHANISM})
CAMERA_SYS = dal.USB_CAMERA_RECORD['SystemName']
IMAGE_PATH = setup.PVM_HOME + '/' + setup.DISK_DIR


# The retained 747 Hdd flags (Enabled, Connected, Remote, Passthrough, DiskType); overridden per control.
RETAINED_HDD_FLAGS = {'Enabled': '1', 'Connected': '1', 'Remote': '0', 'Passthrough': '0', 'DiskType': '1'}


# The retained virtual camera, every field, as the 747 registered config carries it. Literal, not the module's.
RETAINED_USB_CAMERA = ('<USB><Enabled>1</Enabled><Connected>1</Connected><EmulatedType>3</EmulatedType>'
                       '<SystemName>VIRTUAL@UVC@|203a|fff9|high|--|6C707041-05AC-0011-0007-000000000001</SystemName>'
                       '<UserFriendlyName>Virtual Video Camera</UserFriendlyName><Remote>0</Remote>'
                       '<AutoConnect>1</AutoConnect><ConnectReason>0</ConnectReason>'
                       '<DeviceDescription></DeviceDescription><UsbType>2</UsbType></USB>')


def config_xml(vm=VM, index='0', interface='2', system='harddisk1.hdd', uuid=DEVICE_UUID, extra='', tail='',
               usb=RETAINED_USB_CAMERA, **flags):
    """A registered-config excerpt in the retained shape: one Hdd under Hardware, one CdRom.

    Keyword flags override the Hdd's retained flags, for example Passthrough='1' for a physical disk. `tail` is
    XML appended after the root's first section, for a second section."""
    values = dict(RETAINED_HDD_FLAGS, **flags)
    flag_xml = ''.join(f'<{name}>{value}</{name}>' for name, value in values.items())
    return (f'<ParallelsVirtualMachine><Identification><VmUuid>{{{vm}}}</VmUuid></Identification>'
            f'<Hardware><CdRom id="0"><Index>0</Index><Enabled>1</Enabled><Connected>0</Connected>'
            f'<InterfaceType>2</InterfaceType><Passthrough>1</Passthrough></CdRom>'
            f'<Hdd id="1"><Uuid>{uuid}</Uuid><Index>{index}</Index><InterfaceType>{interface}</InterfaceType>'
            f'<SystemName>{system}</SystemName>{flag_xml}</Hdd>{usb}{extra}</Hardware>{tail}</ParallelsVirtualMachine>').encode()


def inventory_devices(extra=(), **hdd_over):
    hdd = {'type': inventory.PDE_HARD_DISK, 'index': 0, 'iface': inventory.PMS_SATA,
           'sys_name': 'harddisk1.hdd', 'image_path': IMAGE_PATH,
           'enabled': 1, 'connected': 1, 'remote': 0, 'passthrough': 0,
           'disk_type': inventory.PHD_EXPANDING_HARD_DISK,
           'emulated_type': inventory.PDT_USE_IMAGE_FILE}
    hdd.update(hdd_over)
    devices = [
        {'type': inventory.PDE_OPTICAL_DISK, 'index': 0, 'iface': inventory.PMS_SATA,
         'sys_name': 'cdrom1.iso', 'image_path': '-', 'enabled': 1, 'connected': 0,
         'remote': 0, 'passthrough': 1, 'disk_type': -1, 'emulated_type': 1},
        hdd,
        {'type': inventory.PDE_USB_DEVICE, 'index': 0, 'iface': -1, 'sys_name': CAMERA_SYS,
         'image_path': '-', 'enabled': 1, 'connected': 1, 'remote': 0, 'passthrough': 0,
         'disk_type': -1, 'emulated_type': 3},
    ]
    devices.extend(extra)
    return devices


def inventory_text(pin=None, devices=None, serial=SERIAL, **over):
    pin = TEST_PINS[TEST_MECHANISM] if pin is None else pin
    fields = {
        'vm': VM, 'home': pin['home'], 'library_path': pin['library_path'],
        'library_sha256': pin['library_sha256'], 'library_version': pin['version'],
        'library_build': pin['library_build'], 'state_before': inventory.VMS_RUNNING,
        'state_after': inventory.VMS_RUNNING,
        'devices': inventory_devices() if devices is None else devices, 'serial': serial,
    }
    fields.update(over)
    return inventory.render_output(**fields)


def host_read(pin=None, **over):
    pin = TEST_PINS[TEST_MECHANISM] if pin is None else pin
    read = {'tool_path': pin['tool_path'], 'tool_sha256': pin['sha256'],
            'tool_version': pin['version'], 'desktop_build': pin['desktop_build'],
            'api': pin['api'], 'argv': list(pin['argv']), 'readonly': True, 'status': 'ok',
            'exit': 0, 'truncated': False, 'vm': VM, 'boot_ids': [BOOT, BOOT],
            'output': inventory_text(pin).encode()}
    read.update(over)
    return read


def guest_blocks(**over):
    blocks = [{'name': 'sda', 'type': 'disk', 'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'pkname': None,
               'removable': 0, 'controller': CONTROLLER},
              {'name': 'sda1', 'type': 'part', 'pkname': 'sda', 'size': 2201600},
              {'name': 'sda2', 'type': 'part', 'pkname': 'sda', 'size': 266231775},
              {'name': 'sr0', 'type': 'rom', 'path': '/dev/sr0', 'size': 1073741312, 'pkname': None}]
    blocks += [{'name': 'loop%d' % i, 'type': 'loop', 'size': 4096, 'pkname': None} for i in range(3)]
    return blocks


def guest(**over):
    facts = {'boot_id': BOOT, 'parent_serial': SERIAL, 'sysfs_serial': None,
             'root': {'device': '/dev/sda2', 'fstype': 'ext4', 'uuid': ROOT_UUID, 'parent': 'sda'},
             'blocks': guest_blocks()}
    facts.update(over)
    return facts


def valid_association(**over):
    """A complete candidate that passes under TEST_PINS. Each control breaks one field of it."""
    association = {'mechanism': TEST_MECHANISM, 'boot_id': BOOT, 'config': config_xml(),
                   'descriptor': identity_tests.doc(), 'host_read': host_read(),
                   'host': {'vm': VM, 'top_guid': TOP, 'chain': list(CHAIN), 'extents': dict(EXTENTS)},
                   'guest': guest()}
    association.update(over)
    return association


def production_association(**over):
    """A complete candidate for the production pin. Fixture bytes, not a native receipt."""
    pin = dal.PINS[dal.MECHANISM]
    association = valid_association(mechanism=dal.MECHANISM, host_read=host_read(pin))
    association.update(over)
    return association


# The guest block observations of the reviewed boot, in the two writer-bound read shapes: the fixed
# sysfs inventory and the documented lsblk inventory. Sizes agree because both describe one boot.
SDA_SECTORS = setup.ROOT_BYTES // 512
SDA_SYSPATH = CONTROLLER


def sysfs_block(name, maj_min, sectors, partition, parent, removable, syspath, serial=None):
    row = {'name': name, 'maj_min': maj_min, 'partition': partition, 'sectors': sectors,
           'read_only': 0, 'removable': removable, 'syspath': syspath,
           'controller_path': syspath.split('/block/')[0], 'controller_kind': 'ata',
           'controller_components': [], 'parent': parent}
    if partition == 0:
        row['serial'] = ({'status': 'observed', 'hex': '', 'bytes': len(serial or ''), 'text': serial}
                         if serial else {'status': 'missing'})
        row['model'] = {'status': 'missing'}
        row['vendor'] = {'status': 'missing'}
    return row


def guest_block_observations(extra_sysfs=(), extra_lsblk=()):
    """The reviewed boot's two guest inventories, plus any extra device a control injects."""
    disks = [
        ('sda', '8:0', SDA_SECTORS, 0, None, 0, SDA_SYSPATH + '/block/sda', SERIAL),
        ('sda1', '8:1', 4096, 1, 'sda', {'status': 'not-applicable'}, SDA_SYSPATH + '/block/sda/sda1', None),
        ('sda2', '8:2', SDA_SECTORS - 4096, 1, 'sda', {'status': 'not-applicable'},
         SDA_SYSPATH + '/block/sda/sda2', None),
        ('sr0', '11:0', 2048, 0, None, 1, '/sys/devices/platform/PLATFORM:00/ata2/block/sr0', None),
        ('loop0', '7:0', 2, 0, None, 0, '/sys/devices/virtual/block/loop0', None),
    ]
    blocks = {row[0]: sysfs_block(*row) for row in disks}
    nodes = {}
    for name, majmin, sectors, partition, parent, removable, syspath, serial in disks:
        nodes[name] = {'name': name, 'maj_min': majmin, 'parent': parent,
                       'type': 'loop' if name.startswith('loop') else ('rom' if name.startswith('sr') else
                                                                      ('part' if partition else 'disk')),
                       'bytes': sectors * 512, 'fstype': None, 'uuid': None, 'mountpoint': None}
    nodes['sda2'].update(fstype='ext4', uuid=ROOT_UUID)
    for name, record in extra_sysfs:
        blocks[name] = record
    for name, record in extra_lsblk:
        nodes[name] = record
    return blocks, nodes


def guest_root_facts():
    return {'status': 'resolved', 'device': 'sda2', 'fstype': 'ext4', 'filesystem_uuid': ROOT_UUID,
            'ancestry': ['sda2', 'sda'], 'controller_kind': 'ata',
            'controller_path': SDA_SYSPATH, 'reasons': []}


def boot_reader(sequence):
    """A guest boot-original reader over a fixed sequence; a control reads what it returns."""
    values = list(sequence)

    def read():
        if not values:
            raise AssertionError('the boot bracket reader was called more times than the control allows')
        return values.pop(0)
    return read


def inventory_transport(output=None, **over):
    """A private fixture transport: the recorded copy of the pinned-grammar output, no process."""
    pin = dal.PINS[dal.MECHANISM]
    stdout = inventory_text(pin) if output is None else output
    if isinstance(stdout, str):
        stdout = stdout.encode()
    result = {'disposition': 'exited', 'waited_exit': 0, 'stdout': stdout, 'stderr': b''}
    result.update(over)
    return lambda argv, stdin_path, timeout, cap: dict(result)


def descriptor_extents():
    """The extent inventory the stage would observe for the documented descriptor's chain."""
    selection = identity.select_top(identity_tests.doc())
    return {value['file']: {'path': '/x/' + value['file'], 'guid': guid} for guid, value in selection['images'].items()}


def disk_witness():
    return valid_association()


def table(association, supported=TEST_SUPPORTED, pins=TEST_PINS):
    return {name: (status, reason) for name, status, reason in dal.premise_table(association, supported, pins)}


class DiskAssociationControls(unittest.TestCase):
    def assertRefusedAt(self, premise, association, supported=TEST_SUPPORTED, pins=TEST_PINS):
        self.assertEqual(table(association, supported, pins)[premise][0], 'refuse', table(association, supported, pins))
        with self.assertRaises(Refusal):
            dal.verify(association, supported, pins)
        # The production integration, with the same pins: the gate must refuse too.
        with mock.patch.object(dal, 'PINS', pins), self.assertRaises(Refusal):
            setup.verify_disk_association(association, supported)

    def test_production_gate_refuses_the_test_mechanism(self):
        self.assertEqual(set(dal.PINS), {dal.MECHANISM})
        self.assertEqual(setup.SUPPORTED_DISK_MECHANISMS, frozenset({dal.MECHANISM}))
        with self.assertRaises(Refusal):
            setup.verify_disk_association(valid_association())
        with self.assertRaises(Refusal):
            dal.verify(valid_association(), frozenset(), dal.PINS)
        with self.assertRaises(Refusal):
            dal.verify(valid_association(), TEST_SUPPORTED, {})
        with self.assertRaises(Refusal):
            dal.verify(valid_association(), frozenset(), TEST_PINS)

    def test_complete_candidate_passes_under_the_test_pin(self):
        result = dal.verify(valid_association(), TEST_SUPPORTED, TEST_PINS)
        self.assertEqual(result['serial'], SERIAL)
        self.assertEqual(result['top_guid'], TOP)
        self.assertEqual(result['chain'], CHAIN)
        self.assertEqual(sorted(result['extents']), sorted(CHAIN))
        self.assertEqual({status for status, _ in table(valid_association()).values()}, {'pass'})

    def test_reviewed_identity_matches_linux_setup_constants(self):
        self.assertEqual(dal.REVIEWED['vm'], setup.VM_UUID)
        self.assertEqual(dal.REVIEWED['disk_uid'], setup.DISK_UID)
        self.assertEqual(dal.REVIEWED['root_size'], setup.ROOT_BYTES)
        self.assertEqual(dal.REVIEWED['home'], setup.PVM_HOME)
        self.assertEqual(dal.REVIEWED['image_path'], setup.PVM_HOME + '/' + setup.DISK_DIR)
        self.assertEqual(dal.REVIEWED['system_name'], setup.DISK_DIR)

    def test_wrong_vm_refuses_config(self):
        self.assertRefusedAt('config', valid_association(config=config_xml(vm='00000000-0000-0000-0000-000000000000')))

    def test_wrong_controller_position_or_bus_refuses_config(self):
        self.assertRefusedAt('config', valid_association(config=config_xml(index='1')))
        self.assertRefusedAt('config', valid_association(config=config_xml(interface='0')))
        self.assertRefusedAt('config', valid_association(config=config_xml(system='harddisk2.hdd')))

    def test_wrong_device_uuid_refuses_config(self):
        self.assertRefusedAt('config', valid_association(config=config_xml(uuid=OTHER_UID)))

    def test_additional_hard_disk_refuses_config_and_is_not_hidden(self):
        extra = '<Hdd id="2"><Uuid>{55555555-5555-5555-5555-555555555555}</Uuid><Index>1</Index>' \
                '<InterfaceType>2</InterfaceType><SystemName>harddisk2.hdd</SystemName></Hdd>'
        self.assertRefusedAt('config', valid_association(config=config_xml(extra=extra)))

    def test_reviewed_hardware_tags_are_the_retained_closed_set(self):
        self.assertEqual(dal.HARDWARE_TAGS, RETAINED_HARDWARE_TAGS)

    def test_unreviewed_hardware_element_refuses_config(self):
        for tag in ('Nvme', 'Floppy', 'Scsi'):
            extra = f'<{tag} id="2"><Index>0</Index><Enabled>1</Enabled></{tag}>'
            self.assertRefusedAt('config', valid_association(config=config_xml(extra=extra)))

    def test_block_device_hidden_under_an_allowed_tag_refuses_config(self):
        """Every hiding place in the retained tag set: a USB disk, a VirtIO block child, a SATA or SCSI
        controller under Sound, a disk under the connect history, and a second USB element."""
        hidden = {
            'usb disk': '<USB><Enabled>1</Enabled><Connected>1</Connected><EmulatedType>1</EmulatedType>'
                        '<UsbType>0</UsbType><SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USB>',
            'virtio block': '<VirtIO><ModernNet>0</ModernNet><VirtIOBlk id="3"><Index>1</Index></VirtIOBlk></VirtIO>',
            'sata under sound': '<Sound><Enabled>1</Enabled><SataController><Index>1</Index></SataController></Sound>',
            'disk under history': '<UsbConnectHistory><USBPort><Location>03000000</Location>'
                                  '<SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USBPort></UsbConnectHistory>',
            'second usb element': '<USB><Enabled>1</Enabled><Connected>1</Connected><EmulatedType>3</EmulatedType>'
                                  '<UsbType>2</UsbType><SystemName>VIRTUAL@UVC@|203a|fff9|high|--|X</SystemName></USB>'
                                  '<USB><Enabled>1</Enabled><Connected>1</Connected><EmulatedType>1</EmulatedType>'
                                  '<UsbType>0</UsbType><SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USB>',
        }
        for label, extra in hidden.items():
            with self.subTest(label):
                association = valid_association(config=config_xml(extra=extra))
                self.assertEqual(table(association)['config'][0], 'refuse', table(association)['config'])
                self.assertEqual(table(association)['guest_blocks'][0], 'pass')

    def test_real_setup_gate_refuses_each_counterexample_and_admits_only_the_pinned_candidate(self):
        """Through setup.verify_disk_association, the production integration, not the module alone."""
        disk_usb = ('<USB><Enabled>1</Enabled><Connected>1</Connected><EmulatedType>1</EmulatedType>'
                    '<UsbType>0</UsbType><SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USB>')
        counterexamples = {
            'bare count, physical disk': valid_association(config=config_xml(Passthrough='1')),
            'bare count, remote disk': valid_association(config=config_xml(Remote='1')),
            'bare count, disabled disk': valid_association(config=config_xml(Enabled='0')),
            'disk hidden under USB': valid_association(config=config_xml(extra=disk_usb)),
            'guest disk removable': valid_association(guest=guest(blocks=[
                dict(b, removable=1) if b['name'] == 'sda' else b for b in guest_blocks()])),
        }
        with mock.patch.object(dal, 'PINS', TEST_PINS):
            for label, facts in counterexamples.items():
                with self.subTest(label), self.assertRaises(Refusal):
                    setup.verify_disk_association(facts, TEST_SUPPORTED)
            self.assertEqual(setup.verify_disk_association(valid_association(), TEST_SUPPORTED)['serial'], SERIAL)
        with self.assertRaises(Refusal):
            setup.verify_disk_association(valid_association(), TEST_SUPPORTED)

    def test_repeated_top_level_section_refuses_config(self):
        """A second Hardware or Identification section must not hide a block device from the first-match reads."""
        second_hardware = ('<Hardware><USB><EmulatedType>1</EmulatedType><UsbType>0</UsbType>'
                           '<SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USB></Hardware>')
        second_identification = '<Identification><VmUuid>{00000000-0000-0000-0000-000000000000}</VmUuid></Identification>'
        for label, tail in (('second Hardware', second_hardware), ('second Identification', second_identification)):
            with self.subTest(label):
                association = valid_association(config=config_xml(tail=tail))
                self.assertEqual(table(association)['config'][0], 'refuse', table(association)['config'])
                self.assertRefusedAt('config', association)

    def test_repeated_field_refuses_config_and_the_later_value_cannot_hide(self):
        """The first Passthrough is 0; a second one set to 1 must refuse, not be read past."""
        base = config_xml()
        assert base.count(b'<Passthrough>0</Passthrough>') == 1
        duplicated = base.replace(b'<Passthrough>0</Passthrough>', b'<Passthrough>0</Passthrough><Passthrough>1</Passthrough>', 1)
        self.assertRefusedAt('config', valid_association(config=duplicated))

    def test_repeated_usb_or_history_field_refuses_config(self):
        camera_twice = ('<USB><EmulatedType>3</EmulatedType><EmulatedType>1</EmulatedType><UsbType>2</UsbType>'
                        '<SystemName>VIRTUAL@UVC@|203a|fff9|high|--|X</SystemName></USB>')
        history_twice = ('<UsbConnectHistory><USBPort><SystemName>VIRTUAL@MOUSE@|203a|fffc</SystemName>'
                         '<SystemName>SanDisk|0781|5581|high|--|1234</SystemName></USBPort></UsbConnectHistory>')
        for label, extra in (('usb field twice', camera_twice), ('history field twice', history_twice)):
            with self.subTest(label):
                self.assertRefusedAt('config', valid_association(config=config_xml(extra=extra)))

    def test_reviewed_usb_camera_and_virtual_history_pass_config(self):
        history = ('<UsbConnectHistory><USBPort><Location>03000000</Location>'
                   '<SystemName>VIRTUAL@MOUSE@|203a|fffc|full|--|PW3.0</SystemName><Timestamp>1794172717586</Timestamp>'
                   '</USBPort><USBPort><SystemName>VIRTUAL@UVC@|203a|fff9|high|--|6C707041-05AC-0011-0007-000000000001'
                   '</SystemName></USBPort></UsbConnectHistory>')
        association = valid_association(config=config_xml(extra=history))
        self.assertEqual(table(association)['config'][0], 'pass')
        with mock.patch.object(dal, 'PINS', TEST_PINS):
            self.assertEqual(setup.verify_disk_association(association, TEST_SUPPORTED)['serial'], SERIAL)

    def test_second_or_altered_usb_camera_refuses_config_through_the_integration(self):
        """Both counts are one for the disk; the USB device is what differs. Each shape refuses at config."""
        second_matching = RETAINED_USB_CAMERA
        second_other_suffix = RETAINED_USB_CAMERA.replace('6C707041-05AC-0011-0007-000000000001', 'AAAAAAAA-0000-0000-0000-000000000002')
        altered_suffix_only = RETAINED_USB_CAMERA.replace('6C707041-05AC-0011-0007-000000000001', 'AAAAAAAA-0000-0000-0000-000000000002')
        extra_child = RETAINED_USB_CAMERA.replace('</USB>', '<Serial>X</Serial></USB>')
        missing_field = RETAINED_USB_CAMERA.replace('<ConnectReason>0</ConnectReason>', '')
        for label, usb, extra in (
                ('second matching camera', RETAINED_USB_CAMERA, second_matching),
                ('second camera, other suffix', RETAINED_USB_CAMERA, second_other_suffix),
                ('single camera, altered suffix', altered_suffix_only, ''),
                ('camera with an extra field', extra_child, ''),
                ('camera with a missing field', missing_field, '')):
            with self.subTest(label):
                association = valid_association(config=config_xml(usb=usb, extra=extra))
                self.assertEqual(table(association)['guest_blocks'][0], 'pass')
                self.assertRefusedAt('config', association)

    def test_nested_xml_or_attributes_inside_a_reviewed_field_refuse(self):
        """The reviewed text is the text before a nested element; the nested part must not pass unread."""
        name = '6C707041-05AC-0011-0007-000000000001'
        shapes = {
            'nested element in SystemName': RETAINED_USB_CAMERA.replace(
                f'<SystemName>VIRTUAL@UVC@|203a|fff9|high|--|{name}</SystemName>',
                f'<SystemName>VIRTUAL@UVC@|203a|fff9|high|--|{name}<Extra>x</Extra></SystemName>'),
            'nested element in Enabled': RETAINED_USB_CAMERA.replace(
                '<Enabled>1</Enabled>', '<Enabled>1<Hidden>0</Hidden></Enabled>'),
            'attribute on Connected': RETAINED_USB_CAMERA.replace(
                '<Connected>1</Connected>', '<Connected state="x">1</Connected>'),
        }
        for label, usb in shapes.items():
            with self.subTest(label):
                self.assertRefusedAt('config', valid_association(config=config_xml(usb=usb)))
        history = ('<UsbConnectHistory><USBPort><SystemName>VIRTUAL@MOUSE@|203a|fffc|full|--|PW3.0'
                   '<Note>x</Note></SystemName></USBPort></UsbConnectHistory>')
        self.assertRefusedAt('config', valid_association(config=config_xml(extra=history)))
        hdd_nested = config_xml().replace(b'<Passthrough>0</Passthrough>', b'<Passthrough>0<Note>x</Note></Passthrough>', 1)
        self.assertRefusedAt('config', valid_association(config=hdd_nested))

    def test_usb_history_name_outside_the_reviewed_set_refuses(self):
        history = ('<UsbConnectHistory><USBPort><SystemName>VIRTUAL@UVC@|203a|fff9|high|--|NOT-REVIEWED</SystemName>'
                   '</USBPort></UsbConnectHistory>')
        self.assertRefusedAt('config', valid_association(config=config_xml(extra=history)))

    def test_bare_count_of_one_is_not_the_disk(self):
        """Both sides count one, so a counting heuristic accepts. The host source is not an image-backed disk."""
        counterexamples = {
            'physical passthrough disk': dict(Passthrough='1'),
            'remote disk': dict(Remote='1'),
            'disabled disk': dict(Enabled='0'),
            'disconnected disk': dict(Connected='0'),
            'non-plain disk type': dict(DiskType='2'),
        }
        for label, flags in counterexamples.items():
            with self.subTest(label):
                association = valid_association(config=config_xml(**flags))
                rows = table(association)
                self.assertEqual(rows['guest_blocks'][0], 'pass')
                self.assertRefusedAt('config', association)
                self.assertEqual(rows['config'][0], 'refuse')

    def test_guest_disk_that_is_removable_or_off_the_platform_ata_refuses(self):
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=[
            dict(b, removable=1) if b['name'] == 'sda' else b for b in guest_blocks()])))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=[
            dict(b, controller='/sys/devices/virtual/block/sda') if b['name'] == 'sda' else b for b in guest_blocks()])))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=[
            dict(b, controller='/sys/devices/pci0000:00/0000:00:1c.0/usb1/1-1/block/sda') if b['name'] == 'sda' else b
            for b in guest_blocks()])))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=[
            {k: v for k, v in b.items() if k != 'controller'} if b['name'] == 'sda' else b for b in guest_blocks()])))

    def test_cdrom_is_counted_and_is_not_a_disk(self):
        self.assertEqual(table(valid_association())['config'][0], 'pass')
        self.assertEqual(dal.verify(valid_association(), TEST_SUPPORTED, TEST_PINS)['device_uuid'], DEVICE_UUID)

    def test_wrong_descriptor_disk_uid_refuses_descriptor_and_swapped_image_refuses_read(self):
        self.assertRefusedAt('descriptor', valid_association(descriptor=identity_tests.doc(uid_xml=f'<UID>{OTHER_UID}</UID>')))
        swapped = host_read(output=inventory_text(devices=inventory_devices(
            image_path='/Volumes/Stash/parallels/other.pvm/harddisk1.hdd')).encode())
        self.assertRefusedAt('host_read', valid_association(host_read=swapped))

    def test_stale_or_drifted_descriptor_and_extents_refuse(self):
        other_top = identity_tests.doc(top_node='<TopGUID>{22222222-2222-2222-2222-222222222222}</TopGUID>')
        self.assertRefusedAt('descriptor', valid_association(descriptor=other_top))
        drifted = dict(valid_association()['host'], chain=list(reversed(CHAIN)))
        self.assertRefusedAt('descriptor', valid_association(host=drifted))
        missing = dict(valid_association()['host'], extents={CHAIN[0]: 'a.hds', CHAIN[1]: 'b.hds'})
        self.assertRefusedAt('descriptor', valid_association(host=missing))
        self.assertRefusedAt('descriptor', valid_association(descriptor=identity_tests.doc(padding='1')))

    def test_guest_extra_or_missing_whole_disk_refuses(self):
        extra = guest_blocks() + [{'name': 'sdb', 'type': 'disk', 'path': '/dev/sdb', 'size': 1, 'pkname': None}]
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=extra)))
        missing = [b for b in guest_blocks() if b['name'] != 'sda']
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=missing)))

    def test_rom_and_loop_are_classified_by_type_and_never_pass_as_disks(self):
        as_disk = [dict(b, type='disk') if b['name'] == 'sr0' else b for b in guest_blocks()]
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=as_disk)))
        loop_disk = [dict(b, type='disk') if b['name'] == 'loop1' else b for b in guest_blocks()]
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=loop_disk)))
        unknown = [dict(b, type='raid') if b['name'] == 'sr0' else b for b in guest_blocks()]
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=unknown)))

    def test_wrong_guest_root_or_device_refuses(self):
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(root=dict(guest()['root'], device='/dev/sda1'))))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(root=dict(guest()['root'], parent='sdb'))))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(root=dict(guest()['root'], uuid='x'))))
        detached = [dict(b, pkname=None) if b['name'] == 'sda2' else b for b in guest_blocks()]
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(blocks=detached)))

    def test_wrong_boot_refuses_guest_and_read(self):
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(boot_id='other-boot')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(boot_ids=[BOOT, 'other-boot'])))

    def test_contradictory_serial_observation_refuses_and_absence_does_not(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=inventory_text(serial='S2').encode())))
        self.assertEqual(table(valid_association(guest=guest(parent_serial='')))['guest_blocks'][0], 'pass')
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(sysfs_serial='S9')))
        self.assertEqual(table(valid_association(host_read=host_read(
            output=inventory_text(serial=None).encode())))['host_read'][0], 'pass')

    def test_absent_sysfs_serial_is_not_a_disagreement(self):
        self.assertEqual(table(valid_association())['guest_blocks'][0], 'pass')

    def test_unsupported_tool_api_or_version_refuses_the_read(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_sha256='b' * 64)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(api='PrlVm_RegEx')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_version='26.0.0')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(desktop_build='27.2.0')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(readonly=False)))

    def test_truncated_failed_or_unknown_read_refuses(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(truncated=True)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(status='unknown')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(exit=1)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(output=b'x' * (inventory.MAX_BYTES + 1))))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=inventory_text().encode().rstrip(b'\n'))))
        doubled = inventory_text().encode().replace(b'vm=' + VM.encode() + b'\n',
                                                    b'vm=' + VM.encode() + b'\nvm=' + VM.encode() + b'\n', 1)
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(output=doubled)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=inventory_text().encode() + b'extra=1\n')))

    def test_labels_and_caller_claims_are_not_evidence(self):
        association = valid_association(serial=SERIAL, verified=True, premises='all pass',
                                        read_sha256='0' * 64, trust=True)
        del association['host_read']
        self.assertRefusedAt('host_read', association)

    def test_mechanism_must_be_supported_and_pinned(self):
        self.assertRefusedAt('mechanism', valid_association(), supported=frozenset(), pins=TEST_PINS)
        self.assertRefusedAt('mechanism', valid_association(), supported=TEST_SUPPORTED, pins={})
        self.assertRefusedAt('mechanism', valid_association(mechanism='fake'))
        self.assertRefusedAt('mechanism', {})

    def test_retained_748_config_and_guest_facts_establish_only_their_premises(self):
        """The retained config's hdd0 and CD-ROM shape, and the retained guest shape: sda, sr0 as ROM, loops.

        Values come from the retained 748 facts (config excerpt, lsblk, step010 probe sha 95c1119d...); the
        loop count is a fixture count, not the retained eleven."""
        retained_config = config_xml(uuid=DEVICE_UUID, extra='')
        config_only = dict(valid_association(config=retained_config), host_read=None)
        rows = table(config_only, frozenset(), {})
        self.assertEqual(rows['config'][0], 'pass')
        self.assertEqual(rows['guest_blocks'][0], 'pass')
        self.assertEqual(rows['mechanism'][0], 'refuse')
        self.assertEqual(rows['host_read'][0], 'refuse')
        self.assertNotIn('serial', rows)
        self.assertNotEqual(DEVICE_UUID.lower(), DISK_UID.lower())
        with self.assertRaises(Refusal):
            dal.verify(config_only, frozenset(), {})


class ProductionInventoryControls(unittest.TestCase):
    def test_inventory_module_is_in_the_request_closure(self):
        import preparation_requests as requests
        self.assertIn('linux_disk_inventory.py', requests.REQUIRED_MODULE_NAMES)

    def test_production_pin_binds_helper_bytes_library_vm_and_home(self):
        pin = dal.PINS[dal.MECHANISM]
        self.assertEqual(pin['tool_path'], str(inventory.HELPER))
        self.assertEqual(pin['sha256'], dal.HELPER_SHA256)
        self.assertEqual(pin['sha256'], inventory.helper_digest(inventory.HELPER))
        self.assertEqual(pin['vm'], VM)
        self.assertEqual(pin['argv'], inventory.inventory_argv())
        self.assertEqual(pin['library_path'], inventory.LIBRARY_PATH)
        self.assertEqual(pin['library_sha256'], inventory.LIBRARY_SHA256)
        self.assertEqual(pin['home'], setup.PVM_HOME)
        self.assertEqual(pin['api'], 'PrlVmCfg_GetAllDevices')
        self.assertNotIn('PrlVmDevHd_GetSerialNumber', inventory.ALLOWLISTED_APIS)
        self.assertNotIn('PrlVm_RegEx', inventory.ALLOWLISTED_APIS)
        self.assertIsNone(inventory.PARALLELS_API_VER)
        self.assertEqual(inventory.SESSION_APIS, (
            'PrlApi_InitEx', 'PrlApi_Deinit', 'PrlSrv_Create', 'PrlSrv_LoginLocal', 'PrlSrv_Logoff',
        ))
        for name in inventory.SESSION_APIS:
            self.assertNotIn(name, inventory.CONFIG_READ_APIS)
        self.assertEqual(inventory.INFERRED_LIST_APIS, ('PrlSrv_GetVmListEx',))
        self.assertNotIn('PrlSrv_GetVmListEx', inventory.CONFIG_READ_APIS)
        self.assertNotIn('PrlSrv_GetVmListEx', inventory.SESSION_APIS)
        self.assertEqual(inventory.ALLOWLISTED_APIS,
                         inventory.SESSION_APIS + inventory.INFERRED_LIST_APIS
                         + inventory.JOB_APIS + inventory.CONFIG_READ_APIS)
        self.assertIn('PrlSrv_LoginLocal state effect', inventory.REMAINING_NATIVE_PREMISES)
        self.assertIn('PrlSrv_GetVmListEx read-only inferred', inventory.REMAINING_NATIVE_PREMISES)
        self.assertIn('PrlVmDev_IsConnected config-or-running', inventory.REMAINING_NATIVE_PREMISES)

    def test_narrowly_bound_model_passes_through_setup_and_is_not_presentation_proof(self):
        result = setup.verify_disk_association(production_association())
        self.assertEqual(result['mechanism'], dal.MECHANISM)
        self.assertEqual(result['trust'], 'narrow-parallels-host-inventory')
        self.assertEqual(result['image_path'], IMAGE_PATH)
        self.assertIn('not independent presentation proof', result['not_established'])
        self.assertIn('not evidence a fresh host inventory ran', result['not_established'])
        self.assertEqual({status for status, _ in table(production_association(),
                                                        setup.SUPPORTED_DISK_MECHANISMS, dal.PINS).values()},
                         {'pass'})

    def test_optical_multiplicity_must_match_the_config_one_cdrom(self):
        self.assertEqual(table(production_association(),
                               setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)['host_read'][0], 'pass')
        setup.verify_disk_association(production_association())
        default_optical = inventory_devices()[0]
        cases = (
            ('no optical', [d for d in inventory_devices() if d['type'] != inventory.PDE_OPTICAL_DISK]),
            ('two optical', inventory_devices(extra=(dict(default_optical, index=1, sys_name='cdrom2.iso'),))),
            ('two identical optical', inventory_devices(extra=(dict(default_optical),))),
        )
        for label, devices in cases:
            with self.subTest(label):
                read = host_read(output=inventory_text(devices=devices).encode())
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')
                with mock.patch.object(dal, 'PINS', TEST_PINS), self.assertRaises(Refusal):
                    setup.verify_disk_association(valid_association(host_read=read), TEST_SUPPORTED)
                prod = host_read(dal.PINS[dal.MECHANISM],
                                 output=inventory_text(dal.PINS[dal.MECHANISM], devices=devices).encode())
                with self.assertRaises(Refusal):
                    setup.verify_disk_association(production_association(host_read=prod))

    def test_extra_matching_camera_or_unknown_storage_refuses_inventory(self):
        extra_camera = {'type': inventory.PDE_USB_DEVICE, 'index': 1, 'iface': -1,
                        'sys_name': CAMERA_SYS, 'image_path': '-', 'enabled': 1, 'connected': 1,
                        'remote': 0, 'passthrough': 0, 'disk_type': -1, 'emulated_type': 3}
        scsi = dict(extra_camera, type=inventory.PDE_GENERIC_SCSI_DEVICE, sys_name='scsi0')
        usb_disk = dict(extra_camera, sys_name='SanDisk|0781|5581|high|--|1234')
        extra_hdd = dict(inventory_devices()[1], index=1, sys_name='harddisk2.hdd',
                         image_path=setup.PVM_HOME + '/harddisk2.hdd')
        for label, extra in (('extra camera', extra_camera), ('generic scsi', scsi),
                             ('usb disk', usb_disk), ('extra hdd', extra_hdd)):
            with self.subTest(label):
                read = host_read(output=inventory_text(devices=inventory_devices(extra=(extra,))).encode())
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')
                with mock.patch.object(dal, 'PINS', TEST_PINS), self.assertRaises(Refusal):
                    setup.verify_disk_association(valid_association(host_read=read), TEST_SUPPORTED)
                prod = host_read(dal.PINS[dal.MECHANISM],
                                 output=inventory_text(dal.PINS[dal.MECHANISM],
                                                       devices=inventory_devices(extra=(extra,))).encode())
                with self.assertRaises(Refusal):
                    setup.verify_disk_association(production_association(host_read=prod))

    def test_passthrough_or_real_device_refuses_inventory(self):
        for over in ({'passthrough': 1}, {'emulated_type': inventory.PDT_USE_REAL_DEVICE},
                     {'emulated_type': inventory.PDT_USE_DIRECT_ASSIGN}, {'remote': 1}):
            with self.subTest(over=over):
                read = host_read(output=inventory_text(devices=inventory_devices(**over)).encode())
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')

    def test_wrong_vm_home_boot_version_tool_or_library_refuses(self):
        pin = TEST_PINS[TEST_MECHANISM]
        self.assertEqual(table(valid_association(host_read=host_read(vm='00000000-0000-0000-0000-000000000000')))
                         ['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=inventory_text(home='/Volumes/Stash/parallels/other.pvm').encode())))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(boot_ids=[BOOT, 'other'])))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(tool_version='26.0.0')))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(tool_sha256='c' * 64)))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=inventory_text(library_sha256='c' * 64).encode())))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(argv=pin['argv'] + ['--fixture'])))
                         ['host_read'][0], 'refuse')

    def test_stale_missing_or_changed_state_refuses(self):
        self.assertEqual(table(valid_association(host_read=host_read(
            output=inventory_text(state_before=inventory.VMS_RUNNING,
                                  state_after=0x30000001).encode())))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=inventory_text(state_before=0x30000001, state_after=0x30000001).encode())))
                         ['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(status='ok', exit=1)))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=None))['host_read'][0], 'refuse')

    def test_swapped_descriptor_chain_or_image_refuses(self):
        drifted = dict(valid_association()['host'], chain=list(reversed(CHAIN)))
        self.assertEqual(table(valid_association(host=drifted))['descriptor'][0], 'refuse')
        other_path = host_read(output=inventory_text(devices=inventory_devices(
            sys_name='harddisk2.hdd', image_path=setup.PVM_HOME + '/harddisk2.hdd')).encode())
        self.assertEqual(table(valid_association(host_read=other_path))['host_read'][0], 'refuse')

    def test_extra_guest_disk_or_mapper_refuses(self):
        extra = guest_blocks() + [{'name': 'sdb', 'type': 'disk', 'path': '/dev/sdb', 'size': 1, 'pkname': None}]
        mapper = guest_blocks() + [{'name': 'sda2_crypt', 'type': 'crypt', 'path': '/dev/mapper/sda2_crypt',
                                    'size': 1, 'pkname': 'sda2'}]
        self.assertEqual(table(valid_association(guest=guest(blocks=extra)))['guest_blocks'][0], 'refuse')
        self.assertEqual(table(valid_association(guest=guest(blocks=mapper)))['guest_blocks'][0], 'refuse')

    def test_malformed_partial_or_nested_inventory_refuses(self):
        text = inventory_text()
        self.assertEqual(table(valid_association(host_read=host_read(output=b'\xff\n')))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(output=b'device_count=1\n')))['host_read'][0],
                         'refuse')
        nested = text.replace('dev_1_sys_name=harddisk1.hdd', 'dev_1_sys_name=harddisk1.hdd\ndev_1_sys_name=x')
        self.assertEqual(table(valid_association(host_read=host_read(output=nested.encode())))['host_read'][0],
                         'refuse')
        partial = '\n'.join(text.splitlines()[:-3]) + '\n'
        self.assertEqual(table(valid_association(host_read=host_read(output=partial.encode())))['host_read'][0],
                         'refuse')

    def test_absent_capability_refuses_init_without_guessing_api_ver(self):
        with self.assertRaises(inventory.Refused) as caught:
            inventory.enumerate_inventory(verify_library=lambda: {'path': 'x'}, api_ver=None)
        self.assertEqual(caught.exception.phase, 'parallels-api-ver-unsourced')
        with self.assertRaises(inventory.Refused) as caught:
            inventory.enumerate_inventory(verify_library=lambda: (_ for _ in ()).throw(inventory.Refused('library-hash-mismatch')))
        self.assertEqual(caught.exception.phase, 'library-hash-mismatch')

    def test_helper_main_refuses_extra_args_and_does_not_accept_a_fixture_path(self):
        self.assertEqual(inventory.main([]), 2)
        self.assertEqual(inventory.main(['inventory', '--fixture', '/tmp/x']), 2)
        self.assertEqual(inventory.main(['inventory', str(inventory.HELPER)]), 2)

    def test_uninjected_dispatch_uses_fixed_argv_with_private_fixture_transport(self):
        seen = []

        def transport(argv, stdin_path, timeout, output_cap):
            seen.append((argv, stdin_path, timeout, output_cap))
            return {'disposition': 'exited', 'waited_exit': 0, 'stdout': inventory_text(dal.PINS[dal.MECHANISM]).encode(),
                    'stderr': b''}

        boots = boot_reader([BOOT, BOOT])
        record = dal.read_inventory(transport=transport, boot_id=BOOT, read_boot_id=boots)
        self.assertEqual(seen, [(inventory.inventory_argv(), None, inventory.INVENTORY_TIMEOUT, inventory.MAX_BYTES)])
        self.assertEqual(record['argv'], inventory.inventory_argv())
        self.assertEqual(record['tool_path'], str(inventory.HELPER))
        self.assertEqual(record['tool_sha256'], dal.HELPER_SHA256)
        self.assertEqual(record['api'], 'PrlVmCfg_GetAllDevices')
        self.assertTrue(record['readonly'])
        # The VM UUID and the boot bracket are readings of this dispatch, not caller constants.
        self.assertEqual(record['vm'], VM)
        self.assertEqual(record['boot_ids'], [BOOT, BOOT])
        self.assertEqual(record['library_sha256'], dal.PINS[dal.MECHANISM]['library_sha256'])
        facts = production_association(host_read=record)
        result = setup.verify_disk_association(facts)
        self.assertEqual(result['mechanism'], dal.MECHANISM)
        self.assertIn('not evidence a fresh host inventory ran', result['not_established'])

    def test_helper_pin_is_an_independent_literal_not_a_self_hash(self):
        """A modified helper cannot re-pin itself: the digest is a committed literal in the consumer."""
        fixture_root = Path(os.environ.get('TMPDIR') or tempfile.gettempdir())
        with tempfile.TemporaryDirectory(dir=fixture_root) as scratch:
            copy = Path(scratch) / 'linux_disk_inventory.py'
            copy.write_bytes(inventory.HELPER.read_bytes() + b'\n# modified helper bytes\n')
            self.assertNotEqual(inventory.helper_digest(copy), dal.HELPER_SHA256)
            pin = dict(dal.PINS[dal.MECHANISM], tool_path=str(copy),
                       argv=inventory.inventory_argv(copy))
            with self.assertRaises(inventory.Refused) as caught:
                dal.read_inventory(transport=inventory_transport(), pin=pin, boot_id=BOOT,
                                   read_boot_id=boot_reader([BOOT, BOOT]))
            self.assertEqual(caught.exception.phase, 'helper-bytes-differ')

    def test_complete_fixture_flow_through_production_dispatch_and_setup(self):
        """One complete association record from the production dispatch and boot bracket, fed through
        setup.verify_disk_association with production PINS. Fixture library/transport bytes only."""
        block, lsblk = guest_block_observations()
        facts = dal.association_facts(
            boot_id=BOOT, read_boot_id=boot_reader([BOOT, BOOT]),
            config=config_xml(), descriptor=identity_tests.doc(), extents=descriptor_extents(),
            block={'blocks': block}, lsblk={'nodes': lsblk}, root=guest_root_facts(),
            transport=inventory_transport())
        self.assertEqual(facts['mechanism'], dal.MECHANISM)
        self.assertEqual(facts['host_read']['vm'], VM)
        self.assertEqual(facts['host_read']['boot_ids'], [BOOT, BOOT])
        self.assertEqual(facts['guest']['boot_id'], BOOT)
        self.assertEqual([row['name'] for row in facts['guest']['blocks']],
                         ['loop0', 'sda', 'sda1', 'sda2', 'sr0'])
        result = setup.verify_disk_association(facts)
        self.assertEqual(result['mechanism'], dal.MECHANISM)
        self.assertEqual(result['vm'], VM)
        self.assertEqual(result['system_name'], 'harddisk1.hdd')
        self.assertEqual(result['boot_id'], BOOT)
        self.assertEqual(result['serial'], SERIAL)
        self.assertEqual(result['trust'], 'narrow-parallels-host-inventory')
        self.assertIn('not independent presentation proof', result['not_established'])

    def test_boot_bracket_must_be_the_authenticated_one(self):
        block, lsblk = guest_block_observations()

        def attempt(reads, boot=BOOT):
            return dal.association_facts(
                boot_id=boot, read_boot_id=boot_reader(reads),
                config=config_xml(), descriptor=identity_tests.doc(), extents=descriptor_extents(),
                block={'blocks': block}, lsblk={'nodes': lsblk}, root=guest_root_facts(),
                transport=inventory_transport())

        cases = (
            ('changed before', [OTHER_BOOT, BOOT]),
            ('changed after', [BOOT, OTHER_BOOT]),
            ('malformed before', ['not-a-boot-id', BOOT]),
            ('no reader', None),
        )
        for label, reads in cases:
            with self.subTest(label):
                if reads is None:
                    with self.assertRaises(inventory.Refused) as caught:
                        dal.association_facts(
                            boot_id=BOOT, read_boot_id=None, config=config_xml(),
                            descriptor=identity_tests.doc(), extents=descriptor_extents(),
                            block={'blocks': block}, lsblk={'nodes': lsblk}, root=guest_root_facts(),
                            transport=inventory_transport())
                    self.assertEqual(caught.exception.phase, 'boot-bracket-missing')
                    continue
                with self.assertRaises(inventory.Refused) as caught:
                    attempt(reads)
                self.assertIn(caught.exception.phase, ('boot-bracket-moved', 'boot-bracket-malformed'))
        with self.assertRaises(inventory.Refused) as caught:
            attempt([BOOT, BOOT], boot='not-a-boot-id')
        self.assertEqual(caught.exception.phase, 'boot-bracket-malformed')

    def test_a_non_observation_never_becomes_a_host_record(self):
        """Partial output, nonzero exit, a timeout, an over-cap read and a grammar break all refuse."""
        pin = dal.PINS[dal.MECHANISM]
        text = inventory_text(pin)
        cases = (
            ('timeout', dict(disposition='timeout', waited_exit=0, stdout=b''), 'inventory-not-exited'),
            ('output cap', dict(disposition='output-cap', waited_exit=0, stdout=text.encode()[:40]),
             'inventory-not-exited'),
            ('resource refusal', dict(disposition='resource-refusal', waited_exit=0, stdout=b''),
             'inventory-not-exited'),
            ('nonzero exit', dict(waited_exit=1), 'inventory-exit-nonzero'),
            ('stub refusal', dict(waited_exit=3, stdout=b''), 'inventory-exit-nonzero'),
            ('missing stdout', dict(stdout=None), 'inventory-output-missing'),
            ('over the cap', dict(stdout=b'x' * (inventory.MAX_BYTES + 1)), 'inventory-truncated'),
            ('not ascii', dict(stdout=b'vm=\xff\n'), 'inventory-output-encoding'),
            ('partial output', dict(stdout=('\n'.join(text.splitlines()[:-3]) + '\n').encode()),
             'inventory-output-grammar'),
            ('repeated field', dict(stdout=text.encode() + b'vm=' + VM.encode() + b'\n'),
             'inventory-output-grammar'),
            ('another vm', dict(stdout=inventory_text(pin, vm=OTHER_VM).encode()), 'inventory-vm-differs'),
        )
        for label, over, phase in cases:
            with self.subTest(label):
                self.assertIn(phase, inventory.READ_PHASES)
                with self.assertRaises(inventory.Refused) as caught:
                    dal.read_inventory(transport=inventory_transport(**over), boot_id=BOOT,
                                       read_boot_id=boot_reader([BOOT, BOOT]))
                self.assertEqual(caught.exception.phase, phase)
        with self.assertRaises(inventory.Refused) as caught:
            dal.read_inventory(transport=inventory_transport(), pin={}, boot_id=BOOT,
                               read_boot_id=boot_reader([BOOT, BOOT]))
        self.assertEqual(caught.exception.phase, 'pin-missing')

    def test_the_init_stub_cannot_count_as_an_executable_reader(self):
        """The production entry refuses before Init and prints nothing a reader accepts. Opening the
        gate by patching the unsourced constants does not turn those bytes into an inventory read."""
        self.assertFalse(dal.reader_executable())
        self.assertIn('PARALLELS_API_VER', dal.reader_limitation())
        self.assertIn(inventory.SOURCE_GAP['next_read'], dal.reader_limitation())
        completed = subprocess.run(inventory.inventory_argv(), stdin=subprocess.DEVNULL,
                                   capture_output=True, timeout=120)
        self.assertNotEqual(completed.returncode, 0)
        self.assertNotIn(b'vm=', completed.stdout)
        stub = (lambda argv, stdin_path, timeout, cap: {
            'disposition': 'exited', 'waited_exit': completed.returncode,
            'stdout': completed.stdout, 'stderr': completed.stderr})
        with mock.patch.object(inventory, 'PARALLELS_API_VER', 17), \
                mock.patch.object(inventory, 'PRL_HANDLE_SIZE', 8):
            self.assertTrue(dal.reader_executable())
            with self.assertRaises(inventory.Refused) as caught:
                dal.read_inventory(transport=stub, boot_id=BOOT, read_boot_id=boot_reader([BOOT, BOOT]))
            self.assertEqual(caught.exception.phase, 'inventory-exit-nonzero')
            # The refused output is not a host record either: the association refuses it without one.
            refused = dict(production_association(), host_read={'output': completed.stdout})
            self.assertEqual({status for _, status, _ in
                              dal.premise_table(refused, setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)
                              if status != 'pass'}, {'refuse'})
            with self.assertRaises(Refusal):
                setup.verify_disk_association(refused)

    def test_guest_facts_refuse_a_hidden_or_disagreeing_device(self):
        block, lsblk = guest_block_observations()
        cases = (
            ('device in one inventory only',
             {'blocks': dict(block, sdb=sysfs_block('sdb', '8:16', 8, 0, None, 0, SDA_SYSPATH + '/block/sdb'))},
             {'nodes': lsblk}),
            ('size disagrees', {'blocks': dict(block, sda=dict(block['sda'], sectors=1))}, {'nodes': lsblk}),
            ('major disagrees', {'blocks': dict(block, sda=dict(block['sda'], maj_min='9:0'))}, {'nodes': lsblk}),
            ('undocumented type', {'blocks': block}, {'nodes': dict(lsblk, sda=dict(lsblk['sda'], type=''))}),
        )
        for label, observed_block, observed_lsblk in cases:
            with self.subTest(label):
                with self.assertRaises(Refusal):
                    dal.guest_facts(BOOT, observed_block, observed_lsblk, guest_root_facts())
        with self.assertRaises(Refusal):
            dal.guest_facts(BOOT, {'blocks': {}}, {'nodes': lsblk}, guest_root_facts())
        extra = sysfs_block('crypt0', '253:0', 8, 0, None, 0, '/sys/devices/virtual/block/crypt0')
        node = {'name': 'crypt0', 'maj_min': '253:0', 'parent': None, 'type': 'crypt', 'bytes': 4096,
                'fstype': None, 'uuid': None, 'mountpoint': None}
        block, lsblk = guest_block_observations(extra_sysfs=(('crypt0', extra),),
                                                extra_lsblk=(('crypt0', node),))
        facts = dal.guest_facts(BOOT, {'blocks': block}, {'nodes': lsblk}, guest_root_facts())
        # The mapper forwards the unknown class; guest_blocks is what refuses it.
        self.assertIn('crypt', {row['type'] for row in facts['blocks']})
        with self.assertRaises(Refusal):
            dal.verify(dict(valid_association(), guest=facts))

    def test_unobserved_extent_or_missing_originals_refuse_the_handoff(self):
        block, lsblk = guest_block_observations()
        with self.assertRaises(Refusal):
            dal.host_facts(identity_tests.doc(), {})
        with self.assertRaises(Refusal):
            dal.host_facts(identity_tests.doc(), {'a.hds': {}})
        self.assertEqual(sorted(dal.host_facts(identity_tests.doc(), descriptor_extents())['extents'].values()),
                         ['a.hds', 'b.hds', 'r.hds'])
        for config, descriptor in ((b'', identity_tests.doc()), (config_xml(), b'')):
            with self.assertRaises(Refusal):
                dal.association_facts(
                    boot_id=BOOT, read_boot_id=boot_reader([BOOT, BOOT]), config=config,
                    descriptor=descriptor, extents=descriptor_extents(),
                    block={'blocks': block}, lsblk={'nodes': lsblk}, root=guest_root_facts(),
                    transport=inventory_transport())

    def test_retained_747_replay_is_offline_and_supplies_no_fresh_evidence(self):
        """747/748 retained shape establishes config/descriptor/guest only. No fresh inventory."""
        rows = table(dict(production_association(), host_read=None),
                     setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)
        self.assertEqual(rows['config'][0], 'pass')
        self.assertEqual(rows['descriptor'][0], 'pass')
        self.assertEqual(rows['guest_blocks'][0], 'pass')
        self.assertEqual(rows['host_read'][0], 'refuse')
        with self.assertRaises(Refusal):
            setup.verify_disk_association(dict(production_association(), host_read=None))


if __name__ == '__main__':
    unittest.main()
