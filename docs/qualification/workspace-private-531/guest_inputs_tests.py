"""Controls for guest_inputs: the fixed finite Colima runtime-input transfer and its guest-side layout.

Every case is local: a tiny genuine-byte stand-in for the pinned OCI/deb set, a fresh mkdtemp directory under
$TMPDIR as the guest parent, and the real framing, receive, readback and derivation code. One case runs the guest
mode in a real child process (the same `python3 -I -B -c <source> transfer <hex spec>` the Colima route launches);
nothing fetches a payload, contacts an engine or touches a real guest.

Run: python3 -I -B guest_inputs_tests.py
"""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import guest_inputs as gi  # noqa: E402
import macos_guest_helper as helper  # noqa: E402

CONTEXT32 = '0' * 31 + '1'
NAME = 'effigy-531-image-' + CONTEXT32
MIB = 1024 * 1024


def sha(data):
    return hashlib.sha256(data).hexdigest()


def oci(data, media_type):
    return dict(digest='sha256:' + sha(data), bytes=len(data), media_type=media_type), data


class Runtime:
    """A tiny stand-in for the pinned OCI/deb set: every identity is a genuine hash of the bytes."""

    def __init__(self):
        config, self.config = oci(json.dumps({'os': 'linux', 'architecture': 'arm64'},
                                             separators=(',', ':')).encode(),
                                  'application/vnd.oci.image.config.v1+json')
        layer, self.layer = oci(b'fake-ubuntu-layer\n', 'application/vnd.oci.image.layer.v1.tar+gzip')
        manifest, self.manifest = oci(json.dumps({
            'schemaVersion': 2, 'mediaType': 'application/vnd.oci.image.manifest.v1+json',
            'config': {'mediaType': config['media_type'], 'size': config['bytes'], 'digest': config['digest']},
            'layers': [{'mediaType': layer['media_type'], 'size': layer['bytes'], 'digest': layer['digest']}],
        }, separators=(',', ':')).encode(), 'application/vnd.oci.image.manifest.v1+json')
        index, self.index = oci(json.dumps({
            'schemaVersion': 2, 'mediaType': gi.OCI_INDEX_MEDIA_TYPE,
            'manifests': [{'mediaType': manifest['media_type'], 'digest': manifest['digest'],
                           'size': manifest['bytes'],
                           'platform': {'architecture': 'arm64', 'os': 'linux', 'variant': 'v8'}}],
        }, separators=(',', ':')).encode(), gi.OCI_INDEX_MEDIA_TYPE)
        self.debs = [dict(name='python3_3.10.6-1~22.04.1_arm64.deb', bytes=len(b'deb-one\n'),
                          sha256=sha(b'deb-one\n')),
                     dict(name='media-types_7.0.0_all.deb', bytes=len(b'deb-two\n'), sha256=sha(b'deb-two\n'))]
        self.pin = dict(index=index, manifest=manifest, config=config, layer=layer, debs=tuple(self.debs))
        self.layout = gi.pin_layout(self.pin)
        self.entries, self.generated = gi.layout_entries(self.layout, self.index)
        self.bodies = {'oci/index.upstream.json': self.index}
        self.bodies['oci/blobs/sha256/' + sha(self.manifest)] = self.manifest
        self.bodies['oci/blobs/sha256/' + sha(self.config)] = self.config
        self.bodies['oci/blobs/sha256/' + sha(self.layer)] = self.layer
        self.bodies['debs/python3_3.10.6-1~22.04.1_arm64.deb'] = b'deb-one\n'
        self.bodies['debs/media-types_7.0.0_all.deb'] = b'deb-two\n'


RUNTIME = Runtime()
SOURCE = Path(gi.__file__).read_text()


class Fixture(unittest.TestCase):
    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='guest-inputs-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.parent = os.path.join(self.tmp, 'guest-parent')
        os.mkdir(self.parent, 0o700)
        info = os.lstat(self.parent)
        self.binding = dict(path=self.parent, device=info.st_dev, inode=info.st_ino, uid=info.st_uid)
        self.spec = gi.transfer_spec(RUNTIME.pin, self.binding, NAME)
        self.encoded = gi.spec_bytes(self.spec).hex()
        self.inputs = os.path.join(self.tmp, 'inputs')
        for entry in RUNTIME.entries:
            path = os.path.join(self.inputs, entry['path'])
            os.makedirs(os.path.dirname(path), 0o700, exist_ok=True)
            data = self.body(entry)
            Path(path).write_bytes(data)
            os.chmod(path, 0o600)

    def body(self, entry):
        self.skipTest('fixture') if False else None
        return RUNTIME.generated[entry['path']] if entry['path'] in RUNTIME.generated \
            else RUNTIME.bodies[entry['path']]

    def sources(self, entries=None):
        entries = entries if entries is not None else RUNTIME.entries
        return {entry['path']: RUNTIME.generated[entry['path']] if entry['path'] in RUNTIME.generated
                else os.path.join(self.inputs, entry['path']) for entry in entries}

    def stream(self, entries=None, sources=None, limits=None, name='transfer.bin'):
        return gi.write_stream(os.path.join(self.tmp, name), entries if entries is not None else RUNTIME.entries,
                               sources if sources is not None else self.sources(),
                               limits if limits is not None else self.spec['limits'])

    def received(self, stream=None, parent=None, encoded=None):
        stream = stream if stream is not None else self.stream()
        encoded = encoded or self.encoded
        parent = parent or self.parent
        if parent != self.parent:
            info = os.lstat(parent)
            spec = gi.transfer_spec(RUNTIME.pin, dict(path=parent, device=info.st_dev, inode=info.st_ino,
                                                      uid=info.st_uid), NAME)
            encoded = gi.spec_bytes(spec).hex()
        with open(stream['path'], 'rb') as source:
            return gi.main(['transfer', encoded], stdin=source, stdout=io.StringIO())

    def clean(self):
        """Remove the two guest input directories this fixture's parent holds, so one case can drive several."""
        for name in (NAME + '.oci', NAME + '.debs'):
            shutil.rmtree(os.path.join(self.parent, name), ignore_errors=True)

    def assert_refused(self, call, reason):
        with self.assertRaises(gi.Refusal) as caught:
            call()
        self.assertIn(reason, str(caught.exception))


class FramingTests(Fixture):
    def test_the_exact_reviewed_sequence_is_framed_and_received(self):
        stream = self.stream()
        self.assertEqual(stream['entries'], RUNTIME.entries)
        self.assertEqual(stream['total_bytes'], sum(entry['bytes'] for entry in RUNTIME.entries))
        with open(stream['path'], 'rb') as source:
            raw = source.read()
        self.assertTrue(raw.startswith(gi.MAGIC))
        header = json.loads(raw.split(b'\n')[2])
        self.assertEqual(header['entries'], RUNTIME.entries)
        self.assertTrue(raw.endswith(b'END\n'))
        receipt = self.received(stream)
        self.assertEqual(receipt['entries'], RUNTIME.entries)
        self.assertEqual(receipt['context'], NAME)
        self.assertEqual([row['path'] for row in receipt['dirs']],
                         [self.parent + '/' + NAME + '.oci', self.parent + '/' + NAME + '.debs'])
        self.assertEqual(receipt['derived']['total_bytes'], sum(entry['bytes'] for entry in RUNTIME.entries))
        self.assertEqual(receipt['derived']['platform'], 'linux/arm64')
        self.assertIn('blobs/sha256/' + sha(RUNTIME.layer), receipt['dirs'][0]['entries'])
        # The written tree is exactly the reviewed one.
        files = sorted(str(path.relative_to(self.parent + '/' + NAME + '.oci'))
                       for path in Path(self.parent + '/' + NAME + '.oci').rglob('*') if path.is_file())
        self.assertEqual(files, sorted(entry['path'][len('oci/'):] for entry in RUNTIME.entries
                                       if entry['path'].startswith('oci/')))

    def test_the_guest_mode_runs_in_a_real_child_and_agrees_with_the_in_process_receipt(self):
        stream = self.stream()
        with open(stream['path'], 'rb') as source:
            completed = subprocess.run([sys.executable, '-I', '-B', '-c', SOURCE, 'transfer', self.encoded],
                                       stdin=source, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        receipt = json.loads(completed.stdout)
        second = os.path.join(self.tmp, 'second-parent')
        os.mkdir(second, 0o700)
        expected = self.received(stream, parent=second)
        self.assertEqual(receipt['parent'], self.binding)
        self.assertEqual(receipt['entries'], expected['entries'])
        self.assertEqual(receipt['derived'], expected['derived'])
        self.assertEqual([row['entries'] for row in receipt['dirs']],
                         [row['entries'] for row in expected['dirs']])
        self.assertEqual([row['path'] for row in receipt['dirs']],
                         [self.parent + '/' + NAME + '.oci', self.parent + '/' + NAME + '.debs'])

    def test_check_inputs_derives_the_written_layout_again(self):
        self.received()
        receipt = gi.main(['check-inputs', self.encoded], stdout=io.StringIO())
        self.assertEqual(receipt['entries'], RUNTIME.entries)
        self.assertEqual(receipt['derived']['manifest_digest'], RUNTIME.pin['manifest']['digest'])

    def test_an_existing_guest_directory_is_never_adopted(self):
        self.received()
        stream = self.stream(name='again.bin')
        self.assert_refused(lambda: self.received(stream), 'already exists')

    def test_a_source_that_is_a_link_or_a_hard_link_refuses(self):
        entry = RUNTIME.entries[-1]
        path = os.path.join(self.inputs, entry['path'])
        moved = path + '.real'
        os.rename(path, moved)
        os.symlink(moved, path)
        self.assert_refused(lambda: self.stream(name='link.bin'), 'input cannot be opened')
        os.unlink(path)
        os.link(moved, path)
        self.assert_refused(lambda: self.stream(name='hard.bin'), 'private regular file')

    def test_a_missing_or_extra_source_refuses(self):
        entry = RUNTIME.entries[-1]
        sources = self.sources()
        sources.pop(entry['path'])
        self.assert_refused(lambda: gi.write_stream(os.path.join(self.tmp, 'gap.bin'), RUNTIME.entries, sources,
                                                    self.spec['limits']), 'no pinned source')
        os.unlink(os.path.join(self.inputs, entry['path']))
        self.assert_refused(lambda: self.stream(name='gone.bin'), 'cannot be opened')

    def test_a_source_of_the_wrong_size_or_content_refuses(self):
        entry = RUNTIME.entries[-1]
        path = os.path.join(self.inputs, entry['path'])
        original = Path(path).read_bytes()
        Path(path).write_bytes(original + b'x')
        self.assert_refused(lambda: self.stream(name='long.bin'), 'size differs from the reviewed pin')
        Path(path).write_bytes(b'x' * len(original))
        self.assert_refused(lambda: self.stream(name='short.bin'), 'source differs from its pinned sha256')
        Path(path).write_bytes(original)

    def test_a_zero_or_too_small_cap_refuses_before_any_payload_is_written(self):
        self.stream()
        zero = dict(self.spec['limits'], per_input_bytes=0)
        with self.assertRaises(gi.Refusal):
            self.stream(limits=zero, name='zero.bin')
        self.assertFalse(os.path.lexists(os.path.join(self.tmp, 'zero.bin')))
        small = dict(self.spec['limits'], per_input_bytes=RUNTIME.entries[-1]['bytes'] - 1)
        self.assert_refused(lambda: self.stream(limits=small, name='small.bin'), 'per-input cap')
        tight = dict(self.spec['limits'], aggregate_bytes=sum(e['bytes'] for e in RUNTIME.entries) - 1)
        self.assert_refused(lambda: self.stream(limits=tight, name='tight.bin'), 'aggregate cap')
        framing = dict(self.spec['limits'], framing_bytes=16)
        self.assert_refused(lambda: self.stream(limits=framing, name='framing.bin'), 'framing')
        self.assertEqual(sorted(p for p in os.listdir(self.tmp) if p.endswith('.bin')), ['transfer.bin'])

    def test_a_collision_at_the_destination_refuses(self):
        self.stream()
        self.assert_refused(lambda: self.stream(), 'never adopted')


class SpecTests(Fixture):
    def test_a_layout_that_is_not_the_reviewed_pin_refuses(self):
        changed = (dict(self.spec['layout'], debs=self.spec['layout']['debs'][:1]),
                   dict(self.spec['layout'], manifest=dict(self.spec['layout']['manifest'], bytes=1)),
                   dict(self.spec['layout'], index=dict(self.spec['layout']['index'], media_type='x')),
                   dict(self.spec['layout'], extra='x'))
        for layout in changed:
            with self.subTest(layout=sorted(layout)):
                # The reviewed spec is one exact layout and its exact caps: a subset is a different contract.
                with self.assertRaises(gi.Refusal):
                    gi.checked_spec(dict(self.spec, layout=layout))
                if 'extra' in layout or layout['index'].get('media_type') == 'x':
                    with self.assertRaises(gi.Refusal):
                        gi.layout_entries(layout, RUNTIME.index)
        # An upstream index that is not the pinned one refuses before anything is written.
        with self.assertRaises(gi.Refusal):
            gi.layout_entries(self.spec['layout'], RUNTIME.index + b' ')
        with self.assertRaises(gi.Refusal):
            gi.layout_entries(self.spec['layout'], json.dumps({'manifests': []}).encode())

    def test_a_parent_or_name_outside_the_reviewed_shape_refuses(self):
        for binding in ({'path': self.parent}, dict(self.binding, inode='1'), dict(self.binding, uid=-1)):
            with self.subTest(binding=binding), self.assertRaises(gi.Refusal):
                gi.transfer_spec(RUNTIME.pin, binding, NAME)
        for path in (self.parent + '/../x', self.parent + '/child', '/etc'):
            with self.subTest(path=path), self.assertRaises(gi.Refusal):
                gi.open_parent(dict(self.binding, path=path))
        for name in ('effigy-531-image-XYZ', 'effigy-531-image-' + '0' * 31, '../effigy-531-image-' + CONTEXT32):
            with self.subTest(name=name), self.assertRaises(gi.Refusal):
                gi.transfer_spec(RUNTIME.pin, self.binding, name)

    def test_caps_that_are_not_the_reviewed_ones_refuse(self):
        for limits in (dict(self.spec['limits'], per_input_bytes=self.spec['limits']['per_input_bytes'] + 1),
                       dict(self.spec['limits'], aggregate_bytes=gi.AGGREGATE_CAP_BYTES + 1),
                       dict(self.spec['limits'], framing_bytes=0),
                       dict(self.spec['limits'], per_input_bytes=True)):
            with self.subTest(limits=limits), self.assertRaises(gi.Refusal):
                gi.checked_spec(dict(self.spec, limits=limits))

    def test_entry_paths_are_only_the_fixed_layout(self):
        for path in ('../etc/passwd', '/etc/passwd', 'oci/blobs/sha256/keys', 'debs/x.tar', 'oci/other.json',
                     'debs/sub/../../x_1.0_all.deb', 'debs/x_1.0_arm64.deb\n'):
            with self.subTest(path=path), self.assertRaises(gi.Refusal):
                gi.check_path(path)
        for path in (gi.OCI_LAYOUT_PATH, gi.INDEX_PATH, gi.UPSTREAM_PATH, 'debs/x_1.0_all.deb',
                     'oci/blobs/sha256/' + 'a' * 64):
            self.assertIsNone(gi.check_path(path), path)

    def test_a_duplicate_entry_refuses(self):
        entries = [RUNTIME.entries[0], RUNTIME.entries[0]]
        with self.assertRaises(gi.Refusal):
            gi.check_entries(entries, aggregate_bytes=MIB, per_input_bytes=MIB)

    def test_an_empty_entry_refuses(self):
        entry = dict(path='debs/x_1.0_all.deb', bytes=0, sha256='a' * 64)
        with self.assertRaises(gi.Refusal):
            gi.check_entries([entry], aggregate_bytes=MIB, per_input_bytes=MIB)


class ReceiveTests(Fixture):
    def framed(self, entries=None, bodies=None, magic=gi.MAGIC, end=b'END\n'):
        entries = entries if entries is not None else RUNTIME.entries
        bodies = bodies if bodies is not None else [self.body(entry) for entry in entries]
        header = gi.compact(dict(entries=entries, total_bytes=sum(entry['bytes'] for entry in entries)))
        out = io.BytesIO()
        out.write(magic + b'HEADER\n' + header + b'\n')
        for entry, body in zip(entries, bodies):
            out.write(b'DATA %d\n' % entry['bytes'] + body)
        out.write(end)
        out.seek(0)
        return out

    def receive(self, stream):
        return gi.main(['transfer', self.encoded], stdin=stream, stdout=io.StringIO())

    def test_a_truncated_stream_refuses_and_retains_what_arrived(self):
        raw = self.framed().getvalue()
        first = RUNTIME.entries[0]
        header_end = raw.index(b'\n', raw.index(b'HEADER\n') + len(b'HEADER\n')) + 1
        cut = raw[:header_end + len(b'DATA %d\n' % first['bytes']) + 3]
        self.assert_refused(lambda: self.receive(io.BytesIO(cut)), 'ended before its declared size')
        self.assertTrue(os.path.isdir(os.path.join(self.parent, NAME + '.oci')))

    def test_trailing_data_after_end_refuses(self):
        stream = self.framed()
        raw = stream.getvalue() + b'extra\n'
        self.assert_refused(lambda: self.receive(io.BytesIO(raw)), 'data after END')

    def test_a_missing_end_or_wrong_magic_refuses(self):
        self.assert_refused(lambda: self.receive(self.framed(magic=b'NOPE\n')), 'magic differs')
        self.assert_refused(lambda: self.receive(self.framed(end=b'')), 'end')

    def test_a_declared_size_that_differs_from_the_record_refuses(self):
        entries = list(RUNTIME.entries)
        bodies = [self.body(entry) for entry in entries]
        header = gi.compact(dict(entries=entries, total_bytes=sum(entry['bytes'] for entry in entries)))
        raw = (gi.MAGIC + b'HEADER\n' + header + b'\n' + b'DATA %d\n' % (entries[0]['bytes'] + 1)
               + bodies[0])
        self.assert_refused(lambda: self.receive(io.BytesIO(raw)), 'declared size')

    def test_a_payload_that_differs_from_its_sha256_refuses(self):
        bodies = [self.body(entry) for entry in RUNTIME.entries]
        bodies[-1] = b'x' * len(bodies[-1])
        self.assert_refused(lambda: self.receive(self.framed(bodies=bodies)), 'declared sha256')

    def test_a_duplicated_extra_or_missing_entry_refuses(self):
        duplicated = [RUNTIME.entries[0]] + list(RUNTIME.entries)
        self.assert_refused(lambda: self.receive(self.framed(entries=duplicated)), 'repeats a path')
        self.clean()
        missing = list(RUNTIME.entries)[:-1]
        self.assert_refused(lambda: self.receive(self.framed(
            entries=missing, bodies=[self.body(entry) for entry in missing])), 'missing or extra entries')
        self.clean()
        extra = list(RUNTIME.entries) + [dict(path='debs/other_1.0_all.deb', bytes=1, sha256=sha(b'x'))]
        self.assert_refused(lambda: self.receive(self.framed(
            entries=extra, bodies=[self.body(entry) for entry in RUNTIME.entries] + [b'x'])),
            'missing or extra entries')

    def test_the_guest_receive_refuses_a_zero_or_over_cap_bound(self):
        # The reviewed spec caps are canonical, so a tighter receive bound is driven directly into the guest
        # receive, which is what enforces them while copying.
        parent_fd = os.open(self.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            for limits, reason in (
                    (dict(self.spec['limits'], per_input_bytes=0), 'per-input cap'),
                    (dict(self.spec['limits'], aggregate_bytes=sum(e['bytes'] for e in RUNTIME.entries) - 1),
                     'aggregate cap')):
                with self.subTest(limits=limits):
                    spec = dict(self.spec, limits=limits)
                    stream = self.framed()
                    with self.assertRaises(gi.Refusal) as caught:
                        gi.receive(stream, spec, parent_fd, Path(self.parent))
                    self.assertIn(reason, str(caught.exception))
                    for name in (NAME + '.oci', NAME + '.debs'):
                        shutil.rmtree(os.path.join(self.parent, name), ignore_errors=True)
        finally:
            os.close(parent_fd)

    def test_a_foreign_or_replaced_parent_refuses(self):
        other = os.path.join(self.tmp, 'other')
        os.mkdir(other, 0o700)
        info = os.lstat(other)
        spec = gi.transfer_spec(RUNTIME.pin, dict(self.binding, path=other), NAME)
        encoded = gi.spec_bytes(spec).hex()
        with self.assertRaises(gi.Refusal):
            gi.main(['transfer', encoded], stdin=self.framed(), stdout=io.StringIO())
        # A replaced parent (same path, other inode) refuses too.
        moved = self.parent + '.moved'
        os.rename(self.parent, moved)
        os.mkdir(self.parent, 0o700)
        with self.assertRaises(gi.Refusal):
            self.receive(self.framed())

    def test_a_tampered_guest_input_refuses_on_the_pre_build_check(self):
        self.received()
        path = Path(self.parent, NAME + '.oci', 'oci-layout')
        os.chmod(path, 0o600)
        path.write_bytes(b'{"imageLayoutVersion":"9.9.9"}\n')
        self.assert_refused(lambda: gi.main(['check-inputs', self.encoded], stdout=io.StringIO()),
                           'differs from its pinned size or sha256')

    def test_an_extra_guest_input_file_refuses(self):
        self.received()
        extra = Path(self.parent, NAME + '.debs', 'other_1.0_all.deb')
        extra.write_bytes(b'x')
        os.chmod(extra, 0o600)
        self.assert_refused(lambda: gi.main(['check-inputs', self.encoded], stdout=io.StringIO()),
                           'missing or extra entries')

    def test_a_replaced_guest_directory_refuses(self):
        self.received()
        debs = Path(self.parent, NAME + '.debs')
        debs.rename(str(debs) + '.moved')
        os.mkdir(debs, 0o700)
        self.assert_refused(lambda: gi.main(['check-inputs', self.encoded], stdout=io.StringIO()),
                           'missing or extra entries')


class HelperOperationTests(unittest.TestCase):
    """The fixed guest helper's new operations: only the pinned source, the reviewed spec and the exact remote
    commands can run. Nothing here spawns a child; run_contained's containment is covered by the helper controls."""

    def test_the_allowlist_names_every_operation(self):
        self.assertTrue({'echo', 'probe', 'stage', 'verify', 'transfer', 'check-inputs', 'build-help', 'build',
                         'read-iidfile', 'image-inspect'} <= helper.ALLOWED)

    def test_input_modes_take_the_pinned_source_and_the_hex_spec(self):
        spec = json.dumps({'parent': {}, 'name': NAME, 'layout': {}, 'limits': {}}, sort_keys=True,
                          separators=(',', ':')).encode().hex()
        argv = helper.inner_argv('transfer', ['print(1)', spec])
        self.assertEqual(argv, [sys.executable, '-I', '-B', '-c', 'print(1)', 'transfer', spec])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('transfer', [spec])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('transfer', ['print(1)'])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('transfer', ['print(1)', 'zz'])
        # A non-canonical (re-spaced) spec refuses: only the reviewed compact bytes cross.
        spaced = json.dumps({'parent': {}, 'name': NAME, 'layout': {}, 'limits': {}}).encode().hex()
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('transfer', ['print(1)', spaced])

    def test_the_remote_commands_are_rebuilt_and_compared(self):
        context = '/root/' + NAME
        build = helper.build_command(context)
        spec = json.dumps({'argv': build}, separators=(',', ':'), sort_keys=True).encode().hex()
        self.assertEqual(helper.inner_argv('build', [spec]), build)
        # build-help takes the same one-spec shape the executor dispatches, and must name exactly the help argv.
        help_spec = json.dumps({'argv': ['nerdctl', 'build', '--help']},
                               separators=(',', ':'), sort_keys=True).encode().hex()
        self.assertEqual(helper.inner_argv('build-help', [help_spec]), ['nerdctl', 'build', '--help'])
        cat = ['sudo', '-n', 'cat', context + '.iid']
        self.assertEqual(helper.inner_argv('read-iidfile',
                                           [json.dumps({'argv': cat}, separators=(',', ':'),
                                                       sort_keys=True).encode().hex()]), cat)
        image = 'sha256:' + 'ab' * 32
        inspect = ['sudo', '-n', 'nerdctl', 'image', 'inspect', image]
        self.assertEqual(helper.inner_argv('image-inspect',
                                           [json.dumps({'argv': inspect}, separators=(',', ':'),
                                                       sort_keys=True).encode().hex()]), inspect)
        for operation, argv in (('build', [a if a != '--network' else '--net' for a in build]),
                                ('build', build + ['--pull']),
                                ('read-iidfile', ['sudo', '-n', 'cat', '/etc/shadow']),
                                ('image-inspect', ['sudo', '-n', 'nerdctl', 'image', 'inspect',
                                                   'effigy-4f6a-runtime:r1'])):
            payload = json.dumps({'argv': argv}, separators=(',', ':'), sort_keys=True).encode().hex()
            with self.subTest(operation=operation, argv=argv), self.assertRaises(helper.Refusal):
                helper.inner_argv(operation, [payload])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('build-help', ['anything'])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('build-help', [])
        varied_help = json.dumps({'argv': ['nerdctl', 'build', '--help', '--pull']},
                                 separators=(',', ':'), sort_keys=True).encode().hex()
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('build-help', [varied_help])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('transfer', ['print(1)', 'ab'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
