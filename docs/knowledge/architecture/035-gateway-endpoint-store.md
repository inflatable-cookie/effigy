# 035 - Gateway endpoint storage API

Owner: gateway engine, `crates/effigy-gateway/src/endpoint_store/`.
Mechanisms: [runtime kernel contract](../contracts/052-runtime-kernel-contract.md)
§5.9 and [architecture 032](032-runtime-kernel-architecture.md).

## Atomic state

`EndpointStore` addresses the operator's `~/.effigy/state/machine.db`. Its
machine-scoped `Store(gateway-endpoints)` resource owns the publication gate
and one versioned payload containing the route table. Host ports are not part
of it: the OS or Compose assigns them. Private fixtures use the same API with a
disposable home.

`transaction` is the one write path. It reads the payload under the machine
owner gate, applies one edit, and publishes the complete payload and its claim
revisions together through `Claims::publish_retained`; the revision is that
write's generation. An edit that changes nothing writes nothing. Pure reads take
no mutate gate. There are no external probes or runtime inventory queries in the
SQLite transaction.

A route is the whole claim on a domain: `domain -> (checkout path, target)`.
The same checkout path always replaces its own route, which covers a new run, a
new generation and a container-to-listener handover. A different checkout path
takes the domain over only when the recorded owner is dead: its checkout path no
longer exists, or its target address does not accept a TCP connection within
200 ms. The takeover is logged in one line. Otherwise the write is refused with
one message naming the other checkout and the command that stops it. Removing the
route frees the domain immediately, so `down` followed by `up` needs no window.
The route table is also the whole record of loopback ownership. An address is
in use exactly when some route references it, as its `dns_ip` or as its target,
so an address claim is the route's own claim and one checkout's routes share
one address. Host ports are not claimed at all: the OS or Compose assigns them.
Conflicting scopes and incompatible targets on one TCP alias tuple refuse
publication.

## Ownership and release

An aliased route names its checkout's address, and the address is chosen where
the route is written: the checkout's recorded address when its routes already
have one, otherwise the lowest `127.1.0.1`-`127.1.0.50` address no route
references. `down` and retirement free the address by removing the routes, so
there is no separate assignment to record, reclaim or release, and no Prepare
step that allocates one.

Every route write also drops routes whose recorded checkout path is gone, for
the whole table rather than only the domains that write touched. A deleted
worktree therefore keeps neither a domain nor an address, and its next write is
a log line rather than a repair.

Removal and its claims commit together, and a released address is immediately
allocatable: the gateway publishes with no reuse quarantine, and its DNS
answers carry a one-second TTL. A domain that moves between checkouts therefore
resolves to its new owner at once.

## Observation

The payload preserves raw JSON documents. Typed edits replace known fields
while retaining unknown fields. Releasing an entry removes that entry and its
claims; it never reconstructs ownership from a runtime scan. A record written
by the port-range build keeps decoding, and its `ports` and separate `loopback`
assignment documents are dropped on the next write.

`read` is Observe-only and creates no files or gates. An unreadable or corrupt
store is Unknown; nothing falls back to a legacy file.

## Callers and reload

Container route registration writes its alias routes through this API; `down`
and retirement release the address by removing them. Pure policy observation
remains effect-free.
Private gateway startup uses the existing 15-second gateway operation budget
while waiting for verified listener publication. Unverified refusals still fail
immediately; the status budget is unchanged.
Policy reads observe routes from one snapshot without admitting writer runs.
Generated Compose records nothing: the alias address is part of the route.
Certificate preparation creates only its directories and certificates; endpoint
writers and daemon startup own the Store gate, avoiding recursive acquisition
during TLS publication.
Alias allocation and route registration are one transaction.

Container and managed-listener route edits run under the Store owner gate.
Managed publication proves process generations, ancestry, readiness and exclusive
listener ownership before committing the route and its fenced claim revision.
An unobservable identity observation (for example a kernel `EPERM` on a recorded
process) is an unknown refusal, never an absence proof: it can never license
another owner to replace the listener, and it never waives the live-foreign
refusal. The supervisor's activation record, not a caller clock, is the barrier
between supervision admission and readiness; readiness itself has two phases,
and the declared budget starts when the reserved port first answers.
A managed listener route's owner is its checkout path, environment and process
name. Exact child-generation teardown withdraws the route, and a stale stop
callback cannot withdraw a successor generation. Retirement and `down` remove
the checkout's routes by path, listener routes included.

The daemon reads Store snapshots and watches the kernel database directory for
reload. Startup creates that directory when no publisher has yet, so a daemon
started before the first route still sees it committed. The kernel's owner and
path checks protect the Store; there is no
legacy file to import or verify. Unknown observations retain the last applied
daemon table and report an error instead of falling back to a file.
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
  publication/retention faults; the route model tests also run.
- `test:gateway:loopback-lifecycle`: alias addresses derived from the route
  table, a new checkout taking an address no live route references, and routes
  of deleted checkouts dropped by any write.
- `proof:gateway:operator:migration`: reads a redacted operator snapshot,
  redacts project paths and domains inside a fresh disposable home, and
  verifies the imported record and unchanged backup bytes. The v0.14.1
  importer's own tests are deferred with the importer (Q-012).
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

## No downgrade handback

The v0.14.1 handback is retired. `effigy gateway setup --remove` stops the user
daemon and removes the platform setup it created; it no longer exports the three
legacy JSON documents, and nothing re-imports them. The endpoint Store is the
only source of routes and loopback aliases, so downgrading to v0.14.1 does not
restore them, and no exported-authority state, legacy writer lock or import
digest exists to reconcile.
