#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
artifact_dir="$repo_root/.effigy/demo/artifacts/lifecycle-window"
status_file="$artifact_dir/status.txt"
heartbeat_file="$artifact_dir/heartbeat.txt"
events_file="$artifact_dir/events.log"
index_file="$artifact_dir/index.html"
started_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

mkdir -p "$artifact_dir"

write_index() {
  cat > "$index_file" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Effigy Demo Lifecycle Window</title>
    <style>
      :root {
        color-scheme: light;
        --ink: #111e30;
        --muted: #506074;
        --line: #d8dee7;
        --panel: #f6f8fb;
        --accent: #006b5f;
      }
      body {
        margin: 0;
        font-family: "Iowan Old Style", "Palatino Linotype", Georgia, serif;
        color: var(--ink);
        background: linear-gradient(180deg, #fbfcfe 0%, #edf4ef 100%);
      }
      main {
        max-width: 760px;
        margin: 0 auto;
        padding: 48px 24px 72px;
      }
      .panel {
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 14px;
        padding: 18px 20px;
        margin-top: 18px;
      }
      a {
        color: var(--accent);
      }
      code {
        font-family: "SFMono-Regular", "Menlo", monospace;
      }
      .muted {
        color: var(--muted);
      }
    </style>
  </head>
  <body>
    <main>
      <h1>Effigy Demo Lifecycle Window</h1>
      <p>
        This run-backed demo stays active until Effigy stops it. It exists to
        prove active attempt state, stoppability, and terminal receipt updates
        on a real process instead of a synthetic fixture.
      </p>
      <div class="panel">
        <strong>Started at:</strong> <span class="muted">$started_at</span><br>
        <strong>PID:</strong> <code>$$</code><br>
        <strong>Status file:</strong> <a href="./status.txt">status.txt</a><br>
        <strong>Heartbeat:</strong> <a href="./heartbeat.txt">heartbeat.txt</a><br>
        <strong>Event log:</strong> <a href="./events.log">events.log</a>
      </div>
      <div class="panel">
        Stop this demo with <code>effigy demo stop lifecycle-window</code>, then
        inspect it again to see the active attempt clear and the latest receipt
        flip to a terminal outcome.
      </div>
    </main>
  </body>
</html>
EOF
}

handle_exit() {
  finished_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  printf 'terminated at %s\n' "$finished_at" > "$status_file"
  printf 'terminated %s pid=%s\n' "$finished_at" "$$" >> "$events_file"
  exit 0
}

trap 'handle_exit' INT TERM

printf 'running since %s\n' "$started_at" > "$status_file"
printf 'started %s pid=%s\n' "$started_at" "$$" >> "$events_file"
write_index

while :; do
  now=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  printf '%s\n' "$now" > "$heartbeat_file"
  printf 'heartbeat %s pid=%s\n' "$now" "$$" >> "$events_file"
  sleep 1
done
