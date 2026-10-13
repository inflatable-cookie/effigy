#!/usr/bin/env python3
"""Offline provenance, archive, secrecy and lifecycle refusal counterexamples."""
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/qualification"))
import minio_fixture_package as package
import minio_fixture_native as native


def elf():
    data = bytearray(64)
    data[:6] = b"\x7fELF\x02\x01"
    data[18:20] = (183).to_bytes(2, "little")
    return bytes(data)


def archive(entries):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w:gz") as target:
        for name, content, kind in entries:
            item = tarfile.TarInfo(name)
            item.type, item.size = kind, len(content)
            target.addfile(item, io.BytesIO(content))
    return stream.getvalue()


class Controls(unittest.TestCase):
    def record_intent(self, root, env):
        control = Path(env["LIMA_HOME"]).parent
        (root / "intent.json").write_text(json.dumps({
            "token": "owned", "profile": native.PROFILE,
            "rootIdentity": [root.stat().st_dev, root.stat().st_ino],
            "controlRoot": str(control),
            "controlRootIdentity": [control.stat().st_dev, control.stat().st_ino],
            "controlDirectoryIdentities": {key: [Path(env[key]).stat().st_dev, Path(env[key]).stat().st_ino]
                                           for key in native.CONTROL_DIRECTORIES},
        }))

    def test_client_exactly_one_bounded_non_escaping_elf(self):
        self.assertEqual(package.client_binary(archive([("mcli", elf(), tarfile.REGTYPE)])), elf())
        for entries in [[("../mcli", elf(), tarfile.REGTYPE)], [("/mcli", elf(), tarfile.REGTYPE)],
                        [("mc", b"", tarfile.SYMTYPE)], [("mc", elf(), tarfile.REGTYPE), ("mcli", elf(), tarfile.REGTYPE)],
                        [("mc", b"wrong platform", tarfile.REGTYPE)]]:
            with self.assertRaises(package.Refusal):
                package.client_binary(archive(entries))

    def test_layer_deterministic_and_verified_before_use(self):
        first = package.make_layer({"usr/local/bin/minio": elf(), "usr/local/bin/mc": elf()})
        self.assertEqual(first, package.make_layer({"usr/local/bin/mc": elf(), "usr/local/bin/minio": elf()}))
        raw = package.unpacked_layer(*first)
        self.assertEqual(package.digest(raw), first[1])
        with self.assertRaises(package.Refusal):
            package.unpacked_layer(first[0], "sha256:" + "0" * 64)
        with patch.object(package, "MAX_LAYER", 32), self.assertRaises(package.Refusal):
            package.unpacked_layer(first[0], first[1])

    def test_content_mutation_refuses_without_network(self):
        with tempfile.TemporaryDirectory() as folder:
            file = Path(folder) / "content"
            file.write_bytes(b"mutated")
            with self.assertRaises(package.Refusal):
                package.acquire("https://example.invalid/input", file, package.digest(b"correct"), 7)
            with self.assertRaises(package.Refusal):
                package.acquire("https://credential:secret@example.invalid/input", file, package.digest(b"correct"), 7)

    def test_redirect_does_not_forward_anonymous_token(self):
        from urllib.request import Request
        request = Request("https://registry.example/a", headers={"Authorization": "Bearer private-token"})
        redirected = package.PublicRedirect().redirect_request(request, None, 302, "", {}, "https://cdn.example/b")
        self.assertIsNone(redirected.get_header("Authorization"))
        with self.assertRaises(package.Refusal):
            package.PublicRedirect().redirect_request(request, None, 302, "", {}, "http://cdn.example/b")

    def test_existing_attempt_refuses_before_vm_inventory(self):
        with tempfile.TemporaryDirectory() as root, patch.object(native, "verify", return_value={}), patch.object(native, "profile_snapshot") as inventory:
            with self.assertRaises(package.Refusal):
                native.run(Path(root) / "unused", Path(root))
            inventory.assert_not_called()

    def test_existing_profile_refuses_before_private_root_creation(self):
        with tempfile.TemporaryDirectory() as root, patch.object(native, "verify", return_value={}), patch.object(native, "profile_snapshot", return_value=[{"name": native.PROFILE}]):
            target = Path(root) / "attempt"
            with self.assertRaises(package.Refusal):
                native.run(Path(root) / "unused", target)
            self.assertFalse(target.exists())

    def test_configuration_change_refuses_cleanup_authority(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            control = Path(folder).resolve()
            root = control / "receipts"
            root.mkdir(mode=0o700)
            env = native.private_environment(control)
            self.record_intent(root, env)
            config = Path(env["COLIMA_HOME"]) / native.PROFILE / "colima.yaml"
            config.parent.mkdir()
            config.write_text("owned configuration")
            native.ownership(root, env, "owned")
            config.write_text("foreign configuration")
            with self.assertRaises(package.Refusal):
                native.ownership(root, env, "owned")

    def test_long_socket_path_refuses_before_control_directories(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / ("deep-evidence-" * 12)
            with self.assertRaisesRegex(package.Refusal, "socket path limit"):
                native.private_environment(root)
            self.assertFalse(root.exists())

    def test_socket_limit_counts_resolved_bytes_and_ssh_suffix(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            root = Path(folder).resolve()
            tail = "/lima/colima-" + native.PROFILE + "/" + native.LONGEST_LIMA_SOCKET
            remaining = native.MACOS_SOCKET_LIMIT - len(os.fsencode(str(root) + "/" + tail))
            valid = root / ("a" * max(1, remaining - 1))
            native.validate_control_paths(valid)
            with self.assertRaisesRegex(package.Refusal, "socket path limit"):
                native.validate_control_paths(root / ("a" * remaining))
            unicode_root = root / ("é" * max(1, remaining))
            with self.assertRaisesRegex(package.Refusal, "socket path limit"):
                native.validate_control_paths(unicode_root)

    def test_control_root_symlink_is_not_a_short_path_escape(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            root = Path(folder).resolve()
            target = root / ("long-evidence-" * 12)
            target.mkdir()
            alias = root / "l"
            alias.symlink_to(target, target_is_directory=True)
            with self.assertRaisesRegex(package.Refusal, "symlink"):
                native.private_environment(alias)
            self.assertEqual(list(target.iterdir()), [])

    def test_deep_receipt_root_keeps_short_controls_and_ownership(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            base = Path(folder).resolve()
            root = base / ("durable-evidence-" * 8)
            control = base / "c"
            root.mkdir(mode=0o700)
            control.mkdir(mode=0o700)
            env = native.private_environment(control)
            self.record_intent(root, env)
            self.assertEqual(native.control_ownership(root, env, "owned"), control)
            self.assertFalse((root / "lima").exists())
            socket = Path(env["LIMA_HOME"]) / ("colima-" + native.PROFILE) / native.LONGEST_LIMA_SOCKET
            self.assertLess(len(os.fsencode(socket)), native.MACOS_SOCKET_LIMIT)

    def test_replaced_control_directory_refuses_cleanup(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            control = Path(folder).resolve()
            root = control / "receipts"
            root.mkdir(mode=0o700)
            env = native.private_environment(control)
            self.record_intent(root, env)
            directory = Path(env["DOCKER_CONFIG"])
            directory.rename(control / "original-docker")
            directory.mkdir(mode=0o700)
            sentinel = directory / "foreign"
            sentinel.write_text("preserved")
            with self.assertRaisesRegex(package.Refusal, "directory identity changed"):
                native.control_ownership(root, env, "owned")
            self.assertEqual(sentinel.read_text(), "preserved")

    def test_control_permissions_change_refuses_cleanup(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            control = Path(folder).resolve()
            root = control / "receipts"
            root.mkdir(mode=0o700)
            env = native.private_environment(control)
            self.record_intent(root, env)
            Path(env["LIMA_HOME"]).chmod(0o755)
            with self.assertRaisesRegex(package.Refusal, "permissions changed"):
                native.control_ownership(root, env, "owned")

    def test_startup_category_preserves_diagnosis_without_raw_output(self):
        native.JOURNAL.clear()
        program = "import sys; print('private-credential instance name too long: UNIX_PATH_MAX=104',file=sys.stderr); sys.exit(1)"
        code, body = native.bounded([sys.executable, "-c", program], dict(os.environ),
                                    time.monotonic() + 3, classify_startup=True)
        self.assertEqual(code, 1)
        self.assertEqual(body, b"")
        self.assertEqual(native.JOURNAL[-1]["diagnosticCategory"], "unix_socket_path_limit")
        self.assertNotIn("private-credential", json.dumps(native.JOURNAL))
        self.assertEqual(native.startup_category(b"private-credential unknown boot error"), "unclassified")

    def test_public_entry_startup_failure_settles_only_its_short_controls(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            base = Path(folder).resolve()
            root = base / ("durable-evidence-" * 8)
            engine = base / "fake-colima"
            engine.write_text("#!" + sys.executable + "\n" + '''
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
home = pathlib.Path(os.environ.get("COLIMA_HOME", "/unused"))
private = home.name == "colima" and home.parent.name.startswith("efm-")
profile = "effigy-minio-fixture-dfefc279"
if args[0] == "version":
    print("colima version 0.10.3 git commit 00f6c297")
elif args[0] == "list":
    print(json.dumps(([{"name": profile}] if (home / profile).exists() else [])
                     if private else [{"name": "foreign"}]))
elif args[0] == "start":
    assert private
    path = home / profile
    path.mkdir()
    (path / "colima.yaml").write_text("owned fake configuration")
    print("private-credential instance name too long: UNIX_PATH_MAX=104", file=sys.stderr)
    sys.exit(1)
elif args[0] == "stop":
    assert private and (home / profile).exists()
elif args[0] == "delete":
    assert private and args[1:] == [profile, "--force", "--data"]
    shutil.rmtree(home / profile)
else:
    raise AssertionError("unexpected engine operation")
''')
            engine.chmod(0o700)
            with patch.object(native, "COLIMA", str(engine)), patch.object(native, "verify", return_value={"artifactSHA256": "pinned"}):
                result = native.run(base / "unused-artifact", root)
            self.assertFalse(result["qualified"])
            self.assertEqual(result["nativeBatch"], "spent")
            self.assertEqual(result["cleanup"], "settled")
            self.assertTrue(result["controlRootRemoved"])
            self.assertEqual(result["startupFailure"], {"exit": 1, "category": "unix_socket_path_limit"})
            self.assertNotIn("private-credential", json.dumps(result))
            self.assertFalse(Path(result["controlRoot"]).exists())
            self.assertTrue((root / "intent.json").is_file())
            self.assertTrue((root / "result.json").is_file())
            self.assertTrue(engine.exists())

    def test_partial_control_setup_retains_owned_path_without_vm_start(self):
        with tempfile.TemporaryDirectory(prefix="efm-", dir="/tmp") as folder:
            base = Path(folder).resolve()
            root = base / "receipts"
            control = base / "control"
            control.mkdir(mode=0o700)
            def partial_setup(path):
                (path / "colima").mkdir(mode=0o700)
                raise OSError("private filesystem detail")
            with patch.object(native, "verify", return_value={}), patch.object(native, "profile_snapshot", return_value=[]), \
                    patch.object(native, "bounded", return_value=(0, b"0.10.3 00f6c297")) as commands, \
                    patch.object(native.tempfile, "mkdtemp", return_value=str(control)), \
                    patch.object(native, "private_environment", side_effect=partial_setup):
                result = native.run(base / "unused", root)
            self.assertEqual(commands.call_count, 1)  # Identity only; no startup.
            self.assertEqual(result["nativeBatch"], "not_started")
            self.assertEqual(result["cleanup"], "held")
            self.assertEqual(result["controlRoot"], str(control))
            self.assertTrue(control.exists())
            self.assertNotIn("private filesystem detail", json.dumps(result))
            self.assertEqual(json.loads((root / "result.json").read_text()), result)

    def test_unlisted_existing_state_refuses_before_effects(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            state = root / native.PROFILE
            state.mkdir()
            with self.assertRaises(package.Refusal):
                native.refuse_existing_state({"COLIMA_HOME": str(root), "LIMA_HOME": str(root / "lima")})

    def test_eof_does_not_bypass_exit_deadline(self):
        started = time.monotonic()
        with self.assertRaisesRegex(package.Refusal, "exit deadline"):
            native.bounded([sys.executable, "-c", "import os,time; os.close(1); os.close(2); time.sleep(30)"], dict(os.environ), started + 0.3)
        self.assertLess(time.monotonic() - started, 3)

    def test_flood_is_bounded_and_secrets_not_in_error(self):
        native.JOURNAL.clear()
        with patch.object(native, "MAX_OUTPUT", 1024), self.assertRaises(package.Refusal) as captured:
            native.bounded([sys.executable, "-c", "import os; os.write(2, b'private-credential'*1000)"], dict(os.environ), time.monotonic() + 3)
        self.assertNotIn("private-credential", str(captured.exception))
        self.assertTrue(native.JOURNAL[-1]["reaped"])

    def test_zombie_only_permission_refusal_needs_positive_reap(self):
        native.JOURNAL.clear()
        with patch.object(native, "MAX_OUTPUT", 1024), patch.object(native.os, "killpg", side_effect=PermissionError), self.assertRaises(package.Refusal):
            native.bounded([sys.executable, "-c", "import os; os.write(2,b'x'*2048)"], dict(os.environ), time.monotonic() + 3)
        self.assertTrue(native.JOURNAL[-1]["reaped"])
        self.assertTrue(native.JOURNAL[-1]["signalRefusalResolvedByExactQuiescence"])

    def test_unreaped_root_is_retained_until_owned_descendant_settles(self):
        native.JOURNAL.clear()
        program = "import os,time; descendant=os.fork();\nif descendant==0:\n os.close(1); os.close(2); time.sleep(30); os._exit(0)\nos.write(2,b'x'*2048); os._exit(0)"
        with patch.object(native, "MAX_OUTPUT", 1024), self.assertRaises(package.Refusal):
            native.bounded([sys.executable, "-c", program], dict(os.environ), time.monotonic() + 3)
        self.assertTrue(native.JOURNAL[-1]["reaped"])
        self.assertFalse(native.HELD_CHILDREN)

    def test_stderr_is_counted_but_not_mixed_into_observations(self):
        code, body = native.bounded([sys.executable, "-c", "import sys; print('{}'); print('private-secret',file=sys.stderr)"], dict(os.environ), time.monotonic() + 3)
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(body), {})
        self.assertNotIn(b"private-secret", body)


if __name__ == "__main__":
    unittest.main()
