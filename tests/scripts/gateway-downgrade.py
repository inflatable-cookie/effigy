"""Runs only inside the proof's fresh Linux container, as its root user."""
import http.server, json, os, pathlib, shutil, socket, struct
import subprocess, tempfile, threading, time, urllib.request

assert os.geteuid() == 0
artifacts = pathlib.Path('/artifacts')
home = pathlib.Path(tempfile.mkdtemp(prefix='gateway-0141-'))
gateway = home / '.effigy/gateway'
gateway.mkdir(parents=True)
for name in ['routes.json', 'loopback-ips.json']:
    shutil.copyfile(artifacts / 'home/.effigy/gateway' / name, gateway / name)
shutil.copyfile(artifacts / 'home/.effigy/ports.json', home / '.effigy/ports.json')
expected = json.loads((artifacts / 'home/expected.json').read_bytes())
paths = {'routes': gateway / 'routes.json', 'loopback': gateway / 'loopback-ips.json', 'ports': home / '.effigy/ports.json'}
for family, path in paths.items():
    assert json.loads(path.read_bytes()) == expected[family]
    # The result consumed by the old binary is byte-equivalent to the export.
    original = artifacts / 'home' / path.relative_to(home)
    assert path.read_bytes() == original.read_bytes()
assert expected['ports']['allocations']['site']['assigned_ports'] == {'web:80': 18080, 'database:5432': 18081}
assert expected['loopback']['assignments']['site']['ip'] == '127.1.0.7'
assert expected['loopback']['assignments']['added']['ip'] == '127.1.0.8'

class Upstream(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = self.headers['Host'].encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *_): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 18080), Upstream)
threading.Thread(target=server.serve_forever, daemon=True).start()
echo = socket.socket()
echo.bind(('127.0.0.1', 18081))
echo.listen()
def echo_loop():
    while True:
        try: conn, _ = echo.accept()
        except OSError: return
        with conn: conn.sendall(conn.recv(128))
threading.Thread(target=echo_loop, daemon=True).start()

def dns(name):
    label = b''.join(bytes([len(x)]) + x.encode() for x in name.split('.')) + b'\0'
    query = struct.pack('!6H', 0x237, 0x100, 1, 0, 0, 0) + label + struct.pack('!2H', 1, 1)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client:
        client.settimeout(.5)
        client.sendto(query, ('127.0.0.1', 15353))
        response, _ = client.recvfrom(4096)
    assert response[:2] == query[:2] and struct.unpack('!H', response[6:8])[0] >= 1
    assert socket.inet_aton('127.1.0.7') in response

binary = artifacts / 'effigy-0.14.1'
version = subprocess.check_output([str(binary), '--version'], text=True, timeout=10).strip()
assert '0.14.1' in version, version
log = (artifacts / 'legacy-gateway.log').open('wb')
env = dict(os.environ, HOME=str(home))
env.pop('SUDO_UID', None)
env.pop('SUDO_USER', None)
process = subprocess.Popen([str(binary), '__gateway-run'], cwd=home, env=env, stdout=log, stderr=log)
try:
    deadline = time.monotonic() + 30
    while True:
        try:
            assert process.poll() is None, 'released gateway exited'
            for domain in ['original.test', 'added.test']:
                req = urllib.request.Request('http://127.0.0.1/', headers={'Host': domain})
                with urllib.request.urlopen(req, timeout=.5) as response:
                    assert response.read() == domain.encode()
            dns('database.test')
            with socket.create_connection(('127.1.0.7', 15432), timeout=.5) as client:
                client.sendall(b'gateway-downgrade-237')
                assert client.recv(128) == b'gateway-downgrade-237'
            break
        except (OSError, AssertionError) as exc:
            if process.poll() is not None or time.monotonic() >= deadline:
                raise AssertionError((str(exc), (artifacts / 'legacy-gateway.log').read_text())) from exc
            time.sleep(.05)
    for family, path in paths.items(): assert json.loads(path.read_bytes()) == expected[family]
    print(json.dumps({'version': version, 'pid': process.pid, 'uid': os.geteuid(), 'http_routes': ['original.test','added.test'], 'dns_tcp_alias': '127.1.0.7:15432', 'pins': expected['ports']['allocations']['site']['assigned_ports'], 'result': 'passed'}))
finally:
    process.terminate()
    try: code = process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        process.kill()
        code = process.wait(timeout=5)
    log.close()
    server.shutdown()
    echo.close()
    (artifacts / 'legacy-exit.txt').write_text(str(code) + '\n')
