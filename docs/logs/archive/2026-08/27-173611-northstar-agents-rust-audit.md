# Northstar AGENTS and Rust Audit

Status: complete with limitations
Created: 2026-08-27
Batch: northstar-agents-rust-audit

## Summary

- Read-only AGENTS audit retained the current section structure. `CLAUDE.md`
  has the exact `@AGENTS.md` bridge.
- Generated the canonical Rust activation/profile/deviations surfaces.
- Added the operator-authorized `vendor/s3` workspace exclusion. The patched
  third-party workspace remains immutable recorder context, not audit or repair
  scope.
- Finalized Rust audit `effigy-20260827-rust-audit` as degraded with no Rust
  repair wave and no audit-owned source changes.

## Evidence

- Instruction checker: 87 nonblank lines, 4,997 bytes, about 1,250 tokens;
  9 headings, 8 links, 1 fenced block; bridge passed.
- Rust scope: 2 discovered workspaces, 38 package manifests, 80 targets, and
  16 features. 95 Effigy-owned mutable anchors; 23 `vendor/s3` paths are
  immutable read-only context.
- Pinned payload SHA:
  `41a6f7dd2f4e19fa49486bec91d4bc122fe81db9040904807181d8c541575530`.
- `stopslop 0.5.1` found two SLOP039 candidates. Both are retained,
  evaluation-only forwarders: public `strip_global_json_flag` compatibility
  spelling and domain-specific `load_env_schema` entry.

## Limitations

- `vendor/s3` is a third-party `[patch.crates-io]` workspace. It was inventoried
  exactly but not assessed, formatted, repaired, or otherwise mutated.
- Cargo reports `proc-macro-error2 v2.0.1` as future-incompatible. The Rust
  recorder records this as an MSRV/dependency limitation; no dependency policy
  change was authorized.

## Validation Performed

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed with Cargo's
  future-incompatibility warning above.
- `effigy health` — completed with the same pre-existing Cargo warning.
- `effigy qa` — succeeded; task report recorded completion at
  `2026-08-27T16:38:57Z`.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `OPERATE`.
- Movement: no Rust-quality audit contract at dispatch -> strict activation,
  deterministic recorder evidence, and explicit vendor boundary.
- Remaining gap: resolve or accept the future-incompatible transitive dependency
  through a separately authorized dependency-policy change.

## Next Task

- Await operator review of this maintenance audit; do not infer a new roadmap
  lane or dependency-policy change.
