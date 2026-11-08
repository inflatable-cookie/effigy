# 006 - Compose Backend Compatibility

This contract defines the local compose-backend capability model that Effigy
expects for container-backed execution.

It exists to stop Docker Compose and Colima + `nerdctl compose` behavior from
being treated as accidental equals.

## Purpose

Effigy does not support "any backend that happens to parse compose YAML".

It supports a bounded local runtime contract. Some parts of that contract must
come directly from the backend. Other parts may be repaired by Effigy when a
supported backend is known to fall short.

This document names that boundary.

## Supported backend posture

Current supported posture:

- Docker Compose-compatible local runtime
- Colima + `nerdctl compose` compatibility path

The second path is supported because Effigy owns fallback behavior for known
gaps, not because `nerdctl compose` is assumed to be behavior-identical to
Docker Compose.

## Capability classes

Effigy should classify backend-sensitive runtime behavior into three buckets:

- backend-required
- Effigy-repaired
- unsupported

### Backend-required

These must work from the backend/runtime itself for Effigy to function:

- compose file parsing and multi-file merge
- basic service bring-up and teardown
- `compose exec` against the selected service
- published-port reporting good enough for runtime inspection
- bind-mounted repo workspace visibility inside the execution target

If these fail, Effigy should stop rather than layering more repair logic on
top.

### Effigy-repaired

These may be repaired by Effigy on supported backend paths when the backend
does not provide them directly:

- missing host bind-mount directory creation before `compose up`
- sibling-service bring-up after a partial or failed runtime start
- a declared service Compose left stopped, brought back by the next apply
- container-local TCP alias visibility inside Effigy-owned execution targets
- generated Compose values that need a literal `$` for the container shell
  (`$code` written as `$$code` so Compose interpolation does not blank it)
- exclusive owned disposable rust/cargo/target named volumes prepared for
  the resolved workspace uid/gid, with actual non-root write/lock
  verification before children launch; named volumes shared by two or more
  compose or managed services are not mutated

These are legitimate product behaviors as long as:

- the repair is derived from the same effective model as the non-repaired path
- runtime repairs run through shared runtime-prep ownership
- the repaired behavior is explicitly documented and tested

### Unsupported

Effigy does not currently promise:

- arbitrary backend parity beyond the supported local paths
- alias visibility inside every compose service regardless of whether Effigy
  dispatches work there
- zero-repair semantics on the Colima + `nerdctl compose` path
- recursive chown of host bind sources, shared/external volumes, or
  arbitrary checkout trees to make rust `target`/cargo writable

Those may widen later, but they are not current contract.

## Current capability matrix

### Host bind-mount preparation

Expected product guarantee:

- repo-owned bind mounts required for runtime bring-up exist before the user
  command dispatches

Backend status:

- Docker Compose commonly auto-creates missing host directories
- `nerdctl compose` may not

Effigy ownership:

- shared runtime-prep now creates repo-owned directory-style bind mounts before
  runtime prep continues

Target compatibility case:

- `bind_mount_host_dirs_are_prepared_before_exec_runtime`

### Sibling-service bring-up

Expected product guarantee:

- if the primary service is considered runnable, required sibling services are
  also brought online before container-backed exec or handoff

Backend status:

- partial `compose up` failure may leave sibling services in `Created` or
  otherwise unavailable state

Effigy ownership:

- shared runtime-prep performs an idempotent `compose up -d` before readiness
  and alias reconciliation

Target compatibility case:

- `runtime_prep_recovers_missing_sibling_services_before_dispatch`


### Workspace artifact builder execution

Expected product guarantee:

- an already running, strictly owned Linux builder can build the current bound
  source of this checkout, and the artifact identity it reports is fresh

Backend status:

- Compose and nerdctl can report a container as running while its mounts or
  project membership have moved; a build must not trust a name

Effigy ownership:

- exec into an owned container resolves and reverifies an inspected binding:
  full container ID, project, declared service, non-oneoff/running status,
  canonical checkout and exact scope, then a native ID-targeted exec. No
  service-name cache authorizes those execs.
- a missing, foreign or replaced binding refuses before exec rather than being
  followed into the successor. Legacy dependent-service recovery keeps its
  declared dependency selection.
- argv, workdir, user, environment, capture/TTY and stdin-file behavior survive
  native transport. Unsupported exec options refuse rather than being dropped.
- inspections and remote exec are separate observations, not an atomic remote
  transaction. A replacement after final inspect cannot redirect ID-targeted
  exec into the successor; the old ID can fail. In-place label/state changes
  after the final observation and remote cancellation are not guaranteed.
- the existing deadline-aware diagnosis/permission transport retains its
  observation sequencing and budgets. Doctor does not gain a workload-readiness
  prerequisite or restart authority from this change.

Target compatibility case:

- `direct_builder_activation_tests` exercises direct and generated no-port,
  no-route activation through policy admission, readiness, registration and
  native exec, including replacement and sibling-state preservation.

Direct Compose labels come from the supported ownership override, never from a
hardcoded checkout in the builder YAML. The maintained
`workspace:linux:artifact` task uses the builder-only
`container::workspace_artifact_build(args)` operation. An already running owned
builder can build current bound source even after the source changed. This is
execution-only recovery: it claims no Compose activation, starts nothing and
stops nothing.

The operation serializes callers with the existing lock machinery, lists the
selected project including stopped containers, requires exactly one builder,
and inspects its full ID, project/service, explicit checkout and exact scope.
It requires one writable bind of this canonical checkout at `/workspace`, no
shadowing mounts below it, and no published ports. It reverifies that identity
and bind at the native ID-targeted exec boundary under the original observation
deadline. Unknown, foreign, missing, stopped, malformed or changing identities
refuse without adoption, restart, teardown, marker removal or publication. A
retained admission with no container refuses; a first activation with neither
container nor admission uses the ordinary container manager. Existing runtime
drift checks remain in force. No VM recovery or broad cleanup is added.

### The builder's source when the profile cannot reach the checkout

A VM sees only the host paths its Colima profile shares. A development checkout
outside every shared mount therefore has no source a bind can carry: the
container runtime creates an empty directory for the inaccessible path and the
build compiles nothing. The maintained `linux-release` environment declares
`workspace_source = "staged"` (effigy#715), and the producer resolves the source
from the profile's own configuration, which it reads and never writes:

- A writable profile mount that contains the checkout keeps the checkout's own
  path as the source. Nothing is staged and nothing changes.
- Otherwise the only existing writable profile mount hosts one owned child for
  this checkout: `<mount>/effigy-builder-source/<project>/source`, bound at
  `/workspace`, with its binding record beside it. An absent, unreadable or
  ambiguous profile config is refused by name rather than guessed at, and no
  mount, profile setting or VM is ever added or changed.

The owned child is never guessed at. Before anything below the mount is
created, written or removed, the producer proves ownership: either the child's
`stage.json` record names this canonical checkout, its scope, project and
profile, or the child does not exist yet (or is an empty directory) and this
producer creates it. A directory at the derived path is not proof, so a child
that already holds entries without this checkout's record refuses whole —
nothing in it is written, cleaned, adopted or published — and so does a record
belonging to another checkout, scope, project or profile. A pre-existing link or
file in the owned chain is refused rather than followed, unlinked or replaced.
The record is written before the first file lands, so an interrupted run leaves
this checkout's own record and a later run completes it; the same record carries
the Compose render generation staged beside the source, and render staging
shares the one ownership rule.

The staged closure is the checkout's tracked working tree — every path
`git ls-files` reports, read from the working tree so uncommitted development
edits are part of the build — with a digest per file and one digest over the
ordered inventory. It is never a recursive copy: Effigy's own `.effigy` state,
local secret files, ignored and untracked paths and submodule contents are
excluded by rule and recorded as excluded; submodule pins are recorded as
evidence, and a symlink or an unreadable tracked path refuses with its concrete
cause. `stage.json` records the canonical checkout, its scope, project and
profile, the closure digest, the caller's `HEAD` and every file digest, so the
producer can name exactly which caller inputs it snapshotted. A record naming
another checkout, scope or project is refused before any write.

A profile that cannot reach the checkout also cannot reach the Compose inputs:
the render lives under the checkout's `.effigy/state/objects` and a
`colima nerdctl -- compose` client reads `-f` files and the project directory
from inside the VM. The same owned child therefore also carries the published
render closure at `<stage>/render/<render-digest>`, byte-for-byte, and every
activation and exec of that environment names those files. The render digest is
unchanged, so admission, records and generation reuse are unaffected; one
generation is kept at a time, and a superseded generation is removed as a
directory or a link, never through one.

Ordinary incremental builds keep one staged directory, so the caller's project
identity and the builder's cargo target stay stable, while changed source is
refreshed in place and source that is no longer tracked is removed. Staging and
cleanup treat a link as an entry, never as a path to follow: a link in the
staged tree is unlinked, its target is untouched, and a tracked directory that
a stale alias occupies becomes the real directory the closure names. The owned
chain is created one component at a time from the profile's mount — which must
be a real directory of its own — so an alias planted between the mount and the
owned child is unlinked rather than followed, and the finished chain's canonical
path must still be inside that mount. Nothing outside the owned child is
created, replaced or removed by staging. Before
compiling, the build asks the container, through its verified immutable ID,
whether `/workspace` carries the whole closure: the inventory inside the bound
tree must carry the digest this build recorded, every listed file must match
its digest, and the source-file count must equal the recorded count. An empty
auto-created VM directory, another source, a changed or missing file and an
unaccounted extra file all refuse by name instead of becoming a build error.
Only bytes the build's own producer receipt blesses for the requested target
and the host's version are copied back into the checkout's artifact directory,
atomically and in the order the existing freshness checks read them. Before any
byte is copied back, the whole binding is re-proved: the caller's current
tracked closure must still hash to the digest that was compiled and the staged
tree must still carry every recorded digest. A caller edit, a concurrent writer
or a changed staged tree during a long build therefore publishes nothing and
says which side moved.

A builder retained from the pre-staging declaration binds the checkout's own
unreachable path. The maintained activation replaces exactly that checkout's
container with the staged bind once; the builder is never adopted, exec'd,
hand-removed or relabelled, and a builder bound to another source still refuses.

The builder remains detached after success or failure, preserving its cache
volumes and completed artifacts for retry. Its original image may be unable to
compile newer source; a failed build then remains a failure and grants no
artifact identity. The maintained task and the host Prepare both keep their
receipt, target, digest and freshness checks. The host Prepare runs this task
for a development host, once per stale artifact, before publication starts;
publication never runs it. A development artifact is current only while the
host's install identity matches the one it was bound to, so a same-version
incremental rebuild makes it stale. Source selection and release-cache
eligibility are specified in [052 §8.9](052-runtime-kernel-contract.md#89-binary-provisioning).

The maintained Linux builder uses the stable private Compose project base
`effigy-artifact-builder-v1` with a checkout-scoped suffix: the ordinary
worktree/ephemeral-clone generation token where the checkout is runtime-scoped,
and a canonical-checkout digest (`-co-`) for an unmarked primary checkout. The
builder therefore owns exactly one project per exact source checkout, including
primary checkouts, and two checkouts on one host never query, reuse or collide
with each other's builder. The suffix is not a new namespace per source
version, upgrade or retry: the same checkout keeps one identity across ordinary
incremental builds, and its builder is reused by its strictly verified
immutable ID. This is a one-time namespace migration from `effigy-linux-release`
and from the previously unscoped primary-checkout spelling, not a per-build
change. Names never prove ownership: the full-ID, project, service, oneoff,
running, checkout/scope, working-directory and writable-source-bind checks all
stay in force, so a same-spelled impostor or a builder of another checkout's
source is refused rather than adopted. Artifact paths and the task interface
are unchanged.

A refused builder observation names the concrete cause (malformed JSON, a wrong
record count, a short id, a missing name or `State.Status`, an ownership or
bind mismatch) and never presents a validation refusal as a process exit code
or a signal; only a command that actually ended reports its termination. The
diagnostic stays bounded to identity facts and is secret-safe.

Legacy `effigy-linux-release` containers and their admission records, cache
volumes and other retained data are left untouched, whether absent, labelled,
or ambiguous. A pre-label nerdctl container lacks the checkout/scope and oneoff
labels; a named `compose run` can reproduce its native name and metadata, and
the old admission does not record its native ID. That evidence cannot prove a
non-oneoff association. Effigy therefore never adopts, executes in, relabels,
stops or recreates that old builder during an explicit
`workspace:linux:artifact` build.
No user reset or cleanup is required. Later operator cleanup is separate and
optional. The new project uses separate named caches, so the first build may
need image/dependency downloads and additional disk space; old caches remain.

A collision in the new project is not an empty project: missing, foreign,
mixed or contradictory ownership evidence still refuses before activation or
exec. No generic ownership check or ordinary application drift guard is
relaxed. Two checkouts in the same old namespace are separated by the
checkout-scoped suffix before any list, so neither builder's resources are
inspected, executed in, stopped, relabelled, mounted into or adopted by the
other. Private transport fixtures exercise the shipped declaration, the
native legacy shape and named-oneoff counterexample, the ordinary and bounded
handoffs, repeat upgrades, another checkout's retained builder in the old
project, and failures; they do not attest live consumer
recovery.

### Container-local TCP alias visibility

Expected product guarantee:

- Effigy-owned execution targets can resolve documented TCP backing-service
  aliases such as `mysql.<site>.legacy.test`

Backend status:

- compose-network alias materialization is not reliable enough on the
  supported Colima + `nerdctl compose` path

Effigy ownership:

- shared runtime-prep reconciles container-local TCP aliases inside the
  execution target from the same effective alias model used by host-visible
  service routing

Target compatibility case:

- `runtime_prep_reconciles_container_local_tcp_aliases`

### Generated Compose literal dollars

Expected product guarantee:

- generated Compose values keep container-shell `$` after Compose interpolation
  so health commands such as the Nginx wget `0`/`8` check still see `$code`

Backend status:

- Docker Compose and `nerdctl compose` interpolate `$VAR` in YAML values,
  warn when the variable is unset, and substitute an empty string

Effigy ownership:

- compose assembly escapes `$` in generated values (`$code` → `$$code`,
  already-escaped `$$` left alone) before writing the Compose file

Target compatibility case:

- `nginx_healthcheck_survives_compose_interpolation_and_treats_http_responses_as_ready`

### VM Compose interpolation

Expected product guarantee:

- a rendered generation carries no Effigy interpolation value, and a plan value
  the VM-hosted backend must interpolate reaches Compose on that backend

Backend status:

- `colima nerdctl` runs Compose inside the VM, so the host process environment
  (including any value the plan forwards for `secrets.delivery =
  "compose-env"`) never reaches Compose interpolation
- `nerdctl compose` accepts exactly one `--env-file` and resolves it against
  paths the VM can read; host paths under the user's home are mounted there
- `nerdctl compose` also hands that file to every service's `nerdctl run`, so
  whatever it holds lands in each container's environment

Effigy ownership:

- generated assets are named relatively and resolved by Compose against the
  project directory, so the generation root is never interpolated and no
  container receives it
- a catalog Dockerfile is rendered as `dockerfile_inline` from the generation's
  own content, because Compose resolves `build.dockerfile` against the build
  context (the checkout) rather than the project directory. Every `$` in that
  content is doubled, Compose's literal dollar, because interpolation runs over
  every string in the file and would otherwise blank the Dockerfile's own
  `${…}` and shell `$VAR`. Both backends read it: `nerdctl compose` 2.3.1 and
  Docker Compose v2.17 and later
- only the plan's forwarded secret values travel in the file; the shared
  invocation transform in `policy::render` (reached by attached, detached,
  `config`, `ps`, `logs`, `restart` and shared-service invocations) writes them
  to a per-invocation file `<effigy home>/compose-env/compose-<32 hex>.env`
  and inserts `--env-file` after the Compose subcommand
- every invocation that needs interpolation writes its own file: the name is
  fresh random bytes, never derived from the contents, so nothing is reused.
  The file is created `O_EXCL|O_NOFOLLOW` with mode 0600 and a single link,
  inside `compose-env` verified through descriptor-relative no-follow opens as
  the operator's own 0700 directory (an owned directory with other bits is
  repaired to 0700; a foreign or symlinked one is refused). The value is written
  only after the invocation holds the file's shared lock and the name still
  names that inode
- direct-child launchers refuse secret-bearing VM Compose before spawn pending
  retained kernel custody. The file owner still cleans up an invocation that
  never started; removal first
  moves the name to a quarantine name by a no-replace rename (`renameatx_np`
  with `RENAME_EXCL` on macOS, `renameat2` with `RENAME_NOREPLACE` on Linux) and
  unlinks only the inode it created. Before rename, it checks the original
  name under the directory lock and leaves a mismatched entry in place. It
  rechecks the held and named file's identity, owner, regular-file type,
  single-link count and private mode; a drifted file is retained as Unknown.
  An occupied quarantine name is refused with nothing moved. A platform without a no-replace rename refuses removal before
  anything moves, with no replacing fallback. A foreign entry moved after the
  initial check is restored without overwrite and reported as unknown; a name already gone, or a
  quarantine that vanishes after the move, is never reported as confirmed
  removal. A failed write whose removal also fails returns the write error with
  the cleanup state named as unknown
- before each creation a sweep removes nothing. It counts `compose-` files,
  quarantine names left by an interrupted removal, and legacy `<16 hex>.env`
  files, and reports the count once per process; each stays in place. A free
  file lock does not prove that the Compose group behind a file has closed, so
  removal waits for a durable binding between file, invocation and group. The
  layout never follows a symlink or removes a hard-linked, non-regular,
  foreign-owned or unrecognised name
- a crash leaves its `compose-` file in place until an operator or a later
  durable-binding sweep removes it; it does not claim that every secret copy is
  erased
- threat model, ruled by Tom on 2026-11-01: cooperative directory locking for
  every Effigy writer and sweep. A non-cooperating same-user process that swaps
  an entry after its stat can cause deletion of a foreign entry before any check
  runs, and that deletion is not guaranteed to be detected. A no-replace rename
  does not close that stat-to-unlink window. Effigy locks the verified directory
  across creation, sweep and removal; the per-file lock remains held throughout
  the file owner’s lifetime. The directory lock does not prove group closure
- Tom’s ruling for #534: every secret-bearing VM Compose invocation moves to a
  retained kernel Run; if it cannot be admitted, refuse before spawn. Resident
  Env custody and production Discard remain refused until separately qualified.
- the live owner can prepare duplicates of its exact locked file and directory
  descriptions, original name and current original/quarantine phase for a
  custody handoff. Preparation disables drop cleanup and refuses ordinary
  removal; a free lock does not authorize removal. The file lock follows the
  open file description until its last duplicate closes, rather than being
  explicitly unlocked when the original owner drops. A failed preparation
  retains the original owner and leaves its entry for review
- limits of this slice: explicit removal borrows the file owner, retaining its
  descriptor and lock after failure, and drop does not retry a failed removal.
  The live owner retains its quarantine name across a failed cleanup and can
  retry that exact phase. The phase is not durable: there is no durable attempt
  or group binding, no production helper handoff, and no caller-drop Unknown
  lifecycle under a Run.
  Discard stays refused. These belong to the retained-lifecycle follow-up, #534,
  which depends on #532 and #533
- a value both readers can take literally stays unquoted, and anything
  else is double quoted with `\`, `"`, newlines and `$` escaped so Compose
  substitutes it literally and the container's copy keeps its value; a
  non-identifier name, a non-UTF-8 value or a carriage return refuses before
  spawning
- the file is outside the immutable closure, so the generation digest never
  depends on the state-home path; an activation that this failure interrupted
  is reconciled by a plain `effigy container up`
- Compose's default `.env` lookup is the first Compose file's directory, which
  inside a generation is the object directory and never holds one, so naming
  this file drops no interpolation the invocation had
- deleting the directory by hand is harmless to a finished invocation; a live
  one that has not yet read its file fails with its own error

Target compatibility cases:

- `a_vm_hosted_invocation_names_a_file_the_vm_can_read`
- `the_same_values_name_the_same_file_and_new_values_add_one`
- `a_render_generation_matches_across_state_home_paths`
- one real-Colima fixture whose Compose stack mounts a generated closure file
  through a relative input

## Validation direction

Compatibility coverage should prefer small targeted tests over one broad smoke
suite.

The first useful coverage set is:

- one proof that standard routed exec and workspace handoff both pass through
  the shared runtime-prep path
- one proof that bind-mount preparation happens before exec runtime dispatch
- one proof that workspace rust ownership prep classifies named volumes as
  owned-disposable and bind rust paths as verify-only, then observes
  non-root write/lock rather than mount-root owner heuristics
- one proof that doctor ownership inspects share the remaining doctor budget
  and report timeout as unavailable rather than clean
- one proof that container-local alias reconciliation runs on the supported
  Colima-sensitive path

Where live backend behavior is hard to reproduce in unit tests, coverage may
combine:

- unit tests for decision/ordering
- narrow live-repro tests for backend-sensitive behavior

## Drift triggers

Update this contract when Effigy changes:

- the supported local backend set
- which capability gaps are repaired by shared runtime prep
- the alias guarantee scope for Effigy-owned execution targets
- the expected compatibility cases used to prove backend-sensitive behavior
- how generated Compose values escape `$` for backend interpolation
- how a VM-hosted backend receives its Compose interpolation environment

## Detached subprocess evidence boundary

Compose detach alone does not isolate stdout/stderr inheritance. The launch
adapter sends a detached backend's stdout and stderr to one regular file in the
checkout's retained runtime record (`.effigy/runtime/logs/<project>.compose.log`)
instead of an Effigy-owned pipe or a null sink: a regular file has no reader
lifetime and stays writable after the CLI exits. After the backend exits the
record keeps only its last 64 KiB behind a visible truncation marker, and a
failure names the file and prints its last 4 KiB. That tail is operator-facing
error text; detached `--json` failure details keep their fixed structural
vocabulary and never carry raw backend bytes. A detached `up` whose record
cannot be created refuses before spawning rather than reverting to a
throwaway sink. Finite Unix inventory capture uses nonblocking reads with
bounded retention and no reader threads. Success requires client exit and both
EOFs; overflow or unverified settlement refuses. Cancellation owns only the
client process group and positively reaps its direct child. Remote engine
cancellation and historical SIGPIPE sender/pipe provenance remain unverified.
The private exit-141 controls (`test:containers:exit141`, effigy#300) add what
this checkout does own: a detached launch gives the client one regular file on
stdout and stderr, so a writer that outlives the client never loses a reader;
a cancelled capture signals only the client's group, and a writer that
inherited the capture pipe dies of SIGPIPE only when Effigy gives up its own
read end. Those controls do not identify the original nerdctl reader, so the
papercut stays open for that observation. Non-Unix bounded pipe capture
requires a missing nonblocking adapter and returns an explicit unsupported
error.

The private CLI matrix runs three overlapping scoped stacks through production
adapters, maintains persistent exec sentinel connections, tests warm
reconciliation and full-ID/port replacement, and exercises noninteractive
retirement ownership refusals. Native engine semantics remain separate from
these private command-transport controls.
