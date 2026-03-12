# 029 - Docs QA Checklist and Validation

Use this checklist before merging documentation changes.

Use it when you need the shortest reliable path from "docs changed" to
"confidence that the docs still hang together."

## Start Here

For most docs changes, the first commands should be:

```sh
effigy qa:docs
effigy docs check-workflow-paths
```

Then widen the scope only when the change touched command behavior, JSON
contracts, or broader repo policy:

```sh
effigy qa
```

## 1) Local Docs QA Checklist

Run in order:

```sh
effigy qa:docs
# dev-checkout fallback:
# effigy-dev qa:docs
# compatibility fallback:
# cargo qa-docs
```

Manual checks:
- newly added guides are linked from at least one landing page
- numbering/title conventions are consistent with nearby guides
- command examples match current CLI flags and behavior
- agent-facing examples do not reintroduce current-directory `--repo` overrides
- JSON examples use current schema names and versions
- completion-candidates examples include both hit and miss telemetry variants
- new log artifacts are indexed in `docs/logs/README.md`
- new log artifacts include a `Vision Target Delta` section
- roadmap/guides vision metadata checks pass via `effigy qa:docs:vision`
- docs-referenced workflow paths resolve via `effigy docs check-workflow-paths`
- vision artifact index is consistent via `effigy docs check-index --policy-index vision`
- vision artifacts have non-empty, actionable follow-on actions via `effigy docs check-next-action --policy vision`
- next-action negative-path coverage lives in Rust CLI tests, not the docs QA runtime bundle

Optional broader check:

```sh
effigy qa
```

## 2) CI Validation Path

Current workflow file:
- `.github/workflows/json-contracts.yml`

Docs gate job:

```yaml
jobs:
  docs-links:
    name: Validate docs links
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Validate docs QA bundle
        run: cargo run --bin effigy -- qa:docs
```

This ensures markdown links resolve, key JSON examples stay contract-aligned, logs remain indexed, and vision metadata coverage is enforced on every pull request/push.
`effigy qa:docs` is now the primary orchestration surface for that bundle.
`effigy qa:docs:agent-defaults` keeps agent-facing docs, setup examples, and
workflow examples from drifting back to copied `--repo .` defaults.
`effigy qa:docs:vision` is now the policy-specific sub-bundle for roadmap/guide heading requirements, report/cutoff text requirements, workflow-path validation, vision index validation, and next-action validation.

## 2a) Built-ins vs Repo Policy

The current docs QA surface intentionally splits into two layers:

- generic built-ins such as `effigy docs check-links`,
  `check-json-examples`, `check-index`, `check-workflow-paths`, and
  `check-forbidden`
- Effigy-specific vision-policy checks that remain in repo policy and task wiring

That boundary is deliberate. Workflow-path validation is generic enough to
reuse across projects, but the remaining vision checks enforce Effigy's own
docs governance model: inventory rules for `docs/vision/README.md`,
`## Next Task` policy, the actionable-verb allowlist, and related rollout
conventions.

Those policy checks should only move into built-ins behind a small config
surface. They should not become hardcoded defaults for every Effigy-adopting
repo.

Design note:
- [`../logs/2026-03/12-093000-docs-policy-config-boundary.md`](../logs/2026-03/12-093000-docs-policy-config-boundary.md)
- [`../logs/2026-03/12-094500-minimal-docs-policy-config-design.md`](../logs/2026-03/12-094500-minimal-docs-policy-config-design.md)

Proposed config direction:
- generic docs engines continue to work with useful defaults
- stricter repo doctrine should move into an optional `[docs_policy]` section
  in `effigy.toml`
- the first config-backed migration target should be the vision index check,
  not the more policy-heavy next-task allowlist logic
- the active next-action path now uses a config-backed built-in, but the
  allowlist semantics still live in repo policy data rather than product
  defaults

## 3) What the Link Checker Validates

Built-in command:
- `effigy docs check-links`

Behavior:
- scans markdown inline links (`[label](target)`)
- ignores `http(s)`, `mailto:`, and in-page anchor links
- resolves relative file targets from the source markdown file directory
- fails if target file/path is missing

Default scope when called with no args:
- `README.md`
- all markdown files under `docs/` recursively

## 4) What the JSON Example Checker Validates

Built-in command:
- `effigy docs check-json-examples`

Behavior:
- inspects section `13) Completion Candidates` in `026-json-payload-examples.md`
- requires at least two JSON example blocks (warm-hit and miss path)
- verifies both blocks include cache telemetry keys:
  - `cache_state`
  - `cache_age_ms`
  - `cache_ttl_ms`
  - `effective_cache_ttl_ms`
  - `cache_ttl_source`
- asserts first block stays `cache_state=hit` and second block stays a miss (`cache_hit=false`)

## 5) What the Logs Index Checker Validates

Built-in command:
- `effigy docs check-index`

Behavior:
- scans `docs/logs/YYYY-MM/*.md` and excludes `docs/logs/README.md`
- parses log links from `docs/logs/README.md`
- fails when any log file is missing from the index
- fails when index entries point to non-existent log files

Helper:
- `effigy docs add-log-index <log-file>` inserts a missing log entry ahead of archived links.

Forward-only policy cutoff:
- logs dated on or after `2026-03-06` must include a `## Vision Target Delta` section
- logs before `2026-03-06` do not require backfill

## 5b) What the Forbidden-Text Checker Validates

Built-in command:
- `effigy docs check-forbidden`

Behavior:
- scans one or more text files for exact substrings that should not appear
- fails when any forbidden substring is found in any scanned file
- works well for agent/docs guardrails such as blocking copied `--repo .`
  examples in active instruction surfaces and workflow snippets

### Named docs-policy indexes

Built-in command:
- `effigy docs check-index --policy-index vision`

Behavior:
- loads a named index definition from `[docs_policy.indexes.<NAME>]` in `effigy.toml`
- uses that config to select the index file, docs directory, optional section scope, and exclusion rules
- lets repo-specific index policy stay declarative instead of hardcoded into the built-in

## 5a) What the Next-Action Checker Validates

Built-in command:
- `effigy docs check-next-action --policy vision`

Behavior:
- loads a named rule from `[docs_policy.next_actions.<NAME>]` in `effigy.toml`
- resolves the indexed artifact set through the named docs-policy index
- requires a configured heading such as `## Next Task`
- requires the first non-empty line in that section to start with an allowlisted actionable verb
- keeps the heading name and allowlist path repo-configurable instead of hardcoded

## 6) Common Failure Modes

### Broken relative path after file move

Symptom:
- `broken link: <file> -> <target>`

Fix:
- update link targets relative to the file that contains the link

### New guide added but not discoverable

Symptom:
- no hard failure, but guide is effectively hidden

Fix:
- add links in one or more entry points:
  - `README.md`
  - `docs/README.md`
  - `docs/guides/README.md`

### Examples drift from runtime flags

Symptom:
- docs reference unsupported flags/options

Fix:
- verify with command help:

```sh
effigy --help
effigy <command> --help
```

## 7) Suggested PR Checklist Section

Copy into PR description:

```md
## Docs QA
- [ ] `effigy qa:docs`
- [ ] `effigy qa:docs:agent-defaults`
- [ ] `effigy qa:docs:vision`
- [ ] `effigy docs check-workflow-paths`
- [ ] `effigy docs check-index --policy-index vision`
- [ ] `effigy docs check-next-action --policy vision`
- [ ] New guide linked from docs entry points
- [ ] Command and JSON examples verified against current behavior
- [ ] Completion-candidates JSON examples include hit + miss telemetry variants
- [ ] New log files indexed in `docs/logs/README.md`
- [ ] New log files dated on/after `2026-03-06` include `Vision Target Delta`
```

Allowlist-change PRs should use:
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)

## 8) Fast Operator Commands

```sh
# docs links only
effigy qa:docs

# agent/default guidance drift
effigy qa:docs:agent-defaults

# vision metadata coverage
effigy qa:docs:vision

# workflow path references in docs
effigy docs check-workflow-paths

# vision closeout index consistency
effigy docs check-index --policy-index vision

# vision next-task section coverage
effigy docs check-next-action --policy vision

# index a newly added log artifact
effigy docs add-log-index docs/logs/YYYY-MM/DD-HHMMSS-topic.md

# json contracts only
effigy qa:json:ci

# all gates
effigy qa
```

## Expected Outcome

After this guide, you should be able to:

- run the right docs validation bundle for a change without guessing
- tell which checks are generic built-ins versus repo-specific policy checks
- catch navigation, workflow-path, and contract-example drift before merge

## Related Guides

- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- [`045-vision-next-task-allowlist-maintenance.md`](./045-vision-next-task-allowlist-maintenance.md)
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)

## Next Step

After the docs QA bundle passes, update the relevant landing pages or workflow
guides in the same change so the documentation remains discoverable instead of
only technically valid.
