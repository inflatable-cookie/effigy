"""Exercise Cargo's real default targets through the hosted partitioner."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

driver = Path(__file__).resolve().parents[2] / "scripts/ci-shared-process.py"
root = Path(subprocess.check_output(
    ["mktemp", "-d", str(Path(os.environ["TMPDIR"]) / "ci-partitioner.XXXXXX")],
    text=True,
).strip())
try:
    (root / "src").mkdir()
    (root / "examples").mkdir()
    (root / "receipts").mkdir()
    (root / "Cargo.toml").write_text('''[package]
name = "ci-partitioner-fixture"
version = "0.0.0"
edition = "2021"
[workspace]
[[example]]
name = "tested"
test = true
''')
    (root / "src/lib.rs").write_text('''/// ```
/// std::fs::write(std::path::PathBuf::from(std::env::var_os("CI_RECEIPTS").unwrap()).join("doc"), "ran").unwrap();
/// ```
pub fn documented() {}
#[test]
fn unit() {
    std::fs::write(std::path::PathBuf::from(std::env::var_os("CI_RECEIPTS").unwrap()).join("unit"), "ran").unwrap();
}
''')
    (root / "examples/tested.rs").write_text('''fn main() {}
#[test]
fn example_test() {
    std::fs::write(std::path::PathBuf::from(std::env::var_os("CI_RECEIPTS").unwrap()).join("example"), "ran").unwrap();
}
''')
    compile_only = root / "examples/compile_only.rs"
    compile_only.write_text('compile_error!("default example compilation control");\nfn main() {}\n')
    env = dict(os.environ, CI_RECEIPTS=str(root / "receipts"))

    def run():
        return subprocess.run([sys.executable, str(driver), "rest"], cwd=root,
                              env=env, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT)

    refused = run()
    assert refused.returncode != 0, refused.stdout
    assert "default example compilation control" in refused.stdout, refused.stdout
    assert not list((root / "receipts").iterdir()), refused.stdout
    compile_only.write_text("fn main() {}\n")
    passed = run()
    assert passed.returncode == 0, passed.stdout
    assert {p.name for p in (root / "receipts").iterdir()} == {"unit", "example", "doc"}, passed.stdout
    print("partitioner Cargo proof: default-example refusal; unit, example and doc receipts; all passed")
finally:
    shutil.rmtree(root)
