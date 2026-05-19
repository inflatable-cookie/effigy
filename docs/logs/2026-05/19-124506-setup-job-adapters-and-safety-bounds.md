# Setup Job Adapters And Safety Bounds

Date: 2026-05-19  
Roadmap: [`g07.053`](../../roadmaps/g07/053-setup-job-adapters-and-mutation-boundaries.md)  
Batch card: [`1003`](../../roadmaps/g07/batch-cards/1003-wire-setup-job-adapters-and-safety-bounds.md)  
Strict lane: [`093`](../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- added a shared init setup inventory under the builtin init surface
- inventory entries now carry:
  - category
  - execution kind
  - safety class
  - applicability
  - summary
  - reason
  - recommended command
- wired inventory detection from real repo context:
  - baseline managed init surfaces
  - `package.json`
  - `Makefile`
  - Cargo alias config
  - local graph index presence
  - manifest-declared `[bundle]`, `[secrets]`, `[containers]`, `[state]`,
    `[deploy]`, `[distribution]`, and `[release]`
  - simple repo validation task presence
- attached each shipped setup job to one real adapter path:
  - direct baseline apply through the existing managed init jobs
  - read-only inspection/guidance through concrete Effigy commands such as:
    - `effigy doctor`
    - `effigy tasks`
    - `effigy test --plan`
    - `effigy graph status --json`
    - `effigy graph index --json`
    - `effigy bundle inspect`
    - `effigy bundle sync`
    - `effigy secrets doctor`
    - `effigy secrets init`
    - `effigy container up`
    - `effigy release status --check-gates`
- threaded the wizard summary through that inventory so plain TTY init now ends
  with repo-specific follow-up setup commands instead of generic prose

## Hard Boundaries Now Encoded

- init does not recommend hidden mutation for:
  - `release prepare --yes`
  - `release execute --yes`
  - `deploy apply`
  - `state apply`
  - distribution publish / first-publish paths
- runtime bring-up remains guidance-only from init
- package-script cleanup remains out of init-owned mutation until there is a
  proven exact-wrapper rewrite path

## Why This Matters

- `1004` now has a real adapter inventory to execute against instead of having
  to invent action names or safety classes
- the wizard can talk about the broader setup surface honestly before it can
  execute every step directly
- the mutation posture is now code-backed rather than only roadmap text

## Validation

- `cargo test -p effigy-builtin inventory_detects_contextual_setup_surfaces -- --nocapture`
- `cargo test -p effigy-builtin follow_up_renderer_surfaces_real_commands -- --nocapture`
- `cargo test -p effigy-builtin wizard_can_apply_baseline_only_and_defer_agent_setup -- --nocapture`
- `cargo test -p effigy-builtin wizard_reports_repo_already_satisfied_without_prompting -- --nocapture`
- `cargo test -p effigy-builtin plain_tty_init_prompts_but_json_explicit_apply_and_non_tty_do_not -- --nocapture`
- `cargo test run_manifest_task_builtin_init_creates_scaffold_when_missing -- --nocapture`
- `cargo clippy -p effigy-builtin --tests -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Execute `1004`.
