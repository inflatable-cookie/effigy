# 017 - Remaining Shell Cleanup And Crate Extraction Program

Generation: `g02`

Status: Closed
Owner: Platform
Created: 2026-04-16
Closed: 2026-04-17
Depends on: 010

## Vision Alignment

`g02.010` is already proving the main modularization boundary, but the root
crate still carries too much shell weight in a handful of large files. The
remaining work is no longer one single seam. It is now a queue of substantial,
disjoint cleanup jobs that can be executed in parallel without destabilizing
the active release-boundary path.

This roadmap exists to keep that remaining cleanup visible and coordinated
instead of letting it turn into ad hoc opportunistic extraction.

The next gains are not just "make the biggest runner file smaller". The
remaining work is now split between:

- heavy command shells that still carry domain logic inline
- shared subsystems that are still root-crate owned
- final adapter cleanup that only becomes worth doing after those subsystems
  move

## Primary Tags

- `MAINT`
- `CONTRACT`
- `ROUTE`

## Target Envelope

- the heaviest remaining `/src` seams are classified into clear, bounded jobs
- shared subsystem seams are separated from command-file cleanup
- each job has an obvious target crate or a justified new crate
- parallel threads can take real batches without colliding with the active
  strict lane
- root-crate shell files move toward honest adapter/runtime ownership instead
  of continuing to mix domain logic inline

## Vision Target Delta

- Move from `remaining shell cleanup tracked implicitly inside g02.010` toward
  `an explicit queued program of substantial remaining shell, shared
  subsystem, and final crate extraction jobs`.

## Problem

The current codebase is materially cleaner than it was, but a meaningful amount
of root-crate shell still remains:

- `src/runner/demo_command.rs`
- `src/runner/release_command.rs`
- `src/runner/distribution_command.rs`
- `src/runner/bootstrap_command.rs`
- `src/tui/demo_browser.rs`
- `src/runner/docs_command.rs`
- `src/runner/contracts_command.rs`
- `src/runner/container_command.rs`
- `src/ui/widgets.rs`

Some of those seams are already partly reduced, but the remaining work is still
large enough that it should be planned deliberately rather than delegated as
loose cleanup.

There is also still meaningful root-crate subsystem ownership outside the
obvious command files:

- `src/process_manager/**`
- `src/cli_help/**`
- `src/ui/**`

Those are smaller than the biggest runner files, but they are more reusable and
more likely to justify one clean extraction each than another long sequence of
command-specific diet passes.

## Goals

- define the remaining substantial shell-cleanup jobs explicitly
- define the remaining shared-subsystem extraction jobs explicitly
- identify which jobs can be delegated safely in parallel
- identify where one more crate is justified instead of forcing everything into
  existing crates
- keep delegated work disjoint from the active release-boundary thread

## Non-Goals

- this roadmap does not replace `g02.010` as the active strict lane
- this roadmap does not itself authorize release work
- this roadmap does not reopen already-paused seams without a concrete reason
- this roadmap does not encourage tiny helper churn
- this roadmap does not force every remaining line of root-crate shell into a
  crate if the honest boundary is adapter/runtime work

## Job Queue

### 1. Demo Runner Shell Cleanup

Primary write set:

- `src/runner/demo_command.rs`
- `crates/effigy-demo/**`

Scope:

- move remaining demo command/result/payload shaping into `effigy-demo`
- move browser bridge helpers that are still demo-domain rather than pure
  process shell
- leave only raw process supervision, final command dispatch, and terminal host
  behavior in the runner

Why this is substantial:

- `demo_command.rs` is still the largest remaining runner shell
- it is still mixing projections, command routing, and runtime control in one
  place

### 2. Docs Runner Shell Cleanup

Primary write set:

- `src/runner/docs_command.rs`
- `crates/effigy-docs-policy/**`

Scope:

- move docs subcommand payload/result contracts into `effigy-docs-policy`
- move shared docs rendering helpers that still belong with docs-policy
- move any remaining index / next-action / log-index orchestration that is
  still clearly docs-domain logic
- leave only CLI entry, final rendering choice, and error mapping local

Why this is substantial:

- docs-policy already owns a large share of the domain, but the runner shell is
  still heavier than an honest adapter

### 3. CLI Help Extraction

Primary write set:

- `src/cli_help/**`
- `crates/effigy-cli/**`

Scope:

- move command help topics into `effigy-cli`
- move shared help-topic registry and rendering helpers into `effigy-cli`
- keep root-crate callers limited to final dispatch into the CLI/help layer

Why this is substantial:

- `cli_help` is still entirely root-crate owned
- the help surface is part of the CLI contract, not a random runner helper
- this is a clean disjoint seam that should not stay stranded in `src`

### 4. Process Runtime Extraction

Primary write set:

- `src/process_manager/**`
- likely target crate: `crates/effigy-exec/**`

Scope:

- classify the reusable supervisor/process/runtime contracts already living in
  `src/process_manager`
- move them into `effigy-exec` if that crate is the honest home
- only create a new process-runtime crate if `effigy-exec` would become
  artificially mixed
- leave root-crate only the app-specific integration points that are not
  reusable execution runtime

Why this is substantial:

- `process_manager` is a cross-cutting subsystem, not a command-local helper
- it is a likely shared dependency for demo, managed execution, TUI, and
  container session ownership
- the line count is not the signal here; boundary value is

### 5. Contracts Surface Extraction

Primary write set:

- `src/runner/contracts_command.rs`
- likely new crate: `crates/effigy-contracts/**`

Scope:

- classify whether the contracts surface deserves its own crate
- if yes, move JSON contract/schema inspection and reusable output shaping into
  that crate
- leave only runner command dispatch and exit/error mapping in the root crate

Why this is substantial:

- `contracts_command.rs` is still large and product-shaped
- this is the cleanest remaining candidate for one more new crate

### 6. UI And Widget Primitive Extraction

Primary write set:

- `src/ui/**`
- `crates/effigy-core/**`
- or, if justified, new crate: `crates/effigy-ui/**`

Scope:

- move shared widget primitives, plain-renderer helpers, and reusable
  render/state helpers out of the root crate
- decide honestly whether those helpers belong in `effigy-core` or deserve a
  dedicated UI crate
- leave only app-local wiring in the root crate

Why this is substantial:

- there is still enough reusable UI surface in `src` to justify another real
  architectural pass
- `ui` should be treated as a subsystem seam, not one tiny `widgets.rs` move

### 7. Bootstrap Follow-Up Classification

Primary write set:

- `src/runner/bootstrap_command.rs`
- `crates/effigy-bootstrap/**`

Scope:

- reassess whether bootstrap was paused too early
- if reusable result/render/orchestration logic still remains in the runner,
  move one more real slice into `effigy-bootstrap`
- otherwise leave a clear stop boundary

Why this is substantial:

- bootstrap is smaller now, but still large enough to justify one more strict
  check instead of assuming the seam is fully honest

### 8. Post-Subsystem Runner Adapter Cleanup

Primary write set:

- whichever runner files are still heavy after jobs 3, 4, and 6 land

Scope:

- rerun the `/src` churn check after shared subsystems move
- reduce any now-obvious adapter residue in runner files
- avoid opening this work early; it only becomes meaningful after the shared
  seams are gone

Why this is substantial:

- subsystem extraction should collapse multiple runner shells at once
- delaying this avoids fake cleanup against still-moving boundaries

## Delegation Rules

- prefer one write set per thread
- do not mix release, demo, docs, and container seams in the same delegated
  batch
- do not mix shared-subsystem extraction with command-file cleanup in the same
  delegated batch unless the dependency is unavoidable
- avoid top-level roadmap/currentness churn from delegated threads unless the
  active thread explicitly requests it
- keep `g02.010` as the source of truth for the active strict lane; use this
  roadmap as the queue for parallel shell cleanup

## Suggested Delegation Order

When the parallel thread is free, prefer this order:

1. demo runner shell cleanup
2. docs runner shell cleanup
3. CLI help extraction
4. process runtime extraction
5. contracts surface extraction
6. UI and widget primitive extraction
7. bootstrap follow-up classification
8. post-subsystem runner adapter cleanup

That order keeps the largest remaining command shells moving first, then shifts
to the cross-cutting subsystem seams that should shrink several root-crate
surfaces at once.

## Exit Condition

This roadmap is complete when the remaining heavy `/src` seams are either:

- reduced to honest adapter/runtime shells, or
- explicitly paused with a defensible reason that they no longer block the
  architecture-complete bar for release.

## Next Task

This roadmap is **Closed** as of 2026-04-17. All eight queued jobs were
landed or confirmed already-done in prior streams:

- Job 1 (Demo runner shell cleanup) — landed in 2 batches; remaining runner
  weight is honest process / renderer shell.
- Job 2 (Docs runner shell cleanup) — landed; docs-policy now owns all
  command-result payload and text shaping via a unified `DocsCheckReport`.
- Job 3 (CLI help extraction) — landed; `src/cli_help/**` removed, help
  surface and CLI header now owned by `effigy-cli::{help::ui, header}`.
- Job 4 (Process runtime extraction) — already complete on main; owned by
  `effigy-process` and `effigy-exec`.
- Job 5 (Contracts surface extraction) — landed; `CheckReport::render_text`
  and `SelectionPayload::render_for_print_mode` now own the runner-facing
  text and print-mode formatting.
- Job 6 (UI / widget primitives) — already complete on main; owned by
  `effigy-ui` and `effigy-core::widgets`.
- Job 7 (Bootstrap follow-up) — reassessment showed
  `src/runner/bootstrap_command.rs` is already an honest 89-line adapter.
- Job 8 (Post-subsystem runner adapter cleanup) — landed; canonical
  `From<DomainError> for RunnerError` impls now replace ~50 inline
  `.map_err(...)` adapters across demo / container / distribution runners.

The remaining heavy `/src` seam is `src/runner/release_command.rs`, which
stays inside the `g02.010` strict lane's coordination boundary. Further
modularization of the release runner should be driven from `g02.010`
rather than reopened here.
