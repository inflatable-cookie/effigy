# 034 - Gateway Migration and Listener Handoff

Owner: gateway
Status: the experimental launchd handoff surface was retired in v0.15 (Q-009); only the internal adoption boundary remains, source-only and unqualified for live use.

## Migration boundary

The standard daemon runs as the operator under kernel custody, with its
control socket in `~/.effigy/gateway/runtime/`. Routine up, down, status and
route registration never elevate. `gateway setup` owns administrator changes
and migration from a live v0.14.1 identity-bearing root daemon, preserving
routes and allocations. Numeric PIDs alone never authorize signaling or
replacement. Older numeric-only records require administrator diagnosis;
the pre-0.14 recovery command is retired.

Kernel runs and gates own the standard lifecycle. The existing transition
lock remains for private gateway operations, while JSON route/allocation
writers retain their current locks until endpoint-store adoption. Socket
inspection belongs to `socket_ownership`. No migration replays or mutates
historical transition receipts.

## Unprovable status and the headless precondition

`gateway status` never upgrades unproven state. When this caller cannot read
or verify the recorded generation — a root-owned identity the kernel denies
(noninteractive/headless), an unreadable record, a legacy numeric-only pair, or
an ambiguous process probe — the command exits non-zero and `--json` prints an
`effigy.gateway.status.v1` refusal with `ok: false`, `status: "unknown"`, the
recorded `pid` when one was read, and `observations.record` /
`observations.process` / `observations.identity` labels. A record this caller
may not open is `unreadable`, distinct from `untrusted-or-malformed`. The
refusal's `headless.satisfied` reports only the documented observation
precondition — `record=authenticated` and `process=running` with a recorded
PID — and never claims readiness or a known generation. Noninteractive plans
gate on those observations instead of exit 0; a denied or unknown identity
stays unknown, and resolving it still needs the bounded read-only check from an
interactive admin-capable terminal or `gateway setup` migration. No
administrator workaround, elevation prompt or weakened Unknown is part of
status.

The historical design analysis below records the root dependencies that led
to the operator route. The implemented transport and remaining machine
qualification are described in [Operator daemon custody and privileged
listeners](#operator-daemon-custody-and-privileged-listeners).

## Operator-selected user-owned gateway route

Tom's 2026-10-13 ruling, relayed verbatim by the Acowtancy owner, is:
"I'm happy for the gateway to restart as me, would make things simpler."
The selected next route is a user-owned gateway with separately bounded
administrator setup and an operator-run migration. This is design and private
qualification authority; no live gateway restart, administrator action or
machine configuration change is authorized for an agent. The root-daemon
attestation proposal below remains unimplemented and is not this route's
prerequisite.

Legacy root dependencies that required separation:

- `/etc/resolver/test` and any declared route-specific resolver suffixes require
  administrator writes. The current daemon reconciles route-driven files
  directly, which assumes root. A user daemon must report missing setup and
  leave those files to the explicit setup path, rather than silently prompting
  or failing to provide a newly registered domain's DNS.
- Creating `lo0` aliases for the bounded `127.1.0.1`–`127.1.0.50` pool requires
  administrator network setup. The allocation registry is user state, separate
  from those interface changes. Alias setup may need repeating after reboot;
  "one-time" does not establish persistent interface configuration.
- HTTP and HTTPS currently bind `127.0.0.1:80` and `127.0.0.1:443`. Effigy
  currently treats all Unix low ports as requiring elevation. The claim that
  recent macOS permits every unprivileged low-port bind is insufficient:
  [Apple's IPv4 bind implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/netinet/in_pcb.c)
  checks reserved-port privilege for a specific non-wildcard address. That
  source observation is not qualification of this machine's installed kernel.
  The reviewed route must establish actual loopback bind support or state the
  additional bounded setup mechanism needed. It must not expose a wildcard
  listener or change URLs to hide this distinction.
- Installing CA trust is an administrator operation when required; ongoing
  certificate, route and registry writes must remain readable and writable by
  the authenticated operator. Existing CA trust and credentials must not be
  replaced as an incidental migration step.

Migration must use the existing fresh-generation verification and bounded
interactive root lifecycle path to stop the old gateway. Preserve the complete
route table, its route ownership and the loopback assignments; specifically,
the reported manual `acowtancy.test` upstream `127.0.0.1:43210` belongs to the
UI-sweep checkout and must not be regenerated from the main checkout. Do not
start a successor until the old exact generation is confirmed stopped.
Ambiguous ownership, replacement, concurrent route edits or uncertain effects
hold the migration and preserve its evidence. No recursive ownership change,
PID-only signal or unknown-state cleanup follows from this ruling.

The final implementation must provide the actual reviewed setup and migration
commands with their remaining administrator requirements. Those commands are
for Tom to execute later; neither this ruling nor private fixtures qualify a
live cross-UID stop or install new packet-filter, socket-activation or privileged
helper machinery. A required new mechanism must be returned as a concrete
bounded design boundary, with its compatibility and network exposure stated.

### Source-only launchd handoff ruling

Tom answered decision `235519af-aa5b-41b7-ac61-1a75579fb34e` version 2 through
the operator board on 2026-10-13:
"Implement source-only launchd handoff; Tom installs/tests later".
This authorizes the bounded implementation and independent private review of
the exact loopback listener handoff. It does not authorize an agent to register
a system or user launchd job, run an administrator command, install network or
CA configuration, stop the live root gateway, migrate live state or reboot.

The new launchd definition must bind only TCP `127.0.0.1:80` and `443`, launch
the daemon as the authenticated non-root operator, and preserve all route,
loopback-registry, certificate and generation ownership contracts. No wildcard
listener, packet filter, URL change, passwordless sudo, setuid program or new
standing Effigy root helper is part of this ruling. Exact supported setup,
activation, migration and rollback commands must come from implementation and
review; source and private fixture qualification cannot establish live
administrator installation or cross-boot behavior. Tom owns those later steps.

### Retained adoption boundary (task 156, cut in v0.15)

Task 156 implemented the capability as `crates/effigy-gateway/src/listener_handoff.rs`
plus the `effigy gateway setup-listener-handoff` command and the internal
`__gateway-run --listener-handoff` opt-in. v0.15 (Q-009) retired the definition
generator, the command and the read-only status/removal plan; the internal
`--listener-handoff` opt-in and the descriptor adoption boundary below remain,
source-only and privately qualified only. The unexecuted set stays exactly as
the ruling requires: no agent has registered a launchd job, bound the
privileged endpoints, migrated a live gateway, or observed a boot. No command
generates, inspects or removes a definition, so an operator who wrote one by
hand is on their own. The retained surface:

- Definition (retired): `ListenerHandoffSetup` rendered deterministic launchd
  plist bytes — label `dev.effigy.gateway.user`, the canonical reviewed
  executable, `__gateway-run --listener-handoff`, the operator's passwd user,
  group and home (`HOME` pinned, never the calling shell's environment),
  `RunAtLoad` and `KeepAlive`, and a `Sockets` dict owning exactly `http` = TCP
  `127.0.0.1:80` and `https` = TCP `127.0.0.1:443` over IPv4. The generator, its
  strict parser and `validate_launchd_definition` were removed in v0.15; use
  `effigy gateway up` instead.
- Activation: the daemon adopts sockets through launchd's documented
  `launch_activate_socket` ABI (`<launch.h>`, macOS 10.10+; `ESRCH` outside a
  launchd job, `ENOENT` for a missing socket name). Every handed descriptor is
  validated — open, not a duplicate number or duplicate kernel socket object
  (the libproc `soi_so` handle; macOS `fstat` cannot distinguish sockets), a
  stream socket, listening (libproc TCP state; macOS has no `SO_ACCEPTCONN`),
  exactly the expected IPv4 loopback endpoint — before adoption, and is marked
  close-on-exec. Any refusal closes every handed descriptor exactly once,
  adopts nothing, and publishes no generation record; DNS, route watching,
  aliases and TLS keep their ordinary paths. Identity refusals are structural:
  a root effective UID or a `SUDO_*` environment refuses the handoff, and no
  environment value ever authorizes it. Private fixtures exercise this same
  adoption boundary on ephemeral loopback endpoints with owned descriptors and
  real accept round trips.
- Operator shape: the standard operator gateway (`gateway setup`,
  `gateway up|down|status`) installs and controls its own listeners; no launchd
  install, verification or rollback plan is shipped.
  Resolver files, `lo0` aliases and CA trust remain separate administrator
  steps.

Unexecuted, still required before any launchd handoff could be depended on:
Tom's administrator install and activation, a live `gateway status` against the
user-owned daemon, and one real cross-boot restore proof.

### Handoff status and removal plan (task 157, retired in v0.15)

Task 157 added the symmetric operator surface: read-only
`effigy gateway listener-handoff-status` and an **unexecuted** administrator
removal plan from `effigy gateway remove-listener-handoff`. v0.15 (Q-009)
retired both commands with the rest of the experimental listener-handoff
surface, and `crates/effigy-gateway/src/listener_handoff/maintenance.rs` with
them. Neither command had installed, run, removed or migrated anything, so the
retirement changes no live gateway state and needs no operator step. The
semantics below record what the surface did while it existed.

- Definition observation: the fixed path
  `/Library/LaunchDaemons/dev.effigy.gateway.user.plist` is opened with
  `O_NOFOLLOW`, bound to a single regular file's kernel identity (device,
  inode, size, mtime, owner, group, mode), read within a 64 KiB bound, and
  re-checked for a concurrent change. Ownership must be the administrator;
  group/other write, symlinks, non-regular objects, oversized files and
  permission refusals are distinct. The bytes must pass the strict parser and
  the complete expected generator shape (now including unknown or duplicated
  top-level keys), the resolved operator user/group/`HOME`, and the current
  canonical executable. A moved or missing executable path is a read-only
  finding, not authenticity: it reports stale/unverified and refuses a plan.
- launchd observation: `launchctl print system/dev.effigy.gateway.user` runs
  as a fixed literal argv (never a shell, never interpolated text) under one
  absolute deadline and byte limit. Only the documented service header, the
  top-level `state` and the on-disk `path` are read structurally; the output
  is explicitly not API (`man launchctl`), so unrecognized structure, a
  non-zero exit, a permission failure, a deadline, a truncated capture or an
  unavailable/unsupported binary stay distinct from the documented
  `Could not find service` absence. A loaded job whose `path` is not the fixed
  definition path is an association mismatch. Label, PID, checksum and the
  displayed executable path are observations only, never signal authority.
- Removal: a plan is emitted only when the definition verifies as the exact
  reviewed generator for the current operator and executable **and** the job
  observation is determinate (loaded with the matching path, or confirmed
  absent). The plan contains only the two fixed literal steps
  `sudo launchctl bootout system/dev.effigy.gateway.user` and
  `sudo rm /Library/LaunchDaemons/dev.effigy.gateway.user.plist`, in safe
  order, with a required fresh identity recheck and a KeepAlive warning. It
  states that its effects are UNEXECUTED, never that a gateway was stopped.
  Foreign ownership, unsafe modes, symlinks, changed bytes, unknown or stale
  shapes, operator/executable mismatches, a loaded job without a verified
  definition, and every unknown or failed observation refuse before any
  usable plan is produced.

The unexecuted set is unchanged: no agent installed, booted out, removed or
mutated a launchd job or plist, and the administrator execution, verification
and reboot proofs remain Tom-run.

## Proposed read-only root-daemon attestation

This is a concrete proposal for a separate operator ruling if Q-002's
interactive identity reader cannot support prompt-free reuse of a healthy
root-owned gateway. It grants no authority under the current task. The proposed
capability only authenticates `AlreadyRunning` reuse of the exact current
generation; replacement, down, legacy adoption and every signal continue to
require their existing generation checks and administrator lifecycle path.

### Endpoint and peer binding

- The root daemon creates one Unix-domain socket for the recorded operator UID
  under a root-owned, non-symlink runtime directory such as
  `/run/effigy/gateway/`. The directory is root-owned and not writable by
  callers. The socket is root-owned and connectable by local users; the server
  accepts requests only when kernel peer credentials identify the UID recorded
  in the trusted gateway identity sidecar (or root itself). The endpoint path,
  owner, mode, file type and parent chain are checked without following
  symlinks. A socket pathname or its presence never authenticates a daemon.
- The client connects to the endpoint selected from trusted local configuration
  and obtains the kernel-reported peer PID and UID (`SO_PEERCRED` on Linux and
  the platform peer-credential API on macOS). The returned PID and UID must
  equal that peer tuple and the sidecar's recorded PID and owner policy. The
  response also carries the boot identity, precise process start identity,
  canonical executable path, canonical gateway target digest, sidecar digest,
  and the existing supported gateway role/endpoints. The client accepts only
  the exact generation and default role already supported by the current
  identity checks; custom, partial or ambiguous endpoints stay unknown.
- Each request includes a fresh 256-bit client nonce, protocol version, target
  digest and sidecar digest. The daemon echoes the nonce and both digests in a
  bounded, length-framed response. The client checks one response on the same
  connected peer, rejects a nonce reuse or mismatch, and caches no attestation.
  A captured response therefore cannot be replayed against another connection,
  target or generation.
- The operation is read-only. It cannot update PID/version/identity records,
  register routes, request signals, authorize recovery, or cause a daemon to
  start. Peer UID, PID, boot/start identity, target digest, sidecar digest and
  role are all required; health responses, HTTP status, PID alone, socket
  ownership alone and readable sidecar bytes alone remain insufficient.

### Refusal, compatibility and qualification

- Missing, stale, replaced, symlinked, wrongly owned, wrongly permissioned or
  untrusted endpoints refuse. A peer UID/PID mismatch, stale boot/start
  identity, target or record digest mismatch, unexpected role, malformed frame,
  unknown protocol, replayed nonce, timeout or peer disconnect returns unknown
  without deleting records or signalling. Local denial-of-service against the
  endpoint is possible; it can cause refusal, never successful reuse.
- New clients use this proof only when the root daemon advertises the versioned
  endpoint and the trusted sidecar binds the same generation. Old daemons have
  no endpoint and continue through Q-002's existing bounded interactive
  administrator reader. A declined prompt, null stdin or unavailable proof
  remains unknown. No protocol negotiation may downgrade a mismatched or
  malformed new-daemon response to a PID or sidecar-only match.
- A private cross-UID qualification must run a daemon under a distinct root
  identity and an unprivileged client in an isolated disposable environment.
  It must pass the exact peer/PID/UID/boot/start/record binding and exercise
  endpoint replacement, symlink, wrong owner/mode, wrong peer, stale generation,
  wrong target/role, malformed response, nonce replay, timeout and disconnect.
  It must also qualify mixed old-client/new-daemon and new-client/old-daemon
  behavior. Injected privilege controls alone cannot claim cross-UID proof.
- Root compromise remains outside this mechanism's protection. A live
  attestation proves which process answered and its current identity evidence;
  it does not prove historical spawn ownership or eliminate the final
  identity-check-to-signal TOCTOU. Any signal still requires the existing
  immediate generation recheck. Endpoint installation, socket permissions,
  runtime-directory lifecycle and the cross-platform peer APIs need an explicit
  operator ruling before implementation.

## Operator daemon custody and privileged listeners

The operator daemon lifecycle uses the kernel gateway run and its control
socket in `~/.effigy/gateway/runtime/`. Routine control must not elevate.
Platform mutations belong to `gateway setup`; the three existing JSON
registries remain the route/allocation input until endpoint-store adoption.

Direct low-port binding is not a qualified macOS transport. An unprivileged
bind to `127.1.0.1:80` returned `EACCES` on the operator host. The selected
fallback (planner ruling, 2026-10-19) is a minimal root bind broker. Setup
installs a root-owned helper image and a per-operator LaunchDaemon whose Unix
socket is root-owned, group-connectable (0660), and under the root-owned run
directory. Its arguments pin the operator UID. `getpeereid` must match that UID
before the broker reads a fixed-size request. Only TCP 80 and 443 at
127.0.0.1 or the setup-installed 127.1.0.1–50 range are accepted. No wildcard
bind, route/state reads, client accepts, TLS or HTTP processing run as root.
The broker sends two listening descriptors using SCM_RIGHTS and retains its
copies until the operator validates and adopts them, then acknowledges that
ownership. The acknowledgement uses the same bounded socket I/O. Exact
endpoints, listening state, identity and close-on-exec are checked before
adoption. Refused/partial exchanges publish nothing.

The daemon starts normally under kernel custody, then requests its listeners
after exec. No listener crosses the custody spawn barrier and no inherited-FD
kernel API is added. Dropping a daemon's listeners releases them; its next
Run can acquire a new pair through the same broker. Status and ordinary
lifecycle remain unprivileged. Linux uses the executable-local
CAP_NET_BIND_SERVICE adapter; setup never changes the host port threshold.
Both adapters refuse foreign setup output before changing it.

Linux keeps the same one-authority model with a launch-only systemd **user**
unit. `gateway setup` writes and enables
`~/.config/systemd/user/effigy-gateway.service`, whose only actions are the
operator commands `effigy gateway up` (start) and `effigy gateway down`
(stop). The unit declares no `Restart=` policy and never runs the daemon
directly, so `systemctl --user restart` is a reconcile request and the
kernel `Run`, gate, journal and settlement stay the sole lifecycle
authority. The unit is user-owned, marker-verified, idempotent, removed only
when Effigy wrote it, and `gateway setup --remove` disables it before the
capability is removed. A unit directory Effigy creates is explicitly 0755, so
the Ubuntu login umask 002 cannot leave Effigy's own directory group-writable
and then refused after the administrator step; a directory that already exists
is never repaired, and an unsafe one is refused with the exact `chmod`. A login-session user manager starts it without help;
a login-less boot start is the operator's explicit `loginctl enable-linger`
opt-in (reversed with `disable-linger`) and setup never enables it or needs
passwordless administration.

The unit's own text is the launch contract. `ExecStart`/`ExecStop` are the
quoted installed image plus `gateway up`/`gateway down`, and
`WorkingDirectory=` is the operator home written bare: that value takes one
literal path and has no quoting rule, so a quoted value would name a directory
whose name includes the quotes. `Environment=` keeps its `HOME=` assignment
quoting. The image is one real file: setup and status open the installed path
with `O_NOFOLLOW`, so a symbolic or hard link is refused for the unit and the
capability even when its target is a safe image, because a link's target can be
retargeted after setup. The source regression layer (`linux_operator` fake
systemd manager and fault fakes) proves clean install, repeated setup, an
upgrade that rewrites the owned unit, login and reboot starts from the installed
unit's own `ExecStart`, `stop`/`restart` as ordinary requests,
`RemainAfterExit` no-ops, a failed `gateway up` leaving nothing running, foreign
unit and capability refusal, a linked image, a replaced image and exact cleanup,
without a live user manager. A reversible break probe on those tests is part of
the change's proof, not a live claim.

Status reports `unit` as `missing`, `installed`, `foreign`, `unsafe` or
`stale`. `stale` is Effigy's own marker-carrying unit whose declared
`ExecStart` image this account can no longer run safely — deleted by a rollback
or no longer one owner-only executable — and it is never reported ready: the
login activation may still be linked, so recovery names the image, the rollback
restore, `effigy gateway setup` and a `systemctl --user restart` reconcile
request. A stale unit is still Effigy's own file, so `setup --remove` disables
and removes it. A replaced image (an upgrade) carries no file capability, so
status reports `low_port: missing` until setup restores the exact record and,
when the path changed, rewrites the unit to name the image setup ran.

After the unit file is written, the user manager is an observation rather than
a setup precondition, and every outcome is reported as the *observed* state
rather than as a claim. A `daemon-reload` that cannot be spawned, refuses or
does not answer within 20 seconds returns the unit as installed with the
activation link exactly as it is on disk: absent for a fresh install, present
when a repeated setup kept it, and present when an `enable` created it before
failing. The manager's own sentence and the exact `systemctl --user
daemon-reload`, `enable` and `start` commands (and the `loginctl
enable-linger` opt-in) are named alongside it.

Removal keeps the same shape in the other direction. It disables the unit,
re-observes the activation with a fresh open — including when the unit started
out disabled or had no activation directory — and refuses before deleting the
unit file if any entry is present or uncertain. A `daemon-reload` that refuses,
cannot be reached or times out after a completed removal is reported as an
error naming the exact `systemctl --user daemon-reload` command, because the
manager may still serve the old definition; it is no longer swallowed. A
refused disable does preserve the unit and names `effigy gateway down` plus the
two commands.

A changed definition disables the login activation before the new unit file is
written, because the user manager may keep the definition it already loaded. A
failed or timed-out `daemon-reload` therefore leaves the unit installed and not
enabled — never a new file whose surviving link still claims the old definition
is live — and the link returns only from an `enable` setup asked for. An
`enable` that reports a failure after writing the link is observed as enabled:
the link is the fact and the reply is not. A repeated setup whose bytes are
unchanged touches neither file nor link, so a failed manager request there
reports the link Effigy kept together with the recovery commands. If the atomic
write fails after the activation was dropped, the previous file is still on
disk: its activation is restored through the same bound directory and the error
says the installed route still works, or names the exact partial state and
commands when the file did change. Rollback after such a partial upgrade is two
supported routes: restore the image the unit names (a deleted or widened image
is `stale`), or run `effigy gateway setup` with the rolled-back binary, which
rewrites the owned unit and enables it again.

The login activation directory `~<home>/.config/systemd/user/default.target.wants`
is treated as an identity of its own. It is opened from the verified unit
directory with `O_NOFOLLOW | O_DIRECTORY` and its owner and mode are checked
before anything else happens, so a `default.target.wants` symlink to a foreign
directory cannot redirect Effigy, or the manager on Effigy's behalf. A directory refusal's `chmod` is emitted as one shell word and names the
directory that failed — the activation directory itself, not the service entry
inside it. Only a mode problem is repaired that way: a symlink, a non-directory
or a foreign-owned directory is an identity failure and is refused with the
move-aside recovery, because a `chmod` through a link follows it to its target
and cannot repair an owner. A missing
unit directory is treated as no activation at all — a fresh installation, or one
whose directory was removed before a retry, has nothing to preserve and still
tears down cleanly — while a linked, foreign-owned, non-directory or
group/world-writable one is refused. The entry
has three states — absent, Effigy's own link (recorded with its inode and target,
read with `fstatat(AT_SYMLINK_NOFOLLOW)`), or foreign (a link to another target,
or something that is not a link) — and a foreign entry is refused, never treated
as absent.

Effigy never deletes an activation entry. `unlinkat` is a name operation with no
inode condition, so a check followed by an unlink can always delete whatever a
same-uid writer put at that name in between, and no scheme Effigy can build on
POSIX closes that interval. The removal therefore belongs to the manager:
`systemctl --user disable` (and `disable --now` on removal) removes the entry,
and Effigy verifies the result through the bound directory. A remaining entry is
refused with the exact `systemctl --user disable effigy-gateway.service` command,
a foreign one is preserved and named, and nothing is deleted by Effigy in either
case. The same rule governs an upgrade: a changed definition disables the login
activation before the rewrite and refuses to rewrite while it remains. The cost
is explicit and deliberate: on a host where the user manager cannot remove the
entry, Effigy refuses and prints the command instead of performing the unlink
itself. Because Effigy performs no such unlink, a replacement that appears at
the entry is preserved and reported rather than deleted.

A repeated setup whose unit bytes are unchanged keeps the existing link and
reports the observed `enabled` state together with any manager failure and its
recovery commands. An `enable` that fails after writing the link is reported as
enabled *and* as a failed manager request. A post-request activation that is
foreign or unsafe is reported as that refusal — after `daemon-reload` as well as
after `enable`, and in a failed rewrite's recovery — and the `enable` recovery is
offered only for a genuinely absent activation. A failed rewrite re-observes the
activation after a failed restore (the restore failure is typed and neutral, so
no nested message can carry enable advice), names an unreadable, missing or changed unit
file as what it is, and reports the activation it observed rather than asserting
it is off. Removal keeps the rule in the other
direction: the unit file and the activation are removed, and a `daemon-reload`
that refuses, cannot be reached or times out is then reported as a failure
naming the exact `systemctl --user daemon-reload` command, because a removal is
not finished while the manager still serves the old definition.

A failed rewrite restores only an activation this run actually disabled, so a
unit the operator had disabled is never enabled by a recovery path, and the
restored entry must verify as Effigy's own link: an entry that appeared
meanwhile is named with its recovery rather than reported as a restore.

The capability is bound to one install identity: the exact binary setup ran,
with one link, owner-private mode and the operator (or root) as owner. A
changed, linked or group/world-writable image is refused before a capability
or unit names it, and so is an image the operator identity cannot execute. That
access is resolved the way POSIX resolves it: the owner execute bit when the
operator owns the image; for an image it does not own, the group class when one
of the operator's groups is the image's group and the other class only
otherwise, so a root-owned `0745` whose group the operator is in is readable but
not executable whatever its other bits say. An unreadable membership list is
refused conservatively (both the group and other bits must allow execution). An
execute bit alone is not enough — an owner-owned mode `0410` image has one and
its owner still cannot run it — so a unit naming such an image is reported
`stale` rather than healthy. The refusal names the rule that decided it — the
owner bit, the matched group bit, or the other bit — with the `chmod` for that
bit, the path emitted as a safe POSIX shell word (single-quoted, `'\''` for an
embedded quote) so `$`, backticks, backslashes and quotes arrive literally; a
path that is not valid UTF-8 cannot be printed as a shell word and the refusal
says so. Identity failures are
separate: a link, a foreign owner or a writable image cannot be repaired by a
permission bit, so they name the install recovery (one real file owned by the
account) and prescribe no `chmod`. Setup verifies the result instead of assuming it: after the
privileged stage, a fresh exec of the installed binary as the operator binds
the configured low ports and reports `bound`, `in_use` or `denied`, and a
denial fails setup rather than leaving a gateway that cannot serve 80/443.
`gateway status` observes the unit, its login activation, `linger` and the
capability record without repairing or elevating; a foreign or unsafe
activation keeps its own refusal (never "not enabled", so no `enable` advice
through a refused path), and an unsafe unit file is named as that file rather
than as a directory, omits an unobservable linger
state, and reports a foreign unit or capability as `foreign` with the exact
recovery commands. Boot and native user-manager qualification on a Linux host
remains a separate exact-candidate clean-base proof; guide 063 carries its
commands, identities and cleanup.

The accepted prerequisite kernel APIs remain Run replace_file/enable_custody,
pure latest-run/effects projections, gateway control-socket placement and
setup-only descriptor-relative ownership transfer. They require independent
review with this engine change. The executable fingerprint size limit is
owned by task #220, not this task.

Reboot, actual launchd activation and CBS/Acowtancy URL qualification remain
operator-run actions on the exact candidate. The guide records the steps.

The shared binary dispatches task, source and gateway helpers through the
task engine's kernel entry point. It accepts the gateway's fixed recovery
invocation alongside task selectors and bundle sync. Kernel-owned gateway
effects settle in the kernel; unrecognized engine effects remain unknown.
Operator startup and setup share the binary's one signal router. Status uses the
kernel's native boot observation rather than a PATH-selected sysctl process.
Legacy UUID spelling is compared without case sensitivity; legacy timeval
records retain their existing conservative comparison. The DNS self-test uses
a valid hostname label accepted by the kernel read-only transport.

Status observes one bounded sequence of kernel facts. Starting and stopping
runs are non-ready without probing a process that may not yet exist or may
already have retired. A generation change during observation retries within
the same three-second budget; a persistently unverified observation reports
`unknown` with its reason and never grants readiness or destructive authority.
The `gateway.status.after-owner-read` fault point proves gate release and
retirement between owner observations. Supervisor detach observes client EOF
before draining durable requests: detach commits before pipe closure, so an
EOF cannot overtake the request and spuriously stop the gateway.

Operator child environments discard inherited `EFFIGY_CUSTODY_NONCE` and
`EFFIGY_PROBE_NONCE`; the kernel assigns the daemon its own custody identity.
Forwarding a parent nonce is a spawn refusal before process registration, so
there is no daemon stderr to capture. Startup consumes the supervisor's spawn
result before probing readiness and retains the 15-second startup budget.
A daemon that exits after exec has its outcome and redacted stderr persisted
by kernel custody. A settled failed run remains `stopped`, with the exit code
and up to 4096 bytes of stderr tail in the status reason; `up` returns that
reason as soon as the run closes. Missing log evidence never changes positive
settlement into an unknown outcome. An ordinary requested shutdown has no
failure reason.
