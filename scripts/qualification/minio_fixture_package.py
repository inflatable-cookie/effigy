#!/usr/bin/env python3
"""Private, content-pinned Linux arm64 fixture packaging; never publishes images."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import tarfile
import time
import urllib.parse
import urllib.request

INPUTS = Path(__file__).with_name("minio_fixture_inputs.json")
MAX_LAYER = 268435456
PROFILE = "effigy-minio-fixture-dfefc279"


class Refusal(Exception):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def checked_digest(value):
    if not isinstance(value, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", value):
        raise Refusal("invalid content digest")
    return value


def checked_elf(data):
    if (len(data) < 64 or data[:6] != b"\x7fELF\x02\x01"
            or struct.unpack_from("<H", data, 18)[0] != 183):
        raise Refusal("binary is not Linux arm64 ELF64")


class PublicRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if urllib.parse.urlsplit(newurl).scheme != "https":
            raise Refusal("public artifact redirected outside HTTPS")
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if urllib.parse.urlsplit(req.full_url).netloc != urllib.parse.urlsplit(newurl).netloc:
            redirected.remove_header("Authorization")
        return redirected


def acquire(url, destination, expected, size, headers=None):
    """One bounded acquisition, atomic content check; no credentials in diagnostics."""
    checked_digest(expected)
    u = urllib.parse.urlsplit(url)
    if u.scheme != "https" or u.username or u.password:
        raise Refusal("artifact URL is not public HTTPS")
    destination = Path(destination)
    if destination.exists():
        if destination.is_symlink() or destination.stat().st_size != size:
            raise Refusal("existing content has unexpected identity")
        data = destination.read_bytes()
        if digest(data) != expected:
            raise Refusal("existing content digest mismatch")
        return data
    temporary = destination.with_suffix(".partial")
    deadline = time.monotonic() + 120
    count = 0
    hasher = hashlib.sha256()
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "Effigy-private-fixture", **(headers or {})})
        opener = urllib.request.build_opener(PublicRedirect())
        with opener.open(request, timeout=20) as response, temporary.open("xb") as output:
            while True:
                if time.monotonic() >= deadline:
                    raise Refusal("artifact acquisition deadline")
                chunk = response.read1(min(65536, size - count + 1))
                if not chunk:
                    break
                count += len(chunk)
                if count > size:
                    raise Refusal("artifact exceeded declared size")
                hasher.update(chunk)
                output.write(chunk)
        if count != size or "sha256:" + hasher.hexdigest() != expected:
            raise Refusal("artifact content identity mismatch")
        temporary.rename(destination)
        destination.chmod(0o400)
        return destination.read_bytes()
    except Refusal:
        temporary.unlink(missing_ok=True)
        raise
    except Exception:
        temporary.unlink(missing_ok=True)
        raise Refusal("public artifact acquisition unavailable") from None


def registry_headers(repository):
    query = urllib.parse.urlencode({"service": "registry.docker.io", "scope": "repository:" + repository + ":pull"})
    with urllib.request.urlopen("https://auth.docker.io/token?" + query, timeout=20) as response:
        body = response.read(65537)
    if len(body) > 65536:
        raise Refusal("anonymous registry response exceeded limit")
    return {"Authorization": "Bearer " + json.loads(body)["token"]}


def unpacked_layer(data, expected):
    checked_digest(expected)
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as source:
        unpacked = source.read(MAX_LAYER + 1)
    if len(unpacked) > MAX_LAYER or digest(unpacked) != expected:
        raise Refusal("base layer unpacked identity or bound mismatch")
    return unpacked


def client_binary(archive):
    selected = []
    total = 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as source:
        for ordinal, member in enumerate(source):
            if ordinal > 64:
                raise Refusal("client archive entry bound exceeded")
            total += member.size
            if member.size < 0 or total > 67108864:
                raise Refusal("client archive expanded size bound exceeded")
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise Refusal("client archive has an escaping path")
            if path.name in ("mc", "mcli"):
                if not member.isfile() or member.size > 67108864:
                    raise Refusal("client executable is not a bounded regular file")
                selected.append(source.extractfile(member).read(member.size + 1))
    if len(selected) != 1:
        raise Refusal("client archive must contain exactly one executable")
    checked_elf(selected[0])
    return selected[0]


def make_layer(files):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as output:
        for name, data in sorted(files.items()):
            if name.startswith("/") or ".." in Path(name).parts:
                raise Refusal("private layer path escapes its root")
            entry = tarfile.TarInfo(name)
            entry.size, entry.mode, entry.uid, entry.gid, entry.mtime = len(data), 0o755, 0, 0, 0
            output.addfile(entry, io.BytesIO(data))
    raw = buffer.getvalue()
    return gzip.compress(raw, mtime=0), digest(raw)


def validate_inputs(inputs):
    if inputs.get("schema") != "effigy.private-minio-inputs.v1" or inputs.get("platform") != "linux/arm64":
        raise Refusal("unsupported fixture inputs or platform")
    base = inputs["base"]
    if base["repository"] != "docker.io/library/ubuntu":
        raise Refusal("unexpected neutral base publisher")
    checked_digest(base["manifestDigest"])
    if len(base["manifest"]["layers"]) != 1:
        raise Refusal("neutral base must have one reviewed layer")
    for item in [base["manifest"]["config"], *base["manifest"]["layers"]]:
        checked_digest(item["digest"])
        if not isinstance(item["size"], int) or not 0 < item["size"] <= MAX_LAYER:
            raise Refusal("neutral base descriptor bound exceeded")
    json_bytes(base["config"])
    if base["config"].get("architecture") != "arm64" or base["config"].get("os") != "linux":
        raise Refusal("base configuration architecture mismatch")
    for item in (inputs["server"], inputs["client"]):
        checked_digest("sha256:" + item["sha256"])
        if item["size"] <= 0 or item["size"] > 134217728:
            raise Refusal("public executable size is outside bounds")


def configuration_for(server_spec, client_spec, base, diff_id):
    return {"architecture": "arm64", "os": "linux", "config": {
            "Env": ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
            "Entrypoint": ["/usr/local/bin/minio"], "Cmd": ["server", "/data", "--console-address", ":9001"],
            "WorkingDir": "/", "ExposedPorts": {"9000/tcp": {}, "9001/tcp": {}}, "Volumes": {"/data": {}},
            "Healthcheck": {"Test": ["CMD", "mc", "ready", "local"], "Interval": 5000000000, "Timeout": 3000000000, "Retries": 5, "StartPeriod": 10000000000},
            "Labels": {"io.effigy.fixture.only": "true", "io.effigy.fixture.server": server_spec["release"], "io.effigy.fixture.client.publisher": "PGSTY", "io.effigy.fixture.client": client_spec["release"]}},
            "rootfs": {"type": "layers", "diff_ids": base["config"]["rootfs"]["diff_ids"] + [diff_id]}}


def build(root):
    root = Path(root).resolve()
    if root.exists():
        raise Refusal("artifact root already exists; construction is not replayed")
    inputs = json.loads(INPUTS.read_text())
    validate_inputs(inputs)
    root.mkdir(mode=0o700, parents=True)
    cache = root / "content"
    cache.mkdir(mode=0o700)
    try:
        server_spec, client_spec = inputs["server"], inputs["client"]
        server = acquire(server_spec["url"], cache / "minio", "sha256:" + server_spec["sha256"], server_spec["size"])
        checked_elf(server)
        archive = acquire(client_spec["url"], cache / "mcli.tar.gz", "sha256:" + client_spec["sha256"], client_spec["size"])
        client = client_binary(archive)
        base = inputs["base"]
        headers = registry_headers("library/ubuntu")
        request = urllib.request.Request(
            "https://registry-1.docker.io/v2/library/ubuntu/manifests/" + base["manifestDigest"],
            headers={**headers, "Accept": "application/vnd.oci.image.manifest.v1+json"})
        with urllib.request.urlopen(request, timeout=20) as response:
            manifest_bytes = response.read(65537)
        if (len(manifest_bytes) > 65536 or digest(manifest_bytes) != base["manifestDigest"]
                or json.loads(manifest_bytes) != base["manifest"]):
            raise Refusal("neutral base manifest differs from immutable publisher input")
        config_descriptor = base["manifest"]["config"]
        config_blob = acquire("https://registry-1.docker.io/v2/library/ubuntu/blobs/" + config_descriptor["digest"], cache / "base-config", config_descriptor["digest"], config_descriptor["size"], headers)
        if json.loads(config_blob) != base["config"]:
            raise Refusal("base configuration differs from pinned metadata")
        if len(base["manifest"]["layers"]) != len(base["config"]["rootfs"]["diff_ids"]):
            raise Refusal("base diff-id coverage is incomplete")
        layers, blobs = [], {}
        for descriptor, diff in zip(base["manifest"]["layers"], base["config"]["rootfs"]["diff_ids"]):
            blob = acquire("https://registry-1.docker.io/v2/library/ubuntu/blobs/" + descriptor["digest"], cache / descriptor["digest"].split(":")[1], descriptor["digest"], descriptor["size"], headers)
            unpacked_layer(blob, diff)
            blobs[descriptor["digest"]] = blob
            layers.append(descriptor)
        if len(layers) != len(base["manifest"]["layers"]) or not layers:
            raise Refusal("base diff-id coverage is incomplete")
        layer, diff_id = make_layer({"usr/local/bin/minio": server, "usr/local/bin/mc": client})
        layer_digest = digest(layer)
        blobs[layer_digest] = layer
        layers.append({"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": layer_digest, "size": len(layer)})
        config = configuration_for(server_spec, client_spec, base, diff_id)
        config_data = json_bytes(config)
        config_digest = digest(config_data)
        blobs[config_digest] = config_data
        manifest = {"schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json", "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": config_digest, "size": len(config_data)}, "layers": layers}
        manifest_data = json_bytes(manifest)
        manifest_digest = digest(manifest_data)
        blobs[manifest_digest] = manifest_data
        index = {"schemaVersion": 2, "manifests": [{"mediaType": manifest["mediaType"], "digest": manifest_digest, "size": len(manifest_data), "platform": {"os": "linux", "architecture": "arm64"}, "annotations": {"org.opencontainers.image.ref.name": "effigy-private-minio:dfefc279"}}]}
        files = {"oci-layout": json_bytes({"imageLayoutVersion": "1.0.0"}), "index.json": json_bytes(index)}
        files.update({"blobs/sha256/" + key.split(":")[1]: value for key, value in blobs.items()})
        artifact = root / "minio-linux-arm64.oci.tar"
        with tarfile.open(artifact, "w", format=tarfile.USTAR_FORMAT) as output:
            for name, data in sorted(files.items()):
                entry = tarfile.TarInfo(name)
                entry.size, entry.mode, entry.mtime = len(data), 0o400, 0
                output.addfile(entry, io.BytesIO(data))
        receipt = {"schema": "effigy.private-minio-package.v1", "artifact": str(artifact), "artifactSHA256": hashlib.sha256(artifact.read_bytes()).hexdigest(), "manifestDigest": manifest_digest, "configDigest": config_digest, "platform": "linux/arm64", "inputs": inputs, "inputsSHA256": hashlib.sha256(INPUTS.read_bytes()).hexdigest(), "clientBinarySHA256": hashlib.sha256(client).hexdigest(), "configuration": config, "nativeQualified": False, "nativeBatch": "unspent", "assurance": "Private new package; not the original upstream OCI image. PGSTY client, not upstream MinIO client. No local signature/SBOM verification or native behavior claim. Bucket CORS is unsupported; global CORS is separate. No public TLS trust-store equivalence claim."}
        (root / "artifact.json").write_text(json.dumps(receipt, indent=2) + "\n")
        verify(root / "artifact.json")
        return receipt
    except Exception:
        (root / "construction-refusal.json").write_text('{"qualified":false,"stage":"artifact_construction"}\n')
        raise


def verify(receipt_path):
    receipt_path = Path(receipt_path).resolve()
    if receipt_path.stat().st_size > 65536:
        raise Refusal("package receipt bound exceeded")
    receipt = json.loads(receipt_path.read_text())
    if receipt.get("schema") != "effigy.private-minio-package.v1":
        raise Refusal("unexpected package receipt schema")
    expected = json.loads(INPUTS.read_text())
    validate_inputs(expected)
    if receipt["inputs"] != expected:
        raise Refusal("package publisher inputs changed")
    artifact = Path(receipt["artifact"])
    if (artifact.parent != receipt_path.parent or artifact.is_symlink()
            or not 0 < artifact.stat().st_size <= MAX_LAYER * 2):
        raise Refusal("artifact path is not privately contained")
    if hashlib.sha256(artifact.read_bytes()).hexdigest() != receipt["artifactSHA256"]:
        raise Refusal("artifact checksum mismatch")
    if hashlib.sha256(INPUTS.read_bytes()).hexdigest() != receipt["inputsSHA256"]:
        raise Refusal("reviewed input pins differ from artifact")
    with tarfile.open(artifact) as source:
        members = []
        for member in source:
            members.append(member)
            if len(members) > 16:
                raise Refusal("OCI archive entry bound exceeded")
        names = [item.name for item in members]
        if (len(members) > 16 or len(names) != len(set(names))
                or any(not item.isfile() or item.size > MAX_LAYER
                       or Path(item.name).is_absolute() or ".." in Path(item.name).parts
                       for item in members)):
            raise Refusal("OCI archive has unexpected entries or sizes")
        checked_digest(receipt["manifestDigest"])
        manifest_data = source.extractfile("blobs/sha256/" + receipt["manifestDigest"].split(":")[1]).read()
        if digest(manifest_data) != receipt["manifestDigest"]:
            raise Refusal("OCI manifest identity mismatch")
        manifest = json.loads(manifest_data)
        expected_names = {"oci-layout", "index.json", "blobs/sha256/" + receipt["manifestDigest"].split(":")[1]}
        expected_names.update("blobs/sha256/" + checked_digest(item["digest"]).split(":")[1]
                              for item in [manifest["config"], *manifest["layers"]])
        if set(names) != expected_names:
            raise Refusal("OCI archive includes unreferenced or missing entries")
        if json.loads(source.extractfile("oci-layout").read()) != {"imageLayoutVersion": "1.0.0"}:
            raise Refusal("OCI layout version changed")
        expected_index = {"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json",
                          "digest": receipt["manifestDigest"], "size": len(manifest_data),
                          "platform": {"os": "linux", "architecture": "arm64"},
                          "annotations": {"org.opencontainers.image.ref.name": "effigy-private-minio:dfefc279"}}]}
        if json.loads(source.extractfile("index.json").read()) != expected_index:
            raise Refusal("OCI import index differs from reviewed single image")
        if manifest["layers"][:-1] != expected["base"]["manifest"]["layers"]:
            raise Refusal("OCI neutral base layers changed")
        configuration = None
        private_layer = None
        for descriptor in [manifest["config"], *manifest["layers"]]:
            checked_digest(descriptor["digest"])
            if not isinstance(descriptor["size"], int) or not 0 < descriptor["size"] <= MAX_LAYER:
                raise Refusal("OCI referenced blob bound exceeded")
            data = source.extractfile("blobs/sha256/" + descriptor["digest"].split(":")[1]).read()
            if len(data) != descriptor["size"] or digest(data) != descriptor["digest"]:
                raise Refusal("OCI referenced blob identity mismatch")
            if descriptor == manifest["config"]:
                configuration = json.loads(data)
            if descriptor == manifest["layers"][-1]:
                private_layer = data
            else:
                if descriptor in manifest["layers"][:-1]:
                    index = manifest["layers"].index(descriptor)
                    unpacked_layer(data, expected["base"]["config"]["rootfs"]["diff_ids"][index])
        if configuration != receipt["configuration"] or configuration["architecture"] != "arm64" or configuration["os"] != "linux":
            raise Refusal("OCI configuration identity mismatch")
        if configuration != configuration_for(expected["server"], expected["client"], expected["base"], configuration["rootfs"]["diff_ids"][-1]):
            raise Refusal("OCI configuration differs from reviewed package contract")
        cfg = configuration["config"]
        if (cfg["Env"] != ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"]
                or cfg["Entrypoint"] != ["/usr/local/bin/minio"]
                or cfg["Cmd"] != ["server", "/data", "--console-address", ":9001"]
                or cfg["Healthcheck"]["Test"] != ["CMD", "mc", "ready", "local"]
                or cfg["Volumes"] != {"/data": {}}
                or cfg["ExposedPorts"] != {"9000/tcp": {}, "9001/tcp": {}}):
            raise Refusal("OCI catalog shape or environment changed")
        archive = (receipt_path.parent / "content/mcli.tar.gz").read_bytes()
        if hashlib.sha256(archive).hexdigest() != expected["client"]["sha256"]:
            raise Refusal("client archive identity mismatch")
        client = client_binary(archive)
        unpacked = unpacked_layer(private_layer, configuration["rootfs"]["diff_ids"][-1])
        with tarfile.open(fileobj=io.BytesIO(unpacked)) as layer:
            names = layer.getnames()
            if (sorted(names) != ["usr/local/bin/mc", "usr/local/bin/minio"]
                    or any(not m.isfile() or m.mode != 0o755 or m.uid != 0 or m.gid != 0
                           for m in layer.getmembers())):
                raise Refusal("private executable layer has unexpected entries")
            minio = layer.extractfile("usr/local/bin/minio").read()
            mc = layer.extractfile("usr/local/bin/mc").read()
            checked_elf(minio)
            if hashlib.sha256(minio).hexdigest() != expected["server"]["sha256"] or mc != client:
                raise Refusal("private executable layer differs from upstream inputs")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--build", type=Path)
    action.add_argument("--verify", type=Path)
    args = parser.parse_args()
    try:
        result = build(args.build) if args.build else verify(args.verify)
        print(json.dumps({"artifact": result["artifact"], "manifestDigest": result["manifestDigest"], "platform": result["platform"], "nativeQualified": False}))
    except Exception as error:
        message = str(error) if isinstance(error, Refusal) else "private package operation failed"
        print(json.dumps({"qualified": False, "error": message}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
