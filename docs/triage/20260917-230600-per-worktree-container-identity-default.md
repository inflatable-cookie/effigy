# Make per-worktree container identity the default

Raised: 2026-09-17. From the Acowtancy workspace (Chatterbox), operator-directed. Companion to
`host-database-structural-check.md` filed the same day.

## Request

Derive the generated-compose **project identity** from the checkout path natively — a per-checkout
suffix — so an isolated container identity is the default for any worktree, rather than something
each repository wires up itself. Call teardown automatically at worktree retirement where the
runtime knows about it, and clean up the associated record.

## What happened without it

In Acowtancy the stack was already per-project in every respect that matters: per-project networks,
auto-allocated published loopback ports, project-prefixed volumes, and the declared hostname
pointed at the project-local postgres through per-container host entries. The immediate
collision was the global compose project name (`acowtancy-dev`, rendered from the bundle block in
`infra/workspace.toml`). One identity across many worktrees produced:

- **One lane's container-routed checks executing in a sibling worktree's source tree.** This is
  the serious one: it is not resource contention, it is the evidence being about the wrong
  checkout. A lane can report green on code it did not write, or red on code it did not break.
  Two lanes merged that day on the strength of runs that may have executed elsewhere.
- **Container legs killed under a sibling lane's cargo work** (exit 137), because one project
  means one set of containers and one build volume.
- **A lane blocked outright**, unable to produce its own acceptance evidence for hours.

## What the repository had to do instead

Each repo currently has to wire this up itself: a fresh-session record with a deterministic id
derived from the worktree path (`wt-<basename>-<hash8>`), attached through `worktree.setup` and
torn down through `worktree.teardown` with `effigy bootstrap teardown --yes` (reset plus
`--wipe-data`), with outcome verification, a retry, and a degraded report if containers survive.
It works — cold, unattended, no host database, no manual step, and the retired stack leaves no
containers or volumes, proven by `nerdctl ps -a` and `container volume list --global --orphans`
both empty afterwards — but it is per-repo work and every repo that does not do it keeps the
collision.

## Shape that would help

1. Derive the project suffix from the checkout path natively, so a linked worktree is isolated by
   default and a repository needs no hook to get correctness.
2. Run teardown at worktree retirement where the runtime is told about it, and delete the record.
3. Keep the record mechanism for repositories that need an explicit identity; the request is to
   make the default safe, not to remove the override.
4. Document the convention, because a project that shares an identity across checkouts has no
   signal that anything is wrong until evidence turns out to be about someone else's tree.

## Related observation worth a clearer message

A repository can make every unattended `container up` demand a vault passphrase on a TTY by
targeting a *required* secret at containers. In Acowtancy that was the cold-start failure that
pushed lanes onto the shared stack and then onto host databases — a configuration mistake with a
long causal tail. It is ours to fix and now is fixed, but the failure deserves a message that
names the consequence rather than only the missing passphrase, because "cannot start unattended"
reads as an inconvenience until you watch what lanes do instead.

## 2026-09-26 gateway hostname collision

Source: Acowtancy Chatterbox, at Tom's request, from g05.225 / PR #368. This
extends the same worktree-isolation problem; it is planning intake, not an
approved implementation lane. Acowtancy is serializing browser lanes meanwhile.

The Acowtancy Queue worktree already had a distinct compose project. Dairy and
Farmyard initially returned 200, but during a signed-in Playwright walkthrough
the fixed `*.acowtancy.test` gateway routes moved to another worktree stack.
Mailpit OTP and the browser session then failed. A later `container up` attempt
recycled the workspace and closed ports 41001/41002. The evidence is
`docs/notes/2026-09/25-235300-g05-225-dairy-signed-in-walkthroughs.md` on that
PR branch. The separate workspace-recycle cause still needs inspection.

Known mechanism in Effigy: `RouteTable` has one entry per domain;
`registration::register_route` upserts it, including across different project
paths. Container registration supplies the worktree root as route owner.
`deregister_gateway_routes_for_container` removes declared domains by name,
and `registration::deregister_route` does not compare the current owner. A
sibling can therefore replace a route, and teardown can remove the sibling's
current route. The Underlay bundle renders DNS domains from its `host` input;
Acowtancy currently supplies the same `acowtancy.test` host to every worktree.
Distinct compose projects alone cannot isolate those public names.

Approved planning split; implementation details remain open:

1. Effigy must reject a live foreign-owned domain claim with an actionable
   collision report, preserve the existing route, and remove routes only when
   ownership still matches. Define how stale owners are detected, how route
   table writes are serialized, and how TLS certificates and TCP aliases are
   handled before marking this ready.
2. A stack needs a stable, discoverable set of unique HTTP hostnames, with the
   main checkout's existing names preserved for ordinary development. Decide
   whether derivation belongs in Effigy, the bundle input, or consumer
   worktree setup. It must cover front, admin, API, Mailpit, S3, and any other
   gateway routes as one set; no fixed helper hostname may remain shared.
3. Acowtancy must feed the effective names through application public URLs,
   allowed origins, cookie scope, WebAuthn RP/origin, dev-admin session and
   Playwright/Mailpit selectors. A bundle-only domain change will not make a
   signed-in session work because those values currently name
   `acowtancy.test`. Verify two simultaneous worktree stacks with distinct
   browser sessions, both healthy throughout startup, selector execution,
   and teardown of one stack.

Tom approved the direction on 2026-09-26: an ephemeral runtime scope per worker
worktree; Effigy-owned resource identity, gateway ownership, effective host
discovery and cleanup; Queue-owned retirement and retry; distinct worker base
domains while the main checkout keeps its current names. The
[plan](../plan.md) owns the priority. The concrete Effigy/Queue interface and
the observed workspace recycle and fixed published ports still need inspection
in the implementation tasks. Governing refs:
`docs/knowledge/contracts/005-container-runtime-contract.md`,
`docs/knowledge/contracts/009-execution-surface-convergence.md`,
`docs/guides/063-container-system-guide.md`, and this note's original
worktree-identity request.
