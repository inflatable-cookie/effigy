#!/usr/bin/env python3
"""Exercise nextest's real scheduling and terminal reporter with private cases."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
COLD_CASES = {
    "command_behavior_tests": [
        "cli_deep_doctor_timeout_terminates_git_identity_tree",
        "cli_deep_doctor_timeout_terminates_health_process_tree",
        "cli_fast_doctor_timeout_terminates_cargo_metadata_tree",
        "doctor_git_cli_serial_observer_counterexample",
        "doctor_git_cli_deliberately_late_descendant",
    ],
    "execution::doctor_inventory::tests": [
        "doctor_git_timeout_process_group_evidence",
        "doctor_git_timeout_before_status_is_incomplete_evidence",
    ],
}

HELPERS = r'''
fn event(name: &str, kind: &str) {
    use std::io::Write;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::var("SCHEDULING_EVENTS").unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    file.write_all(format!("{name}|{kind}|{now}\n").as_bytes()).unwrap();
}
fn probe(name: &str, cold: bool) {
    event(name, "start");
    let control = std::env::var("SCHEDULING_OVERLAP_CONTROL").is_ok();
    if !cold || control {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let text = std::fs::read_to_string(std::env::var("SCHEDULING_EVENTS").unwrap()).unwrap();
            let count = text.lines().filter(|line| line.contains("|start|")
                && line.starts_with("ordinary") == !cold).count();
            if count >= if cold { 2 } else { 3 } { break; }
            assert!(std::time::Instant::now() < until, "expected runner concurrency was absent");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
    event(name, "end");
}
#[cfg(test)] mod ordinary {
    #[test] fn companion_a() { super::probe("ordinary-a", false); }
    #[test] fn companion_b() { super::probe("ordinary-b", false); }
    #[test] fn companion_c() { super::probe("ordinary-c", false); }
}
#[cfg(test)] mod report_control {
    #[test] fn leaky_success() {
        // Deliberately leave an inherited pipe open beyond nextest's leak
        // threshold. The supervisor waits for this finite child before cleanup.
        let child = std::process::Command::new("/bin/sleep").arg("1").spawn().unwrap();
        super::event(&format!("owned-report-child-{}", child.id()), "spawned");
        drop(child);
    }
    #[test] fn expected_failure() { panic!("deliberate terminal reporter control"); }
}
'''


def module_source(parts, tests):
    body = "\n".join(
        f'#[test] fn {name}() {{ crate::probe("{name}", true); }}' for name in tests
    )
    for part in reversed(parts):
        body = f"mod {part} {{ {body} }}"
    return "#[cfg(test)] " + body


def supervise(argv):
    # Keep this session leader alive until its children settle. Its retained
    # Child in the outer controller prevents PID/group reuse during cleanup.
    group = os.getpgrp()
    try:
        code = subprocess.run(argv).returncode
        until = time.monotonic() + 3
        while True:
            probe = subprocess.Popen(["/bin/ps", "-axo", "pid=,pgid=,stat="],
                                     stdout=subprocess.PIPE, text=True)
            output, _ = probe.communicate(timeout=2)
            assert probe.returncode == 0, "could not inspect private process group"
            live = [row for row in (line.split() for line in output.splitlines())
                    if len(row) == 3 and int(row[1]) == group
                    and int(row[0]) not in {os.getpid(), probe.pid}
                    and not row[2].startswith("Z")]
            if not live:
                return code
            assert time.monotonic() < until, f"private children did not settle: {live}"
            time.sleep(0.05)
    except BaseException:
        # Only this newly created, still-owned session; never receipt PIDs.
        os.killpg(group, signal.SIGKILL)
        raise


def run(crate, config, profile, events, log, *, counterexample=False, report=False):
    env = dict(os.environ, SCHEDULING_EVENTS=str(events), CARGO_TARGET_DIR=str(crate / "target"))
    if counterexample:
        env["SCHEDULING_OVERLAP_CONTROL"] = "1"
    else:
        env.pop("SCHEDULING_OVERLAP_CONTROL", None)
    argv = ["cargo", "nextest", "run", "--offline", "--config-file", str(config),
            "--user-config-file", "none", "--profile", profile, "--no-fail-fast",
            "--retries", "0", "--no-tests", "fail", "-j", "4", "--filterset",
            "test(report_control::)" if report else "!test(report_control::)"]
    with log.open("wb") as output:
        child = subprocess.Popen([sys.executable, str(Path(__file__).resolve()),
                                  "--supervise", *argv], cwd=crate, env=env, stdout=output,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=60)
        except BaseException:
            # This exact session was started here and its leader is unreaped.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
            raise
    assert code == (100 if report else 0), (code, log.read_text())
    return code


def intervals(events):
    records = {}
    for line in events.read_text().splitlines():
        name, kind, stamp = line.split("|")
        assert kind in {"start", "end"}, line
        assert kind not in records.setdefault(name, {}), line
        records[name][kind] = int(stamp)
    assert len(records) == 10, records
    assert all(set(row) == {"start", "end"} and row["start"] < row["end"]
               for row in records.values()), records
    return records


def overlaps(left, right):
    return max(left["start"], right["start"]) < min(left["end"], right["end"])


def main():
    reports = ROOT / ".effigy/reports/doctor-fixture-scheduling" / str(os.getpid())
    reports.mkdir(parents=True, mode=0o700)
    candidate = (ROOT / ".config/nextest.toml").read_text()
    weight = 'threads-required = "num-test-threads"'
    assert candidate.count(weight) == 1, "expected one finite-family slot reservation"
    with tempfile.TemporaryDirectory(prefix="effigy-doctor-scheduling-") as private:
        crate = Path(private)
        (crate / "src").mkdir()
        (crate / "Cargo.toml").write_text(
            '[package]\nname="doctor-scheduling-control"\nversion="0.0.0"\nedition="2021"\n[workspace]\n'
        )
        (crate / "src/lib.rs").write_text(HELPERS + "\n" + "\n".join(
            module_source(module.split("::"), names) for module, names in COLD_CASES.items()
        ))
        for name, text in [("candidate", candidate), ("counterexample", candidate.replace(weight, "threads-required = 1"))]:
            (crate / f"{name}.toml").write_text(text)
        receipt = []
        for name, profile, control in [("counterexample", "default", True),
                                       ("candidate", "default", False),
                                       ("candidate", "doctor-process", False)]:
            label = f"{name}-{profile}"
            events = reports / f"{label}.events"
            run(crate, crate / f"{name}.toml", profile, events, reports / f"{label}.log",
                counterexample=control)
            rows = intervals(events)
            cold_overlap = any(overlaps(row, other) for test, row in rows.items()
                               if not test.startswith("ordinary")
                               for peer, other in rows.items() if peer != test)
            assert cold_overlap == control, (label, rows)
            assert any(overlaps(rows[a], rows[b]) for a in rows for b in rows
                       if a != b and a.startswith("ordinary") and b.startswith("ordinary")), rows
            receipt.append({"case": label, "coldOverlap": cold_overlap, "ordinaryOverlap": True,
                            "requiredCases": 7, "skipped": 0, "exit": 0})
        log = reports / "terminal-report.log"
        code = run(crate, crate / "candidate.toml", "default", reports / "report.events", log, report=True)
        text = log.read_text()
        tail = text.encode()[-65536:].decode(errors="replace")
        summary = tail.rfind("Summary")
        assert summary >= 0 and "leaky_success" in tail[summary:] and "expected_failure" in tail[summary:], tail
        assert "LEAK" in tail[summary:] and "FAIL" in tail[summary:], tail
        receipt.append({"case": "terminal-report", "expectedExit": 100, "actualExit": code,
                        "failedAndLeakyIdentitiesInFinalTail": True, "ownedGroupSettled": True})
    (reports / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"reports": str(reports), "controls": receipt}, indent=2))


if __name__ == "__main__":
    def interrupted(signum, _frame):
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, interrupted)
    if len(sys.argv) > 1 and sys.argv[1] == "--supervise":
        sys.exit(supervise(sys.argv[2:]))
    else:
        main()
