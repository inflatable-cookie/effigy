"""Pinned Ubuntu 22.04 arm64 OCI blobs and offline apt debs for the C4/D2 runtime image.

Public upstream metadata only. This module never fabricates a missing payload hash: a later
occurrence fetches the genuine bytes and hashes them against these pins. The assembled OCI
layout's index.json is a filtered single-platform wrapper of the pinned arm64 manifest; it is
not a produced archive and has no payload hash of its own.

No effects on import. Fetch, hash and layout assembly run only from the reviewed helper.
"""
import hashlib
import json
import os
from pathlib import Path
import stat

import preparation_stage as primitive

Refusal = primitive.Refusal

UBUNTU_PORTS = 'https://ports.ubuntu.com/ubuntu-ports/'
REGISTRY = 'https://registry-1.docker.io/v2/library/ubuntu/'
AUTH_TOKEN_URL = ('https://auth.docker.io/token?service=registry.docker.io'
                  '&scope=repository:library/ubuntu:pull')
TOKEN_CAP_BYTES = 16384
# Hub index for ubuntu:22.04@sha256:076e7bf8… (queried 2026-11-08, public metadata only).
INDEX = dict(
    digest='sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11',
    bytes=6688,
    media_type='application/vnd.oci.image.index.v1+json',
    url=REGISTRY + 'manifests/sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11',
)
MANIFEST = dict(
    digest='sha256:a8fb2ba88181746111cbbe2969826d6546b71226785b196c93d2c3445a25000d',
    bytes=424,
    media_type='application/vnd.oci.image.manifest.v1+json',
    url=REGISTRY + 'manifests/sha256:a8fb2ba88181746111cbbe2969826d6546b71226785b196c93d2c3445a25000d',
)
CONFIG = dict(
    digest='sha256:5649198dfcca9dd6955ee455fdd6ba60734f28da600de2919a2f53da113a707c',
    bytes=2069,
    media_type='application/vnd.oci.image.config.v1+json',
    url=REGISTRY + 'blobs/sha256:5649198dfcca9dd6955ee455fdd6ba60734f28da600de2919a2f53da113a707c',
)
LAYER = dict(
    digest='sha256:771eed73ee82c7916df197c0fd7ef3ac4eedcb9a6318b10d2652948c6c4ca08f',
    bytes=27684607,
    media_type='application/vnd.oci.image.layer.v1.tar+gzip',
    url=REGISTRY + 'blobs/sha256:771eed73ee82c7916df197c0fd7ef3ac4eedcb9a6318b10d2652948c6c4ca08f',
)
# python3 + ca-certificates closure from jammy-updates/security (or jammy when that is the pin),
# plus the four extra packages apt-get would pull that the 22.04 base does not pin at these versions:
# libdb5.3, readline-common, libtirpc-common, libtinfo6 (= libncursesw6). libc6/coreutils stay in the base.
DEBS = (
    dict(name='python3_3.10.6-1~22.04.1_arm64.deb', bytes=22766,
         sha256='f507db8cc0bec943aecce435f8ad5d2d400067324992ea763aee2e5e4a67e276',
         path='pool/main/p/python3-defaults/python3_3.10.6-1~22.04.1_arm64.deb'),
    dict(name='python3-minimal_3.10.6-1~22.04.1_arm64.deb', bytes=24306,
         sha256='3763affab256e6eff0098d37d7f35531ecfa6859b0fa18945e0c68353fe364e8',
         path='pool/main/p/python3-defaults/python3-minimal_3.10.6-1~22.04.1_arm64.deb'),
    dict(name='libpython3-stdlib_3.10.6-1~22.04.1_arm64.deb', bytes=6812,
         sha256='699e158385c0078b54b252a82a1c715e592f1d0e9ae5df936d4d00cc1b32517c',
         path='pool/main/p/python3-defaults/libpython3-stdlib_3.10.6-1~22.04.1_arm64.deb'),
    dict(name='python3.10_3.10.12-1~22.04.18_arm64.deb', bytes=508494,
         sha256='76ba042e44128aedfbbee2d73692e1782f64b4202507c9005a111c3eb430cc1a',
         path='pool/main/p/python3.10/python3.10_3.10.12-1~22.04.18_arm64.deb'),
    dict(name='python3.10-minimal_3.10.12-1~22.04.18_arm64.deb', bytes=2244974,
         sha256='65b518c95d569ccbef6c903bb308958cf35c9b100cc2bd98180e5f254603cef7',
         path='pool/main/p/python3.10/python3.10-minimal_3.10.12-1~22.04.18_arm64.deb'),
    dict(name='libpython3.10-minimal_3.10.12-1~22.04.18_arm64.deb', bytes=813988,
         sha256='f3ef036c6b8ab24a2785ee70854b1fe5217e6e24d2d3e8aa36d9404e7d89b6eb',
         path='pool/main/p/python3.10/libpython3.10-minimal_3.10.12-1~22.04.18_arm64.deb'),
    dict(name='libpython3.10-stdlib_3.10.12-1~22.04.18_arm64.deb', bytes=1848248,
         sha256='6f5ce202c209dfe0c2f41471ec55ee31476c80e913e4fd2ae7e5050b42b7a8fb',
         path='pool/main/p/python3.10/libpython3.10-stdlib_3.10.12-1~22.04.18_arm64.deb'),
    dict(name='ca-certificates_20260816ubuntu0.22.04.2_all.deb', bytes=139772,
         sha256='f99e8375d35bdde4800d4842df777be3dafe38bca4d178429fa929de200bf821',
         path='pool/main/c/ca-certificates/ca-certificates_20260816ubuntu0.22.04.2_all.deb'),
    dict(name='openssl_3.0.2-0ubuntu1.30_arm64.deb', bytes=1159974,
         sha256='6a4d2300bc6a40d8948a1cdc31a45fc1b4ff4d526bca606be4f40c5162b4a2ca',
         path='pool/main/o/openssl/openssl_3.0.2-0ubuntu1.30_arm64.deb'),
    dict(name='libssl3_3.0.2-0ubuntu1.30_arm64.deb', bytes=1813678,
         sha256='1c4b8cc736eed8a0caafc385f854fa6852382221d84be4795f1d19a52b9fec53',
         path='pool/main/o/openssl/libssl3_3.0.2-0ubuntu1.30_arm64.deb'),
    dict(name='libexpat1_2.4.7-1ubuntu0.9_arm64.deb', bytes=83528,
         sha256='b61d2bdf82134f6db0ba5c73d9cfe88805a64cbadbd0d0dffff8410958aab005',
         path='pool/main/e/expat/libexpat1_2.4.7-1ubuntu0.9_arm64.deb'),
    dict(name='libmpdec3_2.5.1-2build2_arm64.deb', bytes=88962,
         sha256='e5dcef803ee13a0ef22d5970e7f99e976760d955474b73fd5360b24995a281ee',
         path='pool/main/m/mpdecimal/libmpdec3_2.5.1-2build2_arm64.deb'),
    dict(name='libncursesw6_6.3-2ubuntu0.3_arm64.deb', bytes=142280,
         sha256='587257cb8455cf87dc963bc73cec4142ecba187cc9de7d3973f07b245d8bc6d7',
         path='pool/main/n/ncurses/libncursesw6_6.3-2ubuntu0.3_arm64.deb'),
    dict(name='libtinfo6_6.3-2ubuntu0.3_arm64.deb', bytes=103622,
         sha256='971b5fd1cf8f7d68ba00f40d8eec6474fae6a6adbe19883aa42dd2b74c672e4a',
         path='pool/main/n/ncurses/libtinfo6_6.3-2ubuntu0.3_arm64.deb'),
    dict(name='libnsl2_1.3.0-2build2_arm64.deb', bytes=41420,
         sha256='ab2e58eed36da75635f248fad08a2dd3fbe92c9d1f5e44eb68a99db083e9a9c3',
         path='pool/main/libn/libnsl/libnsl2_1.3.0-2build2_arm64.deb'),
    dict(name='libreadline8_8.1.2-1_arm64.deb', bytes=153336,
         sha256='54291f2dd185d3a759227b31b7de4269e495a084fdad89e24a9cc4d0d20888a7',
         path='pool/main/r/readline/libreadline8_8.1.2-1_arm64.deb'),
    dict(name='readline-common_8.1.2-1_all.deb', bytes=53544,
         sha256='a78d02935f6107a12d3d349c25ce4f92fcdcbf6b65e37f1f5a13a9054b5b9e94',
         path='pool/main/r/readline/readline-common_8.1.2-1_all.deb'),
    dict(name='libsqlite3-0_3.37.2-2ubuntu0.8_arm64.deb', bytes=636316,
         sha256='0bb81a51a7a56aa66a6bf981e6a4dc4040b1873dc2fc29e8834c977585068e9c',
         path='pool/main/s/sqlite3/libsqlite3-0_3.37.2-2ubuntu0.8_arm64.deb'),
    dict(name='libtirpc3_1.3.2-2ubuntu0.1_arm64.deb', bytes=82416,
         sha256='64b3eb9a689049c99e02d447b630879c5b2e0db50a18fe6c5dbf1db06d0833ec',
         path='pool/main/libt/libtirpc/libtirpc3_1.3.2-2ubuntu0.1_arm64.deb'),
    dict(name='libtirpc-common_1.3.2-2ubuntu0.1_all.deb', bytes=7766,
         sha256='cd7b220447999402e87d75f7cedcfad159a2e5d4236927a414d77f2932219bee',
         path='pool/main/libt/libtirpc/libtirpc-common_1.3.2-2ubuntu0.1_all.deb'),
    dict(name='libdb5.3_5.3.28+dfsg1-0.8ubuntu3_arm64.deb', bytes=713828,
         sha256='e5b672aeaca690b3b7ee97b03dbc07ddb49be7b6ea0a770fa6707666ca1cef59',
         path='pool/main/d/db5.3/libdb5.3_5.3.28+dfsg1-0.8ubuntu3_arm64.deb'),
    dict(name='media-types_7.0.0_all.deb', bytes=25510,
         sha256='a993af16a8ccdcefaf75eef360f295f8a3a91a78fd5e5ea1e3fd4f2376df6615',
         path='pool/main/m/media-types/media-types_7.0.0_all.deb'),
)
OCI_BLOBS = (INDEX, MANIFEST, CONFIG, LAYER)
OCI_BYTES = sum(item['bytes'] for item in OCI_BLOBS)
DEB_BYTES = sum(item['bytes'] for item in DEBS)
INPUT_BYTES = OCI_BYTES + DEB_BYTES
SOURCE_PREFIXES = (
    REGISTRY,
    'https://production.cloudfront.docker.com/',
    UBUNTU_PORTS,
)
NETWORK_SOURCES = (
    INDEX['url'],
    MANIFEST['url'],
    CONFIG['url'],
    LAYER['url'],
) + tuple(UBUNTU_PORTS + item['path'] for item in DEBS)
RUNTIME_HOSTS = frozenset({
    'auth.docker.io', 'registry-1.docker.io', 'production.cloudfront.docker.com', 'ports.ubuntu.com',
})
MAX_REDIRECTS = 3
DIGEST = frozenset(item['digest'] for item in OCI_BLOBS)


def hex_digest(digest):
    if not isinstance(digest, str) or not digest.startswith('sha256:') or len(digest) != 71:
        raise Refusal('oci digest must be sha256:<64 hex>')
    value = digest[len('sha256:'):]
    if any(c not in '0123456789abcdef' for c in value):
        raise Refusal('oci digest must be lowercase hex')
    return value


def item_sha256(item):
    return item['sha256'] if 'sha256' in item else hex_digest(item['digest'])


def pin_file_identities(pin):
    """(bytes, sha256) for each fetched payload in pin order: index, manifest, config, layer, debs."""
    items = (pin['index'], pin['manifest'], pin['config'], pin['layer'], *pin['debs'])
    return [(item['bytes'], item_sha256(item)) for item in items]


def production_pin():
    """The reviewed input set. Callers that need a later occurrence swap this through runtime_image_pin."""
    debs = tuple(dict(item, url=UBUNTU_PORTS + item['path']) for item in DEBS)
    return dict(
        bytes=INPUT_BYTES,
        index=dict(INDEX),
        manifest=dict(MANIFEST),
        config=dict(CONFIG),
        layer=dict(LAYER),
        debs=debs,
        oci_blobs=(dict(INDEX), dict(MANIFEST), dict(CONFIG), dict(LAYER)),
    )


def runtime_image_pin(manifest):
    """Manifest summary must equal the reviewed pin. Any other value refuses before a byte is fetched."""
    image = (manifest.get('binding_inputs') or {}).get('runtime_image') or {}
    pin = production_pin()
    if (image.get('bytes') != pin['bytes'] or image.get('index_sha256') != hex_digest(INDEX['digest'])
            or image.get('manifest_sha256') != hex_digest(MANIFEST['digest'])
            or image.get('config_sha256') != hex_digest(CONFIG['digest'])
            or image.get('layer_sha256') != hex_digest(LAYER['digest'])
            or image.get('layer_bytes') != LAYER['bytes']
            or image.get('deb_bytes') != DEB_BYTES
            or image.get('deb_count') != len(DEBS)):
        raise Refusal('runtime image pin differs from the reviewed OCI/deb set: refuse')
    return pin


def open_runtime_https(url, headers=None):
    """HTTPS GET of a pinned runtime URL. Redirects stay on RUNTIME_HOSTS, at most MAX_REDIRECTS.
    Authorization is sent only to the host it was minted for and is dropped on a host change."""
    import http.client
    import ssl
    import urllib.parse
    current = url
    extra = dict(headers or {})
    origin_host = urllib.parse.urlsplit(url).hostname
    for _ in range(MAX_REDIRECTS + 1):
        parts = urllib.parse.urlsplit(current)
        if (parts.scheme != 'https' or parts.hostname not in RUNTIME_HOSTS or parts.port not in (None, 443)
                or parts.username or parts.password or parts.fragment):
            raise Refusal('download host or scheme is not allowlisted: refuse')
        request_headers = {'Accept-Encoding': 'identity', 'User-Agent': 'effigy-qualification'}
        if parts.hostname == origin_host:
            request_headers.update(extra)
        conn = http.client.HTTPSConnection(parts.hostname, 443, timeout=60, context=ssl.create_default_context())
        try:
            target = parts.path + ('?' + parts.query if parts.query else '')
            conn.request('GET', target, headers=request_headers)
            response = conn.getresponse()
        except BaseException:
            conn.close()
            raise
        if response.status in (301, 302, 303, 307, 308):
            location = response.getheader('Location')
            conn.close()
            if not location:
                raise Refusal('redirect without a Location: refuse')
            current = urllib.parse.urljoin(current, location)
            continue
        return conn, response
    raise Refusal('too many redirects: refuse')


def prefetch_pinned_sha256(source, destination, size, sha256, checkpoint=None):
    """Bounded prefetch of one sha256-pinned artifact: exact size, O_EXCL, fsync, chunked readback."""
    if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
        raise Refusal('pinned size required')
    if not isinstance(sha256, str) or len(sha256) != 64 or any(c not in '0123456789abcdef' for c in sha256):
        raise Refusal('pinned sha256 required')
    try:
        fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    except FileExistsError as exc:
        raise Refusal(f'{os.path.basename(destination)}: name already taken; never adopted') from exc
    digest, total = hashlib.sha256(), 0
    with os.fdopen(fd, 'wb') as out:
        while True:
            chunk = source.read(min(1024 * 1024, size - total + 1))
            if not chunk:
                break
            total += len(chunk)
            if total > size:
                out.flush()
                os.fsync(out.fileno())
                raise Refusal('artifact larger than its pinned size; partial file retained')
            digest.update(chunk)
            out.write(chunk)
            if checkpoint is not None:
                checkpoint()
        out.flush()
        os.fsync(out.fileno())
    if total != size or digest.hexdigest() != sha256:
        raise Refusal('artifact size or sha256 differs from the pin; partial file retained')
    readback = hashlib.sha256()
    with open(destination, 'rb') as check:
        for block in iter(lambda: check.read(1024 * 1024), b''):
            readback.update(block)
    if readback.hexdigest() != sha256:
        raise Refusal('readback differs from the pinned sha256')
    return dict(bytes=total, sha256=sha256)


def fetch_pinned_https(url, destination, size, sha256, *, opener, headers=None, checkpoint=None):
    """Fetch one pinned artifact. Refuses unless the final response is 200 with identity encoding, no chunked
    transfer, and a Content-Length equal to the pinned size. Only then are payload bytes written."""
    conn, response = opener(url, headers)
    try:
        if response.status != 200:
            raise Refusal(f'download answered {response.status}: refuse')
        if (response.getheader('Content-Encoding') or 'identity').strip().lower() != 'identity':
            raise Refusal('content encoding is not identity: refuse')
        if response.getheader('Transfer-Encoding'):
            raise Refusal('chunked transfer has no pinned length: refuse')
        length = response.getheader('Content-Length') or ''
        if not length.isdigit() or int(length) != size:
            raise Refusal('Content-Length is absent or differs from the pinned size: refuse before any byte is written')
        return prefetch_pinned_sha256(response, destination, size, sha256, checkpoint=checkpoint)
    finally:
        conn.close()


def fetch_docker_token(opener):
    """Anonymous Hub token. Not a payload and not written. Cap is TOKEN_CAP_BYTES."""
    conn, response = opener(AUTH_TOKEN_URL, None)
    try:
        if response.status != 200:
            raise Refusal(f'token answered {response.status}: refuse')
        if (response.getheader('Content-Encoding') or 'identity').strip().lower() != 'identity':
            raise Refusal('token encoding is not identity: refuse')
        length = response.getheader('Content-Length') or ''
        if length.isdigit() and int(length) > TOKEN_CAP_BYTES:
            raise Refusal('token larger than its cap: refuse')
        body = response.read(TOKEN_CAP_BYTES + 1)
        if len(body) > TOKEN_CAP_BYTES:
            raise Refusal('token larger than its cap: refuse')
        try:
            token = json.loads(body.decode())['token']
        except (ValueError, KeyError, UnicodeDecodeError) as exc:
            raise Refusal('token is not a JSON object with a token: refuse') from exc
        if not isinstance(token, str) or not token or len(token) > 8192:
            raise Refusal('token is empty or oversized: refuse')
        return token
    finally:
        conn.close()


def select_arm64_descriptor(index_bytes, pin):
    """The Hub index must name exactly one linux/arm64 (v8) manifest, and it must be the pinned one."""
    try:
        index = json.loads(index_bytes)
    except ValueError as exc:
        raise Refusal('upstream index is not JSON: refuse') from exc
    if not isinstance(index, dict) or index.get('mediaType') != pin['index']['media_type']:
        raise Refusal('upstream index media type differs from the pin: refuse')
    matches = []
    for item in index.get('manifests') or []:
        if not isinstance(item, dict):
            continue
        platform = item.get('platform') or {}
        if platform.get('os') == 'linux' and platform.get('architecture') == 'arm64':
            matches.append(item)
    if len(matches) != 1:
        raise Refusal('upstream index must name exactly one linux/arm64 manifest: refuse')
    item = matches[0]
    if item.get('digest') != pin['manifest']['digest'] or item.get('size') != pin['manifest']['bytes']:
        raise Refusal('arm64 manifest descriptor differs from the pin: refuse')
    if (item.get('platform') or {}).get('variant') not in (None, 'v8'):
        raise Refusal('arm64 variant is not v8: refuse')
    return item


def verify_arm64_config(config_bytes):
    try:
        config = json.loads(config_bytes)
    except ValueError as exc:
        raise Refusal('image config is not JSON: refuse') from exc
    if not isinstance(config, dict):
        raise Refusal('image config is not an object: refuse')
    if config.get('os') != 'linux' or config.get('architecture') != 'arm64':
        raise Refusal('image config architecture is not linux/arm64: refuse')
    return config


def verify_arm64_manifest(manifest_bytes, pin):
    try:
        manifest = json.loads(manifest_bytes)
    except ValueError as exc:
        raise Refusal('image manifest is not JSON: refuse') from exc
    if not isinstance(manifest, dict) or manifest.get('mediaType') != pin['manifest']['media_type']:
        raise Refusal('image manifest media type differs from the pin: refuse')
    config = manifest.get('config') or {}
    layers = manifest.get('layers') or []
    if config.get('digest') != pin['config']['digest'] or config.get('size') != pin['config']['bytes']:
        raise Refusal('image manifest config descriptor differs from the pin: refuse')
    if len(layers) != 1 or layers[0].get('digest') != pin['layer']['digest'] or layers[0].get('size') != pin['layer']['bytes']:
        raise Refusal('image manifest layer descriptor differs from the pin: refuse')
    return manifest


def write_exclusive(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())


def mkdir_exclusive(path):
    try:
        os.mkdir(path, 0o700)
    except FileExistsError as exc:
        raise Refusal(f'{os.path.basename(path)}: name already taken; never adopted') from exc
    info = os.lstat(path)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise Refusal('created directory is not owned 0700')
    return path


def assemble_oci_layout(oci_dir, descriptor):
    """Write oci-layout and a single-platform index.json that names only the pinned arm64 manifest.
    The upstream index bytes stay in index.upstream.json. No assembled-archive hash is claimed."""
    write_exclusive(os.path.join(oci_dir, 'oci-layout'), b'{"imageLayoutVersion":"1.0.0"}\n')
    filtered = dict(schemaVersion=2, mediaType=INDEX['media_type'], manifests=[descriptor])
    write_exclusive(os.path.join(oci_dir, 'index.json'),
                    (json.dumps(filtered, separators=(',', ':')) + '\n').encode())


def fetch_runtime_inputs(destination, pin, *, opener, checkpoint=None):
    """Fetch the pinned OCI blobs and debs into an empty owned directory, then assemble the layout."""
    try:
        info = os.lstat(destination)
    except OSError as exc:
        raise Refusal('the runtime input directory is missing: the make step must create it first') from exc
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700
            or os.listdir(destination)):
        raise Refusal('the runtime input directory must be the empty, owned, 0700 directory the make step created')
    token = fetch_docker_token(opener)
    bearer = {'Authorization': 'Bearer ' + token,
              'Accept': 'application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json'}
    oci = mkdir_exclusive(os.path.join(destination, 'oci'))
    blobs = mkdir_exclusive(os.path.join(oci, 'blobs'))
    blob_dir = mkdir_exclusive(os.path.join(blobs, 'sha256'))
    debs_dir = mkdir_exclusive(os.path.join(destination, 'debs'))
    fetched = 0
    files = []

    def pull(item, directory, name, headers):
        nonlocal fetched
        dest = os.path.join(directory, name)
        digest = item_sha256(item)
        result = fetch_pinned_https(item['url'], dest, item['bytes'], digest, opener=opener,
                                    headers=headers, checkpoint=checkpoint)
        fetched += result['bytes']
        files.append(dict(path=dest, **result))
        return dest

    index_path = pull(pin['index'], oci, 'index.upstream.json', bearer)
    index_bytes = Path(index_path).read_bytes()
    descriptor = select_arm64_descriptor(index_bytes, pin)
    pull(pin['manifest'], blob_dir, hex_digest(pin['manifest']['digest']), bearer)
    verify_arm64_manifest(Path(os.path.join(blob_dir, hex_digest(pin['manifest']['digest']))).read_bytes(), pin)
    config_path = pull(pin['config'], blob_dir, hex_digest(pin['config']['digest']), bearer)
    verify_arm64_config(Path(config_path).read_bytes())
    pull(pin['layer'], blob_dir, hex_digest(pin['layer']['digest']), bearer)
    for deb in pin['debs']:
        pull(deb, debs_dir, deb['name'], None)
    assemble_oci_layout(oci, descriptor)
    if fetched != pin['bytes']:
        raise Refusal('fetched payload bytes differ from the pin')
    return dict(bytes=fetched, oci=oci, debs=debs_dir, files=files)
