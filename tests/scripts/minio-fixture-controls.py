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
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            env = native.private_environment(root)
            (root / "intent.json").write_text(json.dumps({"token": "owned", "profile": native.PROFILE, "rootIdentity": [root.stat().st_dev, root.stat().st_ino]}))
            config = root / "colima" / native.PROFILE / "colima.yaml"
            config.parent.mkdir()
            config.write_text("owned configuration")
            native.ownership(root, env, "owned")
            config.write_text("foreign configuration")
            with self.assertRaises(package.Refusal):
                native.ownership(root, env, "owned")

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
