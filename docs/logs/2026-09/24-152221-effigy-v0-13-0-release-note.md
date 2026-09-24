# Effigy v0.13.0 Release Note

Date: 2026-09-24
Owner: Effigy maintainers
Related roadmap: g10 — Agent-Native Skill Execution
Release: 0.13.0

## Summary

Effigy 0.13.0 brings the unreleased task, graph, documentation, skill, catalog-pack, and reliability work into one CI-installable binary. It also removes executable command-group aliases. The full per-change record is in the [0.13.0 changelog](../../../CHANGELOG.md#0130---2026-09-24).

## User-Visible Changes

- `[drafts]` gives provisional tasks explicit inventory and execution commands while `[tasks]` remains the published command surface. Draft expiry is advisory.
- Segmented catalog graph scopes and repository-defined documentation graph profiles keep code and docs queries local to the selected scope. `effigy docs context --sources` can query explicitly opted-in neighboring repositories with source provenance.
- `effigy skill run` can resolve a named installed skill and use `--stdio passthrough` for raw bytes and the leaf task's exit status. Explicit `--path` remains authoritative.
- `effigy service pack` can install, inspect, update, roll back, and reset independently versioned catalog packs. The compiled baseline remains available; ordinary catalog use makes no network request. The 0.13.0 support policy records this as the first public `service pack update` release.
- Default `effigy doctor` is a bounded structural check. `effigy doctor --deep` adds content scans and selected-scope health tasks under an overall deadline.
- Release gates persist redacted environment evidence and full per-gate logs. Many routing, dependency-link, container, and JSON-contract failures are corrected; see the changelog for the complete list.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`, `RELEASE`.
- Movement: one flat task surface and workspace-wide graph/doc assumptions -> explicit draft lifecycle, catalog-scoped code graphs, committed docs profiles, opt-in cross-repository context, and bounded health. Unreleased local behavior -> a gate-checked 0.13.0 release candidate.
- Remaining gap: None for the 0.13.0 release. Consumer CI adoption remains the next observation point.

## Migration Notes

- Replace executable group aliases such as `effigy local <command>`, `effigy repo <command>`, `effigy deliver <command>`, `effigy extend <command>`, and `effigy admin <command>` with the corresponding direct `effigy <command> ...` invocation. Use `effigy help <group>` to find the command. The former group words now follow ordinary repository selector routing.
- Call `effigy doctor --deep` when content scans or repository health tasks are required. The default doctor runs structural checks only. `doctor --deep` may run repo-owned tasks and can have their side effects.
- Pack installation and update are explicit operations. Existing repositories need no pack configuration to retain the compiled catalog behavior.

## Validation

- Exact candidate `13e86c558ef8c8f7165a52a71b45569c19d2d5f7`: GitHub `ci.yml` workflow-dispatch run [36008949852](https://github.com/inflatable-cookie/effigy/actions/runs/36008949852) passed all jobs.
- `effigy release gates` passed all seven gates on that candidate before the release-owned file changes.
- `effigy release gates` passed all seven gates on the prepared 0.13.0 files: exact-candidate CI, format, workspace tests, QA, release build, smoke, and metadata.
- [Release workflow 36022457909](https://github.com/inflatable-cookie/effigy/actions/runs/36022457909) passed release gates, all four Linux/macOS platform builds, GitHub release creation, and Homebrew tap update.
- `effigy release verify-install --tag v0.13.0` passed all five checks: install from the Git tag, binary version, task fixture, prefixed built-ins, and JSON help.

## Rollback Notes

- `v0.12.1` is the previous known-good tag. CI consumers can pin that exact tag if 0.13.0 blocks installation or operation.
- Pack installation/update uses a durable store with rollback and reset commands. Rollback revalidates the selected record; reset returns selection to the compiled baseline and preserves installed content.
- Do not move or reuse a published 0.13.0 tag. A post-tag fix goes into a new patch release.

## Compatibility

- Repositories without `[drafts]` or `[catalog.graph]` retain their prior task and graph behavior.
- Pack selection keeps project and user overrides ahead of the active installed pack, with the compiled baseline as fallback. Failed pack operations preserve the previous selection.
- Existing JSON command envelopes remain `effigy.command.v1`; new payloads and fields are versioned or additive as listed in the changelog.
- `storage::put` create-only behavior is opt-in. Omitting `create_only` retains unconditional writes.

## Next

Observe consumer CI installing the published `v0.13.0` binaries. Choose the next strategic runway through Northstar planning.
