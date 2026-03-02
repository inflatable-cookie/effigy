# 029 - Docs QA Checklist and Validation

Use this checklist before merging documentation changes.

## 1) Local Docs QA Checklist

Run in order:

```sh
cargo qa-docs
# fallback:
# ./scripts/check-quality-gates.sh --docs-only
```

Manual checks:
- newly added guides are linked from at least one landing page
- numbering/title conventions are consistent with nearby guides
- command examples match current CLI flags and behavior
- JSON examples use current schema names and versions
- completion-candidates examples include both hit and miss telemetry variants
- new report artifacts are indexed in `docs/reports/README.md`

Optional broader check:

```sh
cargo qa
```

## 2) CI Validation Path

Current workflow file:
- `.github/workflows/json-contracts.yml`

Docs QA gate job:

```yaml
jobs:
  docs-qa:
    name: Validate docs docs-only gates
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Validate docs links, JSON examples, and report index
        run: ./scripts/check-quality-gates.sh --docs-only
```

This ensures markdown links resolve, key JSON examples stay contract-aligned, and reports remain indexed on every pull request/push.

## 3) What the Link Checker Validates

Script:
- `scripts/check-doc-links.sh`

Behavior:
- scans markdown inline links (`[label](target)`)
- ignores `http(s)`, `mailto:`, and in-page anchor links
- resolves relative file targets from the source markdown file directory
- fails if target file/path is missing

Default scope when called with no args:
- `README.md`
- `docs/README.md`
- `docs/guides/*.md`

## 4) What the JSON Example Checker Validates

Script:
- `scripts/check-doc-json-examples.sh`

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

## 5) What the Report Index Checker Validates

Script:
- `scripts/check-doc-reports-index.sh`

Behavior:
- scans `docs/reports/*.md` and excludes `docs/reports/README.md`
- parses report links from `docs/reports/README.md`
- fails when any report file is missing from the index
- fails when index entries point to non-existent report files

Helper:
- `scripts/add-report-index-entry.sh <report-file>` inserts a missing report entry ahead of archived links.

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
- [ ] `cargo qa-docs`
- [ ] New guide linked from docs entry points
- [ ] Command and JSON examples verified against current behavior
- [ ] Completion-candidates JSON examples include hit + miss telemetry variants
- [ ] New report files indexed in `docs/reports/README.md`
```

## 8) Fast Operator Commands

```sh
# docs links only
cargo qa-docs

# index a newly added report artifact
./scripts/add-report-index-entry.sh docs/reports/YYYY-MM-DD-topic.md

# json contracts only
cargo qa-json-ci

# all gates
cargo qa
```

## Related Guides

- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
