# 021 Unified Init And Starter Emission Strict Lane

Status: staged
Updated: 2026-04-22
Roadmap: `g02.021` (productization slice promoted from `g01.029` Wave 5)

## Context

`effigy init` already scaffolds a baseline `effigy.toml` from a hardcoded
string, with `--dry-run`, `--force`, and `--json` flags. Starter emission for
richer shapes (Underlay, and later Northstar) has been designed as a separate
`effigy starter init <name>` command layer, currently deferred.

Shipping a parallel starter command would create two scaffolders doing the
same job: materialize embedded templates into a repo. The shape difference
(one file vs. a tree with post-emission guidance) is an implementation detail,
not a UX split. This lane folds starter emission into `init` so the product
has one discovery point and one code path.

The lane is also the concrete landing slot for the `g01.029` Wave 5 candidate
surface "`effigy init --northstar` or equivalent scaffold path" — resolved
here as `effigy init <name>`.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/guides/065-underlay-starter.md`
- `docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `crates/effigy-builtin/src/init.rs`
- `crates/effigy-builtin/src/init/{scaffold,request,output}.rs`
- `crates/effigy-catalog/starters/underlay/`
- `crates/effigy-cli/src/help/topics/init.rs`

## Lane Focus

This lane owns:

- promoting the current hardcoded baseline scaffold into an embedded starter
  (`minimal`) under `crates/effigy-catalog/starters/`
- a `starter.toml` descriptor per starter directory declaring name,
  description, emitted files, and post-emission guidance
- `rust-embed` loading of the starter set into the `init` code path
- extending the `init` CLI surface with positional starter name and `--list`,
  while preserving all existing flags and defaults
- post-emission guidance rendering for multi-file starters (text + JSON
  shapes)
- bringing `underlay` online as the second named starter, replacing the manual
  copy-paste adoption flow documented in guide 065

Out of scope:

- new starter content beyond `minimal` and `underlay`
- a Northstar-specific starter (tracked separately; this lane unblocks it)
- any change to the `effigy-catalog` service-catalog surface — starters live
  alongside but do not mix with the service catalog model

## Current Posture

`staged`

Substrate already in place:

- `init` has a clean request/scaffold/output module split ready to absorb a
  starter-name input
- `rust-embed` is already used by the service catalog, so the embedding
  pattern is familiar
- the Underlay reference tree and its composition proof-test
  (`crates/effigy-manifest/tests/underlay_starter.rs`) already validate the
  emitted shape — this lane consumes that foundation rather than rebuilding it

Settled design decisions carried into execution:

- `effigy init` with no argument continues to emit the `minimal` starter; the
  observable default stays identical
- a positional starter name is preferred over `--starter <name>` for ergonomic
  parity with `cargo new <name>` style flows
- `starter.toml` per starter directory is the single source of truth for
  listing, description, file set, and guidance text
- `--list` emits the starter catalog in both human and JSON shapes
- existing flags (`--dry-run`, `--force`, `--json`) apply uniformly across all
  starters; multi-file starters extend `--force` to overwrite any conflicting
  emitted file, not just `effigy.toml`

## Integration Constraint

This lane should execute in bounded batches:

- land the starter storage + loader refactor first, with `minimal` as the only
  starter, so the existing `init` behavior is preserved byte-for-byte before
  any surface change
- add the CLI surface (`<name>` positional, `--list`) in a second batch with
  no new starters yet
- bring `underlay` online as the third batch, including guidance rendering
  and the JSON contract extension
- do not let starter content work reopen the `init` refactor — new starters
  land as pure content PRs against the stable loader

## Staged Continuation Chain

The intended execution order:

1. Promote the baseline scaffold into `crates/effigy-catalog/starters/minimal/`
   with a `starter.toml` descriptor; refactor `init/scaffold.rs` to load via
   `rust-embed`; preserve exact current output under `effigy init` with no
   args. Existing tests must pass unchanged.
2. Extend the CLI surface: positional `<name>` argument, `--list` flag, help
   topic updates, JSON contract extension for `--list` output. Still only
   `minimal` in the catalog.
3. Land `underlay` as an embedded starter. Port the multi-file tree and
   guidance from guide 065 into `starters/underlay/`. Extend output rendering
   for post-emission guidance. Update guide 065 to point at the command
   instead of manual steps.
4. Update `g01.029` Wave 5 to mark the `effigy init --northstar`-shaped
   surface resolved by this lane's `effigy init <name>` form. Note the
   Northstar starter itself as a follow-up content slot, not a separate
   command.

## Exit Condition

This strict lane is complete when:

- `effigy init` with no arguments emits exactly the current baseline manifest,
  sourced from an embedded `minimal` starter rather than a hardcoded string
- `effigy init <name>` emits any registered starter, honoring `--dry-run`,
  `--force`, and `--json` uniformly
- `effigy init --list` enumerates available starters in human and JSON shapes
- `underlay` is a registered starter producing the same manifest shape that
  guide 065's manual steps produce today
- post-emission guidance renders cleanly in both text and JSON modes for
  starters that declare it
- guide 065 documents the command path as the primary flow, with manual steps
  demoted to reference
- no parallel `effigy starter` command surface exists or is planned

## Next Task

Execute batch 1: promote the baseline scaffold into
`crates/effigy-catalog/starters/minimal/`, define the `starter.toml`
descriptor shape, and refactor `init/scaffold.rs` to load via `rust-embed`
while preserving current `effigy init` output byte-for-byte.
