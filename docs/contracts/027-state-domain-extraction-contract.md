# State Domain Extraction Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/035-state-domain-extraction.md`](../roadmaps/g04/035-state-domain-extraction.md)
Strict lane: [`../specs/071-state-domain-extraction-strict-lane.md`](../specs/071-state-domain-extraction-strict-lane.md)
Status: Draft
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the boundary for extracting state-stack domain behavior from
`src/runner/state_command.rs` into `effigy-state`.

The state surface is now central to layered seed, migration, capture, UAT, and
future media/object-store workflows. The runner cannot remain the primary owner
of pure state model rules without turning every state change into a whole-system
edit.

## Hard Boundaries

- no command grammar changes
- no state manifest/config grammar changes
- no JSON schema change unless a batch card explicitly scopes it
- no provider/deploy behavior changes
- no media/object-store implementation
- no Acowtancy-specific transformation logic
- no database rollback behavior
- no `.github/workflows/` edits
- no release execution

## Domain Ownership

`effigy-state` owns pure state behavior that can be computed or validated
without side effects.

It should own:

- state report structs where they are not runner-specific
- report path and identity helpers
- lineage summary construction
- apply/capture plan construction
- history inventory and latest-report selection
- state blockers and warnings derived from config and recorded reports

Runner owns impure command behavior.

It should own:

- CLI dispatch
- manifest loading
- task execution
- artifact staging
- SQL import execution
- hook execution
- provider/deploy composition
- output mode selection

## Extraction Rules

- move stable types before moving behavior
- keep command text and JSON output compatible unless a card scopes otherwise
- do not move side effects into `effigy-state`
- do not create a new crate; `effigy-state` is already the state-domain owner
- do not paper over unclear ownership with generic utility modules
- preserve unrelated worktree edits in `state_command.rs`

## Current Responsibility Classification

### Already Domain-Owned

`effigy-state` already owns:

- `StateStackManifest`
- `StateStackLayer`
- `StateEnvironment`
- `StateLayerRole`
- `StateLayerApplyMode`
- `StateLayerEnvironmentPolicy`
- lineage planning
- lineage reports
- stack validation
- layer ordering and environment policy validation

### Runner-Owned Pure Domain Candidates

`src/runner/state_command.rs` currently owns pure or mostly pure behavior that
should move to `effigy-state` in staged slices:

- state report path conventions under `.effigy/reports/state/<stack>/`
- history report inventory and filtering
- history report item classification from schema/path
- state apply report structs and planned status construction
- state capture report structs and produced-layer planning
- capture role parsing and capture mode derivation
- state apply/capture context model structs
- serde plain-string helpers for state enums

### Runner-Owned Adapter And Side Effects

The runner should keep:

- CLI dispatch and option handling
- state config loading from the composed Effigy manifest
- file writes for reports and hook/task contexts
- task execution
- artifact staging and capture
- SQL import execution
- hook execution
- text rendering and JSON output routing

### Existing Worktree Edits

`state_command.rs` already contains unrelated apply-hook changes:

- apply hook execution after successful task, artifact, or SQL layer application
- hook status/error/context fields on apply layer reports
- apply hook context file writing
- hook environment construction
- hook-focused tests

Those edits are compatible with this extraction lane, but they should not be
the first moved surface. The first extraction should avoid this active hook
diff and target report path/history helpers.

## Compatibility Rules

Every extraction slice must keep existing command behavior stable.

If implementation discovers a state report shape is already consumed as a
public JSON contract, the slice must either preserve the shape exactly or stop
for an explicit schema decision.

## Acceptance Boundary

This contract is satisfied when:

- state report/path/history/planning behavior has clear domain ownership
- runner state code is mostly orchestration and side effects
- targeted state tests prove the extracted domain behavior
- existing state command output remains compatible
- future media/object-store state work can depend on `effigy-state` instead of
  runner internals

## Implemented Boundary

The first extraction lane moved these pure state pieces into `effigy-state`:

- state report path conventions and history path generation
- state history report scanning, filtering, classification, and summaries
- state apply report planning from lineage
- state apply layer and hook status models
- state capture mode derivation
- state capture produced-layer planning

The runner still owns:

- report file writes
- text rendering
- state config loading from the composed Effigy manifest
- task execution
- artifact staging and capture
- SQL import
- apply hooks and capture tasks
- context file writes

This is the intended stop point for `g04.035`; further state extraction should
be justified by a new lane or by the later media/object-store implementation.
