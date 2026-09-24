# g10 — Agent-Native Skill Execution

Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources, then carry the
same agent-operability discipline into bounded reliability repairs without
weakening source isolation, machine-safe output, or runtime contracts.

## Approved frontier

- [`g10.014`](./014-resolve-compatible-dependabot-lock-updates.md) — complete
  through PR #117.
- [`g10.015`](./015-resolve-direct-dependabot-upgrades.md) — running on the
  direct dependency upgrades.
- [`g10.016`](./016-resolve-hickory-follow-up.md) — approved downstream for
  the bot PRs retargeted to Hickory 0.26.3; wait for `g10.015`.

`g10.001` through `g10.014` are complete.

## Boundaries

- Existing explicit `--path`, human output, and JSON envelope behavior stay
  compatible.
- Skill lookup is local and deterministic. No registry or network acquisition.
- Consumer catalogs and runtime configuration remain isolated.
- Release and workflow mutations remain separately gated.

## Queue lifecycle adoption

- [g10.002 Effigy-hosted lifecycle hook](002-adopt-effigy-hosted-lifecycle-hook.md)
  completed through Queue and published its terminal lifecycle record at
  revision 8. It changed no product priority and authorizes no sibling work.
- [g10.009 prospective-merge protocol migration](009-prospective-merge-protocol-migration.md)
  owns the operator-authorized v4 manifest update. It is configuration-only
  and does not change the g10 product frontier.

## Papercut intake disposition

- The cold `graph explore` hang entry is closed as stale: current main already
  applies the shared graph time budget and typed timeout path through commit
  `6ce994047c54486c2f38e9f33a882565b019ade7`.
- Vendored skill portfolio sync remains triage-only and is not in this frontier.

## Monorepo graph intake

- Effective catalog membership and aliases are the only partition topology.
- Member manifests opt into `[catalog.graph] segmented`; `independent` selects
  a separate physical database and lock.
- Root queries prune segmented members. One-catalog work never refreshes a
  sibling; whole-workspace fan-out is explicit.

## Dependency-maintenance prerequisite

- `g10.007` repaired the base Cargo resolution through PR #110 without widening
  the retained graph lane.
- `g10.006` then rebased, revalidated, and merged through PR #109. Both tasks
  have terminal revision-8 lifecycle records.
- The operator requested resolution of ten open Dependabot PRs after the
  0.13.0 release and selected consolidated batch PRs. `g10.014` owns six
  compatible lockfile updates; `g10.015` owns the direct Argon2, tree-sitter,
  and tabled upgrades plus the overlapping tree-sitter-language update.
  `Cargo.lock` ownership and merges are serial.
- Dependabot retargeted #97 and #100 to Hickory 0.26.3 after PR #117 merged
  0.26.2. `g10.016` owns that follow-up and the corrected source-PR disposition.

## Task-surface intake

- `[tasks]` remains the compatible published repository command interface.
- Full-table `[drafts]` carries temporary proofs and environments behind
  explicit inventory and execution commands.
- Draft expiry is visible cleanup evidence, never automatic deletion or an
  execution block.
- Machine-local drafts and pruning remain future decisions, not part of
  `g10.008`.

## Doctor scalability intake

- Default `doctor` is a 10-second structural tier and never runs content scans
  or repository `health`.
- Explicit `doctor --deep` owns catalog-scoped scans and health under a
  120-second default deadline.
- One selected scope gets one shared content inventory and exact per-file
  cache facts. Root work prunes members; whole-catalog fan-out is explicit.
- Timeout is a non-zero partial report with process-tree cleanup, not silence
  or warning-only success.

## Next Task

Run the queued Hickory follow-up after the direct-dependency batch reaches
terminal closeout; then return to planning with the operator. Doctor
cache pruning and S3 retirement remain separate decisions.
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:4bdb9760429347b040d4820f8d9f496b8c7c1ef3926e55cdbae97da0e131a8d9 -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
| g10.004 | complete | none | 8 | sha256:493fff197276ab92aab90e09838bed45ba1de970a3a5d11cbc5fa8091e719e5d |
| g10.005 | complete | none | 8 | sha256:bbdc3e4907f88ceb64be891fff87847854e123e4d552c565080fcda70a4d8881 |
| g10.006 | complete | none | 8 | sha256:b0134c685fb320d7a8acdcd4f0db4a6765270c3dd8b8bb61496cad0682e6dc49 |
| g10.007 | complete | none | 8 | sha256:04342987457e1bea973556297a4bef39e1592794e0dd3d52ebcb4457ae915995 |
| g10.008 | complete | none | 8 | sha256:6cfd213603aefd7367edc6d5cdc5eb9dae4b3ff09062defd6fa1cf80c736dc8b |
| g10.009 | complete | none | 8 | sha256:e9eec1299d0c8d71b4112224a5275f574100b411d9d7e690b937bf01f953a1f7 |
| g10.010 | complete | none | 8 | sha256:9b534c710ef6277f3996c4a3b0eee36cf128019480fd7e8c1b0b5f2dc710303b |
| g10.011 | complete | none | 8 | sha256:662faf219b8d797233e1c1fafd7ade3e3620ac1e6983de7eef1b9333feb3aa58 |
| g10.012 | complete | none | 8 | sha256:77c770dee88cc0c6e080d20ea7b19e6e00f68bbf4f73f8b9bdd4cb6272398ae9 |
| g10.013 | complete | none | 8 | sha256:1806cd3a5f2835288804585cb0c7541b52f17fe2dedff13d5547046ceb80192e |
| g10.014 | complete | none | 8 | sha256:63fb0f233fad44d1f74af5cfe3ed024e86a8d3102c9ad0d514f5637f3832a799 |
<!-- northstar:lifecycle:end -->
