# 021 - Unified Init And Starter Emission

Generation: `g02`

Status: Planned
Owner: Platform
Created: 2026-04-22
Depends on: 011

## Vision Alignment

Effigy already ships `effigy init`, which emits a baseline `effigy.toml` from
a hardcoded scaffold string. A second scaffolding surface for richer starter
shapes (Underlay today, Northstar later) has been designed as a separate
`effigy starter init <name>` command path and deferred.

Shipping two parallel scaffolders would split one operator outcome —
"materialize an Effigy-shaped template into this repo" — across two command
surfaces for no product reason. The shape difference between a one-file
scaffold and a multi-file starter with post-emission guidance is an
implementation detail, not a UX split.

This roadmap folds starter emission into `effigy init`, so scaffolding has
one discovery point and one code path. It is also the concrete landing slot
for the `g01.029` Wave 5 candidate surface "`effigy init --northstar` or
equivalent scaffold path".

## Primary Tags

- `CLI`
- `CATALOG`
- `DX`

## Target Envelope

- one scaffolding surface: `effigy init [<name>]`
- the current baseline scaffold continues to emit unchanged when `name` is
  omitted
- named starters emit multi-file trees with optional post-emission guidance
- `--dry-run`, `--force`, and `--json` apply uniformly across all starters
- Underlay adoption becomes a command invocation, not a manual copy-paste
- no parallel `effigy starter` command surface exists or is planned

## Vision Target Delta

- Move from `one hardcoded baseline scaffold, plus deferred separate starter
  command` toward `one init surface that emits any registered starter, with
  the baseline reified as the default starter`.

## Problem

The current state splits the scaffolding story in two ways that do not earn
their weight:

- the baseline scaffold is a hardcoded string in `init/scaffold.rs`, while
  the Underlay starter lives as a reference tree under
  `crates/effigy-catalog/starters/underlay/` with no emission code
- guide 065 documents Underlay adoption as manual copy-paste plus edits
- the deferred `effigy starter init <name>` design would add a second
  top-level scaffolding surface, duplicating dry-run/force/json semantics
  and discovery

That leaves the product with two half-built scaffolding stories instead of
one finished one.

## Goals

- promote the hardcoded baseline into an embedded `minimal` starter under
  `crates/effigy-catalog/starters/minimal/`
- define a `starter.toml` descriptor per starter directory declaring name,
  description, emitted files, and post-emission guidance
- load the starter set into `init` via `rust-embed`, consistent with the
  service-catalog embedding pattern
- extend the CLI surface with a positional `<name>` argument and a `--list`
  flag, preserving existing flag semantics
- render post-emission guidance in both text and JSON modes for starters
  that declare it
- register `underlay` as the second embedded starter and rewrite guide 065
  to point at the command path

## Non-Goals

- this roadmap does not ship a Northstar starter; it only unblocks the slot
- this roadmap does not add starter authoring tooling, validation command
  family, or starter composition beyond single-directory emission
- this roadmap does not change the service-catalog surface or mix starters
  into it — starters live alongside but stay separate
- this roadmap does not add remote or versioned starter sources; the
  embedded set remains the only source in v1
- this roadmap does not replace `effigy bootstrap` or change its semantics

## Contract Direction

### 1. Starter Storage

Starters live under `crates/effigy-catalog/starters/<name>/` and carry a
`starter.toml` descriptor at the directory root.

- `starter.toml` declares `name`, `description`, `files` (paths relative to
  the starter directory, emitted into the target repo), and optional
  `guidance` (multiline post-emission text)
- the embedded set is loaded via `rust-embed`, consistent with the service
  catalog
- the baseline scaffold becomes the `minimal` starter and is the single
  source of the default `effigy init` output

### 2. CLI Surface

`effigy init [<name>] [--list] [--dry-run] [--force] [--json]`.

- no `<name>` argument selects the `minimal` starter
- `<name>` selects the named starter, erroring if unknown
- `--list` emits the registered starter catalog in human and JSON shapes,
  bypassing emission
- existing flags keep their current semantics; `--force` extends to cover
  any emitted file for multi-file starters, not only the manifest
- help topic and JSON contract update in the same batch as the surface

### 3. Emission And Guidance

Emission runs as a single planned write set with conflict detection.

- the request layer resolves `<name>` to an embedded starter descriptor
- the scaffold layer materializes the declared files, honoring `--dry-run`
- the output layer renders written/skipped files plus guidance text in
  human and JSON modes
- guidance is optional per starter and must not change the exit-code
  contract

### 4. Underlay Adoption

`underlay` ships as the first non-trivial starter.

- the current reference tree under `starters/underlay/` becomes the
  embedded source
- `starter.toml` declares the emitted set and the post-emission guidance
  now living in guide 065
- guide 065 is rewritten to document the command path as primary, with
  manual adoption demoted to reference

## Workstreams

### 1. Storage And Loader

Primary write set:

- `crates/effigy-catalog/starters/minimal/**` (new)
- `crates/effigy-catalog/starters/minimal/starter.toml` (new)
- `crates/effigy-builtin/src/init/scaffold.rs`
- starter loader module (new)
- existing `init` tests

Scope:

- embed and load starters via `rust-embed`
- preserve current `effigy init` output byte-for-byte with `minimal` as the
  default
- no CLI surface change in this batch

### 2. CLI Surface

Primary write set:

- `crates/effigy-builtin/src/init.rs`
- `crates/effigy-builtin/src/init/{request,output}.rs`
- `crates/effigy-cli/src/help/topics/init.rs`
- `--list` JSON contract additions
- CLI behavior tests

Scope:

- positional `<name>` argument
- `--list` flag
- help topic and JSON contract extension
- still only `minimal` in the catalog

### 3. Underlay Starter

Primary write set:

- `crates/effigy-catalog/starters/underlay/starter.toml` (new)
- minor shape adjustments if needed in the existing Underlay tree
- post-emission guidance rendering plumbing
- `docs/guides/065-underlay-starter.md` (rewrite)

Scope:

- land `underlay` as a registered starter
- render guidance in human and JSON modes
- rewrite guide 065 around the command path

## Exit Condition

This roadmap is complete when:

- `effigy init` with no arguments emits exactly the current baseline
  manifest, sourced from an embedded `minimal` starter
- `effigy init <name>` emits any registered starter honoring `--dry-run`,
  `--force`, and `--json` uniformly
- `effigy init --list` enumerates the registered starters in human and JSON
  shapes
- `underlay` is a registered starter producing the same manifest shape that
  guide 065's manual steps produce today
- guide 065 documents the command path as primary
- no parallel `effigy starter` command surface exists or is planned, and
  `g01.029` Wave 5 records the `init --northstar`-shaped candidate as
  resolved by this lane's `init <name>` form

## Next Task

Keep `g02.007` as the active release-prep lane and `g02.019` as the next
post-release audit lane.

When this roadmap is picked up, execute workstream 1: promote the baseline
scaffold into `crates/effigy-catalog/starters/minimal/`, define the
`starter.toml` descriptor shape, and refactor `init/scaffold.rs` to load via
`rust-embed` while preserving current `effigy init` output byte-for-byte.
