let artifact_dir = path_join(repo_root, ".effigy/demo/artifacts/browser-proof-report");
let generated_at = now_utc();

create_dir(artifact_dir);

let list_path = path_join(artifact_dir, "list.txt");
let grouped_path = path_join(artifact_dir, "grouped-by-gap.txt");
let inspect_path = path_join(artifact_dir, "inspect-lifecycle-window.txt");
let html_path = path_join(artifact_dir, "index.html");

fn run_capture(label, capture_path, process_args) {
    let result = run_process("cargo", process_args);
    if !result["success"] {
        throw "[error] " + label + " failed"
            + "\nstdout:\n" + result["stdout"].to_string()
            + "\nstderr:\n" + result["stderr"].to_string();
    }
    write_file(capture_path, result["stdout"].to_string());
}

run_capture("demo list", list_path, ["run", "--bin", "effigy", "--", "demo", "list"]);
run_capture(
    "demo list --group-by gap",
    grouped_path,
    ["run", "--bin", "effigy", "--", "demo", "list", "--group-by", "gap"],
);
run_capture(
    "demo inspect lifecycle-window",
    inspect_path,
    ["run", "--bin", "effigy", "--", "demo", "inspect", "lifecycle-window"],
);

let html_lines = [
    "<!doctype html>",
    "<html lang=\"en\">",
    "  <head>",
    "    <meta charset=\"utf-8\">",
    "    <title>Effigy Demo Browser Proof Report</title>",
    "    <style>",
    "      :root {",
    "        color-scheme: light;",
    "        --ink: #122033;",
    "        --muted: #4c5a6a;",
    "        --line: #d8dee7;",
    "        --panel: #f6f8fb;",
    "        --accent: #0059b3;",
    "      }",
    "      body {",
    "        margin: 0;",
    "        font-family: \"Iowan Old Style\", \"Palatino Linotype\", Georgia, serif;",
    "        color: var(--ink);",
    "        background: linear-gradient(180deg, #fbfcfe 0%, #eef3f8 100%);",
    "      }",
    "      main {",
    "        max-width: 860px;",
    "        margin: 0 auto;",
    "        padding: 48px 24px 72px;",
    "      }",
    "      h1, h2 {",
    "        margin: 0 0 12px;",
    "      }",
    "      p {",
    "        line-height: 1.6;",
    "      }",
    "      .meta,",
    "      .panel {",
    "        background: var(--panel);",
    "        border: 1px solid var(--line);",
    "        border-radius: 14px;",
    "        padding: 18px 20px;",
    "      }",
    "      .meta {",
    "        margin: 24px 0 28px;",
    "      }",
    "      .panel + .panel {",
    "        margin-top: 18px;",
    "      }",
    "      ul {",
    "        padding-left: 20px;",
    "      }",
    "      code {",
    "        font-family: \"SFMono-Regular\", \"Menlo\", monospace;",
    "        font-size: 0.95em;",
    "      }",
    "      a {",
    "        color: var(--accent);",
    "      }",
    "      .muted {",
    "        color: var(--muted);",
    "      }",
    "    </style>",
    "  </head>",
    "  <body>",
    "    <main>",
    "      <h1>Effigy Demo Browser Proof Report</h1>",
    "      <p>",
    "        This artifact is produced by the repo's own",
    "        <code>browser-proof-report</code> demo. It proves that the shipped",
    "        demo registry can be listed, grouped, inspected, and linked to",
    "        operator-visible artifacts without any custom external harness.",
    "      </p>",
    "      <div class=\"meta\">",
    "        <strong>Generated at:</strong> <span class=\"muted\">" + generated_at + "</span><br>",
    "        <strong>Repo:</strong> <code>effigy</code><br>",
    "        <strong>Coverage claims:</strong> <code>effigy.demo.registry</code>,",
    "        <code>effigy.demo.browser-query</code>",
    "      </div>",
    "      <div class=\"panel\">",
    "        <h2>Inventory snapshots</h2>",
    "        <ul>",
    "          <li><a href=\"./list.txt\">demo list</a></li>",
    "          <li><a href=\"./grouped-by-gap.txt\">demo list --group-by gap</a></li>",
    "          <li><a href=\"./inspect-lifecycle-window.txt\">demo inspect lifecycle-window</a></li>",
    "        </ul>",
    "      </div>",
    "      <div class=\"panel\">",
    "        <h2>How to extend this proof</h2>",
    "        <p>",
    "          Run <code>effigy demo run lifecycle-window</code> in another terminal,",
    "          then rerun this report to compare a registry-only snapshot against one",
    "          with active state and a latest recorded receipt.",
    "        </p>",
    "      </div>",
    "    </main>",
    "  </body>",
    "</html>",
];

write_lines(html_path, html_lines);
log("browser-proof-report refreshed at " + generated_at);
