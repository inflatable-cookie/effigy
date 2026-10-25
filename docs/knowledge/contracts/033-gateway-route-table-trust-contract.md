# Gateway Route-Table Trust Contract

Owner: Platform maintainers

## Current applicability

The v0.15 user-owned gateway uses the transactional endpoint Store described
in [architecture 035](../architecture/035-gateway-endpoint-store.md) and
[runtime kernel contract 052](052-runtime-kernel-contract.md). The JSON trust
checks below apply to legacy route input before import. Once imported, the
kernel's owner-checked Store is authoritative; the retained JSON backup cannot
steer route reload. Unknown Store observations preserve the applied table and
refuse fallback. The following sections record the v0.14 trust boundary that
migration preserves for its legacy input.

The explicit downgrade handoff in architecture 035 exports a frozen Store
snapshot and marks the JSON files authoritative again. Exported routes carry
the v0.14 trust marker checked below. Re-upgrade stops the verified legacy
writer before importing its current files; ordinary v0.15 edits never write
both authorities.

## Purpose

Define the trust boundary for the local gateway's route table. The gateway
daemon runs with elevated privilege (binds `:80`/`:443` and writes
`/etc/resolver/*` on macOS) and reverse-proxies traffic to whatever upstream
each route names. The route table file is the single input that steers that
privileged proxy. This contract states what the daemon is allowed to trust that
file to assert, how it verifies the file's provenance, and what it does when the
file fails verification.

The launchd listener handoff (experimental; retired in v0.15 by Q-009;
[architecture 034](../architecture/034-gateway-legacy-upgrade-recovery.md))
was an operator-owned daemon that adopted two handed loopback listeners
instead of binding them. v0.15 removed its definition generator, read-only
status and unexecuted removal plan; only the internal `--listener-handoff`
adoption boundary remains, and the gateway runs as the operator through
`effigy gateway up`. Any adoption reads the same route table under the same
checks; nothing in this contract changes with who owns the listener sockets.
The retired commands never read, trusted, rewrote or removed `routes.json`, and
their removal changes no route-table ownership, markers or generations.

## Problem

Today `routes.json` (typically `~/.effigy/gateway/routes.json`) is a plain
JSON document:

```json
{ "routes": { "myapp.test": { "target": "127.0.0.1:8080", "tls": true, "project": "/abs/path", "source": "container", "registered": "..." } } }
```

`RouteTable::save` writes it atomically (temp + rename) but sets **no file
mode** and stamps **no managed-by marker**. `RouteTable::load` reads and parses
it with no provenance or integrity check. The elevated daemon then proxies a
root-trusted listener to each route's `target` host:port.

Consequences:

- Any local process that can write `routes.json` can redirect the
  root-trusted reverse proxy to an arbitrary upstream (`target` /
  `tcp_target`), or change a route's `tls`/`dns_ip`/`tcp_port`, without any
  integrity gate on the read path.
- This is asymmetric with two sibling subsystems that already model trust:
  resolver files carry the `Managed by Effigy gateway` marker and are matched
  by `file_is_effigy_managed` before the daemon will remove them, and the
  secret vault enforces a `0o600` permission check via
  `inspect_vault_permissions`. The route table has neither.

## Threat Model

- **Asset:** the elevated gateway daemon's proxy/DNS/TCP-alias behavior.
- **Trust anchor:** the route table file on the local filesystem.
- **In-scope adversary:** a local process running as a *different, non-owner*
  user that can write the gateway directory or the route table file; and the
  owner accidentally corrupting or hand-editing the file.
- **Out-of-scope adversary:** a process running as the file's owner or as root.
  If an attacker already has the owner's UID or root, they own the daemon
  regardless; this contract does not attempt to defend that case. Effigy's
  gateway is a single-user localhost developer tool, not a hardened
  multi-tenant proxy, and this contract sets that ceiling deliberately.
- **Primary risk addressed:** another local user (or a wrong-permission file)
  silently steering the privileged proxy to an upstream the owner never
  registered.

## Core Rules

- The gateway directory and the route table file are owned by the user that
  runs the gateway, and must not be writable by group or other.
- Before the elevated daemon trusts the route table on the read path, it must
  verify file integrity: ownership and permission (no group/other write), in
  the same spirit as `inspect_vault_permissions`.
- A route table that Effigy writes must carry an explicit Effigy-managed
  provenance signal, and the daemon must validate that signal on load — not
  only on teardown. Reuse the managed-marker pattern already established for
  resolver files rather than inventing a divergent mechanism.
- The route table is trusted only to assert routing intent: domain → optional
  upstream `target`, `tcp_target`, `dns_ip`, `tcp_port`, `tls`, `source`, and
  `project`. It is never trusted to expand the daemon's privilege, change its
  bind addresses, or name a privileged action beyond proxy/DNS/alias routing.
- Managed host-listener routes also carry the exact recorded process and
  listener generation needed for read-time ownership verification. The proxy
  checks that identity and the bound loopback socket after connecting and
  before forwarding request bytes, including WebSocket upgrades. This data
  does not authorize the daemon to launch or signal a process.
- A managed route is unavailable when its matching state record is not
  `ready`, its process identity changes, its ancestry is lost, its socket no
  longer belongs to the recorded process, or another same-user process shares
  the endpoint. Static `target_host` routes remain external route intent and
  are not certified as managed child listeners.
- Trust verification is a read-path gate, not a one-time setup check; it runs on
  initial load and on every watcher-triggered reload.
- The verification must not add friction to the normal single-user flow: a
  correctly owned, correctly permissioned, Effigy-written table loads silently.

## Integrity Mechanism

The implementation milestone selects one mechanism that satisfies the Core
Rules; the contract fixes the requirements, not the exact bytes:

- **Ownership + permission check** (required): refuse group/other-writable
  tables; confirm owner match on platforms where it is meaningful, mirroring
  the vault's `VaultPermissionStatus` model.
- **Managed provenance marker** (required): the table records that Effigy wrote
  it; an unmarked or foreign-marked table is treated as untrusted. A signature
  is permitted but not required — a marker plus ownership/permission is the
  baseline for a single-user localhost tool.

The route table file itself must be created with a restrictive mode
(no group/other write; align with the gateway directory's existing posture).

## Failure Mode

When the route table fails the trust check, the daemon must choose a single,
documented, fail-closed behavior. The implementation milestone picks one and
records it here; the default this contract endorses is:

- **Refuse to load the untrusted table and keep the last-known-good in-memory
  table**, emit a clear warning, and surface the failure in `effigy gateway`
  status and `effigy doctor`. The daemon does not silently adopt an untrusted
  table, and does not crash.

A missing table remains valid (empty table), unchanged from current behavior.

## Operator Visibility

- `effigy gateway` status reports route-table trust state (trusted / untrusted
  with reason).
- `effigy doctor` surfaces an untrusted or wrong-permission route table as a
  finding with remediation.

## Out of Scope

- Defending against a same-UID or root adversary.
- Network-level authentication, multi-tenant isolation, or per-route ACLs.
- Encrypting the route table (it carries routing intent, not secrets; secret
  material stays in the vault under `032`).

## Change Policy

- This contract governs `g08.014`. Changes to the trust boundary, integrity
  mechanism, or failure mode require a contract update in the same change.
- The implementation must match this contract; if implementation reveals the
  endorsed default failure mode is wrong, update this contract first, then the
  code.

## Drift Triggers

- Any change to how `routes.json` is written, permissioned, or marked.
- Any new privileged action the daemon performs based on route-table content.
- Planning review against `g08.014` plus focused trust-verification fixtures
  (well-formed, tampered, wrong-permission, foreign-marked tables) once
  implementation starts.

## Container reconciliation binding proof

A route reservation is not a published binding. The production registration
adapter re-admits selected inventory rows through fresh inspect under one
cumulative deadline. Canonical full ID, exact project/service, explicit
non-oneoff running state, checkout and scoped label identity must agree before
current published-port associations enter route resolution. Missing, foreign,
shared, stale or unavailable associations refuse without guessing a target.
An owned backing replacement updates its TCP target through the existing locked
route transaction, releasing the old own association without changing foreign
or shared route claims. Private fake-transport/process evidence qualifies this
adapter path, not a real engine or elevated gateway daemon.
For an unscoped checkout the inspected scope label must also be absent; an
unexpected, null, empty or unreadable label is not absence-to-absence proof.

### Detached admission at publication

Detached `container up` carries an immutable admitted context through runtime
observation into the existing locked route reconciliation. It compares the
original snapshot with current checkout/scope/backend/profile/project/primary,
manifest and Compose identities and bytes, and requires the persisted marker
to match. Missing, unreadable or changed context refuses after the owned probe,
before locked preparation, and immediately before saving the route table.
The refusal preserves the existing own/foreign table and the original marker;
it never records a baseline from the changed source. Restore the admitted
files and marker, then run `effigy container up --detach` to reconcile the
still-running owned stack. Attached and unrelated registrations carry no
detached admission guard and retain their existing behavior.

These checks cover the interval from admission through the final pre-save
read. The route-table lock serializes route-table writers only. It does not
exclude filesystem writers, make the multi-file reads atomic, detect a change
and restoration between reads, or guarantee that files stay unchanged after
the final read. Cooperating source writers must keep admitted inputs stable
through publication; no latest-source or filesystem quiescence guarantee is
claimed. Private production registration controls mutate source and marker
files after actual owned probes and during delayed preparation, verify byte-
exact retained tables on refusal, and restore the admitted inputs to reconcile.

### TLS dependency maintenance

The resolved rustls dependency is 0.23.46, which satisfies the patched range
for RUSTSEC-2026-0346 through RUSTSEC-2026-0349. This lock-only maintenance
changes no gateway TLS features, cipher selection, certificate validation or
client authentication policy. The advisory finding on 0.23.45 does not by
itself establish exploitability in Effigy's configuration.

## Listener port reservations and host CA trust (effigy#321)

The endpoint store also owns concrete loopback ports for managed host
listeners. A `bind = "127.0.0.1:0"` declaration reserves one port from the
environment's generated range before launch, keyed `listener:<name>` in that
project's allocation; the assignment excludes generated Compose bindings and
ports already listening on loopback. The reservation
survives supervisor restarts within the run and is released when the
environment settles, so a settled environment holds no listener port. This is
the same machine authority and the same write path as Compose port ranges: no
second port allocator exists, and a foreign lifetime owner is refused rather
than replaced.

Host processes receive the active gateway's CA so a host-side client can call
its own HTTPS route:

| Variable | Runtime that reads it | Delivery |
| --- | --- | --- |
| `EFFIGY_GATEWAY_CA_FILE` | any process that wants the path itself | always, when the CA is known |
| `NODE_EXTRA_CA_CERTS` | Node, Bun | only when the operator has not exported it and the process does not declare it |
| `SSL_CERT_FILE` | curl, Python, Ruby (OpenSSL/rustls defaults) | same |

A private gateway (`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`) contributes
`<root>/ca/rootCA.pem`, the same file its certificates are signed with; the
shared operator gateway contributes mkcert's root CA. Values are never
invented: when the CA file is absent, no trust variable is set. The
runtime-specific names exist because those runtimes do not read a neutral
variable, and the operator-set rule exists because a host process replacing a
person's own trust store is a silent security regression.

## Managed listener route ownership and withdrawal (effigy#335)

A route is one claim: `domain -> (checkout path, target)`. A managed listener
route's owner token is its checkout path, environment and process name, which
is stable across runs and restarts. The checkout path is the whole ownership
comparison:

- **Same checkout path.** The listed writer replaces the route, which covers a
  new run, a new child generation and a container-to-listener handover. No
  journal proof is required and no run identity is recorded.
- **Dead owner.** A different checkout path takes the domain over when the
  recorded checkout path no longer exists, or the recorded target does not
  accept a TCP connection within 200 ms. The takeover is logged in one line.
- **Live owner.** Otherwise the write is refused with one message naming the
  other checkout and the command that stops it there
  (`effigy container down`).
- **Withdrawal on stop.** `down`, `reset` and session settle withdraw every
  route owned by the checkout path, listener routes included, matched by this
  checkout path and the declaration's stable owner token. The publishing
  supervisor is not consulted, so a route left by a run that died or was
  refused is still withdrawn and reported. A process whose own stop failed
  keeps its route, because its run may still be live.
- **Daemon hygiene.** The daemon sweeps the same dead-owner rule on a timer so
  `gateway status` lists domains something still serves.

## Out of Scope

- Defending against a same-UID or root adversary.
- Network-level authentication, multi-tenant isolation, or per-route ACLs.
- Encrypting the route table (it carries routing intent, not secrets; secret
  material stays in the vault under `032`).

## Change Policy

- This contract governs `g08.014`. Changes to the trust boundary, integrity
  mechanism, or failure mode require a contract update in the same change.
- The implementation must match this contract; if implementation reveals the
  endorsed default failure mode is wrong, update this contract first, then the
  code.

## Drift Triggers

- Any change to how `routes.json` is written, permissioned, or marked.
- Any new privileged action the daemon performs based on route-table content.
- Planning review against `g08.014` plus focused trust-verification fixtures
  (well-formed, tampered, wrong-permission, foreign-marked tables) once
  implementation starts.

## Container reconciliation binding proof

A route reservation is not a published binding. The production registration
adapter re-admits selected inventory rows through fresh inspect under one
cumulative deadline. Canonical full ID, exact project/service, explicit
non-oneoff running state, checkout and scoped label identity must agree before
current published-port associations enter route resolution. Missing, foreign,
shared, stale or unavailable associations refuse without guessing a target.
An owned backing replacement updates its TCP target through the existing locked
route transaction, releasing the old own association without changing foreign
or shared route claims. Private fake-transport/process evidence qualifies this
adapter path, not a real engine or elevated gateway daemon.
For an unscoped checkout the inspected scope label must also be absent; an
unexpected, null, empty or unreadable label is not absence-to-absence proof.

### Detached admission at publication

Detached `container up` carries an immutable admitted context through runtime
observation into the existing locked route reconciliation. It compares the
original snapshot with current checkout/scope/backend/profile/project/primary,
manifest and Compose identities and bytes, and requires the persisted marker
to match. Missing, unreadable or changed context refuses after the owned probe,
before locked preparation, and immediately before saving the route table.
The refusal preserves the existing own/foreign table and the original marker;
it never records a baseline from the changed source. Restore the admitted
files and marker, then run `effigy container up --detach` to reconcile the
still-running owned stack. Attached and unrelated registrations carry no
detached admission guard and retain their existing behavior.

These checks cover the interval from admission through the final pre-save
read. The route-table lock serializes route-table writers only. It does not
exclude filesystem writers, make the multi-file reads atomic, detect a change
and restoration between reads, or guarantee that files stay unchanged after
the final read. Cooperating source writers must keep admitted inputs stable
through publication; no latest-source or filesystem quiescence guarantee is
claimed. Private production registration controls mutate source and marker
files after actual owned probes and during delayed preparation, verify byte-
exact retained tables on refusal, and restore the admitted inputs to reconcile.

### TLS dependency maintenance

The resolved rustls dependency is 0.23.46, which satisfies the patched range
for RUSTSEC-2026-0346 through RUSTSEC-2026-0349. This lock-only maintenance
changes no gateway TLS features, cipher selection, certificate validation or
client authentication policy. The advisory finding on 0.23.45 does not by
itself establish exploitability in Effigy's configuration.

## Listener port reservations and host CA trust (effigy#321)

The endpoint store also owns concrete loopback ports for managed host
listeners. A `bind = "127.0.0.1:0"` declaration reserves one port from the
environment's generated range before launch, keyed `listener:<name>` in that
project's allocation; the assignment excludes generated Compose bindings and
ports already listening on loopback. The reservation
survives supervisor restarts within the run and is released when the
environment settles, so a settled environment holds no listener port. This is
the same machine authority and the same write path as Compose port ranges: no
second port allocator exists, and a foreign lifetime owner is refused rather
than replaced.

Host processes receive the active gateway's CA so a host-side client can call
its own HTTPS route:

| Variable | Runtime that reads it | Delivery |
| --- | --- | --- |
| `EFFIGY_GATEWAY_CA_FILE` | any process that wants the path itself | always, when the CA is known |
| `NODE_EXTRA_CA_CERTS` | Node, Bun | only when the operator has not exported it and the process does not declare it |
| `SSL_CERT_FILE` | curl, Python, Ruby (OpenSSL/rustls defaults) | same |

A private gateway (`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`) contributes
`<root>/ca/rootCA.pem`, the same file its certificates are signed with; the
shared operator gateway contributes mkcert's root CA. Values are never
invented: when the CA file is absent, no trust variable is set. The
runtime-specific names exist because those runtimes do not read a neutral
variable, and the operator-set rule exists because a host process replacing a
person's own trust store is a silent security regression.

