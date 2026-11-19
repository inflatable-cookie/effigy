# 032 - Runtime Kernel Architecture

Contract: [052](../contracts/052-runtime-kernel-contract.md). That contract owns
every rule, primitive, proof obligation and wire format. This note is only a
map: where the kernel sits, which processes it runs, and where each mechanism's
detail lives. It adds no semantics.

## Placement

One binary, three layers. Isolation comes from crate boundaries and dependency
direction, not separate binaries.

```text
 façade    CLI parsing · help · one output owner (Outcome → text or JSON)
 engines   task engine · run supervisor · environment reconciler · gateway
           delivery (release, state, deploy) · vault · deps · graph · bundles
 adapters  compose backends · Colima · Docker · macOS platform (launchd,
           resolver, lo0) · mkcert
 kernel    effigy-kernel: identity · context · gate · store · run · custody
           signal · remote custody · observe · probe · outcome
```

Dependency rules, enforced by `qa:architecture`:

| From | May depend on | Must not depend on |
| --- | --- | --- |
| `effigy-kernel` | std, `libc`, `rusqlite` (bundled), `serde`, `serde_json`, `sha2`, `unicode-normalization` (NFC identity fields) | any other Effigy crate; Tokio; any adapter |
| adapters | `effigy-kernel` | engines, the façade |
| engines | `effigy-kernel`, adapter traits | the façade, other engines' internals |
| façade | engines | kernel internals beyond `Outcome` and contexts |

The kernel API is synchronous; an engine that uses Tokio calls it from blocking
contexts. Kernel crate module names follow the primitives: `id`, `ctx`, `gate`,
`store`, `run`, `custody`, `signal`, `supervise`, `remote`, `observe`, `probe`,
`outcome`, `ident`, `objects`, plus the test-only `fault` and `strict_fake`
features.

## Processes

```text
 host                                                          guest (container mode only)
 ┌──────────────────────────────────────────────┐              ┌───────────────────────────┐
 │ effigy <task>        inline owner (client)   │──exec──────▶│ workload session (setsid) │
 │   └─ effigy __kernel-sentinel   (per run)    │  in-band ack │                           │
 │ effigy dev           owning client           │◀────────────│                           │
 │   └─ effigy __kernel-supervise  (per run)    │              │                           │
 │        └─ effigy __kernel-sentinel (per run) │              │                           │
 │ workspace session    __kernel-supervise      │◀──channel───│ effigy workspace client   │
 │   └─ channel listener (per session)          │  TLS, pinned │   reads the context       │
 │ effigy status        observer                │              │   no state, gates or runs │
 │   └─ effigy __kernel-probe-guard (backup)    │              └───────────────────────────┘
 │ effigy gateway daemon  (user-owned, resident)│
 │ checkout/.effigy/state/ kernel.db/gates/sock │
 │ ~/.effigy/state/ machine.db · shared gates   │
 └──────────────────────────────────────────────┘
```

- The only resident process is the user gateway. Supervisors and sentinels
  live for one run and its settle; a probe guard for its observer's open
  probes.
- Every per-run process re-execs the binary identified at startup.
- Foreground runs use memory records and checkout lock files. Only retained
  runs open a checkout journal. The machine store holds the shared facts listed
  in contract 052 §7.5, with no checkout catalog or task history.
- Guests never see either writable runtime state root or the vault. Only
  immutable object closures may be mounted, read-only, through a backend.
- In container mode the guest is a workspace client: local reads against a
  materialised context, every authority request over the host channel, and
  workloads in the container under host remote custody (contract §8). Hybrid
  mode has no guest.

## Flows

```text
one-shot task
resolve (Observe) → Run::begin: gate, settle or retry every unresolved predecessor
(refuse on residue unless acknowledged), Admitted
→ sentinel → intend(process) → fork, barrier, record, go → wait, bounded capture
→ Stopping → settle (verify group closure) → Closed → release gate → Outcome

managed run with an environment
client: resolve → spawn supervisor → render events and logs
supervisor: Run::begin → socket intent → bind → receipt → "admitted" → setup
  → attach(env, Borrow or Exclusive): Gate(Env), one transaction
  → wait_all(ExecReady, …) → workloads (local and remote custody) → Active
client EOF without detach → stop request
supervisor: Stopping → settle: remote workloads, local workloads, attachment → Closed

environment transition
Gate(Env), held for the whole run → holders(env) → settle each losing holder
→ permit → claims transaction → per effect: re-check holders and generation,
apply or down → probes → commit applied generation

observation
ObserveContext only → probe (behind the guard) or connect → Outcome
→ filesystem unchanged

recovery
next Mutate on an owner: unresolved predecessor → settle or retry under the new
holder, recording residue for Unknown → admit, or refuse unless acknowledged
```

## Boundaries

- The kernel owns mechanisms. Policy belongs to engines: idle stop, release
  modes, Compose recreation (Q-004), heavy admission (Q-006), gateway
  activation.
- No second execution path. Contract
  [013](../contracts/013-task-execution-request-contract.md)'s request and
  plan stay the only entry to execution; the kernel sits under them.
- QA-group controls from contract
  [051](../contracts/051-bounded-qa-groups-contract.md) reuse the kernel's
  run, custody and deadline. They get no private cancellation path.
- Contracts [005](../contracts/005-container-runtime-contract.md) and
  [049](../contracts/049-heavy-validation-admission-contract.md) describe
  current behavior until their engine lanes reconcile them with 052.

## Where the detail lives

| Mechanism | Owner |
| --- | --- |
| Identity, context, gates, store, journal, settle, custody, signals, deadlines | contract 052 §5 |
| Store layout, trust, retention and machine write budget | contract 052 §7 |
| Host/guest model, guest context, provisioning and the guest's local dispatch | contract 052 §8 |
| Authority map and legacy state | contract 052 §9, §11 |
| Fault harness, matrices, ratchet and per-primitive obligations | contract 052 §12 |
| Process and signal ownership on the container side | contract [005](../contracts/005-container-runtime-contract.md) |
| Heavy admission and run control | contract [049](../contracts/049-heavy-validation-admission-contract.md) |

## QA and tooling

`test:kernel:<area>` selectors own the narrow proof families; `check:kernel`
compiles all kernel targets and runs strict Clippy; `qa:kernel-ratchet` enforces
`config/kernel-ratchet.json` by each construct's total across the production
files, so a rise is new raw authority and fails while a fall or a move at the
same total is bookkeeping; the baseline regenerates only under
`qa:kernel-ratchet:baseline`. `lint:linux` is the required pre-native check: it
runs strict Clippy for `aarch64-unknown-linux-gnu` across the workspace with
zig as the cross C compiler, and needs zig on PATH for the full scope. It does
not replace a native build or proof. The `fault-injection` feature's hooks exist
only under test builds.
