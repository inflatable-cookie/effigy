# 054 - Release Platform Contract

Status: accepted by Tom on 2026-10-22 ("I'll go with your recommendation … release draft questions"). §2, §4 and §5, and the Run of §3, §6 and §7, are implemented for `tag` and `task` targets, including `check`, `verify`, approval and decision checkpoints and hosted waits; §4.4's Effigy release is implemented (§13); native providers are v0.16.
It supersedes the generic mechanics in guide 051 when implemented. Effigy's own release
policy stays in [release.md](release.md).

Owner: release platform (Queue lane `v015-rebuild`, then a release lane)
Kernel: [052](052-runtime-kernel-contract.md), the delivery row in §10 and
failure-model row 22 (`Run` with checkpoints; pushes are `Irreversible` and
reconciled, never replayed)
Ruling: [questions](../questions.md) Q-009, the release platform bullet
Evidence: the planner's portfolio inventory (2026-10, `release-audits/effigy/release-platform-inventory-2026-10/`). These cover 38 project roots and 101 legacy roots.

Q-010 process custody: Git operations, hosted evidence lookups and stage tasks receive the admitted
checkout run explicitly. Before launch, the run registers its sentinel and each
process receipt; closure and bounded output are verified before a stage consumes
the result. Adapter exit codes are interpreted by the stage (a missing tag is
an observed absence), while unknown closure refuses completion. Workspace guests
cannot mutate release state through the host-only release API.

## 1. Purpose

Every project releases through one Effigy protocol. A release is one durable
Run that moves through fixed stages and records a checkpoint after each. It
publishes exactly the candidate it certified, and it ends with a receipt. The
protocol fits the release workflows projects already have; it doesn't replace
them with Effigy's own.

Tom: "I want a consolidated release platform in effigy that all projects can
adhere to … so long as we can fit existing workflows to it".

**v0.15 settles the shape:** the config, stages, Run, receipts, CLI, and the
`tag` and `task` targets. **v0.16 adds native publish providers** (GitHub
Release, npm, Homebrew, OCI, Packagist and others) behind the same target
interface.

## 2. Concepts

| Term | Meaning |
| --- | --- |
| **Release** | One Run of the pipeline for one project at one source commit. |
| **Identity** | One thing that gets a version: a crate, an npm package, an app, a site build or a content set. A release has one or more. |
| **Candidate** | The frozen output of prepare: the source commit, every identity's resolved version, and the artifacts built from them, each with a digest. |
| **Target** | One destination for publish, such as a git tag or a project task that uploads. A release has zero or more. |
| **Receipt** | The durable record: inputs, checkpoints, gate results, artifact digests, approvals, destination IDs and verification results. |

## 3. Stages

```
validate → [pre-gates] → prepare → certify → publish → verify → closeout
```

Each stage boundary is a kernel checkpoint. A failed or interrupted Run
resumes from its last checkpoint with the same source commit, versions and
artifacts. It never silently restarts from the beginning.

| Stage | Does | Mutates | Fails closed when |
| --- | --- | --- | --- |
| **validate** | Reads config, changelog, version sources and policy; resolves each identity's next version; checks a clean tree on the expected branch. | Nothing (R2: a pure read). | Config invalid, tree dirty, changelog doesn't validate, or a tag already exists. |
| **pre-gates** (optional) | Runs `gates` with `stage = "pre"`, for checks that must pass before any version edit. | Whatever the gate tasks do. | Any gate fails. |
| **prepare** | Writes versions to each identity's files, syncs lockfiles, renders notes, then commits. The commit is the candidate source. | The working tree, plus one commit. Not pushed. | A write or sync fails; the Run rolls back to the pre-prepare tree. |
| **certify** | Runs `gates` with `stage = "certify"` against the prepared commit; runs `build` tasks; records each artifact's path and digest. | Builds artifacts into the Run's candidate directory. | Any gate or build fails, or an artifact is missing. |
| **publish** | For each target, in declared order: checks whether it's already published (idempotency), then publishes the recorded candidate. | External destinations. Every publish is an `Irreversible` effect. | A digest mismatch (a refused rebuild), a target failure, or an unknown outcome. Unknown is never retried blind; see §7. |
| **verify** | Runs each target's `verify` task and any release-level `verify` tasks against what was published. | Nothing external. | A verification fails. The release is published but marked unverified; see §7. |
| **closeout** | Pushes the release commit, if not already done by a target; writes the receipt; prints the summary. | The receipt. | n/a |

**Approval and hosted waits.** Any stage boundary can be declared an approval
checkpoint (`approve = ["publish"]`, the default for releases with a non-`tag` target; §10). The Run stops there with status
`awaiting_approval`, and `effigy release approve <run>` continues it,
recording who approved and when. A gate may also be a **hosted wait**, such as
exact-commit CI evidence on GitHub. A Run waiting on it is resumable and
doesn't hold a process open. The current `reuse-hosted-evidence` gate option
becomes this mechanism.

**A hosted wait binds one run, never a sibling.** The wait opens a dispatch
window the first time the gate needs evidence ("no run created before this
instant is this release's") and records it in the Run's record. Exactly one run
of the configured workflow and event, for the exact commit and repository, may
be created inside that window: that run becomes the **bound run**, and its id
is journalled on the gate and in the wait entry. From then on the wait
re-checks the bound run only, so a newer dispatch — another release, an
operator's re-dispatch, a re-run — can never substitute for it. Zero matches
wait (`awaiting-hosted-evidence`), and several matches inside one window fail
the gate closed with the candidates named. A one-shot `effigy release gates`
check has no record to open a window in, so it accepts only a commit's single
matching run and refuses the same way.

## 4. Configuration

The current single-identity `[release]` keys stay valid as a shorthand for a
one-identity release, so the 11 projects already using `[release]` keep
working unchanged. New keys add identities, targets and stages.

### 4.1 Single identity (today's shape, unchanged)

```toml
[release]
version-file = "Cargo.toml"
version-path = "workspace.package.version"
changelog = "CHANGELOG.md"
sync-files = ["Cargo.lock"]
tag-format = "v{version}"
gates = { test = "qa", build = "build:release" }
```

This means one identity named after the repo, gates in `certify`, one `tag`
target, and no artifacts.

### 4.2 Several identities, artifacts and targets (for example Poodle or Longhorn)

```toml
[release]
changelog = "CHANGELOG.md"
approve = ["publish"]

[release.identities.core]
version-file = "packages/core/package.json"
tag-format = "core-v{version}"

[release.identities.react]
version-file = "packages/react/package.json"
tag-format = "react-v{version}"
follows = "core"            # bump together with core; omit for independent bumps

[release.gates]
qa = { task = "qa", stage = "pre" }
pack = { task = "release:pack", stage = "certify" }

[release.artifacts]
core = { task = "release:pack:core", path = "dist/core-*.tgz" }
react = { task = "release:pack:react", path = "dist/react-*.tgz" }

[release.targets.tags]
kind = "tag"                # one annotated tag per identity, pushed

[release.targets.npm]
kind = "task"
task = "release:publish:npm"   # receives the candidate; see §6
check = "release:check:npm"    # idempotency probe; see §7
verify = "release:verify:npm"
```

### 4.3 No version bump: a digest or source-only release (for example Contact Patch or Paseo Queue)

```toml
[release.identities.site]
kind = "digest"             # identity = digest of the built artifact; no version file
[release.artifacts]
site = { task = "build", path = "dist/" }
[release.targets.production]
kind = "task"
task = "deploy:production"
verify = "deploy:health"
```

`kind` is one of these:
- `semver` (the default): a version file, with bumps from the changelog;
- `digest`: the artifact digest is the identity;
- `source`: the commit SHA is the identity;
- `task`: a project task prints the identity, for example Silo's UUIDv7 suite release ID.

Changelog bump rules apply only to `semver`.

### 4.4 Effigy itself

Effigy's `[release]` becomes one `semver` identity. It has `certify` gates
(ci, format, test, qa, build, smoke, metadata), artifacts from the release
binary build, and two targets: `tag`, and a `task` target that dispatches
`release-binaries.yml` and records the GitHub Release and Homebrew tap
results. The Effigy-only checks (`verify-install`, `check-binary`, `proof`,
GLIBC and Homebrew checks) become Effigy repo tasks, wired as gates or as
`verify`.

## 5. CLI

| Command | Does |
| --- | --- |
| `effigy release status [--json]` | Shows the next versions, the active or last Run, its stage and what it's waiting on. |
| `effigy release --plan` | The whole pipeline with no mutation: versions, files, gates, artifacts, targets. Replaces `simulate`. |
| `effigy release run [--to <stage>]` | Starts a Run, or continues the active one, up to `--to` (default `closeout`). Replaces `prepare` plus `execute`. |
| `effigy release resume <run>` | Continues an interrupted Run from its last checkpoint. |
| `effigy release settle <run>` | Settles the recorded invocation without advancing stages. |
| `effigy release approve <run>` | Passes an approval checkpoint. |
| `effigy release gates [--stage pre\|certify]` | Runs gates standalone, with no Run. |
| `effigy release receipt <run> [--json]` | Prints a receipt. Replaces `evidence summary`. |

`effigy changelog validate|format|analyze|extract` stays as it is.

**Retired** (with actionable errors and `retired.toml` entries):
- `prepare` and `execute` → `run`;
- `simulate` → `--plan`;
- `validate`, `preflight`, `proof`, `check-binary`, `verify-install`, and `evidence validate|closeout|summary` → Effigy repo tasks, or `receipt`;
- the `[distribution]` table. Its fields move to the Effigy repo's own release tasks; no consumer uses it.

## 6. The task target protocol

A `task` target, artifact task or verify task runs as an ordinary Effigy task,
with the same environment, secrets and admission as any task. It receives the
candidate as a JSON file at `$EFFIGY_RELEASE_INPUT`, containing:
- the Run ID and stage;
- the source commit;
- each identity's name, kind, version or ID, and tag;
- each artifact's name, path and sha256;
- the target name.

It writes its result as JSON to `$EFFIGY_RELEASE_OUTPUT`, containing:
- `committed` (true, false or unknown);
- destination IDs, such as a release URL, a package@version, an image digest or a deploy ID;
- an optional note.

A task that exits 0 without writing an output records `committed = unknown`.
The schemas are versioned JSON contracts (`effigy.release.input.v1` and
`effigy.release.output.v1`).

The input carries the exact certified artifacts. A task that rebuilds instead
of using them breaks the contract. Effigy can't stop a task rebuilding, but it
re-hashes the recorded artifacts before publish and refuses if any changed.

## 7. Failure, retry and idempotency

- **Publish is irreversible.** Kernel R5 applies: every target publish is an
  `Irreversible` effect, journalled before it runs.
- **Retries never publish blind.** On resume, a target whose last outcome is
  unknown is first reconciled. `tag` targets look the tag up. `task` targets
  run their `check` task, which reports whether this candidate is already at
  the destination. With no `check` task, the Run stops at
  `awaiting_decision`, and the operator records "published" or
  "not published" before it continues.
- **Partial fan-out.** Targets that succeeded stay recorded. Resume continues
  with the remaining targets.
- **Failed verification** leaves the release published-but-unverified in the
  receipt. Effigy never unpublishes. Rollback or fix-forward is the project's
  call, and the receipt records which was chosen.
- **Prepare** rolls back its tree changes on failure.

## 8. Portfolio fit

These are the nine non-fit points the sweep found, and how the contract
answers each:

| Gap | Answer |
| --- | --- |
| Many identities in one repo (Silo, Songsprout, Poodle, Longhorn, Decodelabs JS) | §4.2 identities, each with its own version source, tag format and optional `follows` |
| No SemVer (Contact Patch, Paseo Queue, Northstar, Finch, Bovine content, Silo suite IDs) | §4.3 `kind = digest\|source\|task` |
| Exact artifact continuity (Convergence, Keepsake, Effigy, Bovine Desktop, Longhorn, Nightfire, Poodle, Catalog Pack) | §3 certify records digests; §6 the input carries them and they're re-hashed before publish |
| Multiple or external destinations (Bovine Desktop's private repo and channels, Catalog Pack OCI promotion, multi-package npm) | §4.2 named targets; `task` targets in v0.15; native providers in v0.16 |
| Approval and asynchronous work (protected environments, operator tag authority) | §3 `approve` checkpoints and hosted waits; `release approve` |
| Retry and idempotency | §7 |
| Deployment semantics (Contact Patch, Market, Paseo Queue) | A `task` target plus `verify`. Rollout and rollback policy stay project-owned (the deploy lane). The release records what was deployed and its health |
| Source and destination separation (Bovine Desktop, Northstar packs) | Receipts record the destination identity and owner; `task` targets handle cross-repo publication |
| Verification varies | Per-target and release-level `verify` tasks |

**Migration order** (adoption, not authority to publish):
1. Loophole (S), the smallest Run.
2. Convergence (M), artifact continuity.
3. Mesa (S), the Decodelabs pilot, then the class.
4. Poodle, Longhorn and Underlay: move their existing `[release]` onto identities and targets.
5. Songsprout, before its first release.
6. Northstar language packs.
7. Acowtancy Bovine Desktop and Silo, after approval and identity support are proven.

Projects with an explicit no-release ruling stay out.

## 9. v0.15 scope and v0.16

**v0.15 implements:**
- the Run and its stages;
- identities of all four kinds;
- the gates `stage` option;
- artifacts with digests;
- `tag` and `task` targets with `check` and `verify`;
- approval checkpoints and hosted waits;
- receipts;
- the CLI in §5;
- shorthand compatibility for today's `[release]`;
- the retirements;
- Effigy's own `[release]` migrated onto this contract.

**v0.16 adds:**
- native providers (GitHub Release, npm, Homebrew, OCI, Packagist), each implementing publish, check and verify behind the target interface;
- channels (beta and nightly);
- provider credentials through the secrets vault.

## 10. Rulings (Tom, 2026-10-22, "as recommended")

1. **Tags are pushed at publish,** by the `tag` target, so later targets (npm, deploy) can rely on the tag existing. Closeout pushes only a release commit that no target already pushed.
2. **Run state and receipts live in Effigy-owned state.** The approved Q-010 amendment in [052 §3.1 and §7](052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary) supersedes the original outside-the-repo placement: resumable release runs use the owning checkout's gitignored `.effigy/state/`. Project history stays in git and the orchestrator. A project that wants tracked receipts adds a `closeout` task that writes them.
3. **Approval before publish is on by default** for any release with a non-`tag` target, and off for tag-only releases. `approve = []` or an explicit list overrides it.

## 11. Implementation record: config, status and plan

`effigy release --plan` and `effigy release status` resolve the whole pipeline
from `[release]` and mutate nothing (R2). This section records the resolution
surface and the choices §4 left open; §12 records the Run.

**Implemented here**

- `[release.identities.<name>]` with `kind` (`semver`, `digest`, `source`,
  `task`), `version-file`, `version-path`, `tag-format`, `follows` and `task`;
- `[release.artifacts.<name>]` with `task` and `path`;
- `[release.targets.<name>]` with `kind` (`tag` or `task`), `task`, `check` and
  `verify`;
- gate `stage` (`pre` or `certify`, default `certify`) and gate `task` as an
  alternative to `command`;
- `approve`, with the §10 ruling 3 default;
- the `effigy.release.plan.v1` payload and the extended
  `effigy.release.status.v1` payload;
- `simulate` retired in favour of `--plan`, with an actionable error.

**Choices §4 left open**

- **The shorthand's identity is named after the repository.** Contract §4.1
  says "one identity named after the repo"; resolution uses the release root's
  directory name, and synthesises the shorthand's one `tag` target and no
  artifacts.
- **`follows` binds the follower to its leader's version number.** §4.2 says
  "bump together with core; omit for independent bumps", and one changelog
  feeds every identity, so the follower is released at the leader's next
  version under its own `tag-format`, not merely with the same bump kind. A
  follower whose leader's version is not greater than its own current version
  is a blocker. Chains are allowed; cycles and self-references are refused.
- **A named `semver` identity declares its own `version-file`.** The shorthand's
  file detection does not spread across identities. Every identity in a
  release with more than one identity also declares `tag-format`, because one
  shared default tag would collide.
- **A gate that declares `task` runs as `effigy <task>`.** One runner executes
  both gate forms, and the plan reports the task name alongside the command.
- **`sync-files` stays release-level.** §4.2 gives identities their own version
  source but no per-identity lockfile, so the configured sync files belong to
  the release, which also means a release with no `semver` identity refuses
  `sync-files`.
- **A sync file may be nested, but only the declared file is written.** A
  `Cargo.lock` entry may name a workspace-excluded consumer's lockfile below
  the release root. `prepare` runs `cargo update -p <package>` for each release
  package that lockfile records, from the lockfile's own directory, and refuses
  a lockfile whose owning workspace is anchored elsewhere (a plain member
  directory's `Cargo.lock` belongs to the enclosing workspace). Absolute paths,
  `..` components and paths that resolve outside the release root are refused,
  and a `Cargo.lock` entry must sit beside its own `Cargo.toml`. The primary
  `Cargo.lock` keeps the `cargo update --workspace` behaviour.
- **The changelog is a `semver` identity's business.** §4.3 says bump rules
  apply only to `semver`, so a release with no `semver` identity never reads a
  changelog: the file may be absent (`release changelog path does not exist`
  never fires for it) or unparseable, it is neither validated nor blocked by
  the changelog, and it lists no changelog file to change. The plan and status
  payloads say so with `changelog.used: false`; the text surfaces say "not
  used (no `semver` identity)".
- **`working_tree_clean` is tri-state.** `null` means the release root is not a
  git work tree. Status and plan still report; the Run's validate stage is what
  refuses such a release.
- **`release status` reports the active or last Run.** It fills in the id,
  stage, status and what it waits on from the record; `active` is true for
  every unfinished Run, including a failed one that can still resume.
- **`release prepare`, `release execute` and the prepared-state `release
  resume` are retired.** `release run` replaces the first two and `release
  resume <run>` the third; each refuses with that replacement
  (`retired.toml`, `effigy-release-prepare-execute`). The repo selector
  `release:prepare` becomes `release:run`, which runs `release run --to
  certify` under the same heavy admission.
- **`--plan --version` applies to the single `semver` identity.** It is the
  successor of the retired `simulate --version`, and refuses a release with
  more than one `semver` identity, because one flag cannot say which it means.

## 12. Implementation record: the Run

`effigy release run`, `resume`, `approve` and `receipt` are implemented in
`crates/effigy-release/src/run/`. This section records the choices §3 and §7 left
open.

**One kernel Run per invocation; one release across invocations.** Each
`run`, `resume` or `approve` invocation is a journalled kernel Run for one
release owner (`ResourceId::store(Machine, "release <canonical root>")`). The
kernel holds the owner gate, journals each push as an `Irreversible` effect
before it runs and settles what an interrupted invocation left behind. The
release's own facts — candidate commit, versions, artifact digests,
destinations — live in a record file, and the kernel engine record carries the
checkpoint pointer: release id, invocation id, stage, status and the record's
path. A release therefore stays one logical Run while each invocation is one
journalled kernel Run. `Task` identity tasks, artifact tasks and verify tasks
run as this Effigy binary (`current_exe`), so they inherit the same
environment, admission and selector resolution as any task.

**Checkpoint stops are the crash state.** `--to <stage>` stops a Run after a
boundary and closes its kernel Run; the next invocation settles any predecessor
and continues from the record. That is the same durable state a crash at the
boundary leaves. A crash *between* an irreversible effect and its journal
record is the `Unknown` effect state, proven separately by an ambiguous tag
push that really happened and was not recorded.

**Approvals record who and when.** `approve = [...]` names stage boundaries.
The Run stops at the first unapproved one with status `awaiting-approval` and
`waiting_on` naming it; `release approve <run>` records the approver (git
`user.name`, else `$USER`, else `operator`) and continues at that stage.

**Checkout state layout** (Q-010; §10 ruling 2):
`<checkout>/.effigy/state/release/<owner>/record.json` is the durable record,
`receipt.json` the receipt, rendered from the record and replaced through the same atomic route with every record write, so it is current at each checkpoint and equals `release receipt --json`; `candidate/<artifact>/` the certified artifact
copies that publish re-hashes. The checkout journal holds the checkpoint
pointer. The release owner uses that checkout's retained identity, and a linked
worktree owns its own journal and candidate. State is registered in Git's local
exclude file; tracked receipts remain an explicit `closeout` task. Status and
receipt reads create nothing, and a missing checkpoint record is unknown.
Candidate reset and checkpoint publication happen after admission under the
release owner's gate. Record replacement writes and syncs a private temporary,
renames it and syncs the directory through the kernel. Cleanup gets a fresh 60-second budget when settlement begins. `release settle <run>` acquires the owner gate and settles the recorded invocation without planning or running stages. Settlement is not a stage outcome. `release receipt <run>` and `receipt.json` render the recorded stage outcome and never change on read; no write follows a settlement, and the journal is not projected into the record. `release status` observes pending or incomplete settlement from the journal and shows the bounded `release settle <run>` action. A completed release keeps its `complete` status while its cleanup is pending; a failed stage keeps `failed`. Missing or unreadable journal state is an error, never a guess: a journal that has gone missing while `release/` still holds a record or receipt names that state directory rather than reporting no Run (Tom ruling 2026-10-28, Queue decision b8317fec). The record and receipt are two atomic replaces, so a crash between them leaves `receipt.json` one checkpoint behind until the next invocation that opens the Run; opening it rewrites the file from the record, and unchanged bytes write nothing. `release receipt` reads the record and never the file.

Legacy relocation preserves the old release resource's canonical identity and
run ids in the checkout journal, with a local import alias for subsequent
invocations. The importer requires that owner's retained checkout identity;
an unbound owner must finish or drain under the previous binary. It copies the
record and candidate without following absolute record pointers, re-hashes every
certified artifact, and rebases only the checkout and candidate paths. A missing
record, escaped path or mismatched digest prevents publication. Completed
releases may be dropped with completed history; a committed cutover never
falls back to the old release record.

**Artifacts.** An artifact `path` is a file, a directory (every file beneath
it) or a glob matched below its fixed prefix, relative to the release root.
Certify copies each match into the candidate directory and records path, size
and sha256; a `digest` identity's value is that file's digest, or the digest of
the sorted file digests when an artifact holds several. The artifact task runs
with `EFFIGY_RELEASE_INPUT`, `EFFIGY_RELEASE_OUTPUT`,
`EFFIGY_RELEASE_CANDIDATE_DIR`, `EFFIGY_RELEASE_RUN` and
`EFFIGY_RELEASE_STAGE` set, and the input is the `effigy.release.input.v1`
document of §6.

**The task protocol.** The input document also carries `schema_version: 1` and
a `subject` object (`{"kind": "target" | "artifact" | "identity" | "release",
"name": …}`) plus a top-level `target` for a target or verify task, because §6
names the target and the artifact task needs the same document. A task target's
output is read from `$EFFIGY_RELEASE_OUTPUT` as `effigy.release.output.v1`:
`committed` (`true`, `false` or `"unknown"`), `destinations` (an array of
destination-ID strings) and an optional `note`. `true` publishes the target and
records its destinations; `false` fails the target and records no destination;
anything unreadable — a missing file after exit 0, invalid JSON, an unknown
field or a wrong schema id — is an unknown outcome, never an assumption. A
non-zero exit with a valid `committed: false` is a known failure, so a task
that fails before publishing does not need an operator.

**`check` writes the same document.** §6 covers target, artifact and verify
tasks, and §7 gives a target a `check` that reports whether this candidate is
already at the destination, so `check` writes `effigy.release.output.v1` too:
`committed: true` means the candidate is already there, `false` means it is
not, and `"unknown"` (or an unusable answer) stops the Run at
`awaiting-decision`.

**Decisions and hosted waits are stops, not failures.** A stop records
`waiting_on`: the stage boundary for an approval, the gate name for hosted
evidence, and the target name for a decision. `release decide <run>
--published|--not-published` records the operator (same identity rule as an
approval) and the time in the record's `decisions`, sets the target published
or pending, and continues at `publish`.

An unknown `task` result whose target declares no `check` stops that same
invocation at `awaiting-decision`, so the operator can decide immediately
instead of resuming a failed Run first. A target that cannot be started at all
is an unknown outcome too, so it follows the same rule. A target that declares
a `check` fails at `publish` with the outcome recorded unknown, and the resume
reconciles it through the check before any retry — §7's "on resume, a target
whose last outcome is unknown is first reconciled". A wait also stops the
publish fan-out: every later target stays pending, and none of them publishes
until the outcome it is waiting behind is resolved.

A hosted gate is checked before its command would run, for the candidate commit
at `certify` and the source commit at `pre-gates`. Missing or still-running
evidence stops the Run at `awaiting-hosted-evidence` with no process open, and
it stops the gate stage itself: `certify` captures no artifacts until the gate
has its exact-commit evidence. A resume re-checks the evidence, skips the gates
it already recorded as passed, and continues the stage. A failed run, the wrong
commit or repository, an ambiguous answer or an unusable lookup fails the gate
closed, exactly as the previous synchronous `reuse-hosted-evidence` did.

**Task targets publish in declared order.** A target that succeeded stays
recorded, so a resume continues with the remaining targets. Before a task
target's publish task starts, the Run writes the target as `Unknown` in the
record and journals the publish as an `Irreversible` effect. A crash mid-task
therefore leaves `Unknown`, never the `Pending` a resume would publish again
blind, and the kernel settles the journalled effect as `Irreversible` rather
than blocking the next invocation. A task publish settles `Applied` once the
task reported a definite outcome — `committed` true or false, which the record
keeps — and `Unknown` when the task could not tell or could not run, so only a
real unknown reaches reconciliation.

**The protocol schemas are file contracts.** `effigy.release.input.v1` and
`effigy.release.output.v1` are documents a task reads and writes, not command
payloads, so the schema index carries them under `payload_schemas` and has no
runnable row for them: no `effigy` command emits them. `effigy-release`'s tests
validate the documents a real Run writes and reads against those shipped
documents, so the protocol and its schema cannot drift apart silently.

**One changelog and one lockfile version.** One changelog feeds every identity,
so `prepare` promotes `[Unreleased]` into the first `semver` identity's next
version. `sync-files` stay release-level (§11), so a release whose `semver`
identities resolve to different next versions refuses `prepare` when it
declares sync files: a lockfile carries one version. `follows` is the way to
keep them equal.

**The prepared-state library is now unreachable.** `effigy-release` still
exports `release prepare`'s prepared-state mechanics (`build_release_prepare_plan`,
`execute_release_prepare`, `execute_release`, `collect_release_execute_plan`,
the `.release-prepared.json` reader/writer and the review menus) although no CLI
surface calls them. The Run reuses the parts that are still product behaviour —
mutation snapshots and rollback, `apply_release_mutations`, `build_sync_mutations`
and the changelog/version renderers. Deleting the rest is a bounded follow-up
rather than part of this change, so no consumer loses a working path while both
exist.

**Publication is not requested by a selector.** `release:run` fixes `--to
certify`. Publishing stays an explicit `release run` under the operator's
authority.

**Fixtures.** `tests/fixtures/release_platform/` holds one project per §4 shape
plus Longhorn's, Poodle's and Underlay's real `[release]` tables, so the
shorthand compatibility claim is proven on consumer configuration rather than on
an invented one. Nothing in those fixtures pushes, tags or publishes.

## 13. Implementation record: Effigy's own release (§4.4)

`config/release.toml` is one declaration for Effigy: the `effigy` identity, a
`preflight` pre gate and the `secret-audit`, `ci`, `format`, `test`, `qa`,
`build`, `smoke` and `metadata` certify gates, the `binary` artifact, and the
`tag` and `binaries` targets. §13 records the choices §4.4 left open.

**The Effigy-only commands are repo tasks, and two of them are gates.** The
retired `release validate|preflight|proof|check-binary|verify-install|evidence`
commands became `release:metadata`, `release:preflight`, `release:proof`,
`release:check-binary`, `release:verify:install`,
`release:evidence:validate|closeout` and the `release:verify:binaries`
composition in `config/tasks.toml`; `evidence summary` has no task because the
Run's receipt is the record. Their code is a repo script,
[`scripts/release/checks.sh`](../../../scripts/release/checks.sh), so it can
name Effigy's own artifacts, workflow and tap without becoming product surface.
`metadata` is a `certify` gate declared as `task = "release:metadata"`, because
it reads the prepared version, changelog and lockfile and needs no arguments —
which also keeps the task-gate form (§11) exercised by Effigy's own release.
`preflight` is a **`pre` gate** whose docs slice runs before any version edit;
it runs the repo script with `--skip-smoke` because the `smoke` certify gate
already proves the prepared binary, so no check runs twice. The retired
`release preflight` command's refusal names that `pre-gates` stage. The manual
`effigy release:preflight` task still runs all three slices. `proof` and `evidence closeout` stay manual tasks: both need a human to
name an artifact directory, and neither is part of a Run's stages.

**The artifact is the release binary the build gate produced.** `release:artifact`
composes the existing `build:release` task, and the artifact pattern is
`target/release/effigy`. Certify copies it into the candidate directory and
records its sha256; publish re-hashes that copy, so a task that rebuilt the
binary instead of using the candidate is refused as §6 requires.

**The task target dispatches, then verification inspects the result.**
`release:publish:binaries` reads the candidate at `$EFFIGY_RELEASE_INPUT` and
runs `gh workflow run release-binaries.yml -f tag=<tag>`; it reports
`committed: true` with the workflow run URL and the GitHub Release as
destinations, and never waits for the workflow. `release:check:binaries` answers
§7's idempotency question from `gh release view`. `release:verify:binaries` runs
after the workflow and checks the four binary assets and the two macOS checksum
sidecars on the GitHub Release, the Homebrew formula's version, and the
tagged-source install through `release:verify:install`. A workflow that has not
finished therefore leaves the release published but unverified, which §7
already covers: Effigy never unpublishes, and the receipt records the failure.

**`[distribution]` retires as an ignored section, not a manifest error.** The
table only configured the retired Effigy-only commands, so dropping it changes
nothing else about a v0.14.1 project (052 §11): the manifest removes it and
records one deprecation notice per manifest, naming `[release]` and `[tasks]` as
the replacements. Each retired command still refuses with its repo task, and
`crates/effigy-distribution`, `src/runner/distribution_command` and the
`distribution::` Rhai module were deleted with it.

**The workflow's GLIBC step is a disclosed follow-up.** The release binary
workflow still calls `effigy-<target> release check-binary`. The retirement
makes that step fail, and this change does not edit `.github/workflows/`: the
`metadata` gate warns with the exact replacement
(`./scripts/release/checks.sh check-binary …`) instead of failing closed,
because the workflow is outside the Run. A release owner lands that approved
workflow edit before the next publication; `release:metadata` starts failing if
the guard disappears entirely.

**The proof is a fixture, not a publication.** `tests/cli_output_tests/
release_platform_tests.rs` copies this repository's `config/release.toml` byte
for byte and stubs only what a temporary workspace cannot reproduce: the hosted
CI probe, the fixture's own `build:release` leaf, and the `task` target's
dispatch. The tests run `release --plan`, then `run --to certify` on a local bare
remote, assert nothing was dispatched or tagged, continue to the approval stop,
approve, and complete the Run through the fake dispatch — so the whole
declaration is exercised without publishing anything.
