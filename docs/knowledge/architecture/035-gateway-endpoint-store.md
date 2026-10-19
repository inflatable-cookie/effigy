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

Domain claims include the complete route, including managed-listener identity.
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
Once a payload exists, import never runs again.

The payload preserves raw JSON documents. Typed edits replace known fields
while retaining unknown root and entry fields, including pins. Deleting an
entry removes that entry and quarantines its claims; it does not reconstruct
ownership from a runtime scan.

`read` is Observe-only and creates no files or gates. `read_compatible` reads
legacy input only when the store or endpoint payload is absent. An unreadable,
corrupt or unsupported store is Unknown and refuses fallback. Reading old
files does not import or allocate anything.

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
