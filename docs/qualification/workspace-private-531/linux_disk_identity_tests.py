"""Controls for the documented Parallels top selection (linux_disk_identity.select_top).

Portable controls use synthetic descriptors shaped by the QEMU specification and run no host,
VM, disk or header access. The retained-719 control runs only when EFFIGY_LINUX_719_DESCRIPTOR
names a local copy of the retained before-descriptor; otherwise it is skipped and reported as
not exercised. A pass here is metadata-selection evidence only.
"""
import hashlib
import os
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_disk_identity as identity
import linux_setup as setup

DISK_UID = setup.DISK_UID
OTHER_UID = '{11111111-2222-3333-4444-555555555555}'

A = '{11111111-1111-1111-1111-111111111111}'
B = '{22222222-2222-2222-2222-222222222222}'
R = '{33333333-3333-3333-3333-333333333333}'
P = identity.PREDEFINED_TOP
ZERO = identity.ZERO
BACKUP = identity.BACKUP_ID
SIZE = 524288 * 16 * 32
RETAINED_719_SHA256 = '974876501b91274d3e433d19f7ee5347fa23f5562b6062c8ccb8c8be194c09a0'
TOP_CHAIN = ((A, 'Compressed', 'a.hds'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))
TOP_SHOTS = ((A, B), (B, R), (R, ZERO))
TOP_NODE = f'<TopGUID>{A}</TopGUID>'


def doc(top_node=TOP_NODE, images=TOP_CHAIN, shots=TOP_SHOTS, version='1.0', padding='0', size=SIZE,
        cylinders=524288, heads=16, sectors=32, start='0', end=None, blocksize='2048', storages=1,
        extra='', snapshot_extra='', uid_xml=None):
    """A descriptor in the documented shape; keyword overrides make one field wrong at a time.

    The unspecified nodes mirror the retained shape (sector sizes, an encryption node with a zero
    engine, the disk UID, a name and miscellaneous counters). None of them changes the selection.
    """
    end = str(size) if end is None else end
    uid_xml = f'<UID>{DISK_UID}</UID>' if uid_xml is None else uid_xml
    image_xml = ''.join(f'<Image><GUID>{g}</GUID><Type>{t}</Type><File>{f}</File></Image>' for g, t, f in images)
    storage = f'<Storage><Start>{start}</Start><End>{end}</End><Blocksize>{blocksize}</Blocksize>{image_xml}</Storage>'
    params = (f'<Disk_Parameters><Disk_size>{size}</Disk_size><Cylinders>{cylinders}</Cylinders>'
              f'<Heads>{heads}</Heads><Sectors>{sectors}</Sectors><Padding>{padding}</Padding>'
              '<PhysicalSectorSize>4096</PhysicalSectorSize><LogicSectorSize>512</LogicSectorSize>'
              '<Encryption><Engine>{00000000-0000-0000-0000-000000000000}</Engine><Data></Data><Salt></Salt></Encryption>'
              f'{uid_xml}<Name>harddisk1 SSD</Name><Miscellaneous><Bootable>1</Bootable></Miscellaneous>'
              '</Disk_Parameters>')
    shot_xml = ''.join(f'<Shot><GUID>{g}</GUID><ParentGUID>{p}</ParentGUID></Shot>' for g, p in shots)
    return (f'<Parallels_disk_image Version="{version}">{params}<StorageData>{storage * storages}</StorageData>'
            f'<Snapshots>{top_node}{shot_xml}{snapshot_extra}</Snapshots>{extra}</Parallels_disk_image>').encode()


def refuses(data):
    with unittest.TestCase().assertRaises(identity.Refusal):
        identity.select_top(data)


class DocumentedSelection(unittest.TestCase):
    def test_explicit_top_is_selected_and_bound_to_the_exact_bytes(self):
        data = doc()
        selection = identity.select_top(data)
        self.assertEqual(selection['selector'], 'TopGUID')
        self.assertEqual(selection['top_guid'], A)
        self.assertEqual(selection['top_file'], 'a.hds')
        self.assertEqual(selection['chain'], [A, B, R])
        self.assertEqual(selection['chain_files'], ['a.hds', 'b.hds', 'r.hds'])
        self.assertEqual(selection['descriptor_sha256'], hashlib.sha256(data).hexdigest())
        self.assertEqual(selection, identity.select_top(data))
        self.assertIn('not established', selection['not_established'])
        self.assertNotEqual(selection['digest'], identity.select_top(doc(blocksize='4096'))['digest'])

    def test_predefined_names_the_top_only_when_top_guid_is_absent(self):
        images = ((P, 'Compressed', 'p.hds'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))
        shots = ((P, B), (B, R), (R, ZERO))
        selection = identity.select_top(doc(top_node='', images=images, shots=shots))
        self.assertEqual((selection['selector'], selection['top_guid']), ('predefined', P))

    def test_predefined_guid_is_an_ordinary_image_when_top_guid_is_explicit(self):
        images = TOP_CHAIN + ((P, 'Compressed', 'p.hds'),)
        shots = TOP_SHOTS + ((P, R),)
        selection = identity.select_top(doc(images=images, shots=shots))
        self.assertEqual((selection['selector'], selection['top_guid']), ('TopGUID', A))

    def test_absent_predefined_top_refuses(self):
        refuses(doc(top_node=''))

    def test_explicit_top_refusals(self):
        refuses(doc(top_node='<TopGUID></TopGUID>'))
        refuses(doc(top_node=f'<TopGUID> {A} </TopGUID>'))
        refuses(doc(top_node='<TopGUID>{not-a-guid}</TopGUID>'))
        refuses(doc(top_node=f'<TopGUID>{A}</TopGUID><TopGUID>{A}</TopGUID>'))
        refuses(doc(top_node=f'<TopGUID>{R}</TopGUID>'))
        refuses(doc(top_node=f'<TopGUID>{ZERO}</TopGUID>'))
        unknown = '{99999999-9999-9999-9999-999999999999}'
        refuses(doc(top_node=f'<TopGUID>{unknown}</TopGUID>'))

    def test_backup_id_cannot_be_the_top(self):
        images = ((BACKUP, 'Compressed', 'k.hds'), (R, 'Plain', 'r.hds'))
        shots = ((BACKUP, R), (R, ZERO))
        refuses(doc(top_node=f'<TopGUID>{BACKUP}</TopGUID>', images=images, shots=shots))

    def test_schema_occurrence_and_exact_version(self):
        refuses(doc(version='1.1'))
        refuses(doc().replace(b' Version="1.0"', b''))
        refuses(doc(extra='<Disk_Parameters/>'))
        refuses(doc(extra='<StorageData/>'))
        refuses(doc().replace(b'<Padding>0</Padding>', b''))
        refuses(doc().replace(b'<Disk_size>', b'<Disk_size>1</Disk_size><Disk_size>', 1))

    def test_padding_split_and_geometry(self):
        refuses(doc(padding='1'))
        refuses(doc(padding=''))
        refuses(doc(cylinders=524287))
        refuses(doc(end=str(SIZE - 1)))
        refuses(doc(start='1'))
        refuses(doc(blocksize='0'))
        refuses(doc().replace(b'<Blocksize>2048</Blocksize>', b''))
        refuses(doc(storages=2))
        refuses(doc(size='0' + str(SIZE), end='0' + str(SIZE)))

    def test_image_types_and_overlay_rule(self):
        refuses(doc(images=((A, 'Compressed', 'a.hds'), (B, 'Split', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', 'a.hds'), (B, 'Plain', 'b.hds'), (R, 'Plain', 'r.hds'))))

    def test_identity_names_and_aliases(self):
        refuses(doc(images=((A, 'Compressed', 'a.hds'), (A, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', 'a.hds'), (B, 'Compressed', 'A.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', '../a.hds'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', 'sub/a.hds'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', 'a.img'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', 'a\\x.hds'), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))
        refuses(doc(images=((A, 'Compressed', ''), (B, 'Compressed', 'b.hds'), (R, 'Plain', 'r.hds'))))

    def test_chain_is_one_rooted_acyclic_and_complete(self):
        refuses(doc(shots=((A, B), (B, ZERO), (R, ZERO))))
        refuses(doc(shots=((A, B), (B, A), (R, ZERO))))
        refuses(doc(shots=((A, '{44444444-4444-4444-4444-444444444444}'), (B, R), (R, ZERO))))
        refuses(doc(shots=((A, B), (B, R))))
        refuses(doc(shots=((A, B), (B, R), (R, ZERO), (R, ZERO))))
        # A cycle off the top's path must refuse even though the top chain is sound.
        y = '{55555555-5555-5555-5555-555555555555}'
        images = TOP_CHAIN + ((P, 'Compressed', 'p.hds'), (y, 'Compressed', 'y.hds'))
        shots = TOP_SHOTS + ((P, y), (y, P))
        refuses(doc(images=images, shots=shots))

    def test_top_must_be_a_leaf(self):
        images = TOP_CHAIN + ((P, 'Compressed', 'p.hds'),)
        shots = TOP_SHOTS + ((P, A),)
        refuses(doc(top_node=f'<TopGUID>{B}</TopGUID>', images=images, shots=shots))

    def test_unspecified_nodes_and_attributes_are_uninterpreted(self):
        plain = identity.select_top(doc())
        decorated = doc(extra='<Encryption><Mode>none</Mode></Encryption><Miscellaneous><Note>x</Note></Miscellaneous>',
                        snapshot_extra='<Vendor>internal</Vendor>')
        decorated = decorated.replace(b'Version="1.0"', b'Version="1.0" Vendor="x"')
        result = identity.select_top(decorated)
        for key in ('selector', 'top_guid', 'top_file', 'chain', 'chain_files', 'images', 'disk_size'):
            self.assertEqual(result[key], plain[key])

    def test_disk_uid_is_exactly_one_guid_under_disk_parameters(self):
        refuses(doc(uid_xml=''))
        refuses(doc(uid_xml=f'<UID>{DISK_UID}</UID><UID>{DISK_UID}</UID>'))
        refuses(doc(uid_xml='<UID>not-a-guid</UID>'))
        refuses(doc(uid_xml='<UID> ' + DISK_UID + ' </UID>'))
        self.assertEqual(identity.select_top(doc())['disk_uid'], DISK_UID)

    def test_bounds_and_entity_declarations_refuse(self):
        refuses(b'')
        refuses(doc() + b' ' * identity.MAX_BYTES)
        refuses(b'<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]>' + doc())
        refuses(b'<!doctype x>' + doc())
        refuses(b'<Parallels_disk_image Version="1.0"><Disk_Parameters>')


class SetupVerifierBinding(unittest.TestCase):
    def test_observation_must_match_the_derived_selection(self):
        data = doc()
        obs = identity.observation(data)
        self.assertEqual(setup.verify_writable_top(obs, data)['top_guid'], A)
        for changed in (dict(obs, guid=B), dict(obs, mechanism='unknown'), {k: v for k, v in obs.items() if k != 'result_digest'},
                        dict(obs, result_digest='0' * 64)):
            with self.assertRaises(setup.Refusal):
                setup.verify_writable_top(changed, data)

    def test_a_descriptor_for_another_disk_refuses_even_with_a_matching_observation(self):
        other = doc(uid_xml=f'<UID>{OTHER_UID}</UID>')
        self.assertEqual(identity.select_top(other)['top_guid'], A)
        with self.assertRaises(setup.Refusal):
            setup.verify_writable_top(identity.observation(other), other)

    def test_stale_observation_refuses_after_the_descriptor_changes(self):
        old = doc()
        obs = identity.observation(old)
        with self.assertRaises(setup.Refusal):
            setup.verify_writable_top(obs, doc(blocksize='4096'))

    def test_no_caller_value_adds_a_mechanism(self):
        self.assertEqual(setup.SUPPORTED_TOP_MECHANISMS, frozenset({identity.MECHANISM}))
        with self.assertRaises(setup.Refusal):
            setup.verify_writable_top({'mechanism': 'caller-chosen', 'guid': A}, doc())


class RetainedDescriptorControl(unittest.TestCase):
    """Local-only: the retained 719 before-descriptor, bound by SHA-256, never committed."""

    def test_retained_719_selects_the_predefined_top_chain(self):
        path = os.environ.get('EFFIGY_LINUX_719_DESCRIPTOR')
        if not path:
            self.skipTest('retained-719 local control not run: EFFIGY_LINUX_719_DESCRIPTOR is unset')
        data = Path(path).read_bytes()
        self.assertEqual(hashlib.sha256(data).hexdigest(), RETAINED_719_SHA256)
        selection = identity.select_top(data)
        self.assertEqual(selection['selector'], 'predefined')
        self.assertEqual(selection['top_guid'], P)
        self.assertEqual(selection['top_file'], 'harddisk1.hdd.0.' + P + '.hds')
        self.assertEqual(len(selection['chain']), 4)
        self.assertEqual(len(selection['images']), 37)
        self.assertEqual(selection['disk_uid'], DISK_UID)
        # Metadata selection and the setup verifier's identity comparison only; no runtime or setup proof.
        self.assertEqual(setup.verify_writable_top(identity.observation(data), data)['top_guid'], P)


if __name__ == '__main__':
    unittest.main()
