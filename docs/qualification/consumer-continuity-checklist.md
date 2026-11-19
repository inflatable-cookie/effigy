# Consumer continuity checklist

Tom checks consumers by real use (2026-11-19, ruling 45). This is the
owner-run checklist for the #625 Acow/CBS continuity claim; there is no
request schema, packet, pin set or proof entry.

Run it on a live Acow or CBS checkout after an upgrade, on the backend and
database that consumer actually uses.

1. **Prestate.** Record the database row counts and the named volume list the
   consumer depends on. Keep the checkout clean so a recovery diff is
   attributable.
2. **Bring up and work.** `effigy container up`, then use the site the way the
   users do: load a page that reads the database, save one record, and confirm
   the change is visible after a reload.
3. **Restart.** `effigy container restart`, then confirm the record saved in
   step 2 is still there and the page still serves.
4. **Upgrade in place.** Repeat `effigy container up` with the new binary on
   the same data, then repeat step 2. The saved record survives and new writes
   work.
5. **Recovery.** Run the consumer's documented rollback or restore once on the
   disposable data and confirm the site serves the restored state.
6. **Teardown.** `effigy container down`, then confirm no consumer container
   survives and the named volumes are still present with the same names.

Record the consumer, backend, binary version and the observed result in the
release task. A successful task or a healthy container is not data continuity:
only the observed row and volume facts are.

Interrupted operations, gateway restart, manifest changes and deleted
worktrees stay covered by the release gates in
[release.md](../knowledge/contracts/release.md) and the
[v0.15 roadmap](../knowledge/v015-roadmap.md); this checklist does not
authorize a consumer window or an installation.
