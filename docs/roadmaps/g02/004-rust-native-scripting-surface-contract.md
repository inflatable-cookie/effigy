# g02.004 Rust-Native Scripting Surface Contract

Status: Complete
Updated: 2026-04-14

## Goal

Define Effigy's scripting policy and product boundary so Rust-first repos can
stop depending on ad hoc shell glue and optional Bun installs for local task
automation, while web-oriented repos can still use Bun + TypeScript where that
stack is the natural fit.

## Why Now

The demo/browser lane is shipped and released. The next product-shaping problem
is not another demo slice. It is scripting sprawl across consumer repos:

- Rust-first repos still rely on shell glue for docs, release, QA, and demo
  helpers.
- Some repos use Python or Node only because the scripting surface grew
  historically, not because the underlying product actually needs that runtime.
- Cross-repo manifest cleanup is now good enough that the next real
  consolidation move is the script runtime story.

## Product Direction

Effigy should not pursue one universal external runtime.

The intended split is:

- Rust-first repos:
  - prefer Effigy-native scripting
  - Rhai is the leading candidate runtime
- Web-oriented repos:
  - default to Bun + TypeScript
- Desktop/web hybrid repos:
  - use Bun + TypeScript where frontend/build tooling already depends on that
    ecosystem
  - use Effigy-native scripting for repo automation glue where that reduces
    shell sprawl cleanly

## Rhai Boundary

Rhai is not a replacement for every external toolchain.

It is a strong fit for:

- task orchestration glue
- file and path transforms
- small validation/report helpers
- structured manifest-time or task-time logic
- Rust-first repo automation where the host API can stay narrow and explicit

It is not the right answer for:

- Electron or frontend build tooling that already sits inside JS package
  ecosystems
- arbitrary shell emulation as a first product goal

## Repo Policy Split

### Strong Rhai-First Candidates

- `effigy`
- `keepsake`

### Rhai-First, But Needs Deeper Migration Planning

- `jetstream`

Jetstream is the important exception to the usual “keep Python where it is
substantive” rule. Jetstream's own developer experience intends to promote
Rhai-backed scripting inside the engine, so its current Python/bash surfaces
should be treated as migration targets, not permanent carve-outs. That likely
means some Rust host capability must land behind the scenes to let Rhai replace
today's analysis and orchestration surfaces honestly.

### Bun + TypeScript Defaults

- `convergence`
- `soundcheck`
- `compli-me`
- `songsprout`
- `signal`
- similar web/app repos

### Mixed / Ecosystem-Driven Cases

- `finch`
- other JS/Electron-heavy repos

These can still use Effigy-native scripting for orchestration glue, but should
not distort frontend/build tooling just to make the repo “pure”.

## Contract Questions To Settle

The key boundary is now settled:

- Rhai v1 should support file-backed scripts referenced from the manifest
- Rhai v1 should expose:
  - logging
  - args access
  - env read
  - path helpers
  - file read/write/exists/create-dir
  - JSON/TOML parse + stringify helpers
  - structured subprocess execution without shell parsing
  - task invocation helpers where useful
- Rhai v1 should not attempt:
  - arbitrary shell emulation
  - network APIs
  - frontend/build-tool replacement
  - full Python-analysis replacement in the first slice

That active question is now settled. The Rhai lane is paused on a clean
internal posture after native distribution cutover, and the next product
question has been split into its own optional distribution lane.

Historical note: that pause refers to the old roadmap boundary, not the live
typed helper surface. Rhai now exposes typed deploy and distribution helpers;
see [`../../guides/061-rhai-script-steps-guide.md`](../../guides/061-rhai-script-steps-guide.md)
and [`../../guides/068-rhai-host-surface-audit.md`](../../guides/068-rhai-host-surface-audit.md).

## Migration Classification

### Effigy

Migrate early.

Shipped first targets:

- `link:local`
- `smoke:release`
- `browser-proof-report`
- `lifecycle-window`
- native `effigy distribution first-publish`
- native `effigy distribution check-glibc-floor`

### Keepsake

First external pilot when the repo boundary is safe again.

Best early targets:

- `tools/release-candidate.sh`
- REAPER smoke orchestration wrappers

### Jetstream

Still a full migration target, but deferred for the current batch while active
local work makes it the wrong immediate migration surface.

When Jetstream returns to scope, the intended migration still happens in two
layers:

- first migrate bash orchestration and QA wrappers
- then migrate analysis tools once Rust helper capability exists behind the
  Rhai surface

## Out Of Scope

- deciding the next migration slice before the foundation exists
- removing Bun from web-oriented repos
- solving all plugin/build ecosystem integration in the same batch
- promising that every historical Python or shell script disappears in v1

## Acceptance Target

This milestone is ready to execute only when Effigy has:

- an explicit scripting policy by repo type
- a bounded Rhai host API for Rust-first repos
- a clear pilot repo order
- an honest migration classification for current non-Bun script surfaces

## Next Task

This roadmap is complete on the current product boundary.

If a later external Rhai pilot or new internal scripting gap reopens the
question, do it as fresh planning rather than keeping `g02.004` artificially
live.
