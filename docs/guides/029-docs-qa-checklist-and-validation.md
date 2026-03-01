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

Optional broader check:

```sh
cargo qa
```

## 2) CI Validation Path

Current workflow file:
- `.github/workflows/json-contracts.yml`

Docs link gate job:

```yaml
jobs:
  docs-links:
    name: Validate docs links
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Validate markdown link targets
        run: ./scripts/check-quality-gates.sh --docs-only
```

This ensures markdown links resolve on every pull request/push.

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

## 4) Common Failure Modes

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

## 5) Suggested PR Checklist Section

Copy into PR description:

```md
## Docs QA
- [ ] `cargo qa-docs`
- [ ] New guide linked from docs entry points
- [ ] Command and JSON examples verified against current behavior
```

## 6) Fast Operator Commands

```sh
# docs links only
cargo qa-docs

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
