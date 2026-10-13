#!/usr/bin/env python3
"""One independently reviewed, private Colima lifecycle; never adopts a VM."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import selectors
import shutil
import signal
import subprocess
import tempfile
import time

from minio_fixture_package import PROFILE, Refusal, verify

COLIMA = "/opt/homebrew/bin/colima"
START_FLAGS = ["--runtime", "containerd", "--arch", "aarch64", "--vm-type", "vz",
               "--cpu", "4", "--memory", "4", "--disk", "32", "--root-disk", "20",
               "--activate=false", "--ssh-config=false", "--ssh-agent=false", "--mount", "none",
               "--template=false", "--kubernetes=false", "--binfmt=false", "--vz-rosetta=false",
               "--network-address=false", "--network-host-addresses=false", "--port-forwarder", "none"]
MAX_OUTPUT = 1048576
MAX_PROBE_OUTPUT = 1048576
JOURNAL = []
HELD_CHILDREN = []
CONTROL_DIRECTORIES = {"COLIMA_HOME": "colima", "COLIMA_CACHE_HOME": "cache",
                       "LIMA_HOME": "lima", "DOCKER_CONFIG": "docker"}
LONGEST_LIMA_SOCKET = "ssh.sock.1234567890123456"
MACOS_SOCKET_LIMIT = 104


def validate_control_paths(root):
    # Lima resolves LIMA_HOME before joining the instance and SSH temp socket.
    # A short symlink to a long evidence root therefore cannot fix this limit.
    if root.is_symlink():
        raise Refusal("fixture control root is a symlink; no adoption")
    socket = root.resolve() / "lima" / ("colima-" + PROFILE) / LONGEST_LIMA_SOCKET
    if len(os.fsencode(socket)) >= MACOS_SOCKET_LIMIT:
        raise Refusal("fixture Unix socket path limit; refused before engine effects")


def startup_category(output):
    # Classify only a fixed public tool error; never retain the diagnostic bytes.
    if b"UNIX_PATH_MAX" in output and b"too long" in output:
        return "unix_socket_path_limit"
    return "unclassified"


def owned_group_quiescent(child, deadline):
    """Completion evidence only; the unreaped Child is the signal authority.

    The fixed system ps command has no child-producing operation. Its output is
    drained with the same byte bound; the probe cannot authorize other signals.
    """
    probe = subprocess.Popen(["/bin/ps", "-axo", "pid=,pgid=,stat="],
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, start_new_session=True)
    data = bytearray()
    reader = selectors.DefaultSelector()
    os.set_blocking(probe.stdout.fileno(), False)
    reader.register(probe.stdout, selectors.EVENT_READ)
    try:
        while reader.get_map():
            if time.monotonic() >= deadline:
                raise Refusal("owned group completion observation deadline")
            for key, _ in reader.select(min(0.05, max(0, deadline - time.monotonic()))):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    reader.unregister(key.fileobj)
                elif len(data) + len(chunk) > MAX_PROBE_OUTPUT:
                    raise Refusal("owned group completion observation overflow")
                else:
                    data.extend(chunk)
        try:
            code = probe.wait(timeout=max(0.001, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            raise Refusal("owned group completion observer exit deadline") from None
        require(code, "owned group completion observation")
        rows = []
        for line in data.splitlines():
            fields = line.split()
            if len(fields) != 3 or not fields[0].isdigit() or not fields[1].isdigit():
                raise Refusal("owned group completion observation malformed")
            rows.append((int(fields[0]), int(fields[1]), fields[2]))
        root = [row for row in rows if row[0] == child.pid]
        members = [row for row in rows if row[1] == child.pid]
        # The exited root cannot fork. No other member may remain at its reap.
        return (len(root) == 1 and root[0][1] == child.pid and root[0][2].startswith(b"Z")
                and len(members) == 1 and members[0][0] == child.pid)
    finally:
        reader.close()
        probe.stdout.close()
        if probe.returncode is None:
            # Only the directly owned, fixed non-forking OS probe; never a
            # PID/status-derived target or a second signal to the main group.
            try:
                os.kill(probe.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            try:
                probe.wait(timeout=min(0.2, max(0, deadline - time.monotonic())))
            except subprocess.TimeoutExpired:
                HELD_CHILDREN.append(probe)


def reap_owned_group(child, deadline):
    while time.monotonic() < deadline:
        if owned_group_quiescent(child, min(deadline, time.monotonic() + 0.5)):
            return child.wait(timeout=max(0, deadline - time.monotonic()))
        time.sleep(min(0.02, max(0, deadline - time.monotonic())))
    raise Refusal("owned host group settlement unresolved; child retained")


def bounded(argv, env, deadline, input_file=None, classify_startup=False):
    """Drain both streams with a collection bound; argv/output are never logged.

    An unreaped owned session leader pins its process group through cancellation.
    The separate two-second host reap grace does not assert VM cancellation.
    """
    if time.monotonic() >= deadline:
        raise Refusal("phase deadline exhausted before launch")
    child = subprocess.Popen(argv, env=env, stdin=input_file or subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    entry = {"ownedHostPID": child.pid, "startedMonotonic": time.monotonic(), "deadlineMonotonic": deadline}
    JOURNAL.append(entry)
    output = bytearray()
    diagnostic = bytearray()
    collected = 0
    failure = None
    selector = selectors.DefaultSelector()
    for pipe in (child.stdout, child.stderr):
        os.set_blocking(pipe.fileno(), False)
        selector.register(pipe, selectors.EVENT_READ)
    try:
        while selector.get_map():
            if time.monotonic() >= deadline:
                failure = "phase deadline reached"
                break
            for key, _ in selector.select(min(0.1, max(0, deadline - time.monotonic()))):
                data = os.read(key.fileobj.fileno(), 65536)
                if not data:
                    selector.unregister(key.fileobj)
                elif collected + len(data) > MAX_OUTPUT:
                    failure = "cumulative command output bound exceeded"
                    break
                else:
                    collected += len(data)
                    if classify_startup:
                        diagnostic.extend(data)
                    if key.fileobj is child.stdout:
                        output.extend(data)
            if failure:
                break
        if not failure:
            try:
                code = child.wait(timeout=max(0.001, deadline - time.monotonic()))
                entry.update({"exit": code, "collectedBytes": collected, "reaped": True})
                if classify_startup and code != 0:
                    entry["diagnosticCategory"] = startup_category(diagnostic)
                return code, bytes(output)
            except subprocess.TimeoutExpired:
                failure = "command exit deadline reached after EOF"
        # No wait/poll has reaped the root. Signal only this recorded child session.
        reap_deadline = time.monotonic() + 2
        signal_refusal = False
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass  # The unreaped child may already have exited; no PID is guessed.
        except PermissionError:
            signal_refusal = True
        try:
            # macOS can reject a zombie-only group with EPERM. A successful
            # exit observation AND group settlement AND actual reap are required;
            # the original collection/deadline refusal is never turned into PASS.
            reap_owned_group(child, reap_deadline)
            entry.update({"exit": child.returncode, "collectedBytes": collected, "reaped": True, "refusal": failure})
            if signal_refusal:
                entry["signalRefusalResolvedByExactQuiescence"] = True
        except (Refusal, subprocess.TimeoutExpired):
            HELD_CHILDREN.append(child)
            entry.update({"reaped": False, "refusal": failure, "settlement": "held"})
            raise Refusal("owned host child reap unresolved; VM effects remain uncertain") from None
        raise Refusal(failure)
    finally:
        selector.close()
        child.stdout.close()
        child.stderr.close()
        if not entry.get("reaped") and child not in HELD_CHILDREN:
            # Keep the actual Child capability; a later Popen must not cause
            # Python's orphan cleanup to reap this root before group settlement.
            HELD_CHILDREN.append(child)


def require(code, label):
    if code != 0:
        raise Refusal(label + " failed (output withheld)")


def private_environment(root):
    validate_control_paths(root)
    env = dict(os.environ)
    # Only fixture-owned control roots; HOME and existing profiles are untouched.
    for name, relative in CONTROL_DIRECTORIES.items():
        directory = root / relative
        directory.mkdir(mode=0o700)
        env[name] = str(directory)
    # Do not inherit an alternate daemon/context or registry credential provider.
    for name in ("DOCKER_HOST", "DOCKER_CONTEXT", "CONTAINERD_ADDRESS", "NERDCTL_TOML"):
        env.pop(name, None)
    return env


def refuse_existing_state(env):
    colima_home = Path(env.get("COLIMA_HOME", str(Path.home() / ".colima")))
    lima_home = Path(env.get("LIMA_HOME", str(colima_home / "_lima")))
    if any(path.exists() or path.is_symlink() for path in
           [colima_home / PROFILE, lima_home / ("colima-" + PROFILE)]):
        raise Refusal("fixed fixture profile state already exists; no adoption")


def profile_snapshot(env, deadline=None):
    code, body = bounded([COLIMA, "list", "--json"], env,
                         min(time.monotonic() + 15, deadline) if deadline is not None else time.monotonic() + 15)
    require(code, "profile inventory")
    try:
        value = json.loads(body)
        values = value if isinstance(value, list) else [value]
    except ValueError:
        try:
            values = [json.loads(line) for line in body.splitlines() if line.strip()]
        except ValueError:
            raise Refusal("profile inventory cannot be classified") from None
    if any(not isinstance(item, dict) or "name" not in item for item in values):
        raise Refusal("profile inventory cannot prove names")
    return values


def control_ownership(root, env, token):
    intent = json.loads((root / "intent.json").read_text())
    if intent["token"] != token or intent["profile"] != PROFILE:
        raise Refusal("fixture ownership changed; cleanup held")
    if root.is_symlink() or (root.stat().st_dev, root.stat().st_ino) != tuple(intent["rootIdentity"]):
        raise Refusal("fixture root identity changed; cleanup held")
    control = Path(intent["controlRoot"])
    if control.is_symlink() or (control.stat().st_dev, control.stat().st_ino) != tuple(intent["controlRootIdentity"]):
        raise Refusal("fixture control root identity changed; cleanup held")
    if control.stat().st_mode & 0o777 != 0o700:
        raise Refusal("fixture control root permissions changed; cleanup held")
    validate_control_paths(control)
    for key, relative in CONTROL_DIRECTORIES.items():
        directory = Path(env[key])
        if directory != control / relative or directory.is_symlink():
            raise Refusal("fixture control root identity changed; cleanup held")
        if (directory.stat().st_dev, directory.stat().st_ino) != tuple(intent["controlDirectoryIdentities"][key]):
            raise Refusal("fixture control directory identity changed; cleanup held")
        if directory.stat().st_mode & 0o777 != 0o700:
            raise Refusal("fixture control directory permissions changed; cleanup held")
    return control


def ownership(root, env, token):
    control_ownership(root, env, token)
    config = Path(env["COLIMA_HOME"]) / PROFILE / "colima.yaml"
    identity = root / "profile-identity.json"
    if not config.is_file() or config.is_symlink():
        raise Refusal("new profile identity unavailable; cleanup held")
    current = {"configSHA256": hashlib.sha256(config.read_bytes()).hexdigest(),
               "identity": [config.stat().st_dev, config.stat().st_ino]}
    if identity.exists():
        if json.loads(identity.read_text()) != current:
            raise Refusal("new profile configuration changed; cleanup held")
    else:
        identity.write_text(json.dumps(current) + "\n")


def script(image, username, password, config_digest):
    # Random alphanumeric secrets have no shell metacharacters; sent only on stdin.
    return ('''set -eu
umask 077
phase=preflight
mark() { phase="$1"; printf '{"phase":"%s"}\n' "$phase"; }
trap 'code=$?; if test "$code" != 0; then printf '\''{"failedStep":"%s","exit":%s}\n'\'' "$phase" "$code"; fi' EXIT
image=''' + image + '''
username=''' + username + '''
password=''' + password + '''
work=/tmp/effigy-minio-fixture-dfefc279
test ! -e "$work"
mkdir -m 700 "$work" "$work/data"
command -v curl >/dev/null
command -v nerdctl >/dev/null
test "$(uname -m)" = aarch64
printf 'MINIO_ROOT_USER=%s\nMINIO_ROOT_PASSWORD=%s\nMINIO_API_CORS_ALLOW_ORIGIN=https://fixture.invalid\n' "$username" "$password" > "$work/env"
nerdctl image inspect "$image" > "$work/image.json"
grep -q 'arm64' "$work/image.json"
grep -q 'CONFIG_DIGEST' "$work/image.json"
mark owned-service-start
name=effigy-minio-fixture-dfefc279-server
sentinel=effigy-minio-fixture-dfefc279-sentinel
test -z "$(nerdctl ps -aq --filter name="$name")"
test -z "$(nerdctl ps -aq --filter name="$sentinel")"
sid=$(nerdctl run -d --name "$sentinel" --label io.effigy.fixture.owner=dfefc279 --entrypoint /bin/sh "$image" -c 'sleep 600')
cid=$(nerdctl run -d --name "$name" --label io.effigy.fixture.owner=dfefc279 --env-file "$work/env" -p 127.0.0.1:9000:9000 -v "$work/data:/data" "$image")
printf '{"ownedServerId":"%s","ownedSentinelId":"%s"}\n' "$cid" "$sid"
printf '%s\n' "$cid" > "$work/server-id"
printf '%s\n' "$sid" > "$work/sentinel-id"
ready=0
for i in $(seq 1 30); do
  if curl --silent --fail --max-time 2 http://127.0.0.1:9000/minio/health/ready > /dev/null; then ready=1; break; fi
  sleep 1
done
test "$ready" = 1
mark identity-shell-health
nerdctl exec "$cid" minio --version > "$work/server-version"
grep -q 'RELEASE.2025-09-07T16-13-09Z' "$work/server-version"
grep -q '07c3a429bfed433e49018cb0f78a52145d4bedeb' "$work/server-version"
nerdctl exec "$cid" mc --version > "$work/client-version"
grep -q '2026-09-16' "$work/client-version"
grep -q 'e952aa78f10a2b77dd525a2b7e3143bcda0cd377' "$work/client-version"
nerdctl exec "$cid" /bin/sh -c 'test -x /usr/local/bin/minio && test -x /usr/local/bin/mc && test -d /data'
nerdctl exec "$cid" mc ready local > "$work/packaged-health-result"
mc() { nerdctl exec --env MC_CONFIG_DIR=/tmp/effigy-private-mc "$cid" mc "$@"; }
mc alias set local http://127.0.0.1:9000 "$username" "$password" > "$work/alias-result"
mc ready local > "$work/ready-result"
mark native-s3
mc mb local/effigy-private-fixture > "$work/bucket-result"
printf 'effigy-minio-exact-release-proof\n' > "$work/object"
nerdctl cp "$work/object" "$cid:/tmp/object"
mc cp /tmp/object local/effigy-private-fixture/object > "$work/put-result"
mc cat local/effigy-private-fixture/object > "$work/readback"
cmp "$work/object" "$work/readback"
mc stat local/effigy-private-fixture/object > "$work/stat-result"
mc anonymous set download local/effigy-private-fixture > "$work/anonymous-result"
mark global-cors
curl --silent --fail --max-time 5 -H 'Origin: https://fixture.invalid' -D "$work/cors-headers" http://127.0.0.1:9000/effigy-private-fixture/object > "$work/browser-readback"
cmp "$work/object" "$work/browser-readback"
grep -iq '^access-control-allow-origin: https://fixture.invalid' "$work/cors-headers"
curl --silent --fail --max-time 5 -X OPTIONS -H 'Origin: https://fixture.invalid' -H 'Access-Control-Request-Method: PUT' -D "$work/preflight-headers" http://127.0.0.1:9000/effigy-private-fixture/object > /dev/null
grep -iq '^access-control-allow-origin: https://fixture.invalid' "$work/preflight-headers"
mark bucket-cors-negative
cat > "$work/cors.xml" <<'XML'
<CORSConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><CORSRule><AllowedOrigin>https://fixture.invalid</AllowedOrigin><AllowedMethod>GET</AllowedMethod><AllowedHeader>*</AllowedHeader></CORSRule></CORSConfiguration>
XML
nerdctl cp "$work/cors.xml" "$cid:/tmp/cors.xml"
if mc --json cors set local/effigy-private-fixture /tmp/cors.xml > "$work/bucket-cors-refusal" 2>&1; then exit 61; fi
grep -Eq 'NotImplemented|functionality that is not implemented' "$work/bucket-cors-refusal"
mark restart-persistence
nerdctl stop --time 10 "$cid" > "$work/stop-result"
nerdctl start "$cid" > "$work/start-result"
ready=0
for i in $(seq 1 30); do
  if curl --silent --fail --max-time 2 http://127.0.0.1:9000/minio/health/ready > /dev/null; then ready=1; break; fi
  sleep 1
done
test "$ready" = 1
mc cat local/effigy-private-fixture/object > "$work/persisted"
cmp "$work/object" "$work/persisted"
mark delete-failed-start
mc rm local/effigy-private-fixture/object > "$work/delete-result"
if mc stat local/effigy-private-fixture/object > "$work/deleted-stat" 2>&1; then exit 62; fi
if nerdctl run --rm --entrypoint /usr/local/bin/minio "$image" --effigy-invalid-option > "$work/negative-start" 2>&1; then exit 63; fi
mark owned-cleanup-sentinel
test "$(nerdctl inspect --format '{{.State.Running}}' "$sid")" = true
test "$(nerdctl inspect --format '{{index .Config.Labels "io.effigy.fixture.owner"}}' "$cid")" = dfefc279
nerdctl rm -f "$cid" > "$work/remove-server"
test "$(nerdctl inspect --format '{{.State.Running}}' "$sid")" = true
nerdctl rm -f "$sid" > "$work/remove-sentinel"
test -z "$(nerdctl ps -aq --filter label=io.effigy.fixture.owner=dfefc279)"
rm -rf "$work"
printf '{"serverRelease":"RELEASE.2025-09-07T16-13-09Z","clientPublisher":"PGSTY","clientRelease":"RELEASE.2026-09-16T00-00-00Z","readiness":true,"s3Roundtrip":true,"globalCors":true,"bucketCors":"unsupported","restartPersistence":true,"delete":true,"failedStart":true,"foreignSentinelPreserved":true,"ownedContainersRemoved":true}\n'
''').replace("CONFIG_DIGEST", config_digest).encode()


def run(receipt_path, root):
    JOURNAL.clear()
    receipt = verify(receipt_path)
    root = Path(root).absolute()
    if root.exists() or root.is_symlink():
        raise Refusal("native attempt root exists; no replay")
    refuse_existing_state(dict(os.environ))
    original_profiles = profile_snapshot(dict(os.environ))
    if any(item["name"] == PROFILE for item in original_profiles):
        raise Refusal("fixed fixture profile already exists; no adoption")
    tool_code, tool_version = bounded([COLIMA, "version"], dict(os.environ), time.monotonic() + 15)
    require(tool_code, "Colima identity")
    if b"0.10.3" not in tool_version or b"00f6c297" not in tool_version:
        raise Refusal("installed Colima differs from approved supported tool")
    root.mkdir(mode=0o700, parents=True)
    token = secrets.token_hex(32)
    # Durable receipts may be deep; Unix sockets need a separate short root.
    # mkdtemp creates one new exclusive 0700 directory, never a reused profile.
    control = Path(tempfile.mkdtemp(prefix="efm-", dir="/tmp")).resolve()
    control_identity = [control.stat().st_dev, control.stat().st_ino]
    try:
        env = private_environment(control)
    except Exception:
        # Do not recursively delete partly created or uncertain state. Retain
        # its exact path/identity even when a local filesystem preflight fails.
        cleanup = "held"
        if not control.is_symlink() and [control.stat().st_dev, control.stat().st_ino] == control_identity:
            try:
                control.rmdir()  # Only an empty, positively owned root.
                cleanup = "settled"
            except OSError:
                pass
        result = {"qualified": False, "nativeBatch": "not_started", "cleanup": cleanup,
                  "controlRoot": str(control), "controlRootIdentity": control_identity,
                  "refusal": "fixture control setup failed; no engine started"}
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        return result
    intent = {"schema": "effigy.private-minio-native-intent.v1", "token": token, "profile": PROFILE,
              "colimaSHA256": hashlib.sha256(Path(COLIMA).read_bytes()).hexdigest(),
              "rootIdentity": [root.stat().st_dev, root.stat().st_ino], "artifactSHA256": receipt["artifactSHA256"],
              "observedAgent": os.environ.get("PASEO_AGENT_ID"), "batch": 1, "budgets": {"startup": 300, "proof": 600, "teardown": 120},
              "controlRoot": str(control), "controlRootIdentity": control_identity,
              "controlDirectoryIdentities": {key: [Path(env[key]).stat().st_dev, Path(env[key]).stat().st_ino]
                                             for key in CONTROL_DIRECTORIES},
              "authorizations": ["86a7e90a-9fa3-401c-865a-ad9ff2ace354", "11b9b302-358e-414e-a40a-60dbd17282ef"]}
    with (root / "intent.json").open("x") as output:
        output.write(json.dumps(intent, indent=2) + "\n")
    result = {"qualified": False, "profile": PROFILE, "artifactSHA256": receipt["artifactSHA256"],
              "controlRoot": str(control), "nativeBatch": "spent", "cleanup": "held"}
    attempted = False
    try:
        if profile_snapshot(env):
            raise Refusal("private fixture profile inventory is not empty")
        refuse_existing_state(dict(os.environ))
        if any(item["name"] == PROFILE for item in profile_snapshot(dict(os.environ))):
            raise Refusal("fixed profile appeared before startup; no adoption")
        control_ownership(root, env, token)
        attempted = True
        code, _ = bounded([COLIMA, "start", PROFILE, *START_FLAGS], env, time.monotonic() + 300,
                          classify_startup=True)
        if code != 0:
            result["startupFailure"] = {"exit": code, "category": JOURNAL[-1]["diagnosticCategory"]}
        require(code, "new profile startup")
        ownership(root, env, token)
        proof_deadline = time.monotonic() + 600
        with Path(receipt["artifact"]).open("rb") as source:
            code, _ = bounded([COLIMA, "--profile", PROFILE, "nerdctl", "--", "load"], env, proof_deadline, source)
        require(code, "private OCI import")
        guest_script = root / "guest-proof.stdin"
        guest_script.write_bytes(script("effigy-private-minio:dfefc279", "fixture" + secrets.token_hex(8), secrets.token_hex(24), receipt["configDigest"]))
        guest_script.chmod(0o600)
        try:
            with guest_script.open("rb") as source:
                code, body = bounded([COLIMA, "--profile", PROFILE, "ssh", "--", "sh", "-s"], env, proof_deadline, source)
            try:
                records = [json.loads(line) for line in body.splitlines() if line.strip()]
                allowed = {"owned-service-start", "identity-shell-health", "native-s3", "global-cors",
                           "bucket-cors-negative", "restart-persistence", "delete-failed-start", "owned-cleanup-sentinel", "preflight"}
                result["guestPhases"] = [r["phase"] for r in records if set(r) == {"phase"} and r["phase"] in allowed]
                for record in records:
                    if set(record) == {"ownedServerId", "ownedSentinelId"}:
                        if any(not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value) for value in record.values()):
                            raise Refusal("native resource identities unavailable")
                        if record["ownedServerId"] == record["ownedSentinelId"]:
                            raise Refusal("native resource identities are not distinct")
                        result["ownedGuestContainers"] = record
                    if set(record) == {"failedStep", "exit"} and record["failedStep"] in allowed and isinstance(record["exit"], int):
                        result["guestFailure"] = record
            except (ValueError, TypeError):
                raise Refusal("native structured observations unavailable; output withheld") from None
            require(code, "native exact-release capability proof")
            if "ownedGuestContainers" not in result:
                raise Refusal("native proof lacks exact owned guest container identities")
            observations = records[-1]
            expected = {"serverRelease": "RELEASE.2025-09-07T16-13-09Z", "clientPublisher": "PGSTY",
                        "clientRelease": "RELEASE.2026-09-16T00-00-00Z", "readiness": True,
                        "s3Roundtrip": True, "globalCors": True, "bucketCors": "unsupported",
                        "restartPersistence": True, "delete": True, "failedStart": True,
                        "foreignSentinelPreserved": True, "ownedContainersRemoved": True}
            if observations != expected:
                raise Refusal("native observation schema or required cases incomplete")
            result["observations"] = observations
        finally:
            guest_script.unlink(missing_ok=True)
        result["proofPassed"] = True
    except Exception as error:
        result["refusal"] = str(error) if isinstance(error, Refusal) else "native evidence unavailable"
    finally:
        if attempted:
            try:
                if any(not entry.get("reaped") for entry in JOURNAL) or HELD_CHILDREN:
                    raise Refusal("host child settlement unresolved; VM cleanup held")
                ownership(root, env, token)
                cleanup_deadline = time.monotonic() + 120
                code, _ = bounded([COLIMA, "stop", PROFILE], env, cleanup_deadline)
                require(code, "owned profile stop")
                ownership(root, env, token)
                code, _ = bounded([COLIMA, "delete", PROFILE, "--force", "--data"], env, cleanup_deadline)
                require(code, "owned profile deletion")
                if profile_snapshot(env, cleanup_deadline):
                    raise Refusal("private profile inventory not empty after deletion")
                if profile_snapshot(dict(os.environ), cleanup_deadline) != original_profiles:
                    raise Refusal("external profile inventory changed; no external cleanup attempted")
                control_ownership(root, env, token)
                shutil.rmtree(control)
                result["controlRootRemoved"] = True
                result["cleanup"] = "settled"
            except Exception as error:
                result["cleanupRefusal"] = str(error) if isinstance(error, Refusal) else "owned teardown unresolved"
        result["qualified"] = bool(result.get("proofPassed") and result["cleanup"] == "settled")
        result["hostCommands"] = list(JOURNAL)
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--attempt-root", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = run(args.receipt, args.attempt_root)
        print(json.dumps(result))
        raise SystemExit(0 if result["qualified"] else 1)
    except Exception as error:
        print(json.dumps({"qualified": False, "refusal": str(error) if isinstance(error, Refusal) else "native preflight unavailable (details withheld)"}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
