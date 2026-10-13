# Qualified private docs execution

The first bounded shape is `qa:docs` containing exactly the ordered references
`qa:docs:links`, `qa:docs:paths`, `qa:docs:retired`. Their literal command bodies
are `effigy docs check links`, `effigy docs check paths <declared-path>...`, and
`effigy skill run northstar/retired-concepts`. Declared paths and local links must remain relative and inside the source.
Manifests may include source-local
fragments; task overrides, runtime/default/environment configuration, overlays,
extra arguments, shell operators and other composition refuse.

Run `effigy --json docs qualified resolve` in the source root to obtain
`effigy.docs.qualified.resolution.v1` inside the ordinary command envelope. It
uses authoritative manifest/selector loading and contract 042 named selection.
It records the composed manifests, literal leaves, full source file/directory
inventory, local-link existence observations, selected installed skill assets
and the executing Effigy bytes. Resolution is a declaration, not a resource
grant. This bounded host route requires the approved installed Northstar source;
project-local shadows and other canonical installations refuse.

The Effigy-owned Node module `scripts/qualified-docs-execution.mjs` provides
`prepareQualifiedDocs(controller, packet)`, `preflightQualifiedDocs(controller, packet,
receipt)` and `executeQualifiedDocs(controller, packet, receipt)`. These are
private library calls, not a daemon, task-JSON API or production installer. The
pinned producer program embeds the approved v4 context and uses the reviewed
private launcher/lease constructor. The fixture embeds the bridge code in that
pinned producer program rather than importing mutable worktree code. Preparation authenticates the live original
packet and calls the actual private
Effigy resolver and keeps an in-process, frozen receipt. Serialized receipts
cannot authorize execution. Execution calls the actual reviewed provider's
`lookupDocsInputs` before each leaf, binds retained bytes to the original
selection, executes the existing Docs builtins by literal argv, then uses the
provider's absolute Bun command and environment for the unchanged checker.
There is no execution-time named-skill discovery or explicit-path substitute.

Only the named macOS fixture provider at Nucleus source
`d36cd4af22e78ea2b76ff8d0175e66ddab655841` is supported. The bridge imports its
existing private library artifact; independently qualify source/artifact
correspondence before another handover. Host, Node runtime, Bun/Git/Effigy
installation and same-UID cooperation are accepted infrastructure. They are
not privately verified distribution provenance or physical exclusion.

Each original receipt executes once. Preparation plus leaves uses at most four
top-level process invocations, 15 seconds per invocation and 60 seconds overall,
from preparation through final observations; the original provider deadline
also bounds allocation. The known checker may spawn its
script-controlled Git children; these inherit the owned Bun process group and
are covered by its invocation bound. Output is bounded to 1 MiB per invocation.
Normal child output and nonzero status are retained; first failure stops the
ordered graph. Deadline/overflow termination addresses only the owned live
process group, and no reaped owner is signalled. The provider's original
allocation, byte-verification, namespace and lifetime limits still apply;
preflight uses the same finite lookup ledger. Additional resolver/program reads
are conservatively reserved within the original 512 MiB verification ceiling. No remint or cleanup API is added.

The `qualified-docs` Cargo profile produces a size-bounded private artifact in
ordinary targeted compilation before allocation. The fixture copies it into
its fresh private root. No Cargo, tool installation or build cache operation occurs in
qualified docs execution. The supported source inventory has no symlinks or
outside link reads. Unknown resource/configuration, copied receipt, fake
controller, drift or exhausted original budget yields an explicit unavailable
error, with no later leaf launch.

`effigy.docs.qualified.execution.v1` records actual programs, argv, cwd, output,
exit/signal, timing, selected source, accepted/verified vocabulary and limits.
It is neither a durable local outcome nor native authorization. Existing
consumer Effigy `672548bf`, installed observation `7b26a157`, and this task's
private compiled artifact are distinct. Ordinary consumer APIs retain their
behavior; older binaries do not implement this new resolver. No real Nucleus
project packet, producer eligibility or historical settlement follows from
private fixture success.

Validation selectors are `check:docs:qualified-closure` and
`test:docs:qualified-closure`, plus `fmt:check` and `qa:docs`. The genuine private
proof retains each original allocation and its observations after child exit;
it does not delete or replace an uncertain allocation.
