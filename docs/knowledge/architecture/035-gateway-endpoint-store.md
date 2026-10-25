# 035 - Gateway endpoint storage API

Owner: gateway engine, `crates/effigy-gateway/src/endpoint_store/`.
Mechanisms: [runtime kernel contract](../contracts/052-runtime-kernel-contract.md)
§5.9 and [architecture 032](032-runtime-kernel-architecture.md).

## Atomic state

`EndpointStore` addresses the operator's `~/.effigy/state/machine.db`. Its
machine-scoped `Store(gateway-endpoints)` resource owns the publication gate
and one versioned payload containing routes, loopback assignments, generated
port ranges and original import hashes. Private fixtures use the same API with a
disposable home.

`edit` begins a kernel run, reads the payload while holding its owner gate,
checks an optional expected revision, reserves newly requested conflicts, and
publishes the complete payload and claim revisions together through
`Claims::publish_retained`. The revision is that run's generation. Every exit
after admission enters kernel settle. There are no external probes or runtime
inventory queries in the SQLite transaction.

A route is the whole claim on a domain: `domain -> (checkout path, target)`.
The same checkout path always replaces its own route, which covers a new run, a
new generation and a container-to-listener handover. A different checkout path
takes the domain over only when the recorded owner is dead: its checkout path no
longer exists, or its target address does not accept a TCP connection within
200 ms. The takeover is logged in one line. Otherwise the write is refused with
one message naming the other checkout and the command that stops it. Removing the
route frees the domain immediately, so `down` followed by `up` needs no window.
Address claims cover project loopback identities; numeric port claims cover
generated host ranges. Multiple DNS aliases can share one address owned by the
same scope. Conflicting scopes, overlapping ranges and incompatible targets
on one TCP alias tuple refuse publication.

## Lifetime and release

`reserve_ports`, `read_ports`, `retain_ports` and `release_ports` use an explicit
project name and owning path. Loopback reservations use their stable identity
and owning scope. The kernel transfers committed claims from the reserving
run to the gateway store resource at settle. A Prepare run ending does not
release its environment's bindings. Releasing a different lifetime owner's
reservation refuses; missing runtime inventory never releases a reservation.

Release and removal from the endpoint payload commit together, and a released
address or port is immediately allocatable: the gateway publishes with no
reuse quarantine, and its DNS answers carry a one-second TTL. A domain that
moves between checkouts therefore resolves to its new owner at once.

## Import and observation

The first mutation imports `~/.effigy/gateway/routes.json`,
`~/.effigy/gateway/loopback-ips.json` and `~/.effigy/ports.json` under all three
legacy writer locks in fixed order, with one bounded deadline. Reads use the
kernel's no-follow, owner-checked, size-bounded record reader. Original files
remain untouched as backups; the payload records their exact SHA-256 hashes.
Once a payload exists, legacy input never imports again. Imported claims settle
before the caller edits them, so even a first-command release sees every
endpoint that the old gateway could see. A failed edit does not undo import.

The payload preserves raw JSON documents. Typed edits replace known fields
while retaining unknown root and entry fields, including pins. Releasing an
entry removes that entry and its claims; it does not reconstruct ownership from
a runtime scan.

`read` is Observe-only and creates no files or gates. `read_compatible` reads
legacy input only when the store or endpoint payload is absent. An unreadable,
corrupt or unsupported store is Unknown and refuses fallback. Reading old
files does not import or allocate anything.

## Callers and reload

Container Prepare reserves or retains generated host-port ranges through this
API, including an already assigned legacy range. Before an effectful Prepare
reads policy, it admits endpoint import/initialization if the payload is not yet
verified. This serializes first use with shared machine-store schema bootstrap;
a SQLite file alone is not proof that its schema is ready. Existing verified
payloads need no import run, and pure policy observation remains effect-free.
Unknown state enters the admitted writer's verification/recovery path and never
falls back to an empty registry.
Private gateway startup uses the existing 15-second gateway operation budget
while waiting for verified listener publication, allowing durable import before
bind/publish. Unverified refusals still fail immediately; the status budget is
unchanged.
Policy reads observe routes and loopback assignments from one snapshot without
admitting writer runs; before import, the same pure read accepts the original
legacy mapping. Generated Compose allocates only during Prepare. Certificate preparation creates only its directories and
certificates; endpoint writers and daemon startup own import/recovery, avoiding
recursive acquisition of the endpoint Store gate during TLS publication.
Prepare and route registration use the same persistent loopback allocator; a
missing container from an inventory never frees its address. The previous shared-service key is
translated only for its exact recorded owner, retaining the address.

Container and managed-listener route edits run under the Store owner gate.
Managed publication proves process generations, ancestry, readiness and exclusive
listener ownership before committing the route and its fenced claim revision.
An unobservable identity observation (for example a kernel `EPERM` on a recorded
process) is an unknown refusal, never an absence proof: it can never license
another owner to replace the listener, and it never waives the live-foreign
refusal. The supervisor's activation record, not a caller clock, is the barrier
between supervision admission and the listener's declared readiness budget.
A managed listener route's owner is its checkout path, environment and process
name. Exact child-generation teardown withdraws the route, and a stale stop
callback cannot withdraw a successor generation. Retirement and `down` remove
the checkout's routes by path, listener routes included. Explicit port reclaim
snapshots its pre-edit document through a journaled kernel file publication,
then commits the removals; the original imported JSON stays untouched.

The daemon imports before starting, reads Store snapshots and watches the
kernel database directory for reload. Legacy route-file provenance checks apply
only before import. After import the kernel's owner and path checks protect the
Store; modifying a backup has no effect. Unknown observations retain the last
applied daemon table and report an error instead of falling back to backups.
Status uses the same pure snapshot reader.

## Proxy connection lifetime

The daemon carries the committed Store revision into its in-memory route table.
HTTP/1 pools are keyed by that fence and the complete route identity and target;
a changed fence or route starts a separate pool. Idle senders expire after 30
seconds, with at most eight idle connections per route. Hyper decides readiness
and closes senders for upstream `Connection: close`, failed bodies and upgrades.
Streaming bodies finish before a sender returns to the pool. WebSocket upgrades
use a dedicated connection. Accepted HTTP/HTTPS and upstream sockets use
`TCP_NODELAY`.

Retiring a pool marks it retired and wakes its waiters. If another endpoint
advanced the shared snapshot while a request was preparing, the exact unchanged
route can obtain a new proof under the current fence. A changed or withdrawn
route refuses the old request. Requests never wait for a retired worker to
replenish a socket, and refreshing a fence never reuses its old ownership proof.

A managed host-listener route is forwarded to its published target exactly like
a container route: one connection pool per published route identity, no
per-connection process check, and no second copy of the listener's liveness
proof. The supervisor owns that liveness: it probes the reserved port every five
seconds, withdraws the route after three consecutive misses and republishes it
when the port answers again.

The daemon runs its bounded HTTP/DNS self-test on a blocking worker, allowing
the runtime to poll the same servers under test before identity publication.
A failed HTTP self-test names its class from the bounded observation —
`(deadline)` when the probe budget expired, `(protocol)` when a peer answered
without a usable readiness status, `(listener)` when nothing usable answered —
and preserves the original observation verbatim. The class names the owner of
the failure; it does not claim the cause of a past unexplained refusal.

## Proof surfaces

- `test:gateway:endpoints`: import preservation and idempotence, persistent
  owners, immediate reuse after release, concurrent conflict races, stale
  revisions, collision refusal, untrusted inputs, Unknown reads and
  publication/retention faults; the port and route model tests also run.
- `proof:gateway:endpoints:import`: reads operator registry copies, redacts
  project paths and domains inside a fresh disposable home, then verifies
  every imported document and unchanged backup byte.
- `proof:gateway:proxy:baseline` and `proof:gateway:proxy:latency`: the same
  loopback workload records direct, plain-route and managed-route latency and
  upstream connection counts for 766 sequential requests.

- `test:gateway:proxy`: cached revocation, publication refusal, pool reuse and
  revision changes, streaming/concurrent uploads and upstream close semantics.
  `faults_engine_gateway_endpoint_commit` runs the user-owned daemon and actual
  Store watcher against before/after publication and persistent-transfer faults,
  comparing recovered state with HTTP behavior. This replaces the kernel
  placeholder.
- `test:gateway:readiness-closeout`: the managed-listener readiness and handover
  proofs — dependent replacement and its cold control, scoped host routes, the
  scoped clone round trip, the slow-owner forward handover, an injected
  unobservable-identity control, the typed HTTP self-test fault controls and the
  activation-publication barrier. A shared-process step runs the affected
  gateway cases in one libtest process with concurrent threads and
  `--include-ignored`, so none of them can pass by being skipped.

## Explicit downgrade and re-upgrade

Tom authorized a reversible gateway downgrade on 2026-10-19 (effigy#237).
Export-on-remove is the narrow gateway exception to the general migration
policy's unsupported downgrade; it is not a dual-write compatibility period.
`gateway setup --remove` stops the user daemon under setup ownership, then the
endpoint Store run verifies it is stopped and records `exporting` before
journaled replacements of the three legacy documents. The same Store gate
excludes edits and daemon startup's mandatory import. A daemon admitted after
the stopped observation cannot import/bind until the export finishes, and then
refuses the exported authority state.

Every destination must still contain its import digest or the frozen export's
exact JSON value. All destinations are verified before publication begins.
`Run::replace_file` provides intent, atomic replacement and positive settlement;
partial exports keep the frozen payload for retry. Only after all three files
are published does authority become `legacy`. Ordinary mutations and user
daemon startup refuse both non-Store authority states. Pure compatible reads
use current legacy files after completed handoff. Repeated removal preserves
later old-version edits. Unchanged mappings retain their raw JSON fields and
unchanged documents retain their original bytes.

Setup's privileged child reads only the explicit legacy files and setup
receipt through its authenticated operator descriptor; it never opens the
operator's kernel database. Re-upgrade authenticates/quiesces the v0.14.1 root
daemon, then re-imports current files under the endpoint gate and legacy locks.
Existing retained claims fence reconciliation; removed claims release at once.
A partial export must complete before re-import. Active managed-listener routes
are refused with their exact domains because v0.14.1 lacks that ownership fence.
Withdrawn reservations stay inactive during export; re-import reconciles the
legacy inventory. Interface aliases are retained: setup's boot-job marker does
not prove that setup created any existing address.
