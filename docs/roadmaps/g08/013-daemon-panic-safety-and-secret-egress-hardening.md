# g08.013 - Daemon Panic-Safety And Secret Egress Hardening

Status: Complete
Depends on: `g08.012`
Completed: 2026-06-10

## Goal

Reduce two latent failure modes found in the assessment:

- **Finding 4 — panic-safety.** 693 `unwrap()`/`expect()` calls live in non-test
  code, including always-on gateway request paths (e.g.
  `no_route_response(...).body(...).unwrap()`
  [proxy.rs:854](../../../crates/effigy-gateway/src/proxy.rs)). A panic on
  malformed input in a long-running daemon is an accidental-DoS surface.
- **Finding 5 — secret egress.** `SecretValue` redacts `Debug`/`Display` but its
  `Serialize` impl emits plaintext
  ([lib.rs:261](../../../crates/effigy-secrets/src/lib.rs)). Any future struct
  that derives `Serialize` and contains a `SecretValue` leaks the secret to
  whatever consumes that output. The asymmetry is the foot-gun.

These are paired because both are "prevent a silent, severe failure on a trusted
path" — but they are tracked as distinct batches with distinct acceptance.

## Scope

### Panic-safety
- classify the 693 non-test `unwrap`/`expect` sites by blast radius:
  request/response daemon paths, lifecycle/startup, and provably-safe local
  invariants
- convert daemon-path and request-path panics to handled errors (return a 5xx /
  log-and-continue rather than aborting the process)
- leave provably-safe invariants in place, optionally with `expect("reason")`
  documenting the invariant so the audit is not re-run from zero next time

### Secret egress
- close the serialize asymmetry: either make `SecretValue` serialization
  opt-in/explicit (a dedicated wrapper or method used only by the vault
  round-trip) or guard it so a generic `serde_json::to_string` of an enclosing
  struct cannot emit plaintext
- preserve the vault encrypt/decrypt round-trip, which legitimately needs the
  plaintext bytes
- add a regression test proving an enclosing struct serialized via the generic
  path does not contain the plaintext

## Guardrails

- panic-safety conversions must not change success-path behavior or output shape
- do not blanket-replace `unwrap` with silent error-swallowing; daemon paths log
  and degrade, they do not hide failures
- the secret serialize change is behavior-affecting for the vault file format —
  if the on-disk vault representation would change, update
  `032-secret-and-local-config-management-contract.md` in the same batch and
  keep the existing vault schema id/version unless a migration is explicitly
  planned
- no new dependencies for the panic audit

## Execution Plan

- [x] **Batch A — Panic audit (no behavior change).** Accurate workspace count
  (inline `#[cfg(test)]` modules stripped): **1214** non-test `unwrap`/`expect`
  across 77 files — the original "693" was a looser pre-count. The reachable
  daemon-panic surface is far smaller and dominated by one pattern (see Audit
  Results). The response-builder `.unwrap()`s are mostly provably-safe (hyper
  pre-validates status/headers/body inputs).
- [x] **Batch B — Daemon-path conversion.** Converted every lock-poison
  `.expect(...)` on the gateway hot path and the process supervisor to
  poison-tolerant access via two new helper modules
  ([`effigy-gateway/src/locks.rs`](../../../crates/effigy-gateway/src/locks.rs),
  [`effigy-process/src/locks.rs`](../../../crates/effigy-process/src/locks.rs)).
  A single thread panicking under a lock no longer cascades a panic into every
  later request/reap; the daemon recovers the inner guard and keeps serving.
  Added a poison-recovery regression test. ~29 sites across gateway (proxy, dns,
  routes, tcp_alias, server, tls) and process (supervisor control/lookup/
  shutdown, lifecycle, diagnostics, lib).
- [x] **Batch C — Residual conversion + invariant documentation.** Annotated all
  eight `proxy.rs` response/request builder `.unwrap()`s with documented
  `.expect(...)` invariants. Review confirmed every one is provably safe: the
  static builders use literal status/headers/body; the redirect builds from a
  validated route domain + parsed path; the upstream request from the
  already-parsed inbound method/URI; the upgrade response from the upstream's
  already-parsed headers. None take a reachable failing input, so no conversion
  to handled errors was needed — the documented invariant keeps the audit from
  restarting at zero.
- [x] **Batch D — Secret egress fix.** `SecretValue`'s `Serialize` now emits
  `[REDACTED]` instead of plaintext. The only legitimate exposure — the vault
  payload — opts in explicitly via a `secret_plaintext_serde` field serializer
  on `VaultSecretRecord::value`. Vault encrypt/decrypt round-trip preserved; no
  vault representation change, so contract 032 is unchanged. Added three
  regression tests (bare serialize redacts, enclosing-struct serialize does not
  leak, vault record still exposes for encryption).

## Audit Results

Reachable daemon-panic surface, by pattern:

- **Lock-poison cascade (converted in Batch B).** `RwLock`/`Mutex`
  `.read()/.write()/.lock().expect("...poisoned")` on always-on paths: gateway
  route table + TLS cert resolver (~15), process supervisor child/process-map
  mutexes (~14). One holder panic poisons the lock and every subsequent
  `.expect` re-panics — the highest-value daemon-robustness fix. **Done.**
- **Static response builders (Batch C — annotate).** `Response::builder()
  ...body(...).unwrap()` with static status/headers/body in `proxy.rs`
  (345/356/387/739/832/854). Provably safe — hyper only errors on invalid
  header name/value, and these are literals. Retain with a documented invariant.
- **Request-derived builders (Batch C — review).** `proxy.rs:545` (redirect
  `location` from Host) and `proxy.rs:663` (upstream request `uri` cloned from
  the parsed inbound request). Inputs are pre-validated by hyper, so failure is
  not currently reachable; convert to handled 5xx only if review finds a path.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`032-secret-and-local-config-management-contract.md`](../../contracts/032-secret-and-local-config-management-contract.md)
  (Batch D)

## Acceptance Criteria

- [x] no lock-poison `.expect` remains on gateway request paths or the process
  supervisor; a poisoned lock recovers instead of cascading a panic
- [x] retained safe response-builder sites carry a documented invariant
- [x] a generic serialize of a struct containing a `SecretValue` does not emit
  plaintext (proven by `enclosing_struct_serialize_does_not_leak_secret`)
- [x] vault encrypt/decrypt round-trip still passes
- [x] contract 032 needs no change (no vault-representation change)
- [x] changelog `[Unreleased] > Fixed` records both hardening changes

## Evidence

- panic-safety: new modules `crates/effigy-gateway/src/locks.rs`,
  `crates/effigy-process/src/locks.rs` (poison-tolerant `read_tolerant` /
  `write_tolerant` / `lock_tolerant`); converted sites in gateway (proxy, dns,
  routes, tcp_alias, server, tls) and process (supervisor_control/lookup/
  shutdown, lifecycle/shutdown+monitor, diagnostics, lib); regression
  `locks::tests::read_tolerant_recovers_from_poison`
- builder invariants: eight `proxy.rs` `.unwrap()` → documented `.expect(...)`;
  zero bare builder `.unwrap()` remain
- secret egress: `crates/effigy-secrets/src/lib.rs` redacting `Serialize` +
  `secret_plaintext_serde` field serializer on `VaultSecretRecord::value`;
  regression tests `secret_value_serialize_redacts_plaintext`,
  `enclosing_struct_serialize_does_not_leak_secret`,
  `vault_secret_record_serializes_plaintext_for_encryption`
- validation: `cargo test -p effigy-gateway -p effigy-secrets -p effigy-process`
  green (119 + 15 + 3 + 8 + 13 tests); runner secret round-trip tests green (54);
  `cargo fmt --all -- --check` clean; clippy clean

## Next Task

Open `g08.014` (Gateway Route-Table Trust Model). It is **blocked** until its
Batch A authors and promotes the trust contract
`033-gateway-route-table-trust-contract.md`; that contract-authoring batch is
the ready entry point.
