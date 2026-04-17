# 253 Decide Effigy-Doctor-Runner Extraction Shape

Status: ready
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the doctor-runner extraction before any code moves.
`src/runner/doctor/**` is ~4,547 lines across 65 files — the largest
bounded subsystem left inside the runner after the `effigy-builtin`
extraction. It also sits alongside the existing `effigy-doctor`
library crate, so the naming and boundary need a deliberate
decision rather than a default.

The implement card is `254`.

## Context

`src/runner/doctor/**` contents today (~4.5k LOC, 65 files):

| Subsystem | Path | Files | Role |
|---|---|---:|---|
| Command entry | `doctor.rs`, `doctor/command.rs` | 2 | `run_doctor` entry, error mapping |
| Explain | `doctor/explain/**` | ~10 | `--explain <check>` rendering and analysis |
| Progress | `doctor/progress/**` | ~4 | progress reporter (JSON + text) |
| Render | `doctor/render/**` | ~15 | text + JSON report rendering |
| Report model | `doctor/report/**` | ~6 | finding / group / report structs |
| Run workflow | `doctor/run/**` | ~15 | orchestrator that invokes check registry |
| Checks | `doctor/run/check_registry/**` | ~10 | concrete health checks (scan-backed, manifest, lock, etc.) |

An existing `effigy-doctor` crate already owns the pure-library
surfaces (report types, render contracts, explain data model). This
card decides:

1. **Crate name** — is the new crate `effigy-doctor-runner`? Or does
   the orchestration layer fold into `effigy-doctor` itself?
2. **Error boundary** — `DoctorError` enum mirroring `RunnerError::DoctorNonZero`,
   or reuse `effigy-doctor`'s existing error?
3. **Port surface** — doctor reaches into scan, manifest, locking,
   deferral, and env-schema. Which of these become port traits
   versus direct crate deps?
4. **Split shape** — single crate, or cluster (runner / workflow /
   checks)?
5. **Prerequisite ordering** — any pure-helper relocations needed
   before the crate move (analogous to card `251`'s port inversion
   before card `250`)?

### Residual reach-ins (inventory before decide)

Doctor currently imports from:

- `effigy-manifest` (task manifest loading) — direct dep, no port
- `effigy-scan` (every check that runs a scan) — direct dep, no port
- `effigy-env` (env-schema check) — via `env_schema_support.rs` shim
  (card `252` inlines this)
- `crate::runner::deferral::*` — deferral-policy lookups for
  "which builtins defer?" checks. Possible port boundary.
- `crate::runner::locking::*` — lock-liveness check. Possible port
  boundary.
- `crate::runner::util::*` — task-selector parsing. Should move to
  `effigy-tasks` directly (already done for most other consumers
  under card `251`; verify doctor is clean).
- `crate::runner::error::RunnerError` — every check produces errors
  in `RunnerError::DoctorNonZero` shape. The new crate needs its own
  error with a 1:1 lift to `RunnerError::DoctorNonZero`.

## In Scope

- Decide crate name (`effigy-doctor-runner` vs fold into
  `effigy-doctor` vs new name).
- Decide `DoctorError` shape: which `RunnerError::*` variants need
  preservation beyond `DoctorNonZero` (likely just `DoctorNonZero`,
  `TaskInvocation`, and error-wrappers for Manifest/Scan/Env).
- Decide port surface for runner reach-ins: deferral-policy lookups,
  lock-liveness. Either port traits or direct crate deps on a not-
  yet-extracted `effigy-deferral` / `effigy-locking` (probably out
  of scope for this round).
- Decide split vs single crate.
- Inventory prerequisite relocations: util helpers, shim inlines
  beyond what card `252` already covers, any doctor-specific
  reach-ins that need a Job-8 style `From<DoctorError>` boundary
  before the crate move.
- Plan the implement card's scope (card `254`) as either one commit
  (mechanical) or decide / implement pair (like `244`/`250` or
  `247`/`249`).

## Out Of Scope

- Actual crate creation or code movement. Card `254` implements.
- Extraction of `runner::deferral` or `runner::locking` — these
  stay as port traits (if needed) or direct runner deps. Their
  own extraction is a separate lane decision.
- Changes to the existing `effigy-doctor` library crate's surface
  beyond what's needed to absorb runner-orchestration code.

## Decision Checklist

- [ ] Crate name chosen (`effigy-doctor-runner` or alternative)
- [ ] `DoctorError` variant shape fixed
- [ ] Port trait(s) named (if any) for deferral / locking reach-ins
- [ ] Single crate vs cluster decision recorded
- [ ] Prerequisite relocation card(s) opened (if any)
- [ ] Implement card (`254`) scoped

## Next Task

Card `254` — execute the extraction per this card's decision.
