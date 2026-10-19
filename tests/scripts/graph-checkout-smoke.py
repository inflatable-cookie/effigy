"""Exercise the compiled graph CLI against this checkout under host scheduling."""
import json
from pathlib import Path
import subprocess
import sys

binary = Path(sys.argv[1]).resolve()
root = Path.cwd()


def graph(command, *args):
    result = subprocess.run(
        [str(binary), "--json", "graph", command, *args],
        cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=540 if command == "index" else 180,
    )
    assert result.returncode == 0, result.stderr.decode(errors="replace") + result.stdout.decode(errors="replace")
    packet = json.loads(result.stdout)
    if packet.get("schema") == "effigy.command.v1":
        assert packet["ok"], packet
        packet = packet["result"]
    assert packet["schema"] == f"effigy.graph.{command}.v1", packet
    assert "catalog" not in packet
    assert Path(packet["repo_root"]).resolve() == root.resolve()
    return packet["payload"]


graph("status")
graph("index")  # Pay a potentially cold build explicitly before the lookup smoke.
node = graph("node", "crates/effigy-codegraph/src/index.rs", "--limit", "3")
assert node["freshness"]["usable"] and node["file"]["path"] == "crates/effigy-codegraph/src/index.rs"
assert node["symbols"] and len(node["edges"]) <= 3
explore = graph("explore", "per-file graph refresh transactions", "--max-files", "2", "--max-bytes", "2048")
assert explore["mode"] == "navigation" and explore["index"]["freshness"]["usable"]
assert explore["primary"] and explore["excerpts"]
search = graph("search", "run_index", "--limit", "3")
assert search["freshness"]["usable"] and search["matches"] and len(search["matches"]) <= 3
status = graph("status")
assert status["ready"] and not status["stale_paths"], status["freshness"]
assert not list(root.joinpath(".effigy/graph").glob("graph.complete*"))
print(f"checkout graph smoke passed: {status['counts']['files']} files; node/explore/search/status; no snapshot")
