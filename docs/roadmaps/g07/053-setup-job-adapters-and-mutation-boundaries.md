# g07.053 - Setup Job Adapters And Mutation Boundaries

Status: Complete
Depends on: `g07.052`

## Goal

Wire the setup-job inventory to real Effigy command surfaces while keeping
mutation safety explicit and conservative.

## Scope

- map each setup job to:
  - direct file mutation
  - delegated Effigy command
  - read-only inspection
  - guidance-only output
- define prerequisites and failure handling per job
- define which jobs can run automatically and which require explicit opt-in
- ensure the same adapters are reusable from checklist execution later

## Mutation Classes

`safe_apply`
- baseline files and managed blocks
- vendored skill sync
- local gitignore normalization
- graph index build

`contextual_apply`
- task migration where a supported source exists
- package-script cleanup when exact wrappers are provable
- bundle sync when bundle config exists
- secrets vault init when secrets config exists

`inspect_only`
- doctor
- tasks
- test plan
- graph status
- state / deploy / distribution / release status surfaces

`guidance_only`
- graph watch recommendation
- containers up recommendation
- first validation recommendation
- missing-task / missing-QA follow-up guidance

## Hard Boundaries

- no `release prepare --yes` or `release execute --yes`
- no `deploy apply`
- no `state apply`
- no distribution publish/first-publish mutations
- no container startup as a hidden default side effect
- no package-script rewrites unless Effigy can prove the script is only a thin
  wrapper

## Acceptance Criteria

- every shipped setup job has one real adapter path
- mutation boundaries are encoded in code and docs, not only implied
- failures are reported per job without collapsing the whole wizard unless the
  baseline contract itself is blocked

## Evidence

- [`2026-05/19-124506-setup-job-adapters-and-safety-bounds.md`](../../logs/2026-05/19-124506-setup-job-adapters-and-safety-bounds.md)

## Next Task

Execute `1004`.
