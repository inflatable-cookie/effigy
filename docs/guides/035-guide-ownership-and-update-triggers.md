# 035 - Guide Ownership and Update Triggers

This guide defines when documentation updates are required after code, contract, or workflow changes.

## 1) Purpose

Use this map to decide which guides must be updated when behavior changes.

Rules:
- if command flags or command shape changes, update command-facing guides
- if JSON schema fields or versions change, update schema/payload guides
- if CI scripts/workflows change, update CI/QA guides
- if navigation paths change, update entrypoint indexes

## 2) Trigger Matrix

| Change Type | Required Guide Updates |
| --- | --- |
| New/changed command flag or usage text | `021`, `025`, and relevant command deep-dive (`013`, `018`, `019`, etc.) |
| New/changed JSON payload field | `017`, `026`, and any recipes parsing that payload (`024`) |
| Envelope/schema version change | `017`, `025`, `026`, plus CI contract docs (`024`) |
| Manifest key added/removed | `022`, `027`, `033`, and routing/behavior guides if applicable |
| Routing behavior change | `016`, `021`, `023`, `025` |
| Deferral behavior/policy change | `015`, `023`, `028` |
| Locking/unlock behavior change | `020`, `023`, `025`, `034` |
| Watch/init/migrate behavior change | `019`, `021`, `025`, `027` |
| CI script or workflow changes | `024`, `029`, and this guide (`035`) |
| Docs navigation/index changes | `README.md`, `docs/README.md`, `docs/guides/README.md`, `031`, `032` |

## 3) Guide Ownership Buckets

Operational runtime guides:
- `013`, `015`, `016`, `018`, `019`, `020`, `021`, `022`, `023`, `025`, `027`, `028`

Contracts and machine-consumer guides:
- `017`, `024`, `026`

Docs-process and navigation guides:
- `029`, `031`, `032`, `033`, `034`, `035`

When a change crosses buckets, update at least one guide per affected bucket.

## 4) PR-Level Update Checklist

For behavior changes, add this to PR notes:

```md
## Docs Impact
- [ ] Command surface changed? Updated 021/025 and deep-dive guide(s)
- [ ] JSON payload changed? Updated 017/026 and CI parsing docs (024)
- [ ] Manifest/config changed? Updated 022/027 (and 033 if terminology/style changed)
- [ ] CI/workflow changed? Updated 024/029/035
- [ ] Navigation changed? Updated README.md, docs/README.md, docs/guides/README.md
```

## 5) High-Signal Triggers by File Path

If these files change, docs updates are usually required:

- `src/lib.rs`
  - help text / command shape changes -> `021`, `025`
- `src/runner/builtin/*.rs`
  - built-in command flags/behavior -> `019`, `021`, `025`, `023`
- `src/runner/manifest.rs`
  - manifest contract changes -> `022`, `027`, `033`, `034`
- `src/tests/json_contract_tests.rs`
  - payload contract assertions changed -> `017`, `026`, `024`
- `tests/cli_output_tests.rs`
  - envelope/wrapping expectations changed -> `017`, `025`, `026`
- `.github-bak/workflows/*.yml`, `scripts/check-*.sh`
  - CI/quality-gate behavior -> `024`, `029`, `035`

## 6) Minimum Acceptance Policy

For docs completeness on non-trivial behavior changes, require:
1. at least one updated command guide (`021` or `025`)
2. at least one updated deep-dive/behavior guide
3. `029` checklist still valid for verification steps
4. link-check pass:

```sh
effigy docs check-links --repo . README.md $(find docs -name '*.md' | sort)
```

## Expected Outcome

- behavior, contract, and CI changes consistently trigger matching docs updates
- PRs include explicit docs impact checks instead of ad-hoc judgment
- index and runbook drift is reduced across releases

## Related Guides

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)

## Next Step

When a behavior PR is opened, copy the checklist in Section 4 into the PR description and complete it before review.
