#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
artifact_dir="$repo_root/.effigy/demo/artifacts/browser-proof-report"
generated_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

mkdir -p "$artifact_dir"

cd "$repo_root"

cargo run --bin effigy -- demo list > "$artifact_dir/list.txt"
cargo run --bin effigy -- demo list --group-by gap > "$artifact_dir/grouped-by-gap.txt"
cargo run --bin effigy -- demo inspect lifecycle-window > "$artifact_dir/inspect-lifecycle-window.txt"

cat > "$artifact_dir/index.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Effigy Demo Browser Proof Report</title>
    <style>
      :root {
        color-scheme: light;
        --ink: #122033;
        --muted: #4c5a6a;
        --line: #d8dee7;
        --panel: #f6f8fb;
        --accent: #0059b3;
      }
      body {
        margin: 0;
        font-family: "Iowan Old Style", "Palatino Linotype", Georgia, serif;
        color: var(--ink);
        background: linear-gradient(180deg, #fbfcfe 0%, #eef3f8 100%);
      }
      main {
        max-width: 860px;
        margin: 0 auto;
        padding: 48px 24px 72px;
      }
      h1, h2 {
        margin: 0 0 12px;
      }
      p {
        line-height: 1.6;
      }
      .meta,
      .panel {
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 14px;
        padding: 18px 20px;
      }
      .meta {
        margin: 24px 0 28px;
      }
      .panel + .panel {
        margin-top: 18px;
      }
      ul {
        padding-left: 20px;
      }
      code {
        font-family: "SFMono-Regular", "Menlo", monospace;
        font-size: 0.95em;
      }
      a {
        color: var(--accent);
      }
      .muted {
        color: var(--muted);
      }
    </style>
  </head>
  <body>
    <main>
      <h1>Effigy Demo Browser Proof Report</h1>
      <p>
        This artifact is produced by the repo's own
        <code>browser-proof-report</code> demo. It proves that the shipped
        demo registry can be listed, grouped, inspected, and linked to
        operator-visible artifacts without any custom external harness.
      </p>
      <div class="meta">
        <strong>Generated at:</strong> <span class="muted">$generated_at</span><br>
        <strong>Repo:</strong> <code>effigy</code><br>
        <strong>Coverage claims:</strong> <code>effigy.demo.registry</code>,
        <code>effigy.demo.browser-query</code>
      </div>
      <div class="panel">
        <h2>Inventory snapshots</h2>
        <ul>
          <li><a href="./list.txt">demo list</a></li>
          <li><a href="./grouped-by-gap.txt">demo list --group-by gap</a></li>
          <li><a href="./inspect-lifecycle-window.txt">demo inspect lifecycle-window</a></li>
        </ul>
      </div>
      <div class="panel">
        <h2>How to extend this proof</h2>
        <p>
          Run <code>effigy demo run lifecycle-window</code> in another terminal,
          then rerun this report to compare a registry-only snapshot against one
          with active state and a latest recorded receipt.
        </p>
      </div>
    </main>
  </body>
</html>
EOF

printf 'browser-proof-report refreshed at %s\n' "$generated_at"
