# 054 - Release Platform Contract

Status: accepted by Tom on 2026-10-22 ("I'll go with your recommendation … release draft questions"); not yet implemented.
It supersedes the generic mechanics in guide 051 when implemented. Effigy's own release
policy stays in [release.md](release.md).

Owner: release platform (Queue lane `v015-rebuild`, then a release lane)
Kernel: [052](052-runtime-kernel-contract.md), the delivery row in §10 and
failure-model row 22 (`Run` with checkpoints; pushes are `Irreversible` and
reconciled, never replayed)
Ruling: [questions](../questions.md) Q-009, the release platform bullet
Evidence: the planner's portfolio inventory (2026-10, `release-audits/effigy/release-platform-inventory-2026-10/`). These cover 38 project roots and 101 legacy roots.

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
- **Prepare** rolls back its tree changes on failure, as `release prepare`
  does today.

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
2. **Run state and receipts live in Effigy-owned state outside the repo.** History stays in git and the orchestrator. A project that wants receipts in its tree adds a `closeout` task that writes them.
3. **Approval before publish is on by default** for any release with a non-`tag` target, and off for tag-only releases. `approve = []` or an explicit list overrides it.
