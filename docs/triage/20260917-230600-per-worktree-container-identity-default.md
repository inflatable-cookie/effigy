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
pointed at the project-local postgres through per-container host entries. The **only** global
value was the compose project name (`acowtancy-dev`, rendered from the bundle block in
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
