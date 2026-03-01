# 024 - CI and Automation Recipes

This guide provides copy-paste CI patterns for Effigy JSON contract and command-envelope automation.

## 1) What To Automate

Primary contract checks in this repo:
- `./scripts/check-json-contracts-ci.sh`
- `./scripts/check-json-contracts.sh`
- `./scripts/validate-json-contract-selection-artifact.sh`
- `./scripts/check-selection-artifact-validator-smoke.sh`

Primary machine mode entrypoint:
- `effigy --json <command>`

## 2) Local Reproduction Commands

Before debugging CI, run locally:

```sh
./scripts/check-json-contracts-ci.sh
./scripts/check-json-contracts.sh --fast --print-selected=json
./scripts/check-json-contracts.sh --full --print-selected=text
```

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

Store machine output for failed command triage:

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

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
