# g08.010 - Security And Posture Hardening Suite

Status: Complete
Depends on: `g08.009`
Opened: 2026-06-10
Completed: 2026-06-10

## Closeout

All six assessment findings remediated across five milestones:

1. Discovery fixture leak → g08.011 (fixed; doctor green on clean tree).
2. Gateway route-table trust → g08.014 (contract 033 + read-path integrity gate
   + operator visibility).
3. CI supply-chain gap → g08.012 (deny.toml + CI gate + Dependabot).
4. Daemon panic-safety → g08.013 (lock-poison cascade removed).
5. SecretValue serialize egress → g08.013 (redacted by default).
6. Docs spine sprawl → g08.015 (logs archived + index compacted + convention).

No deferred findings. Residual by deliberate decision: closed-generation
batch-cards remain nested (churn guardrail). With g08 now closed through this
suite, a `g09` rollover can be considered per the rollover guardrail.

## Goal

Turn the 2026-06-10 architecture and security assessment into a bounded
remediation tranche. The assessment confirmed a clean core (modular crates, no
god-files, sound vault crypto, 1405 tests) but found edge-level correctness,
security-posture, and maintainability gaps. This suite sequences the full
remediation of those six findings without changing intended public behavior
except where a finding requires it.

This is a posture tranche inside `g08`, not a new theme generation. It does not
extend the graph-aware scan work; it closes the assessment.

## Findings Under Remediation

1. **Discovery fixture leak** — default catalog discovery has no `tests`/fixture
   exclusion, so `tests/fixtures/**/effigy.toml` surface as live catalogs and
   `effigy doctor` reports a hard error on a clean tree.
   ([discovery.rs:217](../../../crates/effigy-routing/src/discovery.rs))
2. **Gateway route-table trust** — `routes.json` is a plain user-writable file
   consumed by an elevated daemon that proxies to whatever target each route
   names; no provenance or integrity model.
3. **CI supply-chain gap** — 529 locked dependencies, network-facing daemon, no
   `cargo audit` / `cargo deny` / dependabot gate in CI.
4. **Daemon panic-safety** — 693 `unwrap()`/`expect()` in non-test code,
   including always-on gateway request paths; a panic on malformed input is an
   accidental-DoS surface.
5. **`SecretValue` serialize egress** — `Serialize` emits plaintext while
   `Debug`/`Display` redact; asymmetry is a latent accidental-leak foot-gun.
   ([lib.rs:261](../../../crates/effigy-secrets/src/lib.rs))
6. **Docs spine sprawl** — 2141 markdown files / 11 MB; `docs/roadmaps/` ~1122
   and `docs/logs/` ~677 files threaten signal-to-noise and staleness.

## Execution Plan

Each finding cluster is owned by a themed milestone. Sequence runs correctness
→ security gates → robustness → trust model → posture.

- [x] **g08.011 — Discovery and Doctor Correctness** (finding 1): excluded
  fixture/test manifests from ambient discovery via `[catalog.discovery] ignore`
  and closed the adjacent doctor schema-validator gap for `[catalog.discovery]`.
  `effigy doctor` now green on this repo (`err:0`). Complete 2026-06-10.
- [x] **g08.012 — Supply-Chain and CI Security Gates** (finding 3): complete.
  `deny.toml` policy (advisories/bans/licenses/sources), 2 unmaintained-only
  advisories exception-listed, root binary `publish = false`, a `supply-chain`
  `cargo deny check` job in CI (approved 2026-06-10), and Dependabot weekly
  cargo + actions updates. Complete 2026-06-10.
- [x] **g08.013 — Daemon Panic-Safety and Secret Egress Hardening**
  (findings 4 + 5): complete. Panic audit done; every lock-poison `.expect` on
  the gateway hot path + process supervisor converted to poison-tolerant access;
  all eight proxy builder `.unwrap()`s annotated with documented invariants; and
  `SecretValue` now serializes as `[REDACTED]` with the vault as the only
  explicit-exposure path. Complete 2026-06-10.
- [x] **g08.014 — Gateway Route-Table Trust Model** (finding 2): complete.
  Contract [`033`](../../contracts/033-gateway-route-table-trust-contract.md)
  promoted; read-path integrity gate implemented (owner-only `0o600` + managed
  marker on save; daemon fails closed on a group/other-writable or
  unmarked/foreign table, keeping last-known-good); trust state surfaced in
  `effigy gateway status` and `effigy doctor`. Complete 2026-06-10.
- [x] **g08.015 — Docs Spine Compaction** (finding 6): complete. Archived 656
  closed-generation logs to `docs/logs/archive/`, compacted the logs index
  (677 → 21 entries; 839 → ~138 lines), taught the default docs index to exclude
  `archive/**`, and documented the retention convention. Closed-gen batch-cards
  left in place by the churn guardrail. Complete 2026-06-10.

## Guardrails

- no CLI grammar or JSON schema id/version changes except where a finding
  explicitly requires it, and only with a recorded contract update
- no `.github/workflows/` edits without explicit human approval (g08.012's CI
  job was added only after approval was granted 2026-06-10)
- no release execution
- no feature removal without human confirmation
- behavior-changing fixes (finding 5 serialize, finding 2 trust model) must land
  a contract update in the same batch as the code change
- docs compaction must preserve the generation model, front-door truth, and all
  live planning surfaces; archive, do not silently delete, planning history

## Readiness Summary

- **Complete:** g08.011 (discovery/doctor correctness), g08.012 (supply-chain +
  CI gate, workflow approval granted), g08.013 (daemon panic-safety + secret
  egress).
- **Complete:** g08.014 — contract 033 promoted; read-path integrity gate
  implemented and tested; trust surfaced in gateway status + doctor.
- **Ready (final milestone):** g08.015 (docs spine compaction) — the security
  milestones are all done, so it can now run without churning planning docs
  mid-remediation.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`032-secret-and-local-config-management-contract.md`](../../contracts/032-secret-and-local-config-management-contract.md)
  (finding 5)
- [`033-gateway-route-table-trust-contract.md`](../../contracts/033-gateway-route-table-trust-contract.md)
  (finding 2)

## Planning Gaps (resolved)

- **Gateway route-table trust** (g08.014): contract
  [`033`](../../contracts/033-gateway-route-table-trust-contract.md) authored and
  promoted in g08.014 Batch A — gap closed; Batches B+C ready.
- **Supply-chain policy** (g08.012): advisory/ban policy agreed and
  workflow-edit approval granted 2026-06-10 — gap closed; milestone complete.

## Acceptance Criteria

- [ ] every finding maps to exactly one themed milestone with explicit scope
- [ ] each milestone names its governing contract or the planning gap blocking it
- [ ] no milestone is marked ready while a governing contract is still missing
- [ ] closeout records which findings were fully remediated, which carry
  deliberate residual scope, and which were deferred to backlog

## Next Task

Suite complete — all five milestones (g08.011–g08.015) closed. Next planning
move is a `g08` generation closeout / `g09` rollover assessment.
