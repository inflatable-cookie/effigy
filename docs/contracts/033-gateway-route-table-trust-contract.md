# Gateway Route-Table Trust Contract

Status: Active
Owner: Platform maintainers
Roadmaps: `g08.014`

## Purpose

Define the trust boundary for the local gateway's route table. The gateway
daemon runs with elevated privilege (binds `:80`/`:443` and writes
`/etc/resolver/*` on macOS) and reverse-proxies traffic to whatever upstream
each route names. The route table file is the single input that steers that
privileged proxy. This contract states what the daemon is allowed to trust that
file to assert, how it verifies the file's provenance, and what it does when the
file fails verification.

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

## Next Task

`g08.014` Batch B implements the read-path integrity gate against this
contract; Batch C adds operator visibility. No implementation batch is ready
until this contract is promoted and referenced from the contracts index.
