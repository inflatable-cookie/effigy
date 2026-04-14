let artifact_dir = path_join(repo_root, ".effigy/demo/artifacts/lifecycle-window");
let status_file = path_join(artifact_dir, "status.txt");
let heartbeat_file = path_join(artifact_dir, "heartbeat.txt");
let events_file = path_join(artifact_dir, "events.log");
let index_file = path_join(artifact_dir, "index.html");
let started_at = now_utc();
let pid = process_id().to_string();

create_dir(artifact_dir);

write_lines(index_file, [
    "<!doctype html>",
    "<html lang=\"en\">",
    "  <head>",
    "    <meta charset=\"utf-8\">",
    "    <title>Effigy Demo Lifecycle Window</title>",
    "    <style>",
    "      :root {",
    "        color-scheme: light;",
    "        --ink: #111e30;",
    "        --muted: #506074;",
    "        --line: #d8dee7;",
    "        --panel: #f6f8fb;",
    "        --accent: #006b5f;",
    "      }",
    "      body {",
    "        margin: 0;",
    "        font-family: \"Iowan Old Style\", \"Palatino Linotype\", Georgia, serif;",
    "        color: var(--ink);",
    "        background: linear-gradient(180deg, #fbfcfe 0%, #edf4ef 100%);",
    "      }",
    "      main {",
    "        max-width: 760px;",
    "        margin: 0 auto;",
    "        padding: 48px 24px 72px;",
    "      }",
    "      .panel {",
    "        background: var(--panel);",
    "        border: 1px solid var(--line);",
    "        border-radius: 14px;",
    "        padding: 18px 20px;",
    "        margin-top: 18px;",
    "      }",
    "      a {",
    "        color: var(--accent);",
    "      }",
    "      code {",
    "        font-family: \"SFMono-Regular\", \"Menlo\", monospace;",
    "      }",
    "      .muted {",
    "        color: var(--muted);",
    "      }",
    "    </style>",
    "  </head>",
    "  <body>",
    "    <main>",
    "      <h1>Effigy Demo Lifecycle Window</h1>",
    "      <p>",
    "        This run-backed demo stays active until Effigy stops it. It exists to",
    "        prove active attempt state, stoppability, and terminal receipt updates",
    "        on a real process instead of a synthetic fixture.",
    "      </p>",
    "      <div class=\"panel\">",
    "        <strong>Started at:</strong> <span class=\"muted\">" + started_at + "</span><br>",
    "        <strong>PID:</strong> <code>" + pid + "</code><br>",
    "        <strong>Status file:</strong> <a href=\"./status.txt\">status.txt</a><br>",
    "        <strong>Heartbeat:</strong> <a href=\"./heartbeat.txt\">heartbeat.txt</a><br>",
    "        <strong>Event log:</strong> <a href=\"./events.log\">events.log</a>",
    "      </div>",
    "      <div class=\"panel\">",
    "        Stop this demo with <code>effigy demo stop lifecycle-window</code>, then",
    "        inspect it again to see the active attempt clear and the latest receipt",
    "        flip to a terminal outcome.",
    "      </div>",
    "    </main>",
    "  </body>",
    "</html>",
]);

write_file(status_file, "running since " + started_at + "\n");
append_file(events_file, "started " + started_at + " pid=" + pid + "\n");

while !stop_requested() {
    let now = now_utc();
    write_file(heartbeat_file, now + "\n");
    append_file(events_file, "heartbeat " + now + " pid=" + pid + "\n");
    sleep_ms(1000);
}

let finished_at = now_utc();
write_file(status_file, "terminated at " + finished_at + "\n");
append_file(events_file, "terminated " + finished_at + " pid=" + pid + "\n");
