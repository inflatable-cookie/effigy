# Distribution Artifact Summary and Auto-Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Emit a machine-readable summary from first-publish artifact generation.
- Auto-validate artifact completeness at end of first-publish helper.
- Let closeout report generation auto-detect Homebrew expectation from summary.

## Changes

- Updated script:
  - `scripts/check-distribution-first-publish.sh`
    - records step log names
    - writes `distribution-summary.env` in artifacts directory
    - runs `validate-distribution-artifacts.sh` before success
- Updated script:
  - `scripts/generate-distribution-closeout-report.sh`
    - reads `distribution-summary.env` when present
    - auto-enables Homebrew expectation if summary shows Homebrew executed
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-first-publish.sh ./scripts/generate-distribution-closeout-report.sh`
  - result: pass
- command: `tmp="$(mktemp -d)" && touch "$tmp"/01-tag-install-validation.log "$tmp"/02-crates-io-install-validation-0-1-0.log "$tmp"/03-crates-io-binary-help.log "$tmp"/04-crates-io-binary-json-tasks.log "$tmp"/05-homebrew-install.log "$tmp"/06-homebrew-binary-help.log "$tmp"/07-homebrew-binary-json-tasks.log "$tmp"/08-homebrew-upgrade.log && cat > "$tmp"/distribution-summary.env <<'EOS'\nTAG=v0.1.0\nCRATE_VERSION=0.1.0\nREPO_URL=https://github.com/inflatable-cookie/effigy.git\nBREW_FORMULA=inflatable-cookie/effigy/effigy\nHOMEBREW_EXECUTED=1\nLOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation-0-1-0.log,03-crates-io-binary-help.log,04-crates-io-binary-json-tasks.log,05-homebrew-install.log,06-homebrew-binary-help.log,07-homebrew-binary-json-tasks.log,08-homebrew-upgrade.log\nEOS\n./scripts/generate-distribution-closeout-report.sh --tag v0.1.0 --artifacts-dir "$tmp" --output "$tmp"/report.md && test -s "$tmp"/report.md`
  - result: pass

## Outcomes

- First-publish helper now self-validates evidence completeness before reporting success.
- Closeout report generation has less manual coordination for Homebrew evidence expectations.

## Risks / Follow-ups

- Summary format is line-based env text; future parser changes should preserve key names for compatibility.
- Real publish-cycle execution evidence is still required to close remaining distribution acceptance criteria.

## Next Batch Recommendation

- Execute the first real release tag batch with artifacts enabled, generate closeout report from those artifacts, and reconcile remaining acceptance criteria in `distribution-channels.md`.
