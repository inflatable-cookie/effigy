# 058 - Demo System Guide

Use this guide when a repo needs first-class proof demos instead of another pile
of shell scripts, scratch notes, or repo-local QA rituals.

This is the front door for the demo surface: registry shape, runner behavior,
browser behavior, and the practical path for adding or operating demos.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `ADOPT`
- Target movement: demos become a stable repo-owned proof system with one
  operator surface and one automation-safe contract.

## 1) What The Demo System Is

Effigy demos are repo-owned proof entries declared in the manifest at
`[demos.<id>]`.

Each demo gives you:

- stable proof inventory through `effigy demo list`
- one interactive browser through `effigy demo browser`
- latest or active detail through `effigy demo inspect <id>`
- retained attempt review through `effigy demo history <id>`
- normalized execution through `effigy demo run <id>`
- lifecycle control through `demo stop` / `demo rerun` when the runtime owns
  that demo directly

Use demos when proof should be discoverable, inspectable, and runnable without
teaching people where a script lives.

## 2) Define A Demo

Smallest useful shape:

```toml
[demos.login-smoke]
title = "Login smoke"
summary = "Checks that login still produces a working session."
proof = "Operator-visible smoke proof for login."
owner = "platform"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
run = "python3 demos/run_login_smoke.py"
```

Common fields:

- `title`, `summary`, `proof`, `owner`, `mode`, `status`, and `covers` are
  required
- use exactly one runnable entrypoint:
  - `task = "demo:login-smoke"`
  - `run = "..."`
  - `run = [ ... ]`
- `tags`, `receipt`, `artifacts`, `prerequisites`, and `dependencies` are
  optional but useful

Use `task = "..."` when the repo already has a clear reusable task.

Use `run = [ ... ]` when the proof chain belongs to the demo itself and does
not deserve a separate wrapper task.

Example with inline run steps:

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
  { task = "demo:coverage-matrix" },
  { run = "python3 -m json.tool demos/manifests/capability-browser.demo.json > /dev/null" },
  { run = "python3 demos/scripts/run_capability_browser_demo.py --serve" },
]
```

## 3) Discover And Run Demos

Start with:

```sh
effigy demo list
effigy demo inspect login-smoke
effigy demo run login-smoke
```

Use filters when the inventory grows:

```sh
effigy demo list --owner platform --status ready
effigy demo list --tag smoke --mode interactive
effigy demo list --group-by owner --stale-only
```

Use history when the question is “what happened recently?” instead of “what is
the latest state?”:

```sh
effigy demo history login-smoke --limit 5
effigy demo history login-smoke --attempt login-smoke-1775944053944
```

Use lifecycle control when the runner owns the demo process:

```sh
effigy demo run lifecycle-window
effigy demo stop lifecycle-window
effigy demo rerun lifecycle-window
```

## 4) Use The Browser

Open the browser with:

```sh
effigy demo browser
```

Current browser model:

- left pane: demo list plus query/count summary
- right pane: selected demo detail
- detail tabs: `Overview`, `History`, `Terminal`, `Artifacts`

Current controls:

- `Tab` / `Shift+Tab` switch between the list pane and detail pane
- with detail focused, `←` / `→` switch tabs
- `↑` / `↓` act inside the focused pane or active detail tab
- `Enter` opens the action sheet from the list, activates the selected
  detail-side entry, or toggles terminal input capture on the `Terminal` tab
  when supported
- `Esc` closes overlays, leaves terminal input capture, returns non-overview
  tabs to `Overview`, or quits from the root overview
- `/` edits search
- `f` opens the filter sheet

The browser is demo-scoped. It must not launch or embed a nested TUI.

## 5) Understand Terminal Behavior

The terminal tab is now a real terminal surface for the bounded honest cases:

- browser-launched run-backed interactive demos
- browser-launched single-process concurrent-runner interactive demos

What that means:

- live output renders in the browser terminal tab
- typed input can go straight to the active live session when capture is enabled
- resize is reported back through the runner-owned terminal contract

For projected multi-process concurrent runtimes, the browser still consumes the
runner-owned flattened session surface instead of pretending it is one live
terminal.

Human text-mode runs stay direct too:

- `effigy demo run <id>` attaches directly for interactive or hybrid demos when
  the runtime supports it

## 6) Organize Demo Manifests Cleanly

When demo config grows beyond a couple of entries, split it out of the root
manifest:

```toml
[manifest]
include = ["config/demos.toml"]
```

Then keep the root `effigy.toml` as the entrypoint and let the demo fragment
own the demo registry.

Good pattern:

- root `effigy.toml`: repo entrypoint, broad tasks, shared env, release/docs
  policy
- `config/demos.toml`: `[demos.*]` entries and any demo-specific helper
  tasks that really need to live together

For the deeper composition rules, use
[`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md).

## 7) When To Use Demos Instead Of Tasks

Use a plain task when the job is just execution.

Use a demo when the job is proof:

- it should show up in a proof inventory
- operators need inspect/history/browser surfaces
- artifacts or receipts matter
- the repo should name what the proof covers

Not every script should become a demo. The good boundary is “proof surface,”
not “any command we might run sometimes.”

## Expected Outcome

After this guide, you should be able to:

- declare demos in the manifest
- decide between `task`, `run`, and inline `run = [ ... ]`
- use list, browser, inspect, history, run, stop, and rerun coherently
- understand when the browser terminal is live versus projected

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`../roadmaps/g02/003-demo-harness-model-and-runner-contract.md`](../roadmaps/g02/003-demo-harness-model-and-runner-contract.md)

## Next Step

After this guide, move to
[`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md) if
you need to split demo config into manifest fragments, use
[`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
when the next job is migrating a script-based demo surface, or use
[`025-command-reference-matrix.md`](./025-command-reference-matrix.md) when the
next job is command lookup or JSON-contract detail.
