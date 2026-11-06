#!/usr/bin/env python3
"""Run the canonical later QA stages once and retain a truthful partial receipt."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone


MAX_STREAM_BYTES = 64 * 1024 * 1024
CLEANUP_SECONDS = 5
STAGES = (
    "test:kernel:faults",
    "test:kernel:matrix",
    "test:kernel:custody:later",
    "test:kernel:handoff:later",
    "test:kernel:probe",
    "test:kernel:remote",
    "test:kernel:helpers",
    "test:kernel:gateway-budget",
    "test:kernel:machine-facts",
    "test:secrets:vault",
    "qa:docs:links",
    "qa:docs:examples",
    "qa:docs:agent-defaults",
    "qa:docs:knowledge",
    "qa:docs:changelog",
    "qa:docs:cargo-routes",
    "qa:docs:human-error-debug",
    "qa:docs:mktemp",
    "qa:json",
    "lint:feedback:inventory",
    "lint:feedback:clippy",
)
CANCELLED = threading.Event()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True,
    )
    return result.stdout.strip()


def git_bytes(root: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def input_snapshot(root: Path) -> dict[str, object]:
    entries = git_bytes(root, "ls-files", "-s", "-z").split(b"\0")
    files: dict[str, str] = {}
    for entry in entries:
        if not entry:
            continue
        metadata, raw_name = entry.split(b"\t", 1)
        mode, object_id, _stage = metadata.decode().split()
        name = os.fsdecode(raw_name)
        if mode == "160000":
            files[name] = "gitlink:" + object_id
            continue
        path = root / name
        try:
            if path.is_symlink():
                content = os.readlink(path).encode("utf-8", "surrogateescape")
                files[name] = hashlib.sha256(b"symlink\0" + content).hexdigest()
            elif path.is_file():
                files[name] = sha256_file(path)
            else:
                files[name] = "missing"
        except OSError:
            files[name] = "unreadable"
    index_path = Path(git(root, "rev-parse", "--git-path", "index"))
    if not index_path.is_absolute():
        index_path = root / index_path
    index_hash = sha256_file(index_path) if index_path.is_file() else "missing"
    tree_digest = hashlib.sha256(
        json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return {
        "head": git(root, "rev-parse", "HEAD"),
        "index_sha256": index_hash,
        "tracked_tree_sha256": tree_digest,
        "files": files,
    }


def source_binding(root: Path, inputs: dict[str, object]) -> dict[str, str]:
    return {
        "head": str(inputs["head"]),
        "index_sha256": str(inputs["index_sha256"]),
        "tracked_tree_sha256": str(inputs["tracked_tree_sha256"]),
    }


def fast_source_binding(root: Path) -> dict[str, str]:
    index_path = Path(git(root, "rev-parse", "--git-path", "index"))
    if not index_path.is_absolute():
        index_path = root / index_path
    index_hash = sha256_file(index_path) if index_path.is_file() else "missing"
    status = git(root, "status", "--porcelain=v1", "--untracked-files=no")
    return {
        "head": git(root, "rev-parse", "HEAD"),
        "index_sha256": index_hash,
        "tracked_status_sha256": hashlib.sha256(status.encode()).hexdigest(),
    }


def reject_symlink_components(path: Path) -> None:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current = current / part
        if current.is_symlink():
            raise ValueError(f"path contains a symlink: {current}")


def validate_destinations(output: Path, temp: Path, root: Path) -> Path:
    if not output.is_absolute() or not temp.is_absolute():
        raise ValueError("output directory and TMPDIR must be absolute")
    if ".." in output.parts or ".." in temp.parts:
        raise ValueError("output directory and TMPDIR may not contain '..'")
    reject_symlink_components(output)
    reject_symlink_components(temp)
    resolved_output = output.parent.resolve(strict=True) / output.name
    resolved_root = root.resolve(strict=True)
    resolved_temp = temp.resolve(strict=True)
    if not resolved_temp.is_dir():
        raise ValueError("TMPDIR must be an existing directory")
    if resolved_output == resolved_root or resolved_root in resolved_output.parents:
        raise ValueError("output directory must be outside the checkout")
    if resolved_temp == resolved_root or resolved_root in resolved_temp.parents or resolved_temp in resolved_root.parents:
        raise ValueError("TMPDIR must be separate from the checkout")
    if resolved_output == resolved_temp or resolved_temp in resolved_output.parents:
        raise ValueError("output directory must be outside TMPDIR")
    if output.exists() or output.is_symlink():
        raise ValueError("output directory already exists")
    return resolved_output


def write_json(path: Path, value: object) -> None:
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)


def capture_pipe(pipe, output, counts: dict[str, int], errors: list[str], key: str) -> None:
    saved = 0
    total = 0
    try:
        while True:
            block = pipe.read(64 * 1024)
            if not block:
                break
            total += len(block)
            remaining = MAX_STREAM_BYTES - saved
            if remaining > 0 and output is not None:
                chunk = block[:remaining]
                try:
                    output.write(chunk)
                    saved += len(chunk)
                except OSError as exc:
                    errors.append(f"{key}: {exc}")
                    output.close()
                    output = None
    finally:
        pipe.close()
        if output is not None:
            try:
                output.flush()
                output.close()
            except OSError as exc:
                errors.append(f"{key}: {exc}")
        counts[key] = total
        counts[key + "_saved"] = saved


def process_group_exists(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def stop_owned_group(pgid: int) -> None:
    for sig, timeout in ((signal.SIGTERM, CLEANUP_SECONDS), (signal.SIGKILL, CLEANUP_SECONDS)):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            return
        deadline = time.monotonic() + timeout
        while process_group_exists(pgid) and time.monotonic() < deadline:
            time.sleep(0.05)
        if not process_group_exists(pgid):
            return


def owned_command(argv: list[str], cwd: Path, env: dict[str, str], stdout_path: Path,
                  stderr_path: Path, on_cancel=CANCELLED) -> dict[str, object]:
    started = utc_now()
    start_monotonic = time.monotonic()
    counts: dict[str, int] = {}
    errors: list[str] = []
    try:
        stdout_fd = os.open(stdout_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        stderr_fd = os.open(stderr_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except OSError as exc:
        if "stdout_fd" in locals():
            os.close(stdout_fd)
        return {
            "argv": argv, "started_at": started, "ended_at": utc_now(),
            "duration_seconds": round(time.monotonic() - start_monotonic, 3),
            "returncode": None, "signal": None, "externally_cancelled": False,
            "unsettled_child": False, "launch_error": f"cannot create output: {exc}",
            "stdout_bytes": 0, "stdout_saved_bytes": 0,
            "stderr_bytes": 0, "stderr_saved_bytes": 0,
            "output_complete": False,
        }
    stdout_file = os.fdopen(stdout_fd, "wb")
    stderr_file = os.fdopen(stderr_fd, "wb")
    try:
        process = subprocess.Popen(
            argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
        )
    except OSError as exc:
        stdout_file.close()
        stderr_file.close()
        return {
            "argv": argv, "started_at": started, "ended_at": utc_now(),
            "duration_seconds": round(time.monotonic() - start_monotonic, 3),
            "returncode": None, "signal": None, "externally_cancelled": False,
            "unsettled_child": False, "launch_error": str(exc),
            "stdout_bytes": 0, "stdout_saved_bytes": 0,
            "stderr_bytes": 0, "stderr_saved_bytes": 0,
            "output_complete": False,
        }
    out_thread = threading.Thread(target=capture_pipe, args=(process.stdout, stdout_file, counts, errors, "stdout_bytes"), daemon=True)
    err_thread = threading.Thread(target=capture_pipe, args=(process.stderr, stderr_file, counts, errors, "stderr_bytes"), daemon=True)
    out_thread.start()
    err_thread.start()
    externally_cancelled = False
    while process.poll() is None:
        if on_cancel.is_set():
            externally_cancelled = True
            break
        time.sleep(0.05)
    unsettled = False
    if externally_cancelled:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=CLEANUP_SECONDS)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=CLEANUP_SECONDS)
            except subprocess.TimeoutExpired:
                unsettled = True
    else:
        process.wait()
    out_thread.join(timeout=CLEANUP_SECONDS)
    err_thread.join(timeout=CLEANUP_SECONDS)
    if out_thread.is_alive() or err_thread.is_alive() or process_group_exists(process.pid):
        unsettled = True
        stop_owned_group(process.pid)
        out_thread.join(timeout=CLEANUP_SECONDS)
        err_thread.join(timeout=CLEANUP_SECONDS)
    rc = process.poll()
    return {
        "argv": argv,
        "started_at": started,
        "ended_at": utc_now(),
        "duration_seconds": round(time.monotonic() - start_monotonic, 3),
        "returncode": rc,
        "signal": signal.Signals(-rc).name if isinstance(rc, int) and rc < 0 else None,
        "externally_cancelled": externally_cancelled,
        "unsettled_child": unsettled,
        "capture_errors": errors,
        "launch_error": None,
        "stdout_bytes": counts.get("stdout_bytes", 0),
        "stdout_saved_bytes": counts.get("stdout_bytes_saved", 0),
        "stderr_bytes": counts.get("stderr_bytes", 0),
        "stderr_saved_bytes": counts.get("stderr_bytes_saved", 0),
        "output_complete": counts.get("stdout_bytes", 0) <= MAX_STREAM_BYTES
        and counts.get("stderr_bytes", 0) <= MAX_STREAM_BYTES and not unsettled and not errors,
    }


def command_result_ok(result: dict[str, object]) -> bool:
    return result["returncode"] == 0 and not result["externally_cancelled"] and not result["unsettled_child"] and not result.get("launch_error") and bool(result["output_complete"])


def run_collector(output: Path, root: Path, temp: Path, lifecycle: Path,
                  staged_effigy: Path, environ: dict[str, str] | None = None) -> tuple[int, dict[str, object]]:
    env = dict(os.environ if environ is None else environ)
    env["TMPDIR"] = str(temp)
    env["TMP"] = str(temp)
    env["TEMP"] = str(temp)
    env["NEXTEST_SUCCESS_OUTPUT"] = "immediate"
    resolved = validate_destinations(output, temp, root)
    resolved.mkdir(mode=0o700)
    os.chmod(resolved, 0o700)
    receipt: dict[str, object] = {
        "schema": "effigy.qa-later-stages.v1",
        "diagnostic_only": True,
        "default_suite_omitted": True,
        "full_qa_pass": False,
        "success_output": "NEXTEST_SUCCESS_OUTPUT=immediate",
        "stream_limit_bytes_each": MAX_STREAM_BYTES,
        "stage_order": list(STAGES),
        "stages": [],
        "setup": None,
        "teardown": None,
        "source_before": None,
        "source_after": None,
        "source_immutable": None,
    }
    before = input_snapshot(root)
    receipt["source_before"] = before
    setup = owned_command(["sh", str(lifecycle), "setup"], root, env,
                          resolved / "setup.stdout", resolved / "setup.stderr")
    receipt["setup"] = setup
    after_setup = input_snapshot(root)
    setup_source_changed = source_binding(root, before) != source_binding(root, after_setup)
    receipt["setup_source_changed"] = setup_source_changed
    stop = not command_result_ok(setup) or setup_source_changed
    if not stop:
        for index, selector in enumerate(STAGES, 1):
            if CANCELLED.is_set():
                stop = True
                break
            binding_start = fast_source_binding(root)
            prefix = f"{index:02d}-{selector.replace(':', '_')}"
            result = owned_command([str(staged_effigy), selector], root, env,
                                   resolved / f"{prefix}.stdout", resolved / f"{prefix}.stderr")
            binding_end = fast_source_binding(root)
            status = "completed" if command_result_ok(result) else "failed"
            if result["externally_cancelled"] or result["unsettled_child"] or result["signal"]:
                status = "cancelled"
                stop = True
            if binding_start != binding_end:
                result["source_changed_during_stage"] = True
                status = "failed"
                stop = True
            else:
                result["source_changed_during_stage"] = False
            receipt["stages"].append({
                "selector": selector,
                "argv": result["argv"],
                "started_at": result["started_at"],
                "ended_at": result["ended_at"],
                "duration_seconds": result["duration_seconds"],
                "source_binding_start": binding_start,
                "source_binding_end": binding_end,
                "status": status,
                "result": result,
                "stdout": f"{prefix}.stdout",
                "stderr": f"{prefix}.stderr",
            })
            if stop:
                break
    ran = {str(stage["selector"]) for stage in receipt["stages"]}  # type: ignore[index]
    for selector in STAGES:
        if selector not in ran:
            receipt["stages"].append({"selector": selector, "status": "not_started"})

    # Hash the retained raw stage streams before the lifecycle cleanup begins.
    receipt["raw_before_teardown"] = output_hashes(resolved)
    write_json(resolved / "raw-hash-manifest-before-teardown.json", receipt["raw_before_teardown"])
    teardown = owned_command(["sh", str(lifecycle), "teardown"], root, env,
                             resolved / "teardown.stdout", resolved / "teardown.stderr")
    receipt["teardown"] = teardown
    after = input_snapshot(root)
    receipt["source_after"] = after
    immutable = source_binding(root, before) == source_binding(root, after)
    receipt["source_immutable"] = immutable
    receipt["raw_hashes"] = output_hashes(resolved)
    failures = [s["selector"] for s in receipt["stages"] if s["status"] in ("failed", "cancelled")]
    skipped = [s["selector"] for s in receipt["stages"] if s["status"] == "not_started"]
    incomplete = [
        s["selector"] for s in receipt["stages"]
        if s.get("status") in ("failed", "cancelled")
        and isinstance(s.get("result"), dict)
        and not s["result"].get("output_complete", False)
    ]
    receipt["failed_stages"] = failures
    receipt["not_started_stages"] = skipped
    receipt["incomplete_stages"] = incomplete
    receipt["complete_partial_census"] = (
        not failures and not skipped and command_result_ok(setup)
        and command_result_ok(teardown) and immutable and not setup_source_changed
    )
    summary = ["Diagnostic later-stage census (not full QA)."]
    summary.append("Stage failures: " + (", ".join(failures) if failures else "none"))
    summary.append("Not started: " + (", ".join(skipped) if skipped else "none"))
    summary.append("Incomplete output: " + (", ".join(incomplete) if incomplete else "none"))
    summary.append("Setup: " + ("passed" if command_result_ok(setup) and not setup_source_changed else "failed/incomplete"))
    summary.append("Teardown: " + ("passed" if command_result_ok(teardown) else "failed/incomplete"))
    summary.append("Source immutable: " + ("yes" if immutable else "no"))
    receipt["human_summary"] = "\n".join(summary)
    write_json(resolved / "receipt.json", receipt)
    (resolved / "summary.txt").write_text(receipt["human_summary"] + "\n", encoding="utf-8")
    os.chmod(resolved / "summary.txt", 0o600)
    print(receipt["human_summary"])
    print(f"Receipt: {resolved / 'receipt.json'}")
    return (0 if receipt["complete_partial_census"] else 1), receipt


def output_hashes(directory: Path) -> dict[str, str]:
    return {
        path.name: sha256_file(path)
        for path in sorted(directory.iterdir())
        if path.is_file() and path.name not in {"receipt.json", "hash-manifest.json"}
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", help="fresh absolute output directory outside checkout and TMPDIR")
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    temp_value = os.environ.get("TMPDIR")
    if not temp_value:
        parser.error("TMPDIR must name the disposable run directory")
    for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(signum, lambda _sig, _frame: CANCELLED.set())
    try:
        code, receipt = run_collector(
            Path(args.output_dir), root, Path(temp_value),
            root / "scripts" / "worktree-lifecycle.sh",
            root / ".local-install" / "worktree-hooks" / "effigy",
        )
        # Keep a separately named digest manifest for reviewers and proof pinning.
        manifest = {"receipt_sha256": sha256_file(Path(args.output_dir) / "receipt.json"),
                    "outputs": receipt["raw_hashes"]}
        write_json(Path(args.output_dir) / "hash-manifest.json", manifest)
        return code
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"qa:later-stages: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
