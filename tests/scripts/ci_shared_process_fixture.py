"""Prove Cargo's real target inventory, narrowed replay and unified features."""
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
    (root / "tests").mkdir()
    (root / "feature-user/src").mkdir(parents=True)
    (root / "Cargo.toml").write_text('''[package]
name = "ci-partitioner-fixture"
version = "0.0.0"
edition = "2021"
[features]
unified = []
[workspace]
members = ["feature-user"]
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
    (root / "tests/gateway_private.rs").write_text('''#[test]
fn ambient_cohort() {
    assert!(cfg!(feature = "unified"), "workspace features were not unified");
    std::fs::write(std::path::PathBuf::from(std::env::var_os("CI_RECEIPTS").unwrap()).join("gateway"), "ran").unwrap();
}
''')
    (root / "feature-user/Cargo.toml").write_text('''[package]
name = "feature-user"
version = "0.0.0"
edition = "2021"
[dependencies]
ci-partitioner-fixture = { path = "..", features = ["unified"] }
''')
    (root / "feature-user/src/lib.rs").write_text("pub fn helper() {}\n")
    compile_only = root / "examples/compile_only.rs"
    compile_only.write_text('compile_error!("default example compilation control");\nfn main() {}\n')
    env = dict(os.environ, CI_RECEIPTS=str(root / "receipts"))

    def run(suite="gateway"):
        return subprocess.run([sys.executable, str(driver), suite], cwd=root,
                              env=env, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT)

    refused = run()
    assert refused.returncode != 0, refused.stdout
    assert "default example compilation control" in refused.stdout, refused.stdout
    assert not list((root / "receipts").iterdir()), refused.stdout
    compile_only.write_text("fn main() {}\n")
    inventory = run("inventory")
    assert inventory.returncode == 0, inventory.stdout
    for name in ("unit", "example_test", "ambient_cohort"):
        assert f": {name}\n" in inventory.stdout, inventory.stdout
    assert not list((root / "receipts").iterdir()), inventory.stdout
    passed = run()
    assert passed.returncode == 0, passed.stdout
    assert {p.name for p in (root / "receipts").iterdir()} == {"gateway"}, passed.stdout
    print("partitioner Cargo proof: example compilation refusal; complete inventory without bodies; unified features; only gateway replayed")
finally:
    shutil.rmtree(root)
