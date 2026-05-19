# g07.056 - Codebase Leanness And Boundary Hardening Suite

Status: Planned
Depends on: `g07.055`

## Goal

Turn the reusable codebase sweep audit into a bounded cleanup sequence that
makes Effigy easier to extend without changing product behavior.

The target is not fewer files for its own sake. The target is predictable
ownership:

- graph language extractors share record-emission code without sharing language
  semantics
- graph query and manifest indexing code is split into readable units
- `effigy init` remains one coherent setup front door without becoming a hidden
  orchestration blob
- JSON/report/help contracts follow obvious conventions
- runner and test harness debt is reduced only where ownership is clear

## Why This Exists

The graph and init work landed quickly and usefully, but both surfaces are now
large enough that the next feature pass would be slower without cleanup.

The audit found no emergency defect. It did find several places where future
work will keep paying tax:

- duplicated graph record builders across language extractors
- very large `effigy-codegraph` manifest/query modules
- init setup inventory mixing detection, rendering, command construction, and
  execution
- repeated help topic and JSON rendering shapes
- runner modules still carrying domain-heavy command logic
- repeated test fixture setup across runtime/container/bootstrap surfaces

This suite turns those findings into sequential, reviewable work.

## Scope

- reduce duplication without changing public behavior
- split oversized modules along existing ownership lines
- preserve graph JSON contracts and local graph artifact format
- preserve `effigy init` CLI and checklist behavior
- keep runner extraction narrow and adapter-focused
- add or preserve focused tests for each cleanup
- update docs only when a public convention changes

## Non-Goals

- no broad rewrite of graph storage, query ranking, or extractor semantics
- no dynamic language plugins
- no MCP server, daemon, or JavaScript runtime dependency
- no merge of crates merely to reduce crate count
- no release, deploy, state, or distribution mutation changes
- no `.github/workflows/` edits
- no hidden behavior changes in CLI help or JSON contracts

## Ordered Follow-Up Lanes

1. [`057-codegraph-language-emitter-deduplication.md`](./057-codegraph-language-emitter-deduplication.md)
2. [`058-codegraph-manifest-query-module-decomposition.md`](./058-codegraph-manifest-query-module-decomposition.md)
3. [`059-init-setup-module-boundary-cleanup.md`](./059-init-setup-module-boundary-cleanup.md)
4. [`060-json-help-contract-consistency-cleanup.md`](./060-json-help-contract-consistency-cleanup.md)
5. [`061-runner-domain-boundary-and-test-fixture-cleanup.md`](./061-runner-domain-boundary-and-test-fixture-cleanup.md)
6. [`062-crate-boundary-rejustification-and-planning-hygiene.md`](./062-crate-boundary-rejustification-and-planning-hygiene.md)
7. [`063-codebase-leanness-closeout.md`](./063-codebase-leanness-closeout.md)

## Acceptance Criteria

- duplicate scan no longer reports critical graph extractor emission blocks
- graph manifest/query modules are split without behavior or contract drift
- init setup code is divided into clear model/detect/render/execute modules
- JSON/help consistency work has focused proof and no output regressions
- runner/test cleanup has a narrow diff with measurable ownership improvement
- crate-boundary review records what to merge, keep, or defer
- docs/spec planning state names the next active work clearly

## Batch Cards

- [`1006-open-codebase-leanness-lane.md`](./batch-cards/1006-open-codebase-leanness-lane.md)
- [`1007-deduplicate-codegraph-language-emitters.md`](./batch-cards/1007-deduplicate-codegraph-language-emitters.md)
- [`1008-decompose-codegraph-manifest-and-query-modules.md`](./batch-cards/1008-decompose-codegraph-manifest-and-query-modules.md)
- [`1009-split-init-setup-inventory-and-wizard-boundaries.md`](./batch-cards/1009-split-init-setup-inventory-and-wizard-boundaries.md)
- [`1010-normalize-json-report-and-help-topic-conventions.md`](./batch-cards/1010-normalize-json-report-and-help-topic-conventions.md)
- [`1011-trim-runner-domain-and-test-fixture-duplication.md`](./batch-cards/1011-trim-runner-domain-and-test-fixture-duplication.md)
- [`1012-review-crate-boundaries-and-planning-hygiene.md`](./batch-cards/1012-review-crate-boundaries-and-planning-hygiene.md)
- [`1013-close-codebase-leanness-lane.md`](./batch-cards/1013-close-codebase-leanness-lane.md)

## Next Task

Start [`1007-deduplicate-codegraph-language-emitters.md`](./batch-cards/1007-deduplicate-codegraph-language-emitters.md).
