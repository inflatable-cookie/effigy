"""Controls for the controller-to-guest association verifier (linux_disk_association).

Portable controls use synthetic candidates shaped like the retained facts: a registered config excerpt,
the documented descriptor shape from linux_disk_identity_tests, one host inventory in the retained
Parallels CLI grammar and a guest inventory. The retained-grammar fixture below is an offline byte
copy of the genuine 747 `prlctl list --info --json` stdout (its recorded sha256 is asserted), so the
parser is exercised against the genuine grammar; a fixture cannot supply positive production
provenance. TEST_PINS is a test-only mechanism used to isolate premise logic. Production PINS names
parallels-host-inventory-trust-v1. No VM, host service, image, disk or guest is touched, and no
prlctl process runs. A pass of the verifier on fixture bytes is source admission of the logic, not a
fresh native inventory.
"""
import base64
import hashlib
import io
import json
import os
import plistlib
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
from preparation_stage import Refusal

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
TEST_TOOL = '/test/prlctl'
TEST_ARGV = [TEST_TOOL, 'list', '--info', VM, '--json']
TEST_PINS = {TEST_MECHANISM: {
    'tool_path': TEST_HELPER, 'sha256': 'a' * 64, 'version': inventory.DESKTOP_VERSION,
    'desktop_build': inventory.DESKTOP_VERSION + '-' + inventory.DESKTOP_BUILD,
    'api': inventory.ACCESSOR,
    'argv': TEST_ARGV, 'max_bytes': inventory.MAX_BYTES,
    'home': setup.PVM_HOME, 'vm': VM, 'trust': 'test-only',
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


# The genuine retained 747 occurrence's `prlctl list --info <vm> --json` stdout (step-004, 4422 bytes),
# as an offline base64 copy. Its recorded sha256 pins the bytes to the retained original. It is an
# offline derivative for grammar controls only and supplies no fresh evidence.
RETAINED_CLI_STDOUT_SHA256 = '08e8245f626b2a096c8b60167cf0ac04b936222a693bc6fd422385aab7ff175a'
RETAINED_CLI_STDOUT_B64 = (
    'WwoJewoJCSJJRCI6ICI0MzNjOWU1Yi1jN2I5LTQ2OWYtOTYzMy0yNGI0YWRkY2QzYjUiLAoJCSJOYW1lIjogIlVidW50dSAy'
    'Ni4wNCBBUk02NCIsCgkJIkRlc2NyaXB0aW9uIjogIkVmZmlneSBMaW51eCBwcm9vZiBWTSBvbiBWYXVsdCIsCgkJIlR5cGUi'
    'OiAiVk0iLAoJCSJTdGF0ZSI6ICJydW5uaW5nIiwKCQkiT1MiOiAidWJ1bnR1IiwKCQkiVGVtcGxhdGUiOiAibm8iLAoJCSJV'
    'cHRpbWUiOiAiNjExIiwKCQkiSG9tZSBwYXRoIjogIlwvVm9sdW1lc1wvU3Rhc2hcL3BhcmFsbGVsc1wvVWJ1bnR1IDI2LjA0'
    'IEFSTTY0LnB2bVwvY29uZmlnLnB2cyIsCgkJIkhvbWUiOiAiXC9Wb2x1bWVzXC9TdGFzaFwvcGFyYWxsZWxzXC9VYnVudHUg'
    'MjYuMDQgQVJNNjQucHZtXC8iLAoJCSJSZXN0b3JlIEltYWdlIjogIiIsCgkJIkd1ZXN0VG9vbHMiOiB7CgkJCSJzdGF0ZSI6'
    'ICJpbnN0YWxsZWQiLAoJCQkidmVyc2lvbiI6ICIyNy4xLjAtNTg3NTYiCgkJfSwKCQkiTW91c2UgYW5kIEtleWJvYXJkIjog'
    'ewoJCQkiU21hcnQgbW91c2Ugb3B0aW1pemVkIGZvciBnYW1lcyI6ICJhdXRvIiwKCQkJIlN0aWNreSBtb3VzZSI6ICJvZmYi'
    'LAoJCQkiU21vb3RoIHNjcm9sbGluZyI6ICJvZmYiLAoJCQkiS2V5Ym9hcmQgb3B0aW1pemF0aW9uIG1vZGUiOiAiYXV0byIK'
    'CQl9LAoJCSJVU0IgYW5kIEJsdWV0b290aCI6IHsKCQkJIkF1dG9tYXRpYyBzaGFyaW5nIGNhbWVyYXMiOiAib24iLAoJCQki'
    'QXV0b21hdGljIHNoYXJpbmcgYmx1ZXRvb3RoIjogIm9mZiIsCgkJCSJBdXRvbWF0aWMgc2hhcmluZyBzbWFydCBjYXJkcyI6'
    'ICJvZmYiLAoJCQkiQXV0b21hdGljIHNoYXJpbmcgZ2FtZXBhZHMiOiAib24iLAoJCQkiU3VwcG9ydCBVU0IgMy4wIjogIm9u'
    'IgoJCX0sCgkJIlN0YXJ0dXAgYW5kIFNodXRkb3duIjogewoJCQkiQXV0b3N0YXJ0IjogIm9mZiIsCgkJCSJBdXRvc3RhcnQg'
    'ZGVsYXkiOiAwLAoJCQkiQXV0b3N0b3AiOiAic3VzcGVuZCIsCgkJCSJTdGFydHVwIHZpZXciOiAic2FtZSIsCgkJCSJPbiBz'
    'aHV0ZG93biI6ICJjbG9zZSIsCgkJCSJPbiB3aW5kb3cgY2xvc2UiOiAic3VzcGVuZCIsCgkJCSJQYXVzZSBpZGxlIjogIm9m'
    'ZiIsCgkJCSJVbmRvIGRpc2tzIjogIm9mZiIKCQl9LAoJCSJPcHRpbWl6YXRpb24iOiB7CgkJCSJGYXN0ZXIgdmlydHVhbCBt'
    'YWNoaW5lIjogIm9uIiwKCQkJIkh5cGVydmlzb3IgdHlwZSI6ICJhcHBsZSIsCgkJCSJBZGFwdGl2ZSBoeXBlcnZpc29yIjog'
    'Im9mZiIsCgkJCSJEaXNhYmxlZCBXaW5kb3dzIGxvZ28iOiAib24iLAoJCQkiQXV0byBjb21wcmVzcyB2aXJ0dWFsIGRpc2tz'
    'IjogIm9mZiIsCgkJCSJOZXN0ZWQgdmlydHVhbGl6YXRpb24iOiAib2ZmIiwKCQkJIlBNVSB2aXJ0dWFsaXphdGlvbiI6ICJv'
    'ZmYiLAoJCQkiTG9uZ2VyIGJhdHRlcnkgbGlmZSI6ICJvZmYiLAoJCQkiU2hvdyBiYXR0ZXJ5IHN0YXR1cyI6ICJvbiIKCQl9'
    'LAoJCSJTZWN1cml0eSI6IHsKCQkJIkVuY3J5cHRlZCI6ICJubyIsCgkJCSJUUE0gZW5hYmxlZCI6ICJubyIsCgkJCSJUUE0g'
    'dHlwZSI6ICJvZmYiLAoJCQkiUHJvdGVjdGVkIjogIm5vIiwKCQkJIkFyY2hpdmVkIjogIm5vIiwKCQkJIlBhY2tlZCI6ICJu'
    'byIKCQl9LAoJCSJTbWFydCBHdWFyZCI6IHsKCQkJImVuYWJsZWQiOiBmYWxzZQoJCX0sCgkJIk1vZGFsaXR5IjogewoJCQki'
    'T3BhY2l0eSAocGVyY2VudGFnZSkiOiA1MCwKCQkJIlN0YXkgb24gdG9wIjogIm9uIiwKCQkJIlNob3cgb24gYWxsIHNwYWNl'
    'cyAiOiAib24iLAoJCQkiQ2FwdHVyZSBtb3VzZSBjbGlja3MiOiAib24iCgkJfSwKCQkiRnVsbHNjcmVlbiI6IHsKCQkJIlVz'
    'ZSBhbGwgZGlzcGxheXMiOiAib2ZmIiwKCQkJIkFjdGl2YXRlIHNwYWNlcyBvbiBjbGljayI6ICJvbiIsCgkJCSJPcHRpbWl6'
    'ZSBmb3IgZ2FtZXMiOiAib2ZmIiwKCQkJIkdhbW1hIGNvbnRyb2wiOiAib24iLAoJCQkiU2NhbGUgdmlldyBtb2RlIjogImF1'
    'dG8iCgkJfSwKCQkiQ29oZXJlbmNlIjogewoJCQkiU2hvdyBXaW5kb3dzIHN5c3RyYXkgaW4gTWFjIG1lbnUiOiAib24iLAoJ'
    'CQkiQXV0by1zd2l0Y2ggdG8gZnVsbCBzY3JlZW4iOiAib24iCgkJfSwKCQkiVGltZSBTeW5jaHJvbml6YXRpb24iOiB7CgkJ'
    'CSJlbmFibGVkIjogdHJ1ZSwKCQkJIlNtYXJ0IG1vZGUiOiAib2ZmIiwKCQkJIkludGVydmFsIChpbiBzZWNvbmRzKSI6IDYw'
    'LAoJCQkiVGltZXpvbmUgc3luY2hyb25pemF0aW9uIGRpc2FibGVkIjogIm9mZiIKCQl9LAoJCSJFeHBpcmF0aW9uIjogewoJ'
    'CQkiZW5hYmxlZCI6IGZhbHNlCgkJfSwKCQkiQm9vdCBvcmRlciI6ICJoZGQwIGNkcm9tMCAiLAoJCSJCSU9TIHR5cGUiOiAi'
    'ZWZpLWFybTY0IiwKCQkiRUZJIFNlY3VyZSBib290IjogIm9mZiIsCgkJIkFsbG93IHNlbGVjdCBib290IGRldmljZSI6ICJv'
    'ZmYiLAoJCSJFeHRlcm5hbCBib290IGRldmljZSI6ICIiLAoJCSJTTUJJT1Mgc2V0dGluZ3MiOiB7CgkJCSJCSU9TIFZlcnNp'
    'b24iOiAiIiwKCQkJIlN5c3RlbSBzZXJpYWwgbnVtYmVyIjogIiIsCgkJCSJCb2FyZCBNYW51ZmFjdHVyZXIiOiAiIgoJCX0s'
    'CgkJIkhhcmR3YXJlIjogewoJCQkiY3B1IjogewoJCQkJImNwdXMiOiA2LAoJCQkJImF1dG8iOiAib2ZmIiwKCQkJCSJWVC14'
    'IjogdHJ1ZSwKCQkJCSJob3RwbHVnIjogZmFsc2UsCgkJCQkiYWNjbCI6ICJoaWdoIiwKCQkJCSJtb2RlIjogIjY0IiwKCQkJ'
    'CSJ0eXBlIjogImFybSIKCQkJfSwKCQkJIm1lbW9yeSI6IHsKCQkJCSJzaXplIjogIjE2Mzg0TWIiLAoJCQkJImF1dG8iOiAi'
    'b2ZmIgoJCQl9LAoJCQkidmlkZW8iOiB7CgkJCQkiYWRhcHRlci10eXBlIjogInZpcnRpbyIsCgkJCQkic2l6ZSI6ICIwTWIi'
    'LAoJCQkJIjNkLWFjY2VsZXJhdGlvbiI6ICJoaWdoZXN0IiwKCQkJCSJ2ZXJ0aWNhbC1zeW5jIjogIm9uIiwKCQkJCSJoaWdo'
    'LXJlc29sdXRpb24iOiAib2ZmIiwKCQkJCSJoaWdoLXJlc29sdXRpb24taW4tZ3Vlc3QiOiAib24iLAoJCQkJIm5hdGl2ZS1z'
    'Y2FsaW5nLWluLWd1ZXN0IjogIm9mZiIsCgkJCQkiYXV0b21hdGljLXZpZGVvLW1lbW9yeSI6ICJvbiIKCQkJfSwKCQkJIm1l'
    'bW9yeV9xdW90YSI6IHsKCQkJCSJhdXRvIjogInllcyIKCQkJfSwKCQkJImhkZDAiOiB7CgkJCQkiZW5hYmxlZCI6IHRydWUs'
    'CgkJCQkicG9ydCI6ICJzYXRhOjAiLAoJCQkJImltYWdlIjogIlwvVm9sdW1lc1wvU3Rhc2hcL3BhcmFsbGVsc1wvVWJ1bnR1'
    'IDI2LjA0IEFSTTY0LnB2bVwvaGFyZGRpc2sxLmhkZCIsCgkJCQkidHlwZSI6ICJleHBhbmRlZCIsCgkJCQkic2l6ZSI6ICIx'
    'MzEwNzJNYiIsCgkJCQkib25saW5lLWNvbXBhY3QiOiAib24iCgkJCX0sCgkJCSJjZHJvbTAiOiB7CgkJCQkiZW5hYmxlZCI6'
    'IHRydWUsCgkJCQkicG9ydCI6ICJzYXRhOjEiLAoJCQkJImltYWdlIjogIiIsCgkJCQkic3RhdGUiOiAiZGlzY29ubmVjdGVk'
    'IgoJCQl9LAoJCQkidXNiIjogewoJCQkJImVuYWJsZWQiOiB0cnVlCgkJCX0sCgkJCSJuZXQwIjogewoJCQkJImVuYWJsZWQi'
    'OiB0cnVlLAoJCQkJInR5cGUiOiAic2hhcmVkIiwKCQkJCSJtYWMiOiAiMDAxQzQyQzJGOUYyIiwKCQkJCSJjYXJkIjogInZp'
    'cnRpbyIKCQkJfSwKCQkJInNvdW5kMCI6IHsKCQkJCSJlbmFibGVkIjogdHJ1ZSwKCQkJCSJvdXRwdXQiOiAiRGVmYXVsdCIs'
    'CgkJCQkibWl4ZXIiOiAiRGVmYXVsdCIKCQkJfQoJCX0sCgkJIkhvc3QgU2hhcmVkIEZvbGRlcnMiOiB7CgkJCSJlbmFibGVk'
    'IjogZmFsc2UKCQl9LAoJCSJIb3N0IGRlZmluZWQgc2hhcmluZyI6ICJPZmYiLAoJCSJTaGFyZWQgUHJvZmlsZSI6IHsKCQkJ'
    'ImVuYWJsZWQiOiBmYWxzZQoJCX0sCgkJIlNoYXJlZCBBcHBsaWNhdGlvbnMiOiB7CgkJCSJlbmFibGVkIjogdHJ1ZSwKCQkJ'
    'Ikhvc3QtdG8tZ3Vlc3QgYXBwcyBzaGFyaW5nIjogIm9mZiIsCgkJCSJHdWVzdC10by1ob3N0IGFwcHMgc2hhcmluZyI6ICJv'
    'ZmYiLAoJCQkiU2hvdyBndWVzdCBhcHBzIGZvbGRlciBpbiBEb2NrIjogIm9mZiIsCgkJCSJCb3VuY2UgZG9jayBpY29uIHdo'
    'ZW4gYXBwIGZsYXNoZXMiOiAib24iCgkJfSwKCQkiU21hcnRNb3VudCI6IHsKCQkJImVuYWJsZWQiOiBmYWxzZQoJCX0sCgkJ'
    'Ik5ldHdvcmsiOiB7CgkJCSJDb25kaXRpb25lZCI6ICJvZmYiLAoJCQkiaXBBZGRyZXNzZXMiOiBbXQoJCX0sCgkJIk1pc2Nl'
    'bGxhbmVvdXMgU2hhcmluZyI6IHsKCQkJIlNoYXJlZCBjbGlwYm9hcmQgbW9kZSI6ICJvbiIsCgkJCSJTaGFyZWQgY2xvdWQi'
    'OiAib2ZmIgoJCX0sCgkJIkFkdmFuY2VkIjogewoJCQkiVk0gaG9zdG5hbWUgc3luY2hyb25pemF0aW9uIjogIm9mZiIsCgkJ'
    'CSJQdWJsaWMgU1NIIGtleXMgc3luY2hyb25pemF0aW9uIjogIm9mZiIsCgkJCSJTaG93IGRldmVsb3BlciB0b29scyI6ICJv'
    'ZmYiLAoJCQkiU3dpcGUgZnJvbSBlZGdlcyI6ICJvZmYiLAoJCQkiU2hhcmUgaG9zdCBsb2NhdGlvbiI6ICJvbiIsCgkJCSJS'
    'b3NldHRhIExpbnV4IjogIm9mZiIKCQl9Cgl9Cl0K'
)
RETAINED_CLI_STDOUT = base64.b64decode(RETAINED_CLI_STDOUT_B64)


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


# ---- Host inventory fixtures: synthetic shapes of the retained 747 CLI grammar (test-only). ----

def cli_hardware(**over):
    """The reviewed closed Hardware set, values as the retained 747 step-004 carries them."""
    hardware = {
        'cpu': {'cpus': 6, 'auto': 'off', 'VT-x': True, 'hotplug': False, 'accl': 'high',
                'mode': '64', 'type': 'arm'},
        'memory': {'size': '16384Mb', 'auto': 'off'},
        'video': {'adapter-type': 'virtio', 'size': '0Mb', '3d-acceleration': 'highest',
                  'vertical-sync': 'on', 'high-resolution': 'off', 'high-resolution-in-guest': 'on',
                  'native-scaling-in-guest': 'off', 'automatic-video-memory': 'on'},
        'memory_quota': {'auto': 'yes'},
        'hdd0': dict(inventory.HDD0_REVIEWED),
        'cdrom0': dict(inventory.CDROM0_REVIEWED),
        'usb': dict(inventory.USB_REVIEWED),
        'net0': {'enabled': True, 'type': 'shared', 'mac': '001C42C2F9F2', 'card': 'virtio'},
        'sound0': {'enabled': True, 'output': 'Default', 'mixer': 'Default'},
    }
    hardware.update(over)
    return hardware


def cli_vm(pin=None, hardware=None, **over):
    pin = dal.PINS[dal.MECHANISM] if pin is None else pin
    vm = {'ID': VM, 'Name': 'Ubuntu 26.04 ARM64', 'Description': 'Effigy Linux proof VM on Vault',
          'Type': 'VM', 'State': inventory.STATE_RUNNING, 'OS': 'ubuntu', 'Template': 'no',
          'Home': pin['home'] + '/', 'Home path': pin['home'] + '/config.pvs',
          'Hardware': cli_hardware() if hardware is None else hardware}
    vm.update(over)
    return vm


def cli_info_text(pin=None, vm_count=1, **over):
    """The one-VM JSON array the fixed read produces, with trailing newline as prlctl prints it."""
    payload = [cli_vm(pin, **over) for _ in range(vm_count)]
    return json.dumps(payload, indent=4) + '\n'


def host_read(pin=None, **over):
    pin = TEST_PINS[TEST_MECHANISM] if pin is None else pin
    read = {'tool_path': pin['tool_path'], 'tool_sha256': pin['sha256'],
            'tool_version': pin['version'], 'desktop_build': pin['desktop_build'],
            'api': pin['api'], 'argv': list(pin['argv']), 'status': 'ok',
            'exit': 0, 'truncated': False, 'vm': VM, 'boot_ids': [BOOT, BOOT],
            'output': cli_info_text(pin).encode(), 'stderr': b''}
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
    """A private fixture transport: the recorded copy of the pinned CLI grammar, no process."""
    pin = dal.PINS[dal.MECHANISM]
    stdout = cli_info_text(pin) if output is None else output
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


def host_read_with(hardware=None, vm_count=1, pin=None, **vm_over):
    """A host_read whose CLI payload carries the given Hardware section overrides."""
    pin = dal.PINS[dal.MECHANISM] if pin is None else pin
    return host_read(pin, output=cli_info_text(pin, vm_count=vm_count, hardware=hardware, **vm_over).encode())


class DiskAssociationControls(unittest.TestCase):
    def assertRefusedAt(self, premise, association, supported=TEST_SUPPORTED, pins=TEST_PINS):
        self.assertEqual(table(association, supported, pins)[premise][0], 'refuse', table(association, supported, pins))
        with self.assertRaises(Refusal):
            dal.verify(association, supported, pins)
        # The production integration, with the same pins: the gate must refuse too.
        with mock.patch.object(dal, 'PINS', pins), self.assertRaises(setup.Refusal):
            setup.verify_disk_association(association, supported)

    def test_production_gate_refuses_the_test_mechanism(self):
        self.assertEqual(set(dal.PINS), {dal.MECHANISM})
        self.assertEqual(setup.SUPPORTED_DISK_MECHANISMS, frozenset({dal.MECHANISM}))
        with self.assertRaises(setup.Refusal):
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
        self.assertEqual(inventory.REVIEWED_VM, setup.VM_UUID)
        self.assertEqual(inventory.REVIEWED_HOME, setup.PVM_HOME)
        self.assertEqual(inventory.PRLCTL_SHA256,
                         '2f61a1c60db2d51591fb8479a248a982a81658f136f1501a94733b7a81731c57')

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
                with self.subTest(label), self.assertRaises(setup.Refusal):
                    setup.verify_disk_association(facts, TEST_SUPPORTED)
            self.assertEqual(setup.verify_disk_association(valid_association(), TEST_SUPPORTED)['serial'], SERIAL)
        with self.assertRaises(setup.Refusal):
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
        swapped = host_read_with(hardware=cli_hardware(
            hdd0=dict(inventory.HDD0_REVIEWED, image='/Volumes/Stash/parallels/other.pvm/harddisk1.hdd')))
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

    def test_the_cli_inventory_has_no_serial_and_guest_disagreements_still_refuse(self):
        """The CLI configuration inventory carries no serial: nothing is compared or inferred. A
        guest-side disagreement between the probe and the sysfs read still refuses."""
        self.assertEqual(set(inventory.parse_output(RETAINED_CLI_STDOUT.decode('ascii'),
                                                    {'vm': inventory.REVIEWED_VM, 'home': inventory.REVIEWED_HOME})),
                         {'vm', 'home', 'home_path', 'state', 'hardware'})
        self.assertEqual(table(valid_association(guest=guest(parent_serial='')))['guest_blocks'][0], 'pass')
        self.assertRefusedAt('guest_blocks', valid_association(guest=guest(sysfs_serial='S9')))

    def test_unsupported_accessor_argv_or_version_refuses_the_read(self):
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_sha256='b' * 64)))
        self.assertRefusedAt('host_read', valid_association(
            host_read=host_read(api='prlctl exec --')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(tool_version='26.0.0')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(desktop_build='27.2.0')))
        self.assertRefusedAt('host_read', valid_association(
            host_read=host_read(argv=TEST_PINS[TEST_MECHANISM]['argv'] + ['--fixture'])))

    def test_truncated_failed_or_unknown_read_refuses(self):
        text = cli_info_text()
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(truncated=True)))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(status='unknown')))
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(exit=1)))
        self.assertRefusedAt('host_read', valid_association(
            host_read=host_read(output=b'x' * (inventory.MAX_BYTES + 1))))
        self.assertRefusedAt('host_read', valid_association(
            host_read=host_read(output=text[:len(text) // 2].encode())))
        doubled = text.replace('"ID": ' + json.dumps(VM), '"ID": ' + json.dumps(VM) + ', "ID": ' + json.dumps(OTHER_VM), 1)
        self.assertRefusedAt('host_read', valid_association(host_read=host_read(output=doubled.encode())))
        self.assertRefusedAt('host_read', valid_association(
            host_read=host_read(output=text.encode() + b'garbage\n')))

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

    def test_production_pin_binds_helper_bytes_cli_vm_and_home(self):
        pin = dal.PINS[dal.MECHANISM]
        self.assertEqual(pin['tool_path'], str(inventory.HELPER))
        self.assertEqual(pin['sha256'], dal.HELPER_SHA256)
        self.assertEqual(pin['sha256'], inventory.helper_digest(inventory.HELPER))
        self.assertEqual(pin['vm'], VM)
        self.assertEqual(pin['argv'], inventory.inventory_argv())
        self.assertEqual(pin['argv'][:4], [inventory.PYTHON, '-I', '-B', str(inventory.HELPER)])
        self.assertEqual(inventory.cli_argv(), [inventory.PRLCTL, 'list', '--info', VM, '--json'])
        self.assertEqual(inventory.PRLCTL, '/Applications/Parallels Desktop.app/Contents/MacOS/prlctl')
        self.assertNotEqual(inventory.PRLCTL, '/usr/local/bin/prlctl')
        self.assertEqual(pin['api'], 'prlctl list --info --json')
        self.assertEqual(inventory.ACCESSOR, 'prlctl list --info --json')
        self.assertEqual(pin['home'], setup.PVM_HOME)
        self.assertEqual(inventory.CLI_ENV, {'LANG': 'C', 'PATH': '/usr/bin:/bin'})
        self.assertEqual(inventory.STATE_RUNNING, 'running')
        self.assertEqual(inventory.HDD0_REVIEWED['image'], setup.PVM_HOME + '/harddisk1.hdd')
        self.assertEqual(inventory.USB_REVIEWED, {'enabled': True})

    def test_the_supported_reader_is_executable_and_owes_no_source(self):
        """The supported CLI route removed the SDK scaffold from the dispatch path. The SDK route was
        never executable and never exercised; no receipt claims otherwise."""
        self.assertTrue(dal.reader_executable())
        self.assertTrue(inventory.reader_executable())
        self.assertEqual(dal.reader_limitation(), 'none')
        self.assertEqual(inventory.reader_limitation(), 'none')

    def test_the_cli_hardware_set_is_the_retained_closed_set(self):
        self.assertEqual(inventory.HARDWARE_SECTIONS,
                         {'cpu', 'memory', 'video', 'memory_quota', 'hdd0', 'cdrom0', 'usb', 'net0', 'sound0'})

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

    def test_the_retained_747_stdout_parses_and_passes_every_premise_offline(self):
        """The genuine retained step-004 CLI stdout, byte-pinned to its recorded sha256, drives the
        full production premise table over fixture config/descriptor/guest bytes. This is an offline
        derivative of the retained originals: no fresh evidence, no current inventory credit."""
        self.assertEqual(len(RETAINED_CLI_STDOUT), 4422)
        self.assertEqual(hashlib.sha256(RETAINED_CLI_STDOUT).hexdigest(), RETAINED_CLI_STDOUT_SHA256)
        pin = dal.PINS[dal.MECHANISM]
        read = host_read(pin, output=RETAINED_CLI_STDOUT)
        association = production_association(host_read=read)
        rows = table(association, setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)
        self.assertEqual({status for status, _ in rows.values()}, {'pass'}, rows)
        result = setup.verify_disk_association(association)
        self.assertEqual(result['vm'], VM)
        self.assertEqual(result['system_name'], 'harddisk1.hdd')

    def test_extra_or_unknown_device_section_refuses_inventory(self):
        """A second disk, an unknown hardware section, a second USB entry and a nested hiding place
        under an allowed section all refuse the read."""
        cases = {
            'second hard disk': cli_hardware(hdd1={'enabled': True, 'port': 'sata:2',
                                                   'image': setup.PVM_HOME + '/harddisk2.hdd',
                                                   'type': 'expanded', 'size': '131072Mb'}),
            'unknown hardware section': cli_hardware(scsi0={'enabled': True}),
            'usb host device entry': cli_hardware(usb={'enabled': True, 'SanDisk': {'attached': True}}),
            'nested object under cpu': dict(cli_hardware(), cpu={'cpus': 6, 'hidden': {'image': 'x'}}),
        }
        for label, hardware in cases.items():
            with self.subTest(label):
                read = host_read_with(hardware=hardware)
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')
                with mock.patch.object(dal, 'PINS', TEST_PINS), self.assertRaises(setup.Refusal):
                    setup.verify_disk_association(valid_association(host_read=read), TEST_SUPPORTED)
                prod = host_read(dal.PINS[dal.MECHANISM],
                                 output=cli_info_text(dal.PINS[dal.MECHANISM], hardware=hardware).encode())
                with self.assertRaises(setup.Refusal):
                    setup.verify_disk_association(production_association(host_read=prod))

    def test_the_data_disk_must_be_the_reviewed_image_backed_hdd0(self):
        """Any field of the one data disk that differs from the reviewed expanding image at SATA 0
        refuses: port, image, type, size, compaction flag, enabled, an extra or missing field."""
        cases = {
            'other port': dict(inventory.HDD0_REVIEWED, port='sata:3'),
            'other image': dict(inventory.HDD0_REVIEWED, image=setup.PVM_HOME + '/harddisk2.hdd'),
            'plain disk type': dict(inventory.HDD0_REVIEWED, type='plain'),
            'disabled disk': dict(inventory.HDD0_REVIEWED, enabled=False),
            'changed size': dict(inventory.HDD0_REVIEWED, size='65536Mb'),
            'compaction off': dict(inventory.HDD0_REVIEWED, **{'online-compact': 'off'}),
            'extra field': dict(inventory.HDD0_REVIEWED, passthrough='0'),
            'missing field': {k: v for k, v in inventory.HDD0_REVIEWED.items() if k != 'type'},
        }
        for label, hdd0 in cases.items():
            with self.subTest(label):
                read = host_read_with(hardware=cli_hardware(hdd0=hdd0))
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')

    def test_a_boolean_field_will_not_accept_1_or_true_like_integers(self):
        """The reviewed shape is type-exact: JSON true is a Python bool and 1 is an int, so a 1
        cannot stand in for the reviewed enabled flag of any exact-shaped section."""
        cases = {
            'hdd0 enabled 1': cli_hardware(hdd0=dict(inventory.HDD0_REVIEWED, enabled=1)),
            'cdrom0 enabled 1': cli_hardware(cdrom0=dict(inventory.CDROM0_REVIEWED, enabled=1)),
            'usb enabled 1': cli_hardware(usb={'enabled': 1}),
            'cdrom0 enabled 0': cli_hardware(cdrom0=dict(inventory.CDROM0_REVIEWED, enabled=0)),
        }
        for label, hardware in cases.items():
            with self.subTest(label):
                read = host_read_with(hardware=hardware)
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')
                with self.assertRaises(inventory.GrammarError):
                    inventory.parse_output(cli_info_text(dal.PINS[dal.MECHANISM],
                                                         hardware=hardware),
                                           dal.PINS[dal.MECHANISM])

    def test_the_optical_section_is_counted_and_never_a_disk(self):
        missing = {k: v for k, v in cli_hardware().items() if k != 'cdrom0'}
        second = dict(cli_hardware(), cdrom1=dict(inventory.CDROM0_REVIEWED, port='sata:3'))
        altered = cli_hardware(cdrom0=dict(inventory.CDROM0_REVIEWED, state='connected'))
        for label, hardware in (('no optical', missing), ('second optical', second), ('altered optical', altered)):
            with self.subTest(label):
                read = host_read_with(hardware=hardware)
                self.assertEqual(table(valid_association(host_read=read))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association())['host_read'][0], 'pass')

    def test_wrong_vm_home_state_tool_or_helper_refuses(self):
        pin = TEST_PINS[TEST_MECHANISM]
        self.assertEqual(table(valid_association(host_read=host_read(vm=OTHER_VM)))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=cli_info_text(pin, Home=pin['home']).encode())))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=cli_info_text(pin, **{'Home path': pin['home'] + '/other.pvs'}).encode())))['host_read'][0],
            'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(
            output=cli_info_text(pin, State='stopped').encode())))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(boot_ids=[BOOT, 'other'])))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(tool_version='26.0.0')))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(tool_sha256='c' * 64)))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(argv=pin['argv'] + ['--fixture'])))
                         ['host_read'][0], 'refuse')

    def test_stale_missing_or_changed_read_refuses(self):
        self.assertEqual(table(valid_association(host_read=host_read(status='ok', exit=1)))['host_read'][0], 'refuse')
        self.assertEqual(table(valid_association(host_read=host_read(status='ok', truncated=True)))['host_read'][0],
                         'refuse')
        self.assertEqual(table(valid_association(host_read=None))['host_read'][0], 'refuse')

    def test_mismatched_cli_and_xml_refuse(self):
        """The CLI inventory binds to the registered XML: another VM, another home or an image path
        outside the reviewed bundle cannot pair with the reviewed config."""
        pin = dal.PINS[dal.MECHANISM]
        other_vm = host_read(pin, output=cli_info_text(pin, ID=OTHER_VM).encode())
        self.assertEqual(table(production_association(host_read=other_vm),
                               setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)['host_read'][0], 'refuse')
        other_home = host_read(pin, output=cli_info_text(
            pin, Home='/Volumes/Stash/parallels/other.pvm/',
            **{'Home path': '/Volumes/Stash/parallels/other.pvm/config.pvs'},
            hardware=cli_hardware(hdd0=dict(inventory.HDD0_REVIEWED,
                                            image='/Volumes/Stash/parallels/other.pvm/harddisk1.hdd'))).encode())
        self.assertEqual(table(production_association(host_read=other_home),
                               setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)['host_read'][0], 'refuse')

    def test_extra_guest_disk_or_mapper_refuses(self):
        extra = guest_blocks() + [{'name': 'sdb', 'type': 'disk', 'path': '/dev/sdb', 'size': 1, 'pkname': None}]
        mapper = guest_blocks() + [{'name': 'sda2_crypt', 'type': 'crypt', 'path': '/dev/mapper/sda2_crypt',
                                    'size': 1, 'pkname': 'sda2'}]
        self.assertEqual(table(valid_association(guest=guest(blocks=extra)))['guest_blocks'][0], 'refuse')
        self.assertEqual(table(valid_association(guest=guest(blocks=mapper)))['guest_blocks'][0], 'refuse')

    def test_malformed_partial_or_ambiguous_cli_output_refuses(self):
        pin = dal.PINS[dal.MECHANISM]
        text = cli_info_text(pin)
        cases = (
            ('not ascii', b'\xff', 'inventory-output-encoding'),
            ('empty', b'', 'inventory-output-grammar'),
            ('not json', b'list: no such VM', 'inventory-output-grammar'),
            ('two vms', cli_info_text(pin, vm_count=2).encode(), 'inventory-output-grammar'),
            ('zero vms', b'[]', 'inventory-output-grammar'),
            ('object not array', json.dumps(cli_vm(pin)).encode(), 'inventory-output-grammar'),
            ('hardware not a map', cli_info_text(pin, hardware=[]).encode(), 'inventory-output-grammar'),
            ('missing hardware section', cli_info_text(
                pin, hardware={k: v for k, v in cli_hardware().items() if k != 'net0'}).encode(),
             'inventory-output-grammar'),
            ('duplicate id key', text.replace('"ID": ' + json.dumps(VM),
                                              '"ID": ' + json.dumps(VM) + ', "ID": ' + json.dumps(OTHER_VM), 1).encode(),
             'inventory-output-grammar'),
            ('truncated json', text[:len(text) // 2].encode(), 'inventory-output-grammar'),
            ('trailing garbage', text.encode() + b'extra\n', 'inventory-output-grammar'),
        )
        for label, payload, phase in cases:
            with self.subTest(label):
                self.assertIn(phase, inventory.READ_PHASES)
                with self.assertRaises(inventory.Refused) as caught:
                    dal.read_inventory(transport=inventory_transport(output=payload), boot_id=BOOT,
                                       read_boot_id=boot_reader([BOOT, BOOT]))
                self.assertEqual(caught.exception.phase, phase)
        with self.assertRaises(inventory.Refused) as caught:
            dal.read_inventory(transport=inventory_transport(), pin={}, boot_id=BOOT,
                               read_boot_id=boot_reader([BOOT, BOOT]))
        self.assertEqual(caught.exception.phase, 'pin-missing')

    def test_installed_tool_custody_refuses_drift_alias_or_wrong_version_without_executing_it(self):
        """The helper's custody check is local metadata: path, bytes, Info.plist. It never runs the
        tool, and an aliased path, drifted bytes or a wrong plist version refuses before dispatch."""
        fixture_root = Path(os.environ.get('TMPDIR') or tempfile.gettempdir())
        with tempfile.TemporaryDirectory(dir=fixture_root) as scratch:
            base = Path(scratch)
            tool = base / 'prlctl'
            tool.write_bytes(b'fake tool bytes')
            plist = base / 'Info.plist'
            plist.write_bytes(plistlib.dumps({'CFBundleShortVersionString': inventory.DESKTOP_VERSION,
                                              'CFBundleVersion': inventory.DESKTOP_BUILD}))
            fixture_sha = hashlib.sha256(b'fake tool bytes').hexdigest()
            verified = inventory.verify_installed_cli(path=str(tool), plist_path=str(plist),
                                                      expected_sha=fixture_sha)
            self.assertEqual(verified, {'path': str(tool), 'sha256': fixture_sha,
                                        'version': inventory.DESKTOP_VERSION, 'build': inventory.DESKTOP_BUILD})
            alias = base / 'prlctl-alias'
            alias.symlink_to(tool)
            drifted = base / 'prlctl-drifted'
            drifted.write_bytes(b'drifted bytes')
            wrong_build = base / 'wrong-build.plist'
            wrong_build.write_bytes(plistlib.dumps({'CFBundleShortVersionString': inventory.DESKTOP_VERSION,
                                                    'CFBundleVersion': '99999'}))
            cases = (
                ('aliased tool', dict(path=str(alias), plist_path=str(plist), expected_sha=fixture_sha), 'tool-alias'),
                ('missing tool', dict(path=str(base / 'absent'), plist_path=str(plist)), 'tool-unavailable'),
                ('drifted tool', dict(path=str(drifted), plist_path=str(plist), expected_sha=fixture_sha),
                 'tool-hash-mismatch'),
                ('plist unavailable', dict(path=str(tool), plist_path=str(base / 'absent.plist'),
                                           expected_sha=fixture_sha), 'tool-plist-unavailable'),
                ('wrong build', dict(path=str(tool), plist_path=str(wrong_build), expected_sha=fixture_sha),
                 'tool-version-mismatch'),
            )
            for label, kwargs, phase in cases:
                with self.subTest(label):
                    with self.assertRaises(inventory.Refused) as caught:
                        inventory.verify_installed_cli(**kwargs)
                    self.assertEqual(caught.exception.phase, phase)

    def test_helper_main_refuses_extra_args_and_does_not_accept_a_fixture_path(self):
        self.assertEqual(inventory.main([]), 2)
        self.assertEqual(inventory.main(['inventory', '--fixture', '/tmp/x']), 2)
        self.assertEqual(inventory.main(['inventory', str(inventory.HELPER)]), 2)
        completed = subprocess.run(inventory.inventory_argv() + ['--fixture', '/tmp/x'],
                                   stdin=subprocess.DEVNULL, capture_output=True, timeout=120)
        self.assertEqual(completed.returncode, 2)
        self.assertIn(b'usage', completed.stderr)

    def test_the_real_entry_passes_through_exact_originals_and_refuses_with_a_phase(self):
        """The production entry is the concrete reader: it verifies custody, runs the one bounded
        dispatch through the injected seam, and writes the exact original bytes on success. A
        failed or unparseable dispatch refuses with its phase and prints no output. Custody
        verification reads the installed tool's local metadata only (canonical path, bytes, app
        Info.plist) and never executes it. Fixture transport only; no prlctl process runs."""
        pin_payload = cli_info_text(dal.PINS[dal.MECHANISM]).encode()
        dispatched = []

        def transport(argv, stdin_path, timeout, cap, **kwargs):
            dispatched.append(argv)
            self.assertEqual(argv, inventory.cli_argv())
            self.assertIsNone(stdin_path)
            self.assertEqual(timeout, inventory.INVENTORY_TIMEOUT)
            self.assertEqual(cap, inventory.MAX_BYTES)
            return {'disposition': 'exited', 'waited_exit': 0, 'stdout': pin_payload, 'stderr': b''}

        written = []
        with mock.patch.object(inventory.transfers, 'run_transfer', transport), \
                mock.patch.object(sys, 'stdout') as stdout:
            stdout.buffer.write = lambda data: written.append(data)
            self.assertEqual(inventory.main(['inventory']), 0)
            self.assertEqual(b''.join(written), pin_payload)
        self.assertEqual(dispatched, [inventory.cli_argv()])

        def failing(argv, stdin_path, timeout, cap, **kwargs):
            return {'disposition': 'failed', 'waited_exit': 255, 'stdout': b'', 'stderr': b'boom'}

        with mock.patch.object(inventory.transfers, 'run_transfer', failing), \
                mock.patch('sys.stdout', new=io.StringIO()) as fake_out:
            self.assertEqual(inventory.main(['inventory']), 3)
        self.assertIn('status=refused phase=inventory-not-exited', fake_out.getvalue())

        def unparseable(argv, stdin_path, timeout, cap, **kwargs):
            return {'disposition': 'exited', 'waited_exit': 0, 'stdout': b'{"ID": ', 'stderr': b''}

        with mock.patch.object(inventory.transfers, 'run_transfer', unparseable), \
                mock.patch('sys.stdout', new=io.StringIO()) as fake_out:
            self.assertEqual(inventory.main(['inventory']), 3)
        self.assertIn('status=refused phase=inventory-output-grammar', fake_out.getvalue())

    def test_pinned_dispatch_uses_the_fixed_argv_with_private_fixture_transport(self):
        seen = []

        def transport(argv, stdin_path, timeout, output_cap):
            seen.append((argv, stdin_path, timeout, output_cap))
            return {'disposition': 'exited', 'waited_exit': 0,
                    'stdout': cli_info_text(dal.PINS[dal.MECHANISM]).encode(), 'stderr': b''}

        boots = boot_reader([BOOT, BOOT])
        record = dal.read_inventory(transport=transport, boot_id=BOOT, read_boot_id=boots)
        self.assertEqual(seen, [(inventory.inventory_argv(), None, inventory.INVENTORY_TIMEOUT, inventory.MAX_BYTES)])
        self.assertEqual(record['argv'], inventory.inventory_argv())
        self.assertEqual(record['tool_path'], str(inventory.HELPER))
        self.assertEqual(record['tool_sha256'], dal.HELPER_SHA256)
        self.assertEqual(record['api'], inventory.ACCESSOR)
        self.assertNotIn('readonly', record)
        self.assertEqual(record['tool_version'], inventory.DESKTOP_VERSION)
        self.assertEqual(record['desktop_build'], inventory.DESKTOP_VERSION + '-' + inventory.DESKTOP_BUILD)
        # The VM UUID and the boot bracket are readings of this dispatch, not caller constants.
        self.assertEqual(record['vm'], VM)
        self.assertEqual(record['boot_ids'], [BOOT, BOOT])
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
        text = cli_info_text(pin)
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
            ('not ascii', dict(stdout=b'{\xff}'), 'inventory-output-encoding'),
            ('partial output', dict(stdout=text[:len(text) // 2].encode()), 'inventory-output-grammar'),
            ('repeated field', dict(stdout=(text.encode() + text[:20].encode())), 'inventory-output-grammar'),
            ('another vm', dict(stdout=cli_info_text(pin, ID=OTHER_VM).encode()), 'inventory-vm-differs'),
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

    def test_stored_record_reverifies_through_production_setup(self):
        """The stored fact document, not a reduced verify result, is what the setup rows re-verify.

        A stage keeps one JSON document; historical reading calls setup.verify_disk_association on that
        key, so a reduced summary must not be able to stand in for it and a stored document must
        re-verify to the same association.
        """
        block, lsblk = guest_block_observations()
        facts = dal.association_facts(
            boot_id=BOOT, read_boot_id=boot_reader([BOOT, BOOT]),
            config=config_xml(), descriptor=identity_tests.doc(), extents=descriptor_extents(),
            block={'blocks': block}, lsblk={'nodes': lsblk}, root=guest_root_facts(),
            transport=inventory_transport())
        result = setup.verify_disk_association(facts)
        stored = json.loads(json.dumps(dal.stored_facts(facts)))
        self.assertEqual(stored['schema'], dal.FACTS_SCHEMA)
        self.assertEqual(stored['host_read']['output']['bytes'], len(facts['host_read']['output']))
        self.assertEqual(stored['host_read']['output']['sha256'],
                         hashlib.sha256(facts['host_read']['output']).hexdigest())
        self.assertEqual(setup.verify_disk_association(stored), result)
        self.assertEqual(dal.load_facts(stored)['config'], config_xml())
        # A reduced verify result is not facts and is refused by the same consumer entry.
        with self.assertRaises(setup.Refusal):
            setup.verify_disk_association(result)

        def reseal(document, field, data):
            document[field] = {'encoding': 'base64', 'bytes': len(data),
                               'sha256': hashlib.sha256(data).hexdigest(),
                               'data': base64.b64encode(data).decode('ascii')}

        # A stored original that disagrees with its own envelope, or loses the envelope, refuses; a
        # correctly resealed but different original is not a bypass, because the premise re-derives
        # its own facts from those bytes and refuses it.
        for label, mutate in (
                ('config resealed as another document',
                 lambda doc: reseal(doc, 'config', b'<ParallelsVirtualMachine/>')),
                ('stdout digest', lambda doc: doc['host_read']['output'].update(sha256='0' * 64)),
                ('length', lambda doc: doc['descriptor'].update(bytes=1)),
                ('no envelope', lambda doc: doc['descriptor'].clear()),
                ('no schema', lambda doc: doc.pop('schema'))):
            with self.subTest(label):
                tampered = json.loads(json.dumps(stored))
                mutate(tampered)
                with self.assertRaises(setup.Refusal):
                    setup.verify_disk_association(tampered)

    def test_retained_747_replay_is_offline_and_supplies_no_fresh_evidence(self):
        """747/748 retained shape establishes config/descriptor/guest only. No fresh inventory."""
        rows = table(dict(production_association(), host_read=None),
                     setup.SUPPORTED_DISK_MECHANISMS, dal.PINS)
        self.assertEqual(rows['config'][0], 'pass')
        self.assertEqual(rows['descriptor'][0], 'pass')
        self.assertEqual(rows['guest_blocks'][0], 'pass')
        self.assertEqual(rows['host_read'][0], 'refuse')
        with self.assertRaises(setup.Refusal):
            setup.verify_disk_association(dict(production_association(), host_read=None))


if __name__ == '__main__':
    unittest.main()
