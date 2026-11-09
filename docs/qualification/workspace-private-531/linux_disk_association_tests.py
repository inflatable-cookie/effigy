"""Controls for the controller-to-guest association verifier (linux_disk_association).

Portable controls use synthetic candidates shaped like the retained facts: a registered config excerpt,
the documented descriptor shape from linux_disk_identity_tests, one host read and a guest inventory.
TEST_PINS is a test-only mechanism; PINS in production stays empty, and a control asserts that. No VM,
host service, image, disk or guest is touched. A pass here shows the premise logic only, not a host/guest
association: no pinned host read exists.
"""
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_disk_association as dal
import linux_disk_identity_tests as identity_tests
import linux_setup as setup
from linux_supervisor import Refusal

VM = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = setup.DISK_UID
OTHER_UID = '{11111111-2222-3333-4444-555555555555}'
DEVICE_UUID = '{9447f202-8d43-4ec2-80c9-ce527b34bd9d}'
BOOT = '6de5909b-25da-444a-9610-731ee1247b11'
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
TEST_PINS = {TEST_MECHANISM: {'tool_path': '/test/libprl_sdk.dylib', 'sha256': 'a' * 64, 'version': '27.1.0',
                              'desktop_build': '27.1.0-58756', 'api': 'PrlVmDevHd_GetSerialNumber',
                              'max_bytes': 256, 'fields': ('serial', 'image_uid')}}
TEST_SUPPORTED = frozenset({TEST_MECHANISM})


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


def host_read(**over):
    read = {'tool_path': TEST_PINS[TEST_MECHANISM]['tool_path'], 'tool_sha256': 'a' * 64,
            'tool_version': '27.1.0', 'desktop_build': '27.1.0-58756',
            'api': 'PrlVmDevHd_GetSerialNumber', 'readonly': True, 'status': 'ok', 'exit': 0,
            'truncated': False, 'vm': VM, 'boot_ids': [BOOT, BOOT],
            'output': b'serial=' + SERIAL.encode() + b'\nimage_uid=' + DISK_UID.encode() + b'\n'}
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

    def test_production_gate_refuses_the_complete_candidate(self):
        self.assertEqual(dal.PINS, {})
        self.assertEqual(setup.SUPPORTED_DISK_MECHANISMS, frozenset())
        with self.assertRaises(Refusal):
            setup.verify_disk_association(valid_association())
        with self.assertRaises(Refusal):
            dal.verify(valid_association(), frozenset(), dal.PINS)
        # A mechanism named in the gate but not pinned still refuses, and so does a pin without the gate.
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

    def test_wrong_descriptor_disk_uid_refuses_descriptor_and_read(self):
        self.assertRefusedAt('descriptor', valid_association(descriptor=identity_tests.doc(uid_xml=f'<UID>{OTHER_UID}</UID>')))
        read = host_read(output=b'serial=' + SERIAL.encode() + b'\nimage_uid=' + OTHER_UID.encode() + b'\n')
        self.assertRefusedAt('host_read', valid_association(host_read=read))

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

    def test_contradictory_or_missing_serial_refuses(self):
        self.assertRefusedAt('serial', valid_association(host_read=host_read(
            output=b'serial=S2\nimage_uid=' + DISK_UID.encode() + b'\n')))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(parent_serial='')))
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(sysfs_serial='S9')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=b'image_uid=' + DISK_UID.encode() + b'\n')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=b'serial=\nimage_uid=' + DISK_UID.encode() + b'\n')))

    def test_absent_sysfs_serial_is_not_a_disagreement(self):
        self.assertEqual(table(valid_association())['guest_blocks'][0], 'pass')

    def test_unsupported_tool_api_or_version_refuses_the_read(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_sha256='b' * 64)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(api='PrlVmDev_GetFriendlyName')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_version='26.0.0')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(desktop_build='27.2.0')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(readonly=False)))

    def test_truncated_failed_or_unknown_read_refuses(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(truncated=True)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(status='unknown')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(exit=1)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(output=b'x' * 300)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(output=b'serial=S1\nimage_uid=' + DISK_UID.encode())))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=b'serial=S1\nserial=S1\nimage_uid=' + DISK_UID.encode() + b'\n')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(
            output=b'serial=S1\nimage_uid=' + DISK_UID.encode() + b'\nextra=1\n')))

    def test_labels_and_caller_claims_are_not_evidence(self):
        association = valid_association(serial=SERIAL, verified=True, premises='all pass', read_sha256='0' * 64)
        del association['host_read']
        self.assertRefusedAt('host_read', association)
        self.assertRefusedAt('serial', association)

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
        self.assertEqual(rows['serial'][0], 'refuse')
        self.assertNotEqual(DEVICE_UUID.lower(), DISK_UID.lower())
        with self.assertRaises(Refusal):
            dal.verify(config_only, frozenset(), {})


if __name__ == '__main__':
    unittest.main()
