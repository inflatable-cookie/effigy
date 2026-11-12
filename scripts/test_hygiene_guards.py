#!/usr/bin/env python3
"""Owned synthetic-tree proofs for the Cargo-route and mktemp detectors.

Each case copies the exact committed guard (SHA-256 matched) into a private
repo under TMPDIR and runs that copy. Production `qa:docs:cargo-routes` and
`qa:docs:mktemp` stay the live-tree checks; this file does not scan or write
the real checkout.
"""

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

SOURCE_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARGO_GUARD = "check-cargo-routes.py"
MKTEMP_GUARD = "check-mktemp.py"


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        digest.update(handle.read())
    return digest.hexdigest()


def owned_repo():
    scratch = os.environ.get("TMPDIR")
    if not scratch:
        raise RuntimeError("TMPDIR is required so fixtures stay on the run scratch")
    root = tempfile.mkdtemp(prefix="effigy-hygiene-guard.", dir=scratch)
    os.makedirs(os.path.join(root, "scripts"))
    os.makedirs(os.path.join(root, "config"))
    os.makedirs(os.path.join(root, "tests", "scripts"))
    os.makedirs(os.path.join(root, "crates"))
    os.makedirs(os.path.join(root, "docs", "knowledge"))
    return root


def install_guard(repo, name):
    src = os.path.join(SOURCE_ROOT, "scripts", name)
    dest = os.path.join(repo, "scripts", name)
    shutil.copyfile(src, dest)
    if sha256_file(src) != sha256_file(dest):
        raise AssertionError("%s copy hash does not match the committed guard" % name)
    return dest


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


def write_record(repo, routes):
    lines = [
        "# Cargo routes\n\n",
        "| Route | Kind | Rule | Reason |\n",
        "| --- | --- | --- | --- |\n",
    ]
    for route in routes:
        lines.append("| `%s` | genuine exception | 8 | fixture |\n" % route)
    write(os.path.join(repo, "docs", "knowledge", "cargo-routes.md"), "".join(lines))


def run_guard(repo, name):
    script = os.path.join(repo, "scripts", name)
    return subprocess.run(
        [sys.executable, script],
        cwd=repo,
        capture_output=True,
        text=True,
        env=os.environ.copy(),
    )


class CargoRouteGuardTests(unittest.TestCase):
    def tearDown(self):
        root = getattr(self, "repo", None)
        if root and os.path.isdir(root):
            shutil.rmtree(root, ignore_errors=True)

    def _repo(self, catalog, routes=()):
        self.repo = owned_repo()
        install_guard(self.repo, CARGO_GUARD)
        write(os.path.join(self.repo, "effigy.toml"), catalog)
        write_record(self.repo, routes)
        return self.repo

    def test_unrecorded_plain_cargo_fails(self):
        repo = self._repo('[tasks]\n"plain.task" = "cargo test"\n')
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("unrecorded plain-Cargo route: plain.task", result.stdout)

    def test_stale_record_fails(self):
        repo = self._repo("[tasks]\nok = \"echo hi\"\n", routes=["gone.task"])
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("recorded route no longer runs plain Cargo: gone.task",
                      result.stdout)

    def test_recorded_live_route_passes(self):
        repo = self._repo('[tasks]\n"plain.task" = "cargo test"\n',
                          routes=["plain.task"])
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("[ok] 1 plain-Cargo routes", result.stdout)

    def test_oxide_exec_nested_cargo_is_not_a_route(self):
        repo = self._repo(
            '[tasks]\n"ok.oxide" = "sh scripts/oxide-selector.sh exec -- cargo metadata"\n'
        )
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("[ok] 0 plain-Cargo routes", result.stdout)

    def test_escape_is_not_a_route(self):
        repo = self._repo(
            '[tasks]\n"ok.escape" = "EFFIGY_OXIDE_ESCAPE=cargo cargo test"\n'
        )
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("[ok] 0 plain-Cargo routes", result.stdout)

    def test_comments_and_environment_variables_are_not_routes(self):
        repo = self._repo(
            "# cargo test\n[tasks]\n"
            "ok.comment = \"echo hi\"\n"
            'ok.env = "$CARGO test"\n'
        )
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("[ok] 0 plain-Cargo routes", result.stdout)

    def test_include_list_is_traversed(self):
        repo = self._repo('include = ["config/extra.toml"]\n')
        write(os.path.join(repo, "config", "extra.toml"),
              '[tasks]\n"hidden:cargo" = "cargo test"\n')
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("unrecorded plain-Cargo route: hidden:cargo", result.stdout)
        self.assertIn("config/extra.toml:", result.stdout)

    def test_nested_script_cargo_is_a_route(self):
        repo = self._repo('[tasks]\n"uses.script" = "sh scripts/nested.sh"\n')
        write(os.path.join(repo, "scripts", "nested.sh"), "#!/bin/sh\ncargo test\n")
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("unrecorded plain-Cargo route: scripts/nested.sh",
                      result.stdout)

    def test_section_qualified_gate_key(self):
        repo = self._repo(
            "[release.gates]\nformat = \"cargo fmt --all -- --check\"\n"
        )
        result = run_guard(repo, CARGO_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("unrecorded plain-Cargo route: release.gates.format",
                      result.stdout)


class MktempGuardTests(unittest.TestCase):
    def tearDown(self):
        root = getattr(self, "repo", None)
        if root and os.path.isdir(root):
            shutil.rmtree(root, ignore_errors=True)

    def _repo(self):
        self.repo = owned_repo()
        install_guard(self.repo, MKTEMP_GUARD)
        return self.repo

    def test_guard_source_and_tmpdir_template_pass(self):
        repo = self._repo()
        write(
            os.path.join(repo, "tests", "scripts", "ok.sh"),
            'fixture=$(mktemp -d "${TMPDIR:-/tmp}/effigy-fixture.XXXXXX")\n',
        )
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('every harness `mktemp` honours TMPDIR', result.stdout)

    def test_bare_mktemp_fails(self):
        repo = self._repo()
        write(os.path.join(repo, "tests", "scripts", "bare.sh"), "mktemp -d\n")
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("has no template", result.stdout)
        self.assertIn("tests/scripts/bare.sh:1", result.stdout)

    def test_hardcoded_tmp_template_fails(self):
        repo = self._repo()
        write(
            os.path.join(repo, "tests", "scripts", "host.sh"),
            "mktemp -d /tmp/effigy-fixture.XXXXXX\n",
        )
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("ignores TMPDIR", result.stdout)
        self.assertIn("tests/scripts/host.sh:1", result.stdout)

    def test_template_without_placeholder_fails(self):
        repo = self._repo()
        write(
            os.path.join(repo, "tests", "scripts", "nopat.sh"),
            'mktemp -d "${TMPDIR:-/tmp}/effigy-fixture"\n',
        )
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("has no `XXXXXX`", result.stdout)

    def test_guest_temp_exemption_accepts_both_supported_placements(self):
        for placement, source in (
            ("same line", "native_dir=$(mktemp -d)  # mktemp-exempt: container guest temp\n"),
            ("previous line", "# mktemp-exempt: container guest temp\nmktemp -d\n"),
        ):
            with self.subTest(placement=placement):
                repo = self._repo()
                try:
                    write(os.path.join(repo, "tests", "scripts", "guest.sh"), source)
                    result = run_guard(repo, MKTEMP_GUARD)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                finally:
                    shutil.rmtree(repo, ignore_errors=True)
                    self.repo = None

    def test_string_mention_is_not_a_call(self):
        repo = self._repo()
        write(
            os.path.join(repo, "tests", "scripts", "note.sh"),
            'echo "please run mktemp"\n',
        )
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_quoted_substitution_openers_without_templates_fail(self):
        cases = (
            ("dollar", 'dir="$(mktemp -d)"\n'),
            ("backtick", 'dir="`mktemp -d`"\n'),
        )
        for opener, source in cases:
            with self.subTest(opener=opener):
                repo = self._repo()
                try:
                    path = os.path.join(repo, "tests", "scripts", "quoted.sh")
                    write(path, source)
                    result = run_guard(repo, MKTEMP_GUARD)
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn("has no template", result.stdout)
                    self.assertIn("tests/scripts/quoted.sh:1", result.stdout)
                finally:
                    shutil.rmtree(repo, ignore_errors=True)
                    self.repo = None

    def test_quoted_substitution_checks_tmpdir_template_policy(self):
        cases = (
            ("hardcoded host path", 'dir="$(mktemp -d /tmp/effigy-fixture.XXXXXX)"\n', 1,
             "ignores TMPDIR"),
            ("TMPDIR template", 'dir="$(mktemp -d "${TMPDIR:-/tmp}/effigy-fixture.XXXXXX")"\n', 0,
             'every harness `mktemp` honours TMPDIR'),
        )
        for label, source, expected_status, expected_message in cases:
            with self.subTest(case=label):
                repo = self._repo()
                try:
                    write(os.path.join(repo, "tests", "scripts", "quoted.sh"), source)
                    result = run_guard(repo, MKTEMP_GUARD)
                    self.assertEqual(result.returncode, expected_status, result.stdout + result.stderr)
                    self.assertIn(expected_message, result.stdout)
                    if expected_status:
                        self.assertIn("tests/scripts/quoted.sh:1", result.stdout)
                finally:
                    shutil.rmtree(repo, ignore_errors=True)
                    self.repo = None

    def test_literals_and_escaped_substitution_openers_are_not_calls(self):
        cases = (
            ("single-quoted literal", "echo 'mktemp -d'\n"),
            ("dollar", 'echo "\\$(mktemp -d)"\n'),
            ("backtick", 'echo "\\`mktemp -d\\`"\n'),
        )
        for opener, source in cases:
            with self.subTest(opener=opener):
                repo = self._repo()
                try:
                    write(os.path.join(repo, "tests", "scripts", "escaped.sh"), source)
                    result = run_guard(repo, MKTEMP_GUARD)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                finally:
                    shutil.rmtree(repo, ignore_errors=True)
                    self.repo = None

    def test_crate_tests_scripts_are_scanned(self):
        repo = self._repo()
        write(
            os.path.join(repo, "crates", "demo", "tests", "scripts", "host.sh"),
            "mktemp -d\n",
        )
        result = run_guard(repo, MKTEMP_GUARD)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("crates/demo/tests/scripts/host.sh:1", result.stdout)
        self.assertIn("has no template", result.stdout)


if __name__ == "__main__":
    unittest.main()
