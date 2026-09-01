# 113 Catalog-Pack Acquisition Prototype Strict Lane

Status: Active
Created: 2026-09-01
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)
Roadmap: [`g08.040`](../roadmaps/g08/040-catalog-pack-acquisition-prototype.md)
Card: [`1095`](../roadmaps/g08/batch-cards/1095-prototype-catalog-pack-acquisition.md)

## Outcome

Effigy has a complete in-repository acquisition prototype for independently
versioned default catalog packs while current embedded assets remain the
permanent compiled baseline.

## Problem

Concrete service and workspace definitions are compiled into `effigy-catalog`.
The schema and deterministic assembly belong in core, but the definitions need
an independently owned update path. Removing the embedded assets first would
break source installs, offline use, and the current zero-ceremony command path.

## Decisions

- The compiled baseline is permanent, not a temporary compatibility shim.
- Installed pack precedence sits below project and user overrides and above the
  compiled baseline.
- A pack manifest declares schema version, pack identity, pack version, and an
  Effigy compatibility requirement.
- The local store is versioned under Effigy-owned user state and records source
  plus immutable content identity.
- Explicit OCI installation requires `oci://` and a digest-addressed source;
  local-path installation is explicitly operator-selected.
- Candidate acquisition, validation, storage, and activation form one
  transaction; activation changes atomically only after validation.
- A failed candidate preserves the previous active pack.
- The prototype retains every successfully installed content entry. Install,
  rollback, and reset perform no automatic pruning or deletion; garbage
  collection and bounded retention remain a later operator decision.
- An active pack that later becomes unreadable or incompatible yields a visible
  baseline fallback, a structured selection reason, and a `doctor` repair.
- Rollback selects the previous validated installed version. Reset selects the
  compiled baseline without deleting recoverable installed content.
- Official fixed-channel resolution is modeled and adapter-tested, but the
  no-argument update command remains absent until publication.

## Public Prototype Surface

```text
effigy service pack status
effigy service pack install oci://...@sha256:...
effigy service pack install --path <DIR>
effigy service pack rollback
effigy service pack reset
```

Standard leading `--repo` and `--json` behavior remains consistent with the
existing `service` built-in. `service list`, `service extract`, container,
system, workspace, and task invocation keep their current grammar.

## Scope

- pure pack manifest/compatibility/selection types in the catalog domain
- transport orchestration through the existing artifact adapter seam
- versioned local state and atomic active-selection metadata
- catalog resolver integration without a second layering implementation
- CLI grammar, typed help, text output, standard JSON envelope payloads, and
  deterministic exits for the five public shapes
- `doctor` findings and repair guidance
- tests for selection, transactions, fallback, rollback/reset, source trust,
  absence of normal-command network access, and representative current catalog
  behavior
- guide/reference/changelog/evidence and strict-lane closeout

## Acceptance

- a machine with no pack store and no `oras` gets byte-equivalent catalog
  fragment content and unchanged representative assembly from the baseline
- project and user overrides still outrank an active installed pack
- valid explicit local and OCI candidates become active only after full manifest
  and compatibility validation
- OCI reports retain the resolved digest; local installs retain a deterministic
  content identity and explicit local source
- invalid, incompatible, interrupted, or failed acquisitions do not change the
  active selection or damage the prior installed version
- unreadable or newly incompatible active state selects the baseline with a
  warning in text, equivalent facts in JSON, and an actionable doctor finding
- rollback selects exactly the previous validated installed version; reset
  selects baseline; neither mutates project/user overrides or deletes installed
  pack content
- installed pack content cannot redirect the fixed official update source
- no normal catalog-backed command calls the OCI adapter or probes a network
- no public update command, concrete-asset move, or release wiring appears
- focused and full QA pass

## Non-Goals

- publish an official pack or choose its final registry coordinate
- move `crates/effigy-catalog/catalog/` out of the compiled baseline
- modify release archives, Homebrew, workflows, or source-install docs
- automatically check, download, or activate updates
- automatically prune installed pack content or define a retention/garbage-
  collection policy
- create a general extension marketplace or reuse bundle semantics blindly
- change catalog schema/assembly behavior, override precedence, or command
  routing outside the nested pack surface
- move S3 or provider-specific Rhai APIs

## Stop Conditions

Return to the orchestrator if implementation needs a live official OCI
coordinate, workflow or release changes, signing/authenticity policy beyond the
fixed-origin plus digest boundary, a new general plugin/extension store, a
different override order, silent fallback, destructive reset or pruning, an
implicit network check, or movement of concrete catalog assets.

## Next Task

Execute card `1095` only. After its accepted merge, return to planning for
official pack publication and asset cutover; do not infer that follow-up as
ready.
