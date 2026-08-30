# Documentation Graph Profile And Structural Index

Status: complete
Created: 2026-08-30
Roadmap: g08.035
Card: 1088
Spec: 108
Contract: 041

## Summary

- Added typed `[docs_policy.graph]` grammar and validation in `effigy-manifest`.
- Compiled one repository-owned profile in `effigy-codegraph`, with a normalized
  fingerprint in graph freshness identity.
- Replaced whole-file Markdown heading spans with exact hierarchical section
  spans. Baseline documents now use kind `document`; profile kinds, field facts,
  and typed relation edges are extracted only when configured.
- Used generic handbook/playbook/bulletin fixtures. No Northstar path, status,
  or kind entered generic runtime logic. No `docs context` command shipped.

## Fixture Cases

Generic profile vocabulary: roots `handbook`; fields `state` / `steward`;
currentness `live` / `retired`; kinds `playbook` / `bulletin`; relation
`see-also`.

| Case | Result |
| --- | --- |
| Missing `[docs_policy.graph]` | baseline mode, no error |
| Arbitrary tokens round-trip | composed manifest keeps `see-also`, `playbook`, `state` |
| Empty/escaped/absolute roots | deterministic compose errors |
| Unknown graph key | serde unknown-field failure |
| Currentness field missing or overlapping sets | compose error naming the field/value |
| Kind without include; authority `101`; empty relation selectors | compose errors |
| Duplicate single-valued `state` | both facts stored plus a span-bearing diagnostic |
| Kind include overlap | index fails before extraction, naming the path and kinds |
| Symlink root escape | compile rejects the escaped root |
| Field line inside a fence | ignored; typed fence links ignored |
| Profile-only field addition | fingerprint changes; markdown paths go stale; steward fact appears after reindex |

## Exact-Span Examples

Baseline fixture `docs/contracts/example.md`:

```text
# Title
Intro.
## Alpha
Alpha body.
### Nested
Nested body.
## Beta
Beta body.
```

- document kind is `document`, not `contract`
- `heading-h1` `#title` starts at line 1 / byte 0 and continues through later
  headings to EOF
- `heading-h2` `#alpha` ends at the start of `#beta`
- `heading-h3` `#nested` stays inside the alpha section span
- `heading-h2` `#beta` ends at the document end byte
- setext `Title` / `=====` in `notes/intro.md` starts at line 1 / byte 0 and
  ends at EOF

Playbook `handbook/playbooks/setup.md` with the generic profile: document kind
`playbook`; one `doc-field` `state=live` on line 3; one `see-also` edge to
`handbook/playbooks/ops.md` with a `doc-rel` reference span.

## Profile Fingerprint Proof

Freshness metadata key: `docs_profile_fingerprint`.

A first index stores the compiled-profile hash. Adding only
`[docs_policy.graph.fields.steward]` without changing Markdown made
`handbook/playbooks/setup.md` stale, changed the stored fingerprint, and
produced the steward fact on reindex. Unrelated manifest/bundle compose
failures still fall back to structural indexing when no graph profile is
present.

## Validation

| Check | Result |
| --- | --- |
| `cargo test -p effigy-manifest` | passed (115 lib + 14 `docs_policy_graph` + existing integration tests) |
| `cargo test -p effigy-codegraph` | passed (79 tests) |
| `cargo test -p effigy-doctor manifest_schema` | passed (26 tests, including graph keys) |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy -p effigy-manifest -p effigy-codegraph --all-targets -- -D warnings` | passed |
| `git diff --check` | passed |
| `effigy graph affected --stdin` | passed after index rebuild |

Doctor on the worker worktree before edits: `ok:19 warn:1 err:0` (god-files).
No new doctor errors.

## Affected Analysis

Extractor version `markdown-anchors` moved `0.1.0` → `0.2.0` and the profile
fingerprint joined freshness, so the first graph refresh rebuilt Markdown.

- `effigy graph index --json`: `indexed_files=3762`, `failed_paths=[]`,
  `symbols=37358`, `edges=174383`, `references=80701`
- First `graph affected` during that rebuild hit the 120s lock budget
- Retry after rebuild: freshness `ready` (auto-refreshed 16 files in 29158ms);
  35 changed paths; 100 bounded affected files; likely test files were
  heuristic neighbors (bootstrap/builtin tests), not the new crate tests.
  Card validation used `cargo test -p effigy-manifest` and
  `cargo test -p effigy-codegraph` instead of those heuristic neighbors.

## Review Repair

PR 55 requested five blockers. Repairs:

1. Rebased onto `origin/main` (`d2a679b95`, PR 54 Rhai thread-local env). Kept
   that changelog/PAPERCUTS closeout; retained the git-fetch papercut.
2. `load_docs_policy_graph_config` now propagates composition and bundle-default
   errors. A configured graph behind a broken include cannot fall back to
   baseline.
3. Roots and kind globs normalize `./` and `.`; matching uses `globset` with
   literal separators so `setup*guide.md` does not match `setup-guide-extra.md`.
4. Typed relations extract as unresolved destinations, then a post-index pass
   resolves them against the symbols and files that actually exist in that
   generation. Incremental reindex demotes stored `doc-rel` records first so a
   deleted or unindexed target cannot drop the source edge. Edge and reference
   IDs keep the parser-normalized destination, so revalidation does not reparse
   source text. Missing anchors, `.ignore` / `.effigy` exclusions, internal and
   escaping symlinks, non-Markdown files, missing files, and external URLs stay
   unresolved. An unchanged source whose target later leaves the inventory keeps
   the original destination as `unresolved_target`.
5. Repository relation edges use kind `doc-rel` with the declared token in
   provenance detail, so tokens such as `contains` cannot collide with
   structural traversal.

## Readiness Transition

Card `1088` is complete. Card `1089` is ready: bounded `effigy docs context`
retrieval over these structural records. Do not implement `1089` in this PR.

## Vision Target Delta

- Tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Baseline: Markdown indexed as whole-file headings without repository-owned
  kinds, facts, or typed relations
- Current: generic profile grammar, exact section spans, field/relation facts,
  and profile-aware freshness on the shared graph
- Open: card `1089` query/JSON surface; card `1090` generic plus Northstar
  adoption proof

## Next Task

Execute ready card
[`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md).
