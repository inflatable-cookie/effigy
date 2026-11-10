"""Bounded author proof: the existing checked probe under owned CPU load."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

root = Path(tempfile.mkdtemp(prefix="probe-load.", dir=os.environ["TMPDIR"]))
workers = []
results = []
try:
    count = min(12, os.cpu_count() or 1)
    for _ in range(count):
        workers.append(subprocess.Popen(
            [sys.executable, "-I", "-B", "-c",
             "import hashlib,json,os,signal,sys,time; from pathlib import Path\n"
             "stop=[False]; signal.signal(signal.SIGTERM,lambda *_:stop.__setitem__(0,True))\n"
             "until=time.monotonic()+300; started=time.process_time(); cycles=0\n"
             "while time.monotonic()<until and not stop[0]:\n"
             " hashlib.sha256(b'probe-load'*1000).digest(); cycles+=1\n"
             "Path(sys.argv[1]).write_text(json.dumps(dict(pid=os.getpid(),cycles=cycles,cpu_seconds=time.process_time()-started)))",
             str(root / f"worker-{len(workers)}.json")],
            cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        ))
    ownership = {"cpus": os.cpu_count(), "load_pids": [p.pid for p in workers]}
    (root / "load-pids.json").write_text(json.dumps(ownership))
    print(json.dumps(ownership), flush=True)
    for iteration in range(1, 21):
        assert all(p.poll() is None for p in workers), "load ended before proof"
        started = time.monotonic()
        result = subprocess.run(sys.argv[1:], text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT)
        print(result.stdout, flush=True)
        plain = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", result.stdout)
        run_id = re.search(r"Nextest run ID (\S+)", plain)
        receipt = {
            "iteration": iteration, "exit": result.returncode,
            "nextest_id": run_id[1] if run_id else None,
            "wall_seconds": time.monotonic() - started,
            "load_workers_alive": sum(p.poll() is None for p in workers),
        }
        results.append(receipt)
        print(json.dumps(receipt), flush=True)
        assert result.returncode == 0, "checked probe failed under load"
        assert "1 test run: 1 passed" in plain, "missing exact-case receipt"
        assert receipt["load_workers_alive"] == count, "load ended during proof"
    print(json.dumps({"passed": len(results), "attempted": 20}), flush=True)
finally:
    for worker in workers:
        if worker.poll() is None:
            worker.terminate()
        try:
            worker.wait(timeout=5)
        except subprocess.TimeoutExpired:
            worker.kill()
            worker.wait(timeout=5)
    receipts = [json.loads(path.read_text()) for path in sorted(root.glob("worker-*.json"))]
    print(json.dumps({"load_exits": [p.returncode for p in workers],
                      "worker_cpu_receipts": receipts}), flush=True)
    shutil.rmtree(root)
