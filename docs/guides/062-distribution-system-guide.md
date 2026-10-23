# 062 - Distribution System Guide

Status: **retired in v0.15**. The distribution command family and the
`[distribution]` table became Effigy repository tasks and `[release]` settings
under contract [054](../knowledge/contracts/054-release-platform-contract.md).
This guide records what the surface was, where each part went, and what a
distribution project should do now.

Use:
- [`051-release-orchestration.md`](051-release-orchestration.md) for the release
  Run and `[release]` configuration that replaced this surface
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)
  for maintainer policy and binary channel rules
- [`release.md`](../knowledge/contracts/release.md) for Effigy's own release

## What Retired

Q-009 (2026-10-21) cut the distribution surface and Tom accepted the release
platform contract the next day. v0.15 retired:

| Retired | Replacement |
| --- | --- |
| `effigy release validate` | The repo task `effigy release:metadata` (a `certify` gate for Effigy) |
| `effigy release preflight` | The repo task `effigy release:preflight` (a `pre` gate for Effigy) |
| `effigy release proof` | The repo task `effigy release:proof`, plus the Run's certified artifacts and receipt |
| `effigy release check-binary` | The repo task `effigy release:check-binary` (`sh scripts/release/checks.sh check-binary`) |
| `effigy release verify-install` | The repo task `effigy release:verify:install`, wired as the `binaries` target's `verify` |
| `effigy release evidence validate` | The repo task `effigy release:evidence:validate` |
| `effigy release evidence closeout` | The repo task `effigy release:evidence:closeout` |
| `effigy release evidence summary` | `effigy release receipt <run>` |
| `[distribution]` | `[release]` identities, gates, artifacts and targets, plus repo tasks |
| the `distribution::` Rhai module | Repo tasks and ordinary process calls |

Each retired command refuses with the replacement above, and each is recorded
in [retired.toml](../knowledge/retired.toml). A manifest that still declares
`[distribution]` loads and is ignored with one deprecation notice per manifest,
because dropping the table changes nothing else about a v0.14.1 project.

## Why It Retired

The distribution surface was Effigy's own release machinery generalized too
early. It hardcoded Effigy's Cargo package, its docs and smoke tasks, its
GitHub workflow and its Homebrew tap, and it grew an evidence file format
(`distribution-summary.env`) that only Effigy read. Contract 054 replaced it:

- one `[release]` declaration per project: identities, gates, artifacts and
  targets;
- one durable Run with a checkpoint at every stage boundary, so a release
  resumes instead of restarting;
- `task` targets for any destination a project already has, with the candidate
  handed over as `effigy.release.input.v1` and the outcome returned as
  `effigy.release.output.v1`;
- the receipt as the durable record, instead of a sidecar summary file.

Effigy's own checks kept working; they simply moved into this repository as
tasks, where they can name Effigy's workflow, artifacts and tap directly.

## What A Project Does Now

1. Declare `[release]`: one identity per versioned thing, `[release.gates]` for
   the checks, `[release.artifacts]` for what must be published, and
   `[release.targets]` for the destinations. The single-identity shorthand still
   covers a project whose shape matches the old `[release]` table exactly.
2. Move any distribution-specific check into a `[tasks]` entry, and wire it as
   a gate (with `stage = "pre"` or `"certify"`) or as a target's `verify`.
3. Keep publishing in the project's own workflow behind a `task` target. v0.15
   ships no native publish providers; v0.16 adds them behind the same target
   interface.
4. Read `effigy release receipt <run>` for the durable record; add a `closeout`
   task if a project wants a receipt copy in its tree.

Effigy's own mapping is in [`config/release.toml`](../../../config/release.toml)
and [`config/tasks.toml`](../../../config/tasks.toml) under the `release:`
namespace, and
[`release.md`](../knowledge/contracts/release.md#repo-tasks-that-replaced-the-retired-commands)
describes each task.

## Historical Notes Kept

The channel matrix, CI pinning policy and Windows/GLIBC baselines never lived
in this guide; they are in
[`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md),
which still governs them. The macOS checksum sidecars and the annotated-tag
publication rules in
[`release.md`](../knowledge/contracts/release.md) are unchanged by this
retirement.

## Related Guides

- [049-ci-binary-distribution-and-release-protocol.md](049-ci-binary-distribution-and-release-protocol.md)
- [051-release-orchestration.md](051-release-orchestration.md)
- [052-changelog-workflows-and-northstar-profile.md](052-changelog-workflows-and-northstar-profile.md)
- [085-v0.15.0-consumer-migration.md](085-v0.15.0-consumer-migration.md)

## Next Step

For a new project, start at [`051-release-orchestration.md`](051-release-orchestration.md)
with `effigy release --plan`, and declare a `task` target for the destination
you already publish to.
