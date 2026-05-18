# 901 - Baseline Storage, Query Design, And Dependency Spike

Roadmap: [`../002-graph-storage-and-json-contracts.md`](../002-graph-storage-and-json-contracts.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Capture the implementation baseline before adding graph code.

This card decides the initial storage/dependency posture and records enough
evidence for the rest of `g07` to avoid guesswork.

## Scope

- inspect current workspace crates and CLI command layout
- choose initial crate/module placement for graph code
- evaluate native dependencies:
  - SQLite/Rusqlite posture
  - FTS5 availability
  - tree-sitter Rust crate posture
  - first language grammar crate candidates
  - ignore/walkdir reuse
- draft the first graph record vocabulary
- draft the first JSON schemas by shape, not full implementation
- record expected artifact paths under `.effigy/graph/`
- identify the first fixture repo set

## Guardrails

- no public command implementation in this card unless required for a compile
  proof
- no extractor implementation
- no MCP/plugin/server scope
- no dependency addition without a note explaining why it is needed
- no large test sweep; use docs checks and targeted compile checks if code is
  touched

## Acceptance

- dependency decision is recorded
- first crate/module placement is recorded
- storage and JSON shape baseline is recorded
- first fixture set is named
- `902` remains implementable without reopening product boundaries

## Findings

### Crate And Command Placement

- add a new first-party domain crate: `crates/effigy-codegraph`
- keep graph storage, indexing, extractor traits, and query APIs in that crate
- add CLI parsing in `crates/effigy-cli`
- add a new runner command family `src/runner/graph_command/` that stays thin:
  repo resolution, argument adaptation, JSON/text output, and error lifting
- do not place graph logic in `effigy-builtin`; current top-level command
  families such as `docs`, `contracts`, `artifact`, `distribution`, and
  `release` already live under `src/runner/*_command`

### Dependency Posture

- storage: use `rusqlite` as the first storage owner
- SQLite features: require `bundled` and `fts5`
- parsing: use `tree-sitter` Rust bindings only; no JavaScript runtime
- first grammar crates, but only when later cards need them:
  - `tree-sitter-rust`
  - `tree-sitter-php`
  - `tree-sitter-javascript`
  - `tree-sitter-typescript`
- traversal: reuse `ignore` posture already proven in
  `crates/effigy-scan/src/support/traversal/walker.rs`
- change detection later may reuse `notify`, but `901` does not require it
- parallel indexing is deferred; do not add `rayon` until real indexing cost is
  measured

### Storage Posture

- graph artifacts live under `.effigy/graph/`
- first storage file: `.effigy/graph/graph.db`
- public status/query output is JSON; the DB layout is private
- schema versioning should exist in both:
  - storage metadata inside the DB
  - JSON envelope fields on every public response

### First Record Vocabulary

The first normalized owner set should be:

- `file`
  - repo-relative path
  - content fingerprint
  - language id
  - file size
  - skip/index status
- `symbol`
  - stable graph id
  - kind
  - display name
  - canonical name
  - file and range
  - extractor owner/version
- `edge`
  - stable graph id
  - edge kind
  - from/to node ids when resolved
  - unresolved target name when not resolved yet
  - confidence
  - provenance
- `reference`
  - file/range
  - target symbol id or unresolved name
  - reference kind
  - confidence
- `diagnostic`
  - file
  - extractor id
  - severity
  - message
  - optional range
- `index_run`
  - started/finished timestamps
  - repo root
  - graph schema version
  - extractor set version
  - counts
- `extractor`
  - id
  - version
  - language ids
  - capability flags

### First JSON Shape Baseline

Every public `graph` JSON response should include:

- `schema`
- `version`
- `command`
- `repo_root`
- command-specific payload

The first command families expected by later cards are:

- `status`
- `files`
- `search`
- `node`
- `callers`
- `callees`
- `impact`
- `context`

`902` should define typed response owners for these shapes even if some command
families are not executable yet.

### Artifact And Freshness Baseline

- `.effigy/graph/graph.db` is the canonical local artifact
- no daemon-owned cache
- no sidecar generated summary files in v1
- freshness should be tracked against repo-relative file fingerprint state, not
  only mtimes
- default path exclusion posture must include:
  - `.git`
  - `target`
  - `node_modules`
  - `vendor`
  - runtime cache directories under `.effigy/`

### First Fixture Set

Use a narrow first-party fixture set:

- Effigy repo itself for Rust, TOML, manifest, and docs indexing
- existing manifest/include/task fixtures in `effigy-manifest`
- existing docs/contracts/guides surfaces for markdown anchor indexing
- a new minimal PHP fixture repo or test fixture tree for namespace/class/function
  extraction
- a new minimal JS/TS fixture tree later, introduced with `909`

### Open Decisions Intentionally Deferred

- exact graph id encoding format
- whether unresolved references live in a dedicated table or share the edge path
- whether snippets are stored or derived on read
- watch-mode invalidation shape
- cross-language relation ranking

## Suggested Validation

```bash
rg -n "rusqlite|sqlite|tree-sitter|ignore" Cargo.toml crates src
cargo metadata --format-version 1 --no-deps
effigy docs check paths docs/roadmaps/g07 docs/specs/085-code-graph-intelligence-strict-lane.md
git diff --check
```

## Next Task

Execute `902`.
