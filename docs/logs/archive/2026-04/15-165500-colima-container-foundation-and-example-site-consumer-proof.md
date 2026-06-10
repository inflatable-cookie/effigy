# Colima Container Foundation And Contact Patch Consumer Proof

Date: 2026-04-15
Roadmap: `g02.006`
Card: `107`

## Summary

Shipped the first bounded `effigy container` foundation inside Effigy, then
proved it against `example-site` as one safe real consumer repo.

The batch delivered:

- first-class `effigy container` built-ins:
  - `up`
  - `down`
  - `status`
  - `logs`
  - `shell`
  - `reset`
- manifest-backed `[containers]` registry with:
  - named/default container resolution
  - explicit `profile`
  - explicit `primary_service`
  - explicit host `ports`
  - repo-relative `mounts`
  - lifecycle policy
  - one environment health gate
- attached owner-exit shutdown handling strong enough for a real session proof
- Colima fallback through `colima nerdctl` for hosts where `docker` is not on
  `PATH`
- Colima startup alignment with `--runtime containerd` when that fallback path
  is required

## Vision Target Delta

- Primary tags touched: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Moved from `contract-only container lane with no runtime surface` to
  `shipped first bounded container command surface plus one real consumer proof`
- Remains open:
  - attached container UX is still closer to log-follow than a true Effigy
    multi-tab session
  - repo-owned task composition still needs one honest first-class path above
    raw shell composition

## Implementation Notes

Core repo changes:

- added the new command family and parser/help/dispatch surfaces for
  `effigy container`
- added manifest parsing and doctor/schema coverage for `[containers]`
- added config-schema examples for the container registry
- implemented Colima-backed compose orchestration with:
  - named/default container resolution
  - detached bring-up
  - attached owner-session shutdown
  - status inspection
  - log/shell/reset helpers
- added fallback from `docker compose` to `colima nerdctl -- compose`

Documentation changes:

- added `063-container-system-guide.md`
- added the container surface to the guides hub, command matrix, schema output,
  and top-level README surfaces

## Consumer Proof

Chosen consumer repo:

- `example-site`

Why it was safe:

- clean worktree
- existing `effigy.toml`
- existing `docker-compose.yml`
- real web-oriented service stack without unrelated active dirt

Consumer config added:

- `[containers] default = "services"`
- `[containers.services]` over the repo's existing `docker-compose.yml`
- explicit `primary_service = "postgres"`
- explicit host ports and a TCP health gate on `127.0.0.1:5432`

Real proof commands:

```sh
cargo run --bin effigy -- container status --repo /Users/tom/Dev/projects/example-site --json
cargo run --bin effigy -- container up --repo /Users/tom/Dev/projects/example-site --detach --json
cargo run --bin effigy -- container status --repo /Users/tom/Dev/projects/example-site --json
cargo run --bin effigy -- container down --repo /Users/tom/Dev/projects/example-site --json
cargo run --bin effigy -- container status --repo /Users/tom/Dev/projects/example-site --json
colima stop --profile example-site
```

Observed result:

- default container resolution worked
- detached bring-up succeeded on the real machine
- running status showed live `postgres`, `minio`, and `mailhog` services
- graceful teardown succeeded
- cleanup returned the temporary Colima profile to `Stopped`

## Validation

Targeted validation:

- `cargo test --lib container`
- `cargo test --lib run_manifest_task_builtin_config_schema_prints_canonical_template`
- `cargo test --lib command_kind_and_name_maps_command_variants`
- `cargo test --test cli_output_tests container`

Batch validation:

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`
- `git -C /Users/tom/Dev/projects/example-site diff --check`

## Churn Check

Churn stayed acceptable in this batch.

The only mid-batch widening was justified by real-machine proof:

- no-Docker host fallback was not speculation; this laptop has `colima` but not
  `docker`
- Colima running-state detection needed hardening against real lowercase
  `running` output

Those changes were direct product-shape corrections, not scope drift.

## Resulting Boundary

`107` is complete.

The container foundation is now real enough to stop debating the command
contract and move into operator experience work.

## Next Task

Execute
[`108-implement-attached-container-session-ux-and-task-composition.md`](../../../specs/batch-cards/108-implement-attached-container-session-ux-and-task-composition.md)
to widen the attached session UX/TUI surface and add one honest repo-owned task
composition path on top of the shipped foundation.
