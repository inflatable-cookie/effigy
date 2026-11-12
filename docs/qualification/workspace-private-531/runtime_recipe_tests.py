#!/usr/bin/env python3
"""Offline package-order control for the reviewed runtime image recipe (runtime.Dockerfile).

The 775 Desktop build failed because one glob `dpkg -i /debs/*.deb` unpacked python3-minimal before its Pre-Depends
python3.10-minimal was configured, and python3 before python3-minimal. This control reads the real finite package
metadata (fixtures/runtime-deb-control.json, extracted from the retained payloads and sha256-checked against
runtime_image_inputs.DEBS), parses the recipe's own RUN command, and runs its dpkg/dpkg-query segments against a private
fake dpkg whose pre-dependency rule is dpkg's: a package's Pre-Depends must be configured when it is unpacked.

The fake is a model of that one rule plus configure-at-end-of-invocation, not a dpkg replacement. Nothing is installed
on the host, no network is used, and the post-install loader, Python and coreutils checks are asserted as exact argv
but not executed (they name container paths). No native effect.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import runtime_image_inputs as runtime_inputs  # noqa: E402
import macos_preparation as prep  # noqa: E402
import macos_guest_context  # noqa: E402

RECIPE = HERE / 'runtime.Dockerfile'
META = HERE / 'fixtures' / 'runtime-deb-control.json'
# Packages the 22.04 base already provides at the versions the set depends on. Anything else a Depends names is an
# unmet dependency in this model and leaves the package unconfigured, as dpkg would.
BASE = frozenset({'libc6', 'zlib1g', 'libgcc-s1', 'libstdc++6', 'libffi8', 'libbz2-1.0', 'liblzma5', 'libuuid1',
                  'libcrypt1', 'libgssapi-krb5-2', 'dpkg', 'debconf', 'coreutils', 'mime-support'})
# The old recipe's glob order was sorted by file name; the observed 775 failure is on exactly these two packages.
OLD_775_ERRORS = ('python3-minimal', 'python3')
# The fixed stage order: packages listed in one `dpkg -i` must not pre-depend on each other within that invocation.
STAGES = (('python3-minimal',), ('python3',))
FAKE = r'''
import json, os, re, sys
meta = {p['package']: p for p in json.load(open(os.environ['FAKE_DPKG_META']))['packages']}
state_path = os.environ['FAKE_DPKG_STATE']
base = set(json.loads(os.environ['FAKE_DPKG_BASE']))
state = json.load(open(state_path)) if os.path.exists(state_path) else {}
def names(field):
    out = []
    for clause in filter(None, (c.strip() for c in field.split(','))):
        alts = [re.split(r'[ (:\[]', a.strip())[0] for a in clause.split('|')]
        out.append(alts)
    return out
def satisfied(alts):
    return any(state.get(a) == 'installed' or a in base for a in alts)
def save():
    json.dump(state, open(state_path, 'w'), sort_keys=True)
def install(args):
    rc, errors, unpacked = 0, [], []
    for path in args:
        if not os.path.isfile(path):
            sys.stderr.write("dpkg: error: cannot access archive '%s': No such file or directory\n" % path)
            return 2
    for path in args:
        name = os.path.basename(path).split('_', 1)[0]
        pre = names(meta[name]['pre_depends']) if name in meta else []
        if any(not satisfied(alts) for alts in pre):
            missing = [alts[0] for alts in pre if not satisfied(alts)]
            sys.stderr.write('dpkg: regarding %s containing %s, pre-dependency problem:\n' % (path, name))
            for m in missing:
                sys.stderr.write(' %s pre-depends on %s\n  %s is not installed.\n' % (name, m, m))
            sys.stderr.write('dpkg: error processing archive %s (--install):\n pre-dependency problem - not installing %s\n' % (path, name))
            errors.append(path)
            rc = 1
            continue
        state[name] = 'unpacked'
        unpacked.append(name)
    progress = True
    while progress:
        progress = False
        for name in unpacked:
            if state.get(name) == 'unpacked' and all(satisfied(a) for a in names(meta[name]['depends'])):
                state[name] = 'installed'
                progress = True
    for name in unpacked:
        if state.get(name) != 'installed':
            errors.append(name)
            rc = 1
    if errors:
        sys.stderr.write('Errors were encountered while processing:\n')
        for item in errors:
            sys.stderr.write(' %s\n' % item)
    save()
    return rc
def query(args):
    fmt, pkgs = args[1], args[2:]
    if args[0] != '-W' or fmt != "-f=${db:Status-Status}\\n":
        sys.stderr.write('fixture dpkg-query supports only -W -f=${db:Status-Status}\\n\n')
        return 2
    rc = 0
    for name in pkgs:
        if name not in state:
            sys.stderr.write('dpkg-query: no packages found matching %s\n' % name)
            rc = 1
            continue
        sys.stdout.write(state[name] + '\n')
    return rc
prog = sys.argv[1]
argv = sys.argv[2:]
if prog == 'dpkg':
    if argv[:1] != ['-i']:
        sys.stderr.write('fixture dpkg supports only -i\n')
        sys.exit(2)
    sys.exit(install(argv[1:]))
if prog == 'dpkg-query':
    sys.exit(query(argv))
sys.exit(2)
'''


def load_meta():
    return json.loads(META.read_text())


def recipe_run():
    """The recipe's one RUN instruction with its continuations joined; no other instruction is interpreted."""
    text = RECIPE.read_text()
    run = text.split('RUN ', 1)[1].split('\n\n', 1)[0]
    return re.sub(r'\\\n\s*', ' ', run).strip()


def segments():
    return [s.strip() for s in recipe_run().split(' && ')]


def deb_name_of(path):
    return path[len('/debs/'):]


def stage_of_pkg():
    """Stage index each package is installed in, read from the recipe's own dpkg invocations."""
    out = {}
    for index, seg in enumerate(segments()):
        argv = shlex.split(seg)
        if argv[:2] == ['dpkg', '-i']:
            for path in argv[2:]:
                out[path] = index
    return out


class RuntimeRecipeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.meta = load_meta()
        cls.by_name = {p['name']: p for p in cls.meta['packages']}
        cls.by_pkg = {p['package']: p for p in cls.meta['packages']}
        cls.tmp = tempfile.mkdtemp(prefix='runtime-recipe-', dir=os.environ.get('TMPDIR') or None)
        cls.bin = os.path.join(cls.tmp, 'bin')
        os.mkdir(cls.bin, 0o700)
        sim = os.path.join(cls.tmp, 'fake_dpkg.py')
        Path(sim).write_text(FAKE)
        for name in ('dpkg', 'dpkg-query'):
            wrapper = os.path.join(cls.bin, name)
            Path(wrapper).write_text('#!/bin/sh\nexec %s -I %s %s "$@"\n' % (shlex.quote(sys.executable), shlex.quote(sim), name))
            os.chmod(wrapper, 0o700)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def fixture(self, label, omit=()):
        """A private debs directory with the 22 pinned names (sizes only; the control does not read payload bytes),
        and a fresh empty state. omit removes names to model a missing package."""
        root = tempfile.mkdtemp(prefix=label + '-', dir=self.tmp)
        debs = os.path.join(root, 'debs')
        os.mkdir(debs, 0o700)
        for item in self.meta['packages']:
            if item['package'] in omit:
                continue
            Path(debs, item['name']).write_bytes(b'')
        state = os.path.join(root, 'state.json')
        return root, debs, state

    def run_segments(self, debs, state, texts, environment_extra=None):
        """Run the given raw recipe segments in order with &&, with /debs and the fixed dpkg bin pointed at the fixture."""
        script = ' && '.join(t.replace('/debs', debs) for t in texts)
        env = {'PATH': self.bin + ':/usr/bin:/bin', 'LC_ALL': 'C', 'HOME': self.tmp,
               'FAKE_DPKG_META': str(META), 'FAKE_DPKG_STATE': state,
               'FAKE_DPKG_BASE': json.dumps(sorted(BASE))}
        env.update(environment_extra or {})
        proc = subprocess.run(['/bin/sh', '-c', script], env=env, capture_output=True, text=True, timeout=60)
        final = json.loads(Path(state).read_text()) if os.path.exists(state) else {}
        return proc.returncode, proc.stdout, proc.stderr, final

    def test_pins_are_one_recipe_identity_everywhere(self):
        data = RECIPE.read_bytes()
        self.assertEqual(len(data), 1827)
        self.assertEqual(hashlib.sha256(data).hexdigest(), prep.RECIPE_SHA256)
        self.assertEqual(macos_guest_context.RECIPE_SHA256, prep.RECIPE_SHA256)
        import macos_stage_executor as ex
        self.assertEqual((ex.RECIPE_BYTES, ex.RECIPE_SHA256), (len(data), prep.RECIPE_SHA256))
        manifest = json.loads((HERE / 'macos-4f6a5ed3-stages.json').read_text())
        self.assertIn(prep.RECIPE_SHA256, manifest['binding_inputs']['recipe_file'])
        # The historical 531 recipe and the superseded 318-byte 775 recipe are not reviewed inputs any more.
        self.assertEqual(hashlib.sha256((HERE / 'runtime.Dockerfile.531').read_bytes()).hexdigest(),
                         'd290f70febe8ce9ba4cd3cc252a9786d8ee5fc4e82a6219ec0338c884486f514')

    def test_the_metadata_fixture_is_the_pinned_deb_set(self):
        pins = {d['name']: (d['bytes'], d['sha256']) for d in runtime_inputs.DEBS}
        self.assertEqual({p['name']: (p['bytes'], p['sha256']) for p in self.meta['packages']}, pins)
        self.assertEqual(len(self.meta['packages']), 22)

    def test_recipe_installs_exactly_the_pinned_debs_by_explicit_name_without_glob_or_force(self):
        run = recipe_run()
        self.assertNotIn('--force', run)
        self.assertNotIn('--ignore', run)
        self.assertNotIn('||', run)
        self.assertNotRegex(run, r'[*?\[]')  # no glob anywhere in the RUN
        installed = sorted(deb_name_of(path) for path in stage_of_pkg())
        self.assertEqual(installed, sorted(p['name'] for p in self.meta['packages']))
        for seg in segments():
            argv = shlex.split(seg)
            if argv[:1] == ['dpkg']:
                self.assertEqual(argv[:2], ['dpkg', '-i'], seg)

    def test_every_pre_depends_is_installed_in_a_strictly_earlier_invocation(self):
        stage = stage_of_pkg()
        by_pkg_stage = {}
        for path, index in stage.items():
            by_pkg_stage[self.by_name[deb_name_of(path)]['package']] = index
        checked = 0
        for pkg in self.meta['packages']:
            for clause in filter(None, (c.strip() for c in pkg['pre_depends'].split(','))):
                dep = re.split(r'[ (:\[]', clause.split('|')[0].strip())[0]
                if dep in by_pkg_stage:  # in-set pre-dependencies; base ones (libc6) are already configured
                    checked += 1
                    self.assertLess(by_pkg_stage[dep], by_pkg_stage[pkg['package']],
                                    f"{pkg['package']} pre-depends on {dep}")
        self.assertEqual(checked, 2)  # python3-minimal->python3.10-minimal and python3->python3-minimal; libc6 is base

    def test_old_single_glob_reproduces_the_observed_pre_depends_failure(self):
        root, debs, state = self.fixture('old-glob')
        rc, _, err, final = self.run_segments(debs, state, ['dpkg -i /debs/*.deb'])
        self.assertEqual(rc, 1, err)
        for name in OLD_775_ERRORS:
            self.assertIn(f'pre-dependency problem - not installing {name}', err)
        self.assertIn('python3-minimal pre-depends on python3.10-minimal', err)
        self.assertIn('python3 pre-depends on python3-minimal', err)
        self.assertNotEqual(final.get('python3'), 'installed')
        self.assertNotEqual(final.get('python3-minimal'), 'installed')
        self.assertEqual(final.get('python3.10-minimal'), 'installed')  # later configuration does not undo the failure
        self.assertIn('Errors were encountered while processing:', err)

    def test_fixed_order_installs_all_22_configured_and_passes_the_completeness_check(self):
        segs = segments()
        root, debs, state = self.fixture('fixed-order')
        rc, _, err, final = self.run_segments(debs, state, segs[:3] + [segs[4]])
        self.assertEqual(rc, 0, err)
        self.assertEqual(sorted(final), sorted(p['package'] for p in self.meta['packages']))
        self.assertTrue(all(v == 'installed' for v in final.values()), final)
        self.assertNotIn('pre-dependency problem', err)

    def test_missing_package_stops_at_its_first_failing_invocation(self):
        segs = segments()
        root, debs, state = self.fixture('missing-minimal', omit=('python3-minimal',))
        rc, _, err, final = self.run_segments(debs, state, segs[:3] + [segs[4]])
        self.assertNotEqual(rc, 0)
        self.assertIn("cannot access archive '/debs/python3-minimal", err.replace(debs, '/debs'))
        self.assertNotIn('python3', final)  # the chain stopped: the python3 invocation never ran
        self.assertEqual(final.get('python3.10-minimal'), 'installed')

    def test_a_stage_one_failure_stops_before_any_dependent_stage(self):
        segs = segments()
        root, debs, state = self.fixture('missing-stage-one', omit=('libmpdec3',))
        rc, _, err, final = self.run_segments(debs, state, segs[:3] + [segs[4]])
        self.assertNotEqual(rc, 0)
        self.assertNotIn('python3-minimal', final)
        self.assertNotIn('python3', final)

    def test_completeness_check_refuses_a_partial_installed_state(self):
        segs = segments()
        root, debs, state = self.fixture('partial')
        rc, _, _, _ = self.run_segments(debs, state, segs[:3])
        self.assertEqual(rc, 0)
        partial = json.loads(Path(state).read_text())
        partial['python3'] = 'unpacked'
        Path(state).write_text(json.dumps(partial))
        rc, _, _, _ = self.run_segments(debs, state, [segs[4]])
        self.assertNotEqual(rc, 0)  # 21 installed lines: the count gate refuses the partial state

    def test_post_install_validation_tail_is_the_exact_fixed_argv(self):
        # Named container paths: asserted as argv, never executed on the host. stat and printenv redirect to /dev/null.
        tail = [shlex.split(s) for s in segments()[5:]]
        self.assertEqual(len(tail), 4)
        self.assertEqual(tail[0], ['test', '-x', '/lib/ld-linux-aarch64.so.1'])
        self.assertEqual(tail[1], ['python3', '-c', 'import ssl; import os; assert os.getuid() == 0'])
        self.assertEqual(tail[2][:2], ['/usr/bin/stat', '--version'])
        self.assertEqual(tail[3][:2], ['/usr/bin/printenv', '--version'])

    def test_count_check_names_each_package_once_and_expects_the_full_set(self):
        seg = segments()[4]
        self.assertTrue(seg.startswith('test "$(dpkg-query -W -f=\'${db:Status-Status}\\n\' '), seg)
        names = re.findall(r'\s([a-z0-9][a-z0-9.+-]*)(?=\s)', seg.split('|', 1)[0])
        self.assertEqual(sorted(names), sorted(p['package'] for p in self.meta['packages']))
        self.assertTrue(seg.endswith("| grep -c '^installed$')\" = 22"), seg)


if __name__ == '__main__':
    unittest.main(verbosity=2)
