# 035 - Gateway endpoint storage API

Owner: gateway engine, `crates/effigy-gateway/src/endpoint_store/`.
Mechanisms: [runtime kernel contract](../contracts/052-runtime-kernel-contract.md)
§5.9 and [architecture 032](032-runtime-kernel-architecture.md).

## Atomic state

`EndpointStore` addresses the operator's `~/.effigy/state/kernel.db`. Its
machine-scoped `Store(gateway-endpoints)` resource owns the publication gate
and one versioned payload containing routes, loopback assignments, generated
port ranges, original import hashes and released-resource quarantine. Private
fixtures use the same API with a disposable home.

`edit` begins a kernel run, reads the payload while holding its owner gate,
checks an optional expected revision, reserves newly requested conflicts, and
publishes the complete payload and claim revisions together through
`Claims::publish_retained`. The revision is that run's generation. Every exit
after admission enters kernel settle. There are no external probes or runtime
inventory queries in the SQLite transaction.

Active domain claims include the complete route, including managed-listener identity.
A withdrawn route retains its checkout and stable listener domain owner without
an active routing target. Routine down/up can reactivate that domain immediately;
explicit checkout retirement releases the persistent domain claim. Withdrawal
also retains its address and TCP-port conflict set, including imported aliases
without a separate loopback assignment, so an immediate restart can reactivate
the same endpoints.
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

Release and removal from the endpoint payload commit together. Kernel claims
remain quarantined for the 60-second DNS TTL plus the ten-second proxy drain
budget. The payload keeps a conservative allocation hint through the maximum
remaining operation budget, so the allocator cannot select an address or
range whose kernel quarantine has not expired. A reboot ends both boot-clock
deadlines. Unknown clock evidence preserves protection. An address or port
cannot move directly between owners without release and quarantine.

## Import and observation

The first mutation imports `~/.effigy/gateway/routes.json`,
`~/.effigy/gateway/loopback-ips.json` and `~/.effigy/ports.json` under all three
legacy writer locks in fixed order, with one bounded deadline. Reads use the
kernel's no-follow, owner-checked, size-bounded record reader. Original files
remain untouched as backups; the payload records their exact SHA-256 hashes.
Once a payload exists, legacy input never imports again. Imported claims settle
before the caller edits them, so even a first-command release quarantines every
endpoint that the old gateway could see. A failed edit does not undo import.

The payload preserves raw JSON documents. Typed edits replace known fields
while retaining unknown root and entry fields, including pins. Explicitly releasing an
entry removes that entry and quarantines its claims; it does not reconstruct
ownership from a runtime scan.

`read` is Observe-only and creates no files or gates. `read_compatible` reads
legacy input only when the store or endpoint payload is absent. An unreadable,
corrupt or unsupported store is Unknown and refuses fallback. Reading old
files does not import or allocate anything.

## Callers and reload

Container Prepare reserves or retains generated host-port ranges through this
API, including an already assigned legacy range. Before an effectful Prepare
reads policy, it admits endpoint import/initialization if the payload is not yet
verified. This serializes first use with shared kernel-store schema bootstrap;
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
Managed preflight reserves the persistent domain before child startup. Exact
child-generation teardown withdraws active routing without releasing that
stable owner; stale callbacks cannot withdraw a successor. Retirement releases
only the recorded checkout's assignments. Explicit port reclaim snapshots its
pre-edit document through a journaled kernel file publication, then commits
removals and quarantine together; the original imported JSON stays untouched.

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

Managed connections are prepared and fully verified by background workers before
request code can acquire them. Request code never invokes the process table,
ancestry or socket inventory. It checks the bounded supervisor readiness record
and direct kernel identities for the three recorded PIDs, including on a reused
HTTP connection. Thus a supervisor's unavailable record or a reused PID refuses
immediately. The connected socket stays attached to the peer that was verified;
a reconnect cannot consume an unverified connection on a reused port.

Until the supervisor's claim revocation is represented directly in the kernel,
background workers repeat the full liveness proof once per second and before
publishing a newly prepared socket. An observation older than three seconds or
any Unknown/error refuses traffic. Workers are owned by the proxy lifetime and
canceled with it. This bounded cache is the interim liveness seam; the Store
remains the sole persistent endpoint authority.

The daemon runs its bounded HTTP/DNS self-test on a blocking worker, allowing
the runtime to poll the same servers under test before identity publication.

## Proof surfaces

- `test:gateway:endpoints`: import preservation and idempotence, persistent
  owners, quarantine, concurrent conflict races, stale revisions, collision
  refusal, untrusted inputs, Unknown reads and publication/retention faults;
  the existing port and route model tests also run.
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
