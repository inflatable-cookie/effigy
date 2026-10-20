#!/usr/bin/env python3
"""Operator proof: old Compose vs pure CLI on identical imported consumer pins.

All writes, including the archived reference renderer's legacy writes, remain
inside one mkdtemp. Real consumer manifests and caches are read only. Requires
PyYAML; intentionally separate from portable CI fixtures.
"""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import yaml

candidate = Path(sys.argv[1]).resolve()
source = Path(__file__).resolve().parents[2]
real_home = Path.home()


def run(args, cwd, env=None, timeout=90):
    result = subprocess.run([str(a) for a in args], cwd=cwd, env=env,
                            capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise AssertionError((args, result.returncode, result.stdout, result.stderr))
    return result.stdout


def copy_tree(src, dst):
    # A copy must never follow a consumer symlink into a real sibling checkout.
    if src.is_symlink():
        raise AssertionError(f"symlink in proof input: {src}")
    if src.is_dir():
        dst.mkdir(parents=True, exist_ok=True)
        for child in src.iterdir():
            if child.name not in [".git", ".effigy"]:
                copy_tree(child, dst / child.name)
    else:
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)


def snapshot(root):
    result = {}
    for path in [root, *sorted(root.rglob("*"))]:
        m = path.lstat()
        result[str(path.relative_to(root))] = (m.st_mode, m.st_ino, m.st_mtime_ns,
                                             m.st_ctime_ns, None if path.is_dir() else path.read_bytes())
    return result


def unlock_remove(path):
    # Only our private archive and fixtures; never follow links while unsealing.
    for directory, _, _ in os.walk(path, followlinks=False):
        os.chmod(directory, 0o700)
    shutil.rmtree(path)


# Keep a failed private reference build for diagnosis and incremental retry.
# Reuse is accepted only with our owner marker, never an arbitrary directory.
private = os.environ.get("EFFIGY_RENDER_PROOF_REUSE") or tempfile.mkdtemp(prefix="effigy-render-consumers-")
owner = Path(private).resolve()
marker = owner / ".effigy-render-proof-owner"
if os.environ.get("EFFIGY_RENDER_PROOF_REUSE"):
    assert marker.read_text() == str(source), "not this proof's private fixture"
else:
    marker.write_text(str(source))
print(f"private consumer proof: {owner}", flush=True)
def prove_consumers():
    owner = Path(private).resolve()
    baseline = owner / "baseline"
    baseline.mkdir(exist_ok=True)
    base = run(["git", "merge-base", "HEAD", "origin/main"], source).strip()
    archive = subprocess.run(["git", "archive", base], cwd=source, capture_output=True, check=True).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        # This archive is our committed base; skip links so the private build
        # cannot inherit a link into another checkout.
        members = [m for m in tar.getmembers() if not (m.issym() or m.islnk())]
        assert all(not Path(m.name).is_absolute() and ".." not in Path(m.name).parts for m in members)
        tar.extractall(baseline, members=members)
    manifest = baseline / "crates/effigy-containers/Cargo.toml"
    text = manifest.read_text().replace('[dependencies]\n', '[dependencies]\neffigy-kernel = { path = "../effigy-kernel" }\n')
    manifest.write_text(text)
    example = baseline / "crates/effigy-containers/examples/container-render-baseline.rs"
    example.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / "tests/fixtures/container-render-baseline.rs", example)
    (baseline / "effigy.toml").write_text('''[tasks]
"proof:baseline:build" = [
  {run="cargo build -p effigy-containers --example container-render-baseline --profile test", timeout_ms=1200000},
  {run="cargo clippy -p effigy-containers --example container-render-baseline -- -D warnings", timeout_ms=1200000},
]
''')
    # No shared target directory or links to another checkout's build artifacts.
    build_env = dict(os.environ, CARGO_TARGET_DIR=str(baseline / "target"))
    print(f"building reference renderer at {base}", flush=True)
    print(run(["effigy", "proof:baseline:build"], baseline, build_env, timeout=1500), flush=True)
    helper = baseline / "target/debug/examples/container-render-baseline"
    for name, real in [("cbs", real_home / "Dev/legacy/sites/cbs"),
                       ("acowtancy", real_home / "Dev/projects/acowtancy")]:
        case = owner / name
        if case.exists():
            unlock_remove(case)
        repo, home = case / name, case / "home"
        repo.mkdir(parents=True)
        home.mkdir()
        env = dict(os.environ, HOME=str(home), EFFIGY_RENDER_PROOF_ROOT=str(owner),
                   EFFIGY_RENDER_PROOF_EXECUTABLE=str(candidate),
                   EFFIGY_TEST_CONTAINERS_HOME=str(home / ".effigy"),
                   COLIMA_HOME=str(home / ".colima"))
        for key in ["EFFIGY_WORKSPACE_CONTAINER", "EFFIGY_CONTAINER_HANDOFF", "EFFIGY_ACTIVE_VERSION"]:
            env.pop(key, None)
        inspected = json.loads(run([helper, "inspect", real], source))
        (case / "inputs.json").write_text(json.dumps(inspected, indent=2))
        global_config = real_home / ".effigy/config.toml"
        if global_config.is_file():
            copy_tree(global_config, home / ".effigy/config.toml")
        def mount_sources(value):
            if isinstance(value, dict):
                for key, item in value.items():
                    if key == "mounts" and isinstance(item, list):
                        for mount in item:
                            raw = mount.split(":")[0] if isinstance(mount, str) else mount.get("host", mount.get("source"))
                            if not raw or Path(raw).is_absolute() or raw.startswith("~"):
                                continue
                            original, copied = (real / raw).resolve(), (repo / raw).resolve()
                            assert copied.is_relative_to(owner), copied
                            if original.is_dir():
                                copied.mkdir(parents=True, exist_ok=True)
                            elif original.is_file():
                                copy_tree(original, copied)
                    mount_sources(item)
            elif isinstance(value, list):
                for item in value:
                    mount_sources(item)
        mount_sources(inspected["effective"])
        for path in inspected["inputs"]:
            path = Path(path)
            target = repo / path.relative_to(real)
            copy_tree(path, target)
        for directory in ["infra", "state"]:
            if (real / directory).is_dir():
                copy_tree(real / directory, repo / directory)
        # Reproduce explicit sibling mount existence inside this private parent.
        # Only their manifest inputs are needed for catalog member resolution.
        members = inspected["effective"].get("catalog", {}).get("members", {})
        for relative in [*members.values(), *(["../cbs-mortcalc"] if name == "cbs" else [])]:
            real_member = (real / relative).resolve()
            private_member = (repo / relative).resolve()
            assert private_member.is_relative_to(case), private_member
            private_member.mkdir(parents=True, exist_ok=True)
            if (real_member / "effigy.toml").exists():
                member = json.loads(run([helper, "inspect", real_member], source))
                for path in member["inputs"]:
                    path = Path(path)
                    copy_tree(path, private_member / path.relative_to(real_member))
        legacy = repo / inspected["legacy"]
        copy_tree(Path(inspected["bundle"]), legacy)
        # Pin exactly these cached bytes using the owner's Prepare API, no sync.
        run([helper, "pin", repo], repo, env)
        pins = json.loads((repo / ".effigy/sources.lock.json").read_text())
        pin = pins["sources"]["bundle"]
        # Match actual renderer configuration without letting baseline discover
        # a real HOME. Colima's existing config is a read-only input copy.
        profile = home / ".colima/effigy"
        profile.mkdir(parents=True)
        colima_config = real_home / ".colima/effigy/colima.yaml"
        if colima_config.is_file():
            copy_tree(colima_config, profile / "colima.yaml")
        else:
            (profile / "colima.yaml").write_text('network:\n  gatewayAddress: 192.168.5.2\n')
        environments = inspected["effective"]["containers"]
        environments = [k for k, v in environments.items() if isinstance(v, dict)]
        # Qualify each declared environment, including the two Underlay modes.
        for environment in environments:
            old = json.loads(run([helper, "render", repo, environment], repo, env))
            before = snapshot(case)
            output = run([candidate, "container", "render", environment, "--json"], repo, env)
            after = snapshot(case)
            assert after == before, (name, environment, "render wrote private consumer state")
            new = json.loads(output)
            new = new.get("result", new)
            assert not Path(new["path"]).exists(), "render published objects"
            new_files = {f["path"]: f["content"] for f in new["files"]}
            assert len(old["compose_files"]) == len(new["compose_files"]), (name, environment)
            replacements = []
            for f in new["files"]:
                logical = f["name"]
                if logical.startswith("checkout/"):
                    original = repo / logical.removeprefix("checkout/")
                elif logical.startswith("shared/"):
                    original = home / ".effigy" / logical.removeprefix("shared/")
                else:
                    continue
                marker = "__PATH__/" + logical
                replacements.extend([(str(original), marker),
                    ("${EFFIGY_RENDER_ROOT:?Effigy render root is required}/" + logical, marker),
                    (f["path"], marker)])
            def normalize(value):
                if isinstance(value, str):
                    for original, marker in sorted(replacements, key=lambda pair: -len(pair[0])):
                        value = value.replace(original, marker)
                    return value
                if isinstance(value, list):
                    return [normalize(v) for v in value]
                if isinstance(value, dict):
                    return {k: normalize(v) for k, v in value.items()}
                return value
            def compose(content):
                value = yaml.safe_load(content)
                # Compose resolves dockerfile relative to its build context.
                # The new renderer makes this host path explicit to relocate it.
                for service in value.get("services", {}).values():
                    build = service.get("build")
                    if isinstance(build, dict) and "dockerfile" in build:
                        path = build["dockerfile"]
                        if not Path(path).is_absolute() and not path.startswith("${"):
                            build["dockerfile"] = str(Path(build["context"]) / path)
                return normalize(value)
            pairs = list(zip(old["compose_files"], new["compose_files"]))
            for old_path in old["files"]:
                if old_path not in old["compose_files"]:
                    logical = "shared/" + str(Path(old_path).relative_to(home / ".effigy"))
                    pairs.append((old_path, str(Path(new["path"]) / logical)))
            for i, (old_path, new_path) in enumerate(pairs):
                old_value = compose(old["files"][old_path])
                new_value = compose(new_files[new_path])
                if old_value != new_value:
                    import difflib
                    diff = ''.join(difflib.unified_diff(
                        yaml.safe_dump(old_value, sort_keys=True).splitlines(True),
                        yaml.safe_dump(new_value, sort_keys=True).splitlines(True),
                        fromfile=old_path, tofile=new_path))
                    raise AssertionError(f"{name}/{environment}, Compose {i}:\n{diff}")
            print(f"consumer Compose parity: {name}/{environment}; {len(pairs)} files (including shared); "
                  f"only paths differ; source {pin['version']}; render {new['digest']}; empty render filesystem diff", flush=True)
        # Source directories are sealed. Cleanup may only unseal this case.
        unlock_remove(case)
    print("actual cbs and Acowtancy inputs qualified against identical imported source pins", flush=True)
    unlock_remove(owner)

prove_consumers()
