# 024 - CI and Automation Recipes

This guide provides copy-paste CI patterns for Effigy JSON contract and command-envelope automation.

## 1) What To Automate

Primary contract checks in this repo:
- `./scripts/check-json-contracts-ci.sh`
- `./scripts/check-json-contracts.sh`
- `./scripts/validate-json-contract-selection-artifact.sh`
- `./scripts/check-selection-artifact-validator-smoke.sh`
- `./scripts/check-release-gates.sh`
- `./scripts/check-release-smoke.sh`
- `./scripts/check-release-install-from-tag.sh`
- `./scripts/check-distribution-metadata.sh`
- `./scripts/check-distribution-first-publish.sh`
- `./scripts/validate-distribution-artifacts.sh`
- `./scripts/generate-distribution-closeout-report.sh`
- `./scripts/update-homebrew-formula-from-metadata.sh`

Primary JSON mode entrypoint:
- `effigy --json <command>`

## 2) Local Reproduction Commands

Before debugging CI, run locally:

```sh
cargo qa
cargo qa-release
./scripts/check-release-install-from-tag.sh --tag v0.__.__
./scripts/check-distribution-first-publish.sh --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
# writes ./artifacts/distribution-v0.__.__/distribution-summary.env
./scripts/validate-distribution-artifacts.sh --artifacts-dir ./artifacts/distribution-v0.__.__
./scripts/generate-distribution-closeout-report.sh --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
./scripts/check-json-contracts-ci.sh
./scripts/check-json-contracts.sh --fast --print-selected=json
./scripts/check-json-contracts.sh --full --print-selected=text
cargo qa-docs
```

Install pinning and team migration policy:
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)

PR-style changed-only simulation:

```sh
./scripts/check-json-contracts.sh --fast --changed-only origin/main --print-selected=json
```

Validate artifact payload shape:

```sh
./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json
./scripts/check-selection-artifact-validator-smoke.sh
```

## 3) Recipe: PR-Optimized Contracts Job

Use changed-only checks for pull requests and full checks for main/scheduled runs.

```yaml
name: JSON Contracts

on:
  pull_request:
  push:
    branches: [main]
  schedule:
    - cron: "0 2 * * *"
  workflow_dispatch:

jobs:
  json-contracts:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Validate JSON contracts
        run: |
          set -o pipefail
          ./scripts/check-json-contracts-ci.sh | tee json-contracts.log
          grep -m1 '^{"selected":' json-contracts.log > json-contracts-selected.json

      - name: Validate selection artifact contract
        run: ./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json

      - name: Validator smoke check
        run: ./scripts/check-selection-artifact-validator-smoke.sh

      - name: Upload contracts artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: json-contracts-${{ github.run_id }}
          path: |
            json-contracts.log
            json-contracts-selected.json
```

Notes:
- `check-json-contracts-ci.sh` auto-switches behavior by event (`pull_request` vs non-PR).
- `set -o pipefail` ensures failures inside pipe chains fail the step.

## 4) Recipe: Nightly Full Contract Sweep

When you want explicit nightly full coverage:

```yaml
- name: Nightly full JSON contract sweep
  run: ./scripts/check-json-contracts.sh --full --print-selected=json | tee json-contracts-nightly.log
```

Optional artifact upload:

```yaml
- name: Upload nightly contract log
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: json-contracts-nightly-${{ github.run_id }}
    path: json-contracts-nightly.log
```

## 5) Recipe: Capture Effigy JSON for Triage

Store JSON output for failed command triage:

```yaml
- name: Capture doctor JSON
  if: failure()
  run: effigy --json doctor --verbose > doctor-failure.json || true

- name: Upload triage payloads
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: effigy-triage-${{ github.run_id }}
    path: |
      doctor-failure.json
      json-contracts.log
      json-contracts-selected.json
```

## 6) Recipe: Contract Selection Artifact Gate

If a workflow produces a `selected` payload, gate it with the validator:

```yaml
- name: Validate selection artifact contract
  run: ./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json
```

This checks:
- required keys exist,
- `count == length(selected)`,
- `selected` is string-only,
- `mode` is allowed (`fast` or `full`).

## 7) Failure Triage Playbook

### Case: CI fails in `check-json-contracts-ci.sh`

Run locally:

```sh
./scripts/check-json-contracts-ci.sh
```

Then inspect selection payload in log:

```sh
grep -m1 '^{"selected":' json-contracts.log | jq .
```

### Case: invalid selection artifact contract

Run validator directly:

```sh
./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json
```

Then run smoke validator:

```sh
./scripts/check-selection-artifact-validator-smoke.sh
```

### Case: command payload schema mismatch

Run fast checker with selected output:

```sh
./scripts/check-json-contracts.sh --fast --print-selected=text
```

Then run full mode to catch heavy-schema paths:

```sh
./scripts/check-json-contracts.sh --full --print-selected=text
```

## 8) Artifact Conventions

Recommended standard artifacts per run:
- `json-contracts.log`
- `json-contracts-selected.json`
- optional command snapshots (`doctor-failure.json`, `tasks-resolve.json`, etc.)

Naming pattern:
- `json-contracts-${{ github.run_id }}` for core contract artifacts
- `effigy-triage-${{ github.run_id }}` for failure diagnostics

## 9) Automation Safety Rules

- Prefer `effigy --json <command>` for machine consumers.
- Avoid parsing human-rendered text in CI when JSON payload exists.
- Always preserve raw logs as artifacts for post-failure analysis.
- Use changed-only checks for PR speed; keep a full sweep on `main`/nightly.

## 10) Pre-Push CI Stability Checklist

Canonical checklist and troubleshooting live in [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md).

Use this fast path before pushing changes that touch command behavior, JSON schemas, or docs contracts:

```sh
cargo qa
```

## 11) Recipe: Tag-Driven Release Gates

Run consolidated release checks on tags:

```yaml
name: Release Gates

on:
  push:
    tags: [v*]
  workflow_dispatch:

jobs:
  release-gates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: ./scripts/check-release-gates.sh
```

What `check-release-gates.sh` enforces:
- `cargo fmt --check`
- full `cargo test`
- docs + JSON quality gates (`check-quality-gates.sh --all --ci`)
- release binary build
- release smoke checks (`help`, `tasks`, `farmyard/tasks`, `test --plan`, `farmyard/test --plan`)
- distribution metadata validation (`check-distribution-metadata.sh`)
- install validation from the pushed tag (`check-release-install-from-tag.sh`)

## 12) Recipe: Assert Completion Cache Policy Fields

Use this when CI needs deterministic completion-cache policy behavior.

```yaml
- name: Assert completion cache policy telemetry
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "750"
  run: |
    set -euo pipefail
    effigy --json completion candidates --prefix farm > completion-candidates.json
    jq -e '.schema == "effigy.command.v1"' completion-candidates.json >/dev/null
    jq -e '.result.schema == "effigy.completion.candidates.v1"' completion-candidates.json >/dev/null
    jq -e '.result.effective_cache_ttl_ms == 750' completion-candidates.json >/dev/null
    jq -e '.result.cache_ttl_source == "env"' completion-candidates.json >/dev/null
```

Invalid env fallback check:

```yaml
- name: Assert invalid completion ttl fallback
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "not-a-number"
  run: |
    set -euo pipefail
    effigy --json completion candidates --prefix farm > completion-candidates-invalid.json
    jq -e '.result.effective_cache_ttl_ms == 2000' completion-candidates-invalid.json >/dev/null
    jq -e '.result.cache_ttl_source == "env_invalid"' completion-candidates-invalid.json >/dev/null
```

Miss-path nullability check:

```yaml
- name: Assert miss telemetry keeps hit-only ttl field null
  run: |
    set -euo pipefail
    effigy --json completion candidates --prefix farm > completion-candidates-miss.json
    jq -e '.result.cache_state != "hit"' completion-candidates-miss.json >/dev/null
    jq -e '.result.cache_ttl_ms == null' completion-candidates-miss.json >/dev/null
```

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
