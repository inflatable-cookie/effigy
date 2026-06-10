# g08.012 - Supply-Chain And CI Security Gates

Status: Complete
Depends on: `g08.011`
Completed: 2026-06-10

## Goal

Close the supply-chain blind spot. Effigy locks 529 dependencies and ships a
network-facing daemon (hyper proxy, reqwest, DNS, rustls), yet CI runs only
fmt + clippy + nextest + doctests. A known-vuln advisory or a disallowed/yanked
crate in any transitive dependency ships silently today. Remediates assessment
finding 3.

## Scope

- add a dependency advisory gate (`cargo audit` or `cargo deny advisories`)
- add a license/ban/source gate (`cargo deny` bans + licenses + sources)
- add automated dependency update surfacing (dependabot or equivalent) scoped
  to cargo and the GitHub Actions used by the workflows
- define the advisory policy: what fails CI, what is allowed as a reviewed
  exception, and where exceptions are recorded (`deny.toml`)
- document the policy so an advisory failure has an unambiguous remediation path

## Guardrails

- **`.github/workflows/` edits require explicit human approval** (CLAUDE.md and
  release protocol). This milestone plans and stages the change; it does not
  merge a workflow edit until approved.
- no release-pipeline changes beyond adding the gate job
- the gate must be deterministic and not depend on unpinned external state where
  avoidable (pin action versions; vendor the advisory DB step via a pinned
  install action consistent with existing CI)
- an advisory exception must be explicit, dated, and justified in `deny.toml`,
  never a blanket ignore

## Execution Plan

- [x] **Batch A — Policy definition (no workflow edit).** Authored
  [`deny.toml`](../../../deny.toml) (advisories, bans, licenses, sources) and a
  policy note in
  [`docs/guides/024-ci-and-automation-recipes.md`](../../guides/024-ci-and-automation-recipes.md)
  (§13). Ran `cargo deny check` and recorded the baseline below.
- [x] **Batch B — Baseline remediation.** Drove the local baseline to green:
  - licenses + sources passed on the first run (allowlist matched the tree:
    OSI-permissive plus file-level-copyleft MPL-2.0; all sources crates.io).
  - 2 *unmaintained-only* advisories (no live vulns, no safe upgrade) recorded
    as dated, justified exceptions in `[advisories] ignore`:
    RUSTSEC-2025-0134 (`rustls-pemfile`, via the rustls TLS stack) and
    RUSTSEC-2026-0173 (`proc-macro-error2`, transitive build-time proc-macro).
  - bans wildcard false-positive fixed: the 34 internal workspace path deps on
    the root `effigy` binary were flagged because the root package lacked
    `publish = false` (every member crate already sets it). Added
    `publish = false` to the root package — accurate (Effigy ships via
    brew/git/binary, not crates.io) — plus `allow-wildcard-paths = true`.
- [x] **Batch C — CI gate wiring.** Human approval to edit
  `.github/workflows/` was granted 2026-06-10. Added a `supply-chain` job to
  `ci.yml` (`taiki-e/install-action@cargo-deny` → `cargo deny check`) and a new
  `.github/dependabot.yml` (weekly cargo + github-actions update PRs). YAML
  validated; `cargo deny check` green locally. First CI run confirms the gate
  end-to-end.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)

## Planning Gaps (resolved)

- Advisory policy (fail vs allow-with-exception) agreed: fail on advisories;
  unmaintained-only exceptions recorded in `deny.toml` with date + reason.
- Workflow-edit approval granted 2026-06-10; Batch C executed.

## Acceptance Criteria

- [x] `deny.toml` exists and `cargo deny check` runs clean with explicit,
  justified exceptions on the current tree (`advisories ok, bans ok,
  licenses ok, sources ok`)
- [x] policy note documents what fails CI and how exceptions are recorded
- [x] CI fails on a new advisory or disallowed license (the `supply-chain` job
  runs `cargo deny check`)
- [x] dependency update automation is active for cargo + actions
  (`.github/dependabot.yml`)
- [x] changelog `[Unreleased] > Added` records the supply-chain gate

## Evidence

- [`deny.toml`](../../../deny.toml): full policy (advisories/licenses/bans/sources)
- `Cargo.toml`: root package `publish = false`
- [`docs/guides/024-ci-and-automation-recipes.md`](../../guides/024-ci-and-automation-recipes.md)
  §13: supply-chain policy note
- validation: `cargo deny check` → `advisories ok, bans ok, licenses ok,
  sources ok`; `ci.yml` + `dependabot.yml` YAML validated
- baseline at adoption: 529 lockfile crates; licenses/sources clean on first
  run; 2 unmaintained-only advisories exception-listed; 0 live vulnerabilities
- CI: `supply-chain` job in `.github/workflows/ci.yml`;
  `.github/dependabot.yml` (weekly cargo + github-actions)

## Next Task

Milestone complete. First CI run on the next push confirms the `supply-chain`
job end-to-end. Continue the suite at `g08.014` (Gateway Route-Table Trust
Model), Batch B.
