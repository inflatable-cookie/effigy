# 029 - Docs QA Checklist and Validation

Use this checklist before merging documentation changes.

## 1) Local Docs QA Checklist

Run in order:

```sh
effigy qa:docs --repo .
# dev-checkout fallback:
# effigy-dev qa:docs --repo .
# compatibility fallback:
# cargo qa-docs
```

Manual checks:
- newly added guides are linked from at least one landing page
- numbering/title conventions are consistent with nearby guides
- command examples match current CLI flags and behavior
- JSON examples use current schema names and versions
- completion-candidates examples include both hit and miss telemetry variants
- new log artifacts are indexed in `docs/logs/README.md`
- new log artifacts include a `Vision Target Delta` section
- roadmap/guides vision metadata checks pass via `docs/scripts/check-vision-metadata.sh`
- docs-referenced workflow paths resolve via `effigy docs check-workflow-paths --repo .`
- vision artifact index is consistent via `docs/scripts/check-vision-index.sh`
- vision artifacts have non-empty, actionable follow-on actions via `docs/scripts/check-vision-next-task.sh`
- vision next-task lint fixtures pass via `docs/scripts/check-vision-next-task-regression.sh`

Optional broader check:

```sh
effigy qa --repo .
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
        run: cargo run --bin effigy -- qa:docs --repo .
```

This ensures markdown links resolve, key JSON examples stay contract-aligned, logs remain indexed, and vision metadata coverage is enforced on every pull request/push.
`effigy qa:docs --repo .` is now the primary orchestration surface for that bundle.
`./docs/scripts/check-vision-metadata.sh` now delegates workflow-path validation to `effigy docs check-workflow-paths --repo .`, then runs `check-vision-index`, `check-vision-next-task`, and `check-vision-next-task-regression`.

## 3) What the Link Checker Validates

Built-in command:
- `effigy docs check-links --repo .`

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
- `effigy docs check-json-examples --repo .`

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
- `effigy docs check-index --repo .`

Behavior:
- scans `docs/logs/YYYY-MM/*.md` and excludes `docs/logs/README.md`
- parses log links from `docs/logs/README.md`
- fails when any log file is missing from the index
- fails when index entries point to non-existent log files

Helper:
- `effigy docs add-log-index --repo . <log-file>` inserts a missing log entry ahead of archived links.

Forward-only policy cutoff:
- logs dated on or after `2026-03-06` must include a `## Vision Target Delta` section
- logs before `2026-03-06` do not require backfill

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
- [ ] `effigy qa:docs --repo .`
- [ ] `./docs/scripts/check-vision-metadata.sh`
- [ ] `effigy docs check-workflow-paths --repo .`
- [ ] `./docs/scripts/check-vision-index.sh`
- [ ] `./docs/scripts/check-vision-next-task.sh`
- [ ] `./docs/scripts/check-vision-next-task-regression.sh`
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
effigy qa:docs --repo .

# vision metadata coverage
./docs/scripts/check-vision-metadata.sh

# workflow path references in docs
effigy docs check-workflow-paths --repo .

# vision closeout index consistency
./docs/scripts/check-vision-index.sh

# vision next-task section coverage
./docs/scripts/check-vision-next-task.sh

# vision next-task regression fixtures
./docs/scripts/check-vision-next-task-regression.sh

# index a newly added log artifact
effigy docs add-log-index --repo . docs/logs/YYYY-MM/DD-HHMMSS-topic.md

# json contracts only
effigy qa:json:ci --repo .

# all gates
effigy qa --repo .
```

## Related Guides

- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- [`045-vision-next-task-allowlist-maintenance.md`](./045-vision-next-task-allowlist-maintenance.md)
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)
