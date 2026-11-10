//! The outer Rhai helper must relay build bytes before the producer closes.
//! A PTY is essential: pipe and JSON output deliberately retain capture.
use std::fs;
use std::process::Command;

use effigy_test_support::process::FixtureCommandExt;

use super::support::temp_execution_workspace;

#[test]
fn rhai_build_output_streams_before_closure_and_receipt_parses_afterward() {
    let root = temp_execution_workspace("live-build-output");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\ndescription = \"Building Linux fixture\"\nrun = [{ rhai = \"build.rhai\" }]\n",
    )
    .expect("manifest");
    fs::write(
        root.join("build.rhai"),
        r#"
let result = process::stream("sh", ["build.sh"]);
if !result["success"] { throw "build failed"; }
if fs::file_size("build.receipt") > 65536 { throw "receipt over cap"; }
let receipt = fs::env_file_map("build.receipt");
if receipt["binary"] != "fixture" || receipt["sha256"] != "fixture-digest" {
    throw "invalid receipt";
}
fs::write_file("receipt-parsed", "ok");
"#,
    )
    .expect("Rhai producer");
    fs::write(
        root.join("build.sh"),
        "set -eu\nprintf 'BUILD-STDOUT-LIVE\\n'\nprintf 'BUILD-STDERR-LIVE\\n' >&2\nwhile [ ! -f release-build ]; do sleep 0.02; done\nprintf 'binary=fixture\\nsha256=fixture-digest\\n' > build.receipt\n",
    )
    .expect("build producer");

    // Python owns the PTY descriptors and the exact child PID, and always reaps
    // it. The file handshake proves ordering without a timing-only assertion.
    let output = Command::new("python3")
        .args(["-I", "-B", "-c", PTY_PROOF])
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .arg(&root)
        .output_owned()
        .expect("PTY proof");
    assert!(
        output.status.success(),
        "PTY proof failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join("receipt-parsed")).unwrap(),
        "ok"
    );
}

const PTY_PROOF: &str = r#"
import errno, os, pathlib, pty, select, subprocess, sys, time
binary, root = sys.argv[1:]
root = pathlib.Path(root)
env = os.environ.copy()
for key in ('CI', 'EFFIGY_MANAGED_PANE', 'EFFIGY_INTERNAL_MANAGED_PANE', 'EFFIGY_EXECUTABLE'):
    env.pop(key, None)
env['EFFIGY_EXECUTABLE'] = binary
env['TERM'] = 'xterm-256color'
master, slave = pty.openpty()
child = subprocess.Popen([binary, 'build'], cwd=root, env=env,
                         stdin=subprocess.DEVNULL, stdout=slave, stderr=slave)
os.close(slave)
transcript = bytearray()
def read_once():
    if select.select([master], [], [], .05)[0]:
        try:
            data = os.read(master, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                return False
            raise
        transcript.extend(data)
        return bool(data)
    return True
try:
    deadline = time.monotonic() + 30
    while not all(marker in transcript for marker in (b'BUILD-STDOUT-LIVE', b'BUILD-STDERR-LIVE')):
        assert child.poll() is None, bytes(transcript)
        assert time.monotonic() < deadline, bytes(transcript)
        read_once()
    assert child.poll() is None, bytes(transcript)
    assert not (root / 'build.receipt').exists(), bytes(transcript)
    (root / 'release-build').write_text('release')
    while child.poll() is None:
        assert time.monotonic() < deadline, bytes(transcript)
        read_once()
    while read_once():
        assert time.monotonic() < deadline, bytes(transcript)
    assert child.wait() == 0, bytes(transcript)
    assert (root / 'receipt-parsed').read_text() == 'ok', bytes(transcript)
    text = transcript.decode(errors='replace')
    assert text.count('BUILD-STDOUT-LIVE') == 1, text
    assert text.count('BUILD-STDERR-LIVE') == 1, text
    assert '\u2713' in text and 'finished' in text, text
    print(text)
finally:
    (root / 'release-build').touch()
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
    os.close(master)
"#;
