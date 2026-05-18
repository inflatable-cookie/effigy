# g07.003 - Graph Index Command And Freshness Model

Status: Complete
Depends on: `g07.002`

## Goal

Implement the repo walking, indexing command, and freshness model.

This lane makes graph creation predictable before language intelligence becomes
deep.

## Scope

- add `effigy graph index`
- add `effigy graph status --json`
- create `.effigy/graph/`
- walk the repo with gitignore-aware filtering
- exclude generated and heavy directories by default
- track file hashes and mtimes
- track extractor versions in freshness state
- report stale, deleted, new, skipped, and failed files
- make repeated indexing idempotent

## Default Exclusions

Exclude by default:

- `.git/`
- `.effigy/runtime/`
- `.effigy/cache/`
- `target/`
- `node_modules/`
- `vendor/`
- package-manager caches
- generated compose output
- binary and large media files

Do not exclude source files just because they live in `crates/`, `src/`,
`tests/`, `docs/`, `skills/`, or external fixture directories. Use path policy,
size policy, and file type policy explicitly.

## CLI Contract

`effigy graph index --json` should report:

- repo root
- files seen
- files indexed
- files skipped
- files stale
- diagnostics
- duration
- graph artifact paths

`effigy graph status --json` should report:

- graph exists
- graph version
- last index time
- dirty/stale state
- language coverage
- file/symbol/edge counts
- recommended next command

## Non-Goals

- no deep language extraction beyond a placeholder/no-op extractor
- no watch mode in this lane
- no context-pack ranking
- no full-text search command yet

## Tests

- fixture repo indexing
- ignored path exclusion
- stale file detection
- deleted file cleanup
- repeated index idempotence
- JSON status snapshots

## Acceptance Criteria

- `effigy graph index` creates a local graph artifact
- `effigy graph status --json` tells an agent whether the graph is usable
- stale state is deterministic and does not require guesswork
- heavy/generated paths are not indexed accidentally

## Next Task

Continue `g07.006`.
