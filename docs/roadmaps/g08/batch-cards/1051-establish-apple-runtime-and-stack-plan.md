# 1051 - Establish Apple Runtime And Stack Plan

Roadmap: [`../018-apple-containers-native-backend-prototype.md`](../018-apple-containers-native-backend-prototype.md)
Strict lane: [`../../../specs/099-apple-containers-native-backend-prototype.md`](../../../specs/099-apple-containers-native-backend-prototype.md)

Status: Complete
Owner: Platform / container planning seam
Created: 2026-08-01
Completed: 2026-08-01

## Purpose

Establish a trustworthy Apple Containers 1.2 baseline and remove Compose
invocation from the semantic center of generated catalog stacks.

## Work

- download and verify the signed Apple Containers 1.2 installer asset
- install the runtime, start its system service, and record version/capability
  baseline without creating persistent project resources
- introduce a typed effective stack-plan model covering the catalog features
  needed by the representative proof stack
- make generated catalog assembly expose the semantic plan and preserve its
  current Compose output from the same resolved service definitions
- make unsupported or unrepresentable catalog fields explicit errors
- expose manager capability/operation planning without registering an Apple
  backend or changing backend detection

## Guardrails

- use only the official signed 1.2.0 installer
- preserve current Compose bytes or prove any normalization is behavior-neutral
- no public manifest driver or backend selector yet
- no Apple project containers, networks, or volumes survive baseline probes
- direct `compose_file` behavior is unchanged
- keep types backend-neutral; no Apple CLI flags in the stack-plan model

## Acceptance

- Apple `container --version` and `container system status` produce a recorded
  baseline on this host
- signed installer provenance and package signature are verified
- a generated app/web/database/cache fixture yields a deterministic typed plan
- typed plan covers image/build, command, environment, user/workdir, mounts,
  ports, dependencies/readiness, network, and project/service identity
- current catalog Compose fixtures remain green
- manager tests prove semantic planning does not require native backends to
  implement Compose invocation

## Validation

- `pkgutil --check-signature <downloaded-pkg>`
- Apple `container --version` and `container system status`
- `cargo test -p effigy-catalog`
- `cargo test -p effigy-containers`
- `effigy qa:architecture:runtime-container-drift`
- `effigy qa:docs`
- `git diff --check`

## Stop Conditions

- stop if installer verification fails or administrator interaction cannot be
  completed
- stop if stack-plan extraction would institutionalize arbitrary Compose
  translation
- stop if preserving current generated Compose behavior requires a breaking
  catalog format change

## Evidence

- [`01-125307-apple-containers-stack-plan-foundation.md`](../../../logs/2026-08/01-125307-apple-containers-stack-plan-foundation.md)

## Next Task

Batch complete. Ready card `1052` is active.
