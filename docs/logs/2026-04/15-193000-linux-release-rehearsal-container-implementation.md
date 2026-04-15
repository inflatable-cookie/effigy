# Linux Release Rehearsal Container Implementation

Date: 2026-04-15 19:30 Europe/London
Roadmap: `g02.007`

## Summary

Implemented the first local Linux release-rehearsal surface for Effigy.

Shipped pieces:

- manifest-backed `linux-release` container in
  `containers/effigy.containers.toml`
- Ubuntu 22.04 build image in `infra/release/linux/`
- repo-owned `release:linux:env` attached task for manual inspection
- repo-owned `release:linux:rehearse` Rhai task for repeatable pre-release
  proof
- command-surface fix so `effigy container shell --command <CMD>` now runs via
  `sh -lc`

## Real Proof

Ran:

- `cargo run --bin effigy -- release:linux:rehearse`

That real machine proof:

- built the Linux binary inside the local Ubuntu 22.04 container
- ran `smoke:release` against the built Linux binary
- ran `distribution check-glibc-floor --max-glibc 2.35`
- copied the resulting binary to
  `.effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu`
- wrote local proof metadata to
  `.effigy/linux-release/artifacts/rehearsal.txt`
- shut the container environment back down cleanly

## Real Gaps Closed In This Batch

- `docker compose exec` command override was not honest before this batch;
  `--command` now runs as a shell command string instead of one argv token
- the first Jammy image attempt used an invalid upstream Rust image tag
- the detached rehearsal service could not keep `tty`/`stdin_open` on the
  `colima nerdctl` path
- the initial pinned Rust 1.87 toolchain was too old for the current
  dependency set, so the rehearsal image now tracks `stable`
- the rehearsal task now drops the stale local builder image before bring-up so
  Dockerfile/toolchain edits are not masked by an old local tag

## Vision Target Delta

- Tags: `RELEASE`, `OPERATE`, `MAINT`
- Moved: `Linux release build trusted mostly to CI and manual operator
  choreography` -> `repeatable local Linux build/smoke/GLIBC rehearsal through
  Effigy's own shipped container surface`
- Open: decide whether `g02.007` is now ready to move from Linux proof into the
  actual Effigy release-closure batch.

## Next Task

Execute
[`112-decide-post-linux-rehearsal-release-boundary.md`](../../specs/batch-cards/112-decide-post-linux-rehearsal-release-boundary.md).
