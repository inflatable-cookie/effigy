# 060 - Consumer Demo Migration Guide

Use this guide when a repo already has demo scripts, proof tasks, or ad hoc QA
flows and you want to migrate them into Effigy's native demo surface cleanly.

This is a practical migration path, not a contract deep dive. It shows how to
move from “scripts in a demos folder” to native `[demos.*]`, browser support,
history, and inline run definitions without tying the guide to one specific
consumer repo.

## Vision Alignment

- Primary tags: `ADOPT`, `OPERATE`, `CONTRACT`
- Target movement: consumer repos move from script piles and wrapper tasks to a
  stable demo registry with one operator surface.

## 1) Start With What Already Exists

Before changing the manifest, inventory the current proof surface.

Typical starting point:

- `demos/` folder with scripts
- one or more `demo:*` tasks
- receipts or artifacts written to repo-local paths
- no unified browser or history surface

Ask four questions for each proof flow:

1. what does it prove?
2. who owns it?
3. should it be headless, interactive, or hybrid?
4. does it deserve a first-class demo entry, or is it just a helper task?

If the answer is “operators need to discover or review it,” it probably wants a
demo entry.

## 2) Extract A Demo Manifest Fragment

Keep the root `effigy.toml` as the entrypoint and move demo definitions into a
dedicated fragment:

```toml
[manifest]
include = ["config/demos.toml"]
```

This keeps the root manifest readable and gives the proof surface one obvious
home.

Good rule:

- root `effigy.toml`: broad repo contract
- `config/demos.toml`: demo registry and demo-local helpers

## 3) Promote Existing Scripts Into `[demos.*]`

Start with one or two high-signal demos, not the whole pile at once.

Example:

```toml
[demos.runtime-recovery-inspector]
title = "Runtime recovery inspector"
summary = "Shows runtime recovery behavior for operator review."
proof = "Operator-visible proof for recovery behavior."
owner = "runtime"
mode = "interactive"
status = "ready"
covers = ["runtime.recovery"]
run = "python3 demos/scripts/run_runtime_recovery_inspector.py"
artifacts = ["demos/artifacts/runtime-recovery-inspector/index.html"]
```

That is enough to make the proof discoverable in:

```sh
effigy demo list
effigy demo inspect runtime-recovery-inspector
effigy demo run runtime-recovery-inspector
```

## 4) Inline Per-Demo Wrapper Tasks When They Add No Value

A common migration smell is one wrapper task per demo:

```toml
[tasks]
"demo:capability-browser" = [
  { task = "demo:conventions" },
  { run = "python3 demos/scripts/run_capability_browser_demo.py --serve" },
]

[demos.capability-browser]
task = "demo:capability-browser"
```

If the wrapper task is only there for this one demo, inline it:

```toml
[demos.capability-browser]
title = "Capability browser"
summary = "Builds and serves the capability browser demo."
proof = "Operator-visible proof for capability browsing."
owner = "platform"
mode = "interactive"
status = "ready"
covers = ["plugin.capabilities"]
run = [
  { task = "demo:conventions" },
  { run = "python3 demos/scripts/run_capability_browser_demo.py --serve" },
]
```

Keep a separate task only when it is genuinely reusable across multiple demos
or outside the demo system.

## 5) Keep Shared Helpers Shared

Do not inline everything blindly.

Good shared helpers to keep as tasks:

- `demo:conventions`
- `demo:coverage-matrix`
- shared build or fixture setup

Good per-demo steps to inline:

- one-off validation for that demo
- one demo-specific launch command
- one demo-specific artifact post-check

The goal is not “zero tasks.” The goal is “no fake wrapper tasks.”

## 6) Validate In Small Batches

After the first migrated demos land, validate the native surface:

```sh
effigy demo list
effigy demo inspect capability-browser
effigy demo run capability-browser
effigy demo history capability-browser --limit 3
effigy qa:docs
```

Then use the browser for operator proof:

```sh
effigy demo browser
```

What to check:

- list rows are readable
- overview/history/terminal/artifacts feel coherent
- interactive demos use the terminal tab honestly
- headless demos complete and record receipts/history correctly

## 7) What “Done” Looks Like

A consumer repo migration is in good shape when:

- the demo inventory lives in `config/demos.toml` or another clearly
  named fragment
- `[demos.*]` entries name proof ownership and coverage explicitly
- per-demo wrapper tasks are gone unless they are truly shared
- operators can use `demo list`, `browser`, `inspect`, `history`, and `run`
  without chasing repo-local script names

That is the point where the repo has adopted the demo system, not just renamed
scripts.

## Expected Outcome

After this guide, you should be able to:

- extract a demo fragment from root manifest clutter
- migrate script-based proof flows into `[demos.*]`
- choose when to inline `run = [ ... ]`
- validate the migrated surface through both CLI and browser flows

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)

## Next Step

After this guide, use [`058-demo-system-guide.md`](./058-demo-system-guide.md)
for the steady-state operator surface, then use
[`025-command-reference-matrix.md`](./025-command-reference-matrix.md) when the
next job is exact command shape or JSON-contract lookup.
