# Gateway Route-Table Trust Contract

Owner: Platform maintainers

## Current applicability

The gateway's routes live in the transactional endpoint Store described in
[architecture 035](../architecture/035-gateway-endpoint-store.md) and
[runtime kernel contract 052](052-runtime-kernel-contract.md). The Store is the
whole authority: the daemon reads committed revisions, and there is no route
JSON file to trust, import, export or re-verify.

The boundary this contract used to describe — proof that a file an elevated
daemon read had not been tampered with — is now the kernel's own boundary. A
route write goes through the Store's machine owner gate and commits its endpoint
claims with the payload; a reader opens the same Store as the operator and sees
only committed rows. Nothing else can steer the proxy.

## Purpose

State what a route claim is and why there is no separate trust verdict.

## What the daemon trusts

1. **Committed Store rows.** A route, its owner checkout path and its loopback
   alias are read from the endpoint payload of one committed revision. A
   partially written or unreadable Store is `Unknown`: the daemon keeps the
   table it applied last and reports the observation failure. It never falls
   back to a file or to an empty table.
2. **One owner per domain.** The route table holds exactly one owner for a
   domain. A different checkout path takes the domain over only when the
   recorded owner is provably gone: its checkout path no longer exists, or its
   target does not accept a TCP connection within 200 ms. Otherwise the write
   is refused, naming the other checkout and the command that stops it.
3. **Route targets name the publishing port.** A container route's target is
   the host port the running container publishes, read back after `up`; a route
   never names a port Effigy guessed.
4. **A managed listener route names its address, not the process behind it.**
   It carries its owner token, its supervisor and child generations and the
   loopback address handed to the listener; the proxy forwards it like any
   other route, including WebSocket upgrades. A listener is live while that
   port answers, and the supervisor withdraws the route when it stops
   answering. This data never authorizes the daemon to launch or signal a
   process, and a static `target_host` route stays external route intent.

## Gateway lifetime

1. **The machine gateway outlives its routes.** Withdrawing the last route
   (`container down`, retire, handover) does not stop the machine gateway. It
   stays up until `effigy gateway down` or a host restart stops it, so the next
   `container up` finds a running gateway. Its route reload arms no idle stop.
2. **A private gateway idles out.** A private state root's gateway stops once
   its route table has stayed empty for five minutes. A route that returns in
   that window cancels the pending stop.
3. **Every idle stop is recorded.** The stop writes one `gateway_stop` event to
   the gateway's operator log before the clean exit, with the cause
   `route table empty for 300 s`.

## What changed from v0.14

The file provenance marker, the owner/group permission check and the legacy
import are retired, along with the v0.14.1 export handback that made the JSON
files authoritative again. `effigy gateway setup --remove` stops the daemon and
removes the platform setup it created; it no longer exports routes, loopback
addresses or port pins.
