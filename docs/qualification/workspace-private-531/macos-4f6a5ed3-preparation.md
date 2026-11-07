# macOS 4f6a5ed3 preparation: Colima and Desktop stages

Preparation only. It arms nothing and gives no proof credit. The executable plan is
[macos-4f6a5ed3-stages.json](macos-4f6a5ed3-stages.json), manifest revision r3. Each stage
needs its own fresh GO under standing decision `63c338e2-1c50-435e-9783-e78e9d8558e6` v2,
naming the stage, the manifest, source and input hashes, the engine identity and a window
of at most 1200 seconds. The standing decision is not a blanket authority, and no GO is
recorded here.

## Revision r3 (supersedes r2)

r2 (`macos-4f6a5ed3-stages-r2`, manifest sha256 `50d6f46c…`, `macos_preparation.py`
`d651b3b6…`) was reviewed against upstream Colima 0.10.3 and Docker flag metadata. These
defects are corrected in r3:

- **C1 `--cpu` is not a Colima 0.10.3 flag.** The flag is `--cpus` (`-c`). The installed
  `colima start --help` example line still shows `--cpu 4`, but its flag table defines only
  `--cpus`. Tables win over examples.
- **C1 global writes.** `--ssh-config` and `--activate` default to true upstream. The stage
  passes `--ssh-config=false --activate=false`. Booleans take the `=` form, since a separate
  `false` token is parsed as a positional argument.
- **C1 host Docker config.** Colima runs `docker context create` on every start, whatever
  `--activate` says, so it writes the host Docker config store. The stage pins
  `DOCKER_CONFIG` to an owned empty directory.
- **C1 image cache.** Colima caches the VM image under `COLIMA_CACHE_HOME`, falling back
  to `~/Library/Caches/colima`. The stage pins `COLIMA_CACHE_HOME` to an owned directory.
- **C2 route echo first.** Colima passes `colima ssh` arguments through to `limactl shell`,
  so quoting is not established by Colima's source. The route echo now runs before any
  probe, stage or check source crosses the route. It is `printf %s <hostile>`, not a
  Python one-liner, and its characters cannot redirect, pipe or background.
- **C4 and C5 deadlines.** Each remote `nerdctl build` and `run` has a guest deadline
  inside a host envelope (table below). The check also carries an in-container alarm.
- **C4 and C5 image identity.** The build writes `--iidfile`. The check runs by that
  authenticated `sha256:` ID with `--pull=never`, never by the mutable tag. The ID is
  inspected for `linux/arm64`.
- **D2 build `--pull`.** Docker `build --pull` is boolean. `--pull=never` is not valid
  syntax for build, so the stage omits `--pull`. A missing base digest is pulled once from
  the official registry under the download cap. A present base is used with no network.
  `run --pull=never` is valid and stays on D3 and C5.
- **D4 cleanup authority.** Label, name prefix and same-name resources are not authority.
  A resource is removable only when this occurrence's creation receipt matches its exact
  engine, ID, name and label. The receipt comes from the stage's own `--iidfile`,
  `--cidfile` or `volume inspect` output.

C0 and D0 are byte-identical to r2. Their canonical hashes are `5eec2ae2…` and `91bad78a…`.
The planner can readiness-check them at the r2 manifest hash. Their evidence is an input
later only after exact pin comparison. Other stages bind to r3 only.

## Current local metadata

Observed once, 2026-11-07 20:42Z, by [macos_metadata_tranche.py](macos_metadata_tranche.py)
(runner commit `1ccd55420`). Five local commands, all exited 0, retained under the
private child `linux-artifact-preparation.l0tx7tam/macos-metadata-rfzuiadz`.

| Item | Observed |
| --- | --- |
| colima | `/opt/homebrew/bin/colima` 0.10.3 |
| docker client | `/usr/local/bin/docker` 29.8.0 (client `--version` only) |
| Colima profiles | `effigy` Stopped, `effigy-release` Stopped; `effigy-531-qualification` absent |
| Desktop context | `desktop-linux`, host `unix:///Users/tom/.docker/run/docker.sock`, TLS verify on |
| Other contexts | `colima-swallowtail-kiro-owner-proof` and `default` exist; not used by any stage |

This is local metadata. It is not a running-engine, daemon or guest observation.
Desktop reachability and the engine baseline are unknown until D0 runs under a GO.

Flag metadata read in this revision, with no daemon, VM or guest contact: installed
`colima start --help` (run with `COLIMA_HOME`, `COLIMA_CACHE_HOME`, `DOCKER_CONFIG` pinned
to a scratch fixture), `docker build --help` and `docker run --help`. Upstream sources read:
Colima `v0.10.3` `cmd/start.go`, `cmd/ssh.go`, `cmd/root/root.go`,
`environment/container/docker/docker.go`, `environment/vm/lima/yaml.go`,
`util/downloader/download.go`, `config/files.go` and `embedded/images/images.txt`. Docker
buildx build reference. nerdctl `v2.3.1` `docs/command-reference.md`.

## Paths (fresh, owned, pure-checked)

Control root `/Volumes/Stash/scratch/effigy/mac-4f6a-<token>`, with `<token>` eight
lowercase hex minted at C0. Owned children:

- Colima home `<control>/c`, the only Colima data root.
- Colima image cache `<control>/cache`, created by C1 before start.
- Colima Docker config `<control>/docker-config`, created by C1 before start.
- Qualification root `<control>/s`, empty and 0700, the only permitted host share. C1 passes
  `--mount none`, so the stage has no share; the root stays empty.
- Desktop config `<control>/d/docker-config` and context `<control>/d/ctx`, both owned and empty.
- Desktop image ID file `<control>/d/image-id` and container ID file `<control>/d/container-id`.

Profile `effigy-4f6a-qual`. Socket paths, computed by the boundary test:

- `ha.sock` and `ga.sock`: 87 bytes each.
- `docker.sock`: 78 bytes.

The limit is 104 bytes, so the margin is 16. Collisions, aliases, Builds paths and
over-limit sockets refuse before any effect. `effigy`, `effigy-release` and any
historical 531 resource are never adopted.

## Executable rules

- Every exec step is an argv list whose first element is the pinned absolute tool.
  Strings, shell text and English placeholders are refused.
- Placeholders are `{slot}` names from a fixed set. `render()` refuses an unknown or
  unbound slot. `{{json .}}` is a literal Docker format string.
- Colima effect steps (C1–C6) pin `COLIMA_HOME`, `COLIMA_CACHE_HOME` and `DOCKER_CONFIG`
  to owned roots, and unset `LIMA_HOME`, `DOCKER_HOST` and `DOCKER_CONTEXT`. C0 keeps its
  r2 pin set, which `check_step(legacy=True)` applies only to C0 and D0.
- Desktop steps pin `DOCKER_HOST` to the observed endpoint and `DOCKER_CONFIG` to the
  owned empty config, and unset `DOCKER_CONTEXT`. Desktop argv is exec'd directly, with no
  shell. Each step runs against the observed socket, not a context name.
- Colima `start` must carry `--cpus`, `--ssh-config=false`, `--activate=false` and
  `--mount none`, and must not carry bare `--cpu`, `--ssh-config` or `--activate`.
- `docker build` and `nerdctl build` carry no `--pull` form, an `--iidfile` into an owned
  path, and `--platform linux/arm64`. `run` carries `--pull=never` and the immutable
  `{image_id}`, never a tag.
- A remote `nerdctl build` or `run` carries a `guest_deadline`. Its `timeout -k` wrapper
  sits inside the host envelope, with at least 30 seconds of receipt margin.
- No argument carrying source or JSON (`probe_source`, `stage_source`, `verify_source`,
  `check_source`, `guest_spec`, `guest_verify_spec`) crosses the Colima route before the
  c2 route echo has matched byte-for-byte.
- Helper steps name one of a fixed set of host actions, each a pure check or a receipt
  producer. No helper runs a shell.

## Stage order and bounds

Numeric caps are bytes. Storage is allocated bytes, measured by the runner on the named
owned roots. Downloads are declared sources, each pinned. Exceeding a cap is
Unknown and retained. Caps and sources are enforced by the runner, which is a remaining
gate below.

Colima (`colima`):

- **C0** admission (unchanged from r2): tool hashes against the tranche, tranche summary
  check, intent claim, owned roots created 0700 and empty, `colima start --help`.
- **C1** start, 900 s stage, 600 s command. `c1-make-owned-dirs` creates the cache and
  Docker config, then `colima start -p effigy-4f6a-qual --runtime containerd --vm-type vz
  --arch aarch64 --cpus 2 --memory 2 --disk 20 --root-disk 20 --mount none --ssh-config=false
  --activate=false`. Download cap 2 GiB; storage cap 8 GiB. Source: Colima's own embedded
  image list for arm64 containerd, `https://github.com/abiosoft/colima-core/releases/download/v0.10.4/ubuntu-24.04-minimal-cloudimg-arm64-containerd.raw.gz`,
  sha512 `86fd0879…` (Colima verifies the digest on download). Colima does not pin the
  raw image size, so the planner takes the size from release metadata at activation and
  may lower the cap only with that number.
- **C2** identity, 300 s stage: `colima status -p effigy-4f6a-qual --json`,
  `printf %s <route echo>` and the byte comparison, then the guest parent probe. Nothing
  else crosses the route until the comparison passes.
- **C3** stage Dockerfile by the existing helper, with the same argv and
  `runtime.Dockerfile` on stdin. Storage cap 1 MiB.
- **C4** verify and build, 1200 s stage, 120 s reserve. Steps: render verify spec, verify
  context, `nerdctl build --help` and its flag check, then one local-path build:
  `timeout -k 5 840 sudo -n env DOCKER_CONFIG={guest_context}/docker-config nerdctl build
  --platform linux/arm64 --provenance=mode=max --iidfile {guest_iidfile} --tag
  effigy-4f6a-runtime:r1 {guest_context}`. Host command envelope 900 s; the guest deadline
  is 840 s plus a 5 s kill grace, leaving 55 s of host receipt margin. Then the iidfile is
  read and checked as a `sha256:` ID, and `sudo -n nerdctl image inspect {image_id}` is
  checked for linux/arm64. The tag is never used after the build. Download cap 1 GiB;
  storage cap 4 GiB. Sources: the base digest `docker.io/library/ubuntu@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11`
  and `http://ports.ubuntu.com/ubuntu-ports` for apt, anonymous through the empty config.
- **C5** one disposable check, 360 s stage: `timeout -k 5 200 sudo -n nerdctl run --pull=never
  --rm --network none --name effigy-4f6a-check-{token} {image_id} python3 -I -c {check_source}`.
  Host envelope 240 s, so 35 s of receipt margin. The check raises an in-container alarm at
  120 s. `check_runtime_facts` passes only with exit 0 and uid 0, Python 3, OpenSSL, a GNU C
  library loader and coreutils. C5 needs C4's closure and identity.
- **C6** observe only: `colima list --json`. No stop. A running VM is retained until a
  separate GO covers a stop.

Desktop (`desktop`), with the pinned env above:

- **D0** endpoint, owned dirs and baseline (unchanged from r2). The owned/foreign split is
  observation only.
- **D1** owned volume `effigy-4f6a-qual-<token>`, labelled `effigy.qual=<token>`, then
  `volume inspect` as the creation receipt.
- **D2** runtime image, 1200 s stage: stage the Dockerfile, then
  `docker build --platform linux/arm64 --provenance=false --label effigy.qual=<token> --tag
  effigy-4f6a-desktop-runtime:<token> --iidfile {desktop_iidfile} {desktop_context_dir}`.
  `--provenance=false` is the documented boolean form that disables attestations. It is
  pinned so the image's provenance state is explicit, not left to the driver default.
  Host envelope 900 s, and a kill or timeout is Unknown. Then the iidfile is read, and
  `docker image inspect {image_id}` is checked for linux/arm64 and the occurrence label.
  Download cap 1 GiB; storage cap 2 GiB. Sources as C4.
- **D3** disposable check: `docker run --pull=never --rm --network none --label
  effigy.qual=<token> --name effigy-4f6a-dcheck-<token> --cidfile {desktop_cidfile}
  {image_id} python3 -I -c {check_source}`, host envelope 240 s. The cidfile is read as
  the container creation receipt, and the facts are checked.
- **D4** cleanup by receipt only: observe the engine ID (`docker info`), then ps, volume
  and the image by authenticated ID (`docker image inspect {image_id}`, which carries
  RepoTags and Config.Labels; `docker image ls` does not, so it is not used). The helper
  refuses if the engine ID changed since D0, then selects with `occurrence_receipts`: the
  names and labels each stage set, bound to the IDs its own outputs returned.
  An image that cannot be inspected by its ID refuses the selection; nothing is removed
  and the occurrence is retained. Only exact
  matches on this engine are removed, by ID. Unauthorized and foreign resources are never
  touched. Each removal runs only when its selected set is non-empty.

## Proof-entry bindings (G1–G3, gated)

The frozen 627 artifacts are bound in the manifest by path and sha256, unchanged from r2.
Each file was verified present with its index sha256 on 2026-11-07. Bound, not run:

- Source: head `4f6a5ed3`, tree `dcb4c791`, candidate
  `/Volumes/Stash/scratch/effigy/release-source/candidate-4f6a5ed3.xwm4pr7j`, and
  the E14 binding file sha. Identity is dev `16777245`, inode `156037736`, uid 501, 0700.
- Baseline D (627 `baseline_D`, reused not rebuilt): `old_binary` at
  `/Volumes/Stash/scratch/effigy/627-baseline-correction.f_43nyce/producer-replacement-823d859d-r1/frozen/effigy-old-823d859d-r1`,
  sha256 `a603c82a…`, `old_source_head` `823d859d`, and the `old_source_receipt` hash
  `f86a1f3a…` (the index gives no receipt path). Launch's binding record reads these
  cells, so they are bound here and not rebuilt.
- Receipts: candidate `895b3cf2…`, production `52e2c55b…`.
- Frozen: production, candidate, nextest binary, kernel and custody test executables,
  kernel custody fixture, utility, cargo metadata, nextest config (`e4afe6df`) and
  derived binaries metadata (`d247ddd1`).
- **Nextest fixture path rule.** The kernel custody fixture is an absolute frozen copy
  in `non-test-binaries`. Nextest joins the build directory with the path, and an
  absolute path replaces the join. 627 did not exercise this. G3 refuses until a
  metadata-only probe in the qualification root proves the handling. The guest path
  is the identical absolute path, or a planner-approved rewrite. It is never an
  assumed join.

Gates:

- **G1** E14 source placement. Either a copy of the exact candidate tree or a single
  read-only share of only the candidate directory. The planner chooses at activation.
- **G2** artifact, sidecar and producer receipt placement. Byte-identical copies under
  `<source_root>/.effigy/linux-release/artifacts`, each matching its frozen sha256. No
  producer receipt is synthesized. RecipeE and 686 are missing.
- **G3** proof run: `launch.py` with an exact per-backend GO, after G1, G2 and the
  fixture path proof.

## Closure, retention and failure

- Closed means a waited exit 0 with no deadline or cap. `may_continue` admits the next
  step only on `closed`. A deadline, a cap, a missing waited exit or a host kill is
  **Unknown and retained**. Unknown never replays, and no later disposable check or cleanup
  runs on that stage.
- A daemon can keep building after its client is killed. A timeout or kill claims no
  container or build closure. The retained resources are listed per stage.
- Each activation's intent is claimed once. A reused intent refuses before effects.
- Old 531 markers and consumed activations stay history, and the boundary test refuses
  them. The guest name `effigy-531-image-<token32>` is the helper's shape. The token is
  fresh, and no 531 receipt is reused.
- Retain intent, GO copy, every stream, PID, waited exit and hash under the run's
  private child. Receipts stay outside Queue cleanup.

## Decisions

- Zero host shares is the preferred C1 form. `--mount none` returns nil mounts upstream,
  so no `$HOME` mount is added. The qualification root is an empty directory, not a share.
- Colima 0.10.3 flags come from its own `start` command table. The installed help example
  is not a flag source.
- Colima effect stages pin the Docker config and image cache to owned roots. C0 keeps the
  r2 pin set, since it only reads help.
- The Colima build is a local-path build from a helper-staged file, not tar-stdin.
  Installed nerdctl 2.3.1 refuses tar-stdin contexts, so no builder switch is made.
- Desktop pins the observed endpoint with `DOCKER_HOST` and an owned empty
  `DOCKER_CONFIG`, not a context name. Desktop build and run do not use `--pull` in the form
  r2 wrote. Build omits it and run uses `=never`. D2 pins `--provenance=false`, and C4 pins
  `--provenance=mode=max`, one explicit form per backend. Each build argv must carry exactly
  its form.
- The guest name stays under the helper's `effigy-531-image-` shape. The helper
  enforces it, so the plan binds the name instead of changing the helper.
- Route echo is `printf %s` with a hostile constant that has no redirect, pipe or
  background character. Its outcome is a syntax failure or a harmless echo if a shell
  parses it, never a write.
- `c4-nerdctl-build-help` reads flag metadata before the build. The build only starts if
  `--iidfile`, `--platform` and `--provenance` are in the guest's own help text.

- Manifest changes: pre-effect steps `d1-engine-info` and `d1-volume-absent` before the D1 create, and
  `d2-engine-info` and `d3-engine-info` before their effects. D1's three volume argv take the
  per-activation `{owned_volume_name}` in place of the fixed name. D4 gains `d4-post-ps` and
  `d4-post-volume` after its removals, and its stage budget grows from 300 to 360 seconds so the
  step total (280) still fits under the reserve. Each stage's step total stays under its budget.

## Executor (source only)

`macos_stage_executor.py` is the reviewed runner. Nothing in this revision starts Colima or
Desktop, and no stage can run from this source.

- **Execution is refused, globally.** Every resolved request carries
  `execution.admitted = false` and the reason: no owner-controlled activation mechanism exists in
  this repository. A file the same account can create and chmod grants no authority. The
  prerequisite is a planner-provided mechanism that proves the owner authorised the exact request
  bytes, verified before any effect. Until it exists, `execute_stage` refuses every stage before
  any effect.
- **Static admission in source: none.** Every stage is refused in production, first by the global
  owner-activation refusal and then by its own static reason:
  - C0 and D0: the helper residual. Their helpers are bounded only for interruptible work (an
    uninterruptible kernel wait is not bounded), and the storage monitor is sampled at helper
    boundaries, so the shared deadline and the live threshold are not enforced across helpers.
  - C1, C4 and D2: the download cap is payload bytes, wire traffic is not counted, and the manifest
    records no pinned image size.
  - C2: a multi-effect guest stage. The guest closure is unknown after the route echo, so the probe
    is not reachable through this adapter.
  - C3: `guest_spec` derives from the C2 probe output and the `token32` that C0 mints.
  - D1: `docker volume create` is idempotent and the name is observable, so exclusive creation
    cannot be established.
  - D4: no per-target re-observation before removal, and no image foreign list.
  - D3, C5 and C6: unadmitted because their predecessor is.
- **C0 (fresh tokens).** Its handlers are `verify_tool_sha` (the live colima and docker binaries
  against the sha256 the pinned 688 C0 record names) and `verify_tranche_summary` (the tranche
  summary against the sha the same record names). C0 creates its own roots under the scratch root
  and refuses on any collision. For the recorded token it refuses: that C0 is historical, and C1
  binds it. C0 is statically refused by the helper residual above. Each claimed C0 activation mints one
  `token32` (32 lowercase hex), written exclusively as `token32.json` and recorded by hash in the receipt
  as `origin`. It is the chain's origin: `origin_token32` reads it back from the verified C0 receipt before
  C3 and C4 claim. A historical C0 minted none, so C3 and C4 refuse for the recorded token.
- **C1 binds the pinned 688 C0 record, by identity.** The record is
  `macos-4f6a5ed3-activation-7deaf1b3/C0.stage.json` (sha256 `15a5373e…`) and its tranche summary
  (`macos-metadata-rfzuiadz/summary.json`, sha256 `cdfb7fb6…`). C1 accepts it only when the bytes
  match the pinned sha, the record is a passed, zero-share, safe C0 for the recorded token, the live
  colima and docker binaries match the recorded tools, and the recorded roots match the recorded
  identities. This is a binding, not a receipt: C1 claims its own activation, and nothing is
  replayed, adopted or recreated.
- **Plan.** `plan` is pure. Each stage resolves to its argv, pinned env, timeouts, caps, runtime
  preconditions, the sources it derives at run time (`runtime_derived`), and both its global and
  static admission reasons. Derived and unpinned slots render as explicit placeholders.
- **Bindings.** Checked against the reviewed paths for the token, the pinned socket URL and the
  pinned check program. A caller who supplies a derived slot or an unpinned source refuses.
- **Inputs.** The executor's own set (program files, including the pinned guest helper, the manifest, `runtime.Dockerfile`, tool binaries)
  is bound by the activation and re-hashed by `run_stage` before every launch.
- **Claims.** `macos-stage-claims/<activation_id>.claim`, exclusive, under a provisioned directory.
  A missing directory refuses. The executor never creates one under the scratch root.
- **Pins.** The evidence, claims, activation and owned directories, and for D1 to D4 the private
  Docker config and context directories, are pinned by canonical path, owner, mode 0700, device and
  inode. Each is rechecked before every step. C0 pins its roots once it creates them.
- **Deadline (helpers).** Each helper runs under a timer set to its remaining allowance. The
  handler raises inside the helper, which interrupts Python-level work and interruptible system
  calls; a FIFO read blocked in the kernel was interrupted in the test. The timer does not interrupt
  an uninterruptible kernel wait, and the adapter does not bound one. That is a stated residual, not
  an enforced guarantee. Exec windows are bounded by `run_stage`'s wait loop, which enforces the
  remaining allowance and the output cap. Helpers are timer-bounded only, with that residual.
- **Monitor (helpers and windows).** The stage-wide `Monitor` is sampled before and after every
  helper and every exec window. It is not sampled during a helper. Helpers that write to the
  control root create owned directories only. Directory allocation on the monitored filesystem is
  not assumed to be zero: an allocation is observed at the helper's post-check and refuses before any
  later effect, so it is a window-granularity bound, not a live one. Helpers that write the stage's
  evidence are outside the control root and capped by their own sizes (the recipe at 64 KiB). A zero
  storage cap is enforced exactly at each window. A failed post observation is retained and stops the
  stage. The spawn record is taken with no subprocess, so nothing blocks live monitoring. It does not
  read the kernel's process start time, and PID identity rests on the recorded process group and the
  child's waited exit.
- **After an unknown guest closure.** A guest step whose closure is unknown halts later effects. The
  only helper that may still run is the C2 route-echo comparison, a read of the captured bytes against
  the reviewed constant before any payload. Every later step is recorded as `not_dispatched`.
- **Verification of a preceding receipt** is field-level. The receipt must be a closed executor
  receipt on the same chain, with the expected PRIOR stage and a genuine origin. Its claim and
  activation copy must match. Its step inventory and kinds must equal the manifest's. Each step's
  receipt and both streams must match recorded hashes. Each creation receipt must equal, field for
  field (kind, id, name, label, engine), what its producing stage's receipt, recorded argv and
  original streams establish.
- **Recorded identities.** C1 onward binds the three 688 C0 roots (token `7deaf1b3`) by device,
  inode, owner and mode; entry counts are checked only at C1. D1 to D4 require the chain engine to be
  `ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd`, the engine 690 observed. An empty or default `docker info`
  template refuses.
- **Guest sources (pinned).** The C2 probe, the C3 stage and the C4 verify run the reviewed
  `macos_guest_context.py` (sha256 `713cb10e…`) as `python3 -I -B -c <source> <mode> <spec>`. It is a
  copy of the historical `image_context.py` (sha256 `af74047f…`, unchanged), with one contract
  change: `stage` also reports the parent binding, so the C4 verify spec derives from the C3 output.
  The executor reads the source from the pinned file and refuses a changed one. The file is also one of
  the bound program files: the GO commits to its bytes, so a change after activation refuses before any
  dispatch as an input drift. The pin is the content check; the binding is what the activation commits to.
- **Derived specs.** `render_guest_spec` builds C3's spec with `build_guest_spec` from the C2 probe's
  parent binding, the chain's `token32` and the recipe hash. In C4 it builds the verify spec with
  `build_verify_spec` from the C3 output; the spec's context must carry the same `token32`. The context
  path becomes `guest_context`, and `guest_iidfile` is that path with `.iid` appended. None of these is
  caller-supplied: the plan shows placeholders, and the run derives the values from the chain. C0 is
  refused here, so in this source they are never derived.
- **Colima and Desktop.** Colima children resolve `docker` to `/usr/local/bin/docker` through an owned
  `bin/docker` link at the front of `PATH`, re-checked on every Colima step. Desktop runs with its
  pinned socket, `DOCKER_HOST`, an owned `DOCKER_CONFIG`, no `HOME` and no `DOCKER_CONTEXT`.
- **Guest.** Any waited exit of a guest step, zero or not, is its client's exit. The guest's own closure
  is unknown, so the step is `guest_unknown` and the stage stops there. A timeout, cap or monitor refusal is
  `unknown_retained`.
- **Test seams (tests only).** `TEST_SEAM` lifts the owner refusal and the static reasons inside the
  test process only, and `simulate_guest_closure` treats a clean guest exit as closed so the loop can run
  past guest steps (its receipts are labelled and refused by the chain verifier). Production defaults are off. Tests that replace predecessor verification (C3, C4, C5
  and C6), the C0 origin walk for C3 and C4, or the historical record (`HISTORICAL_C0`) say so.
- **Desktop context (D2).** `host_stage_dockerfile` writes the reviewed recipe exclusively as the build
  context's `Dockerfile`, reads it back and compares it with the pin. An existing `Dockerfile` refuses:
  nothing is adopted or overwritten.
- **Dispatch coverage (`CoverageLedgerTests`).** Every manifest step has one entry. The kinds are:
  `executed` (the adapter ran it with a real local child or the real handler, including D2's
  `host_stage_dockerfile`), `executed_stub_chain` (the adapter ran it with a stubbed predecessor),
  `helper_direct` (the handler ran on controlled inputs, not through a stage), and
  `executed_closure_simulated` (the stage loop dispatched it under the test-only closure simulation, with its
  exact argv, env, stdin and outputs asserted). C2's probe and every C4 step are `executed_closure_simulated`.
  Under the simulation a guest step whose local child exits 0 is treated as closed, and every receipt it writes
  is labelled `closure_simulated`; the chain verifier refuses such a receipt. The simulated chain is C2, then C3
  on the genuine C2 receipt, then C4 on the genuine C3 receipt. Each link is a stub `verify_chain` returning the
  genuine receipt, labelled as such. C3 and C4 also stub the C0 origin walk. The C3 and C4 guest steps run the
  pinned source through a local child that replaces only the `ssh` wrapper; the C4 build writes its iidfile in
  the guest parent, and the iidfile read returns exactly those bytes. A not_dispatched step is refusal coverage
  only. Production coverage of the same steps: a matching C2 echo halts before the probe; a mismatched echo
  refuses; an echo whose allowance runs out stops before the probe. A clean C4 verify halts before the build; a
  non-zero client exit is still `guest_unknown`; a timeout stops the stage. D2: the Dockerfile is staged, built
  and read back; a pre-existing Dockerfile refuses before the build; a build whose allowance runs out stops
  before the readback. Static admission and the owner refusal still refuse with the simulation on.
- **Not proven.** Native execution. Owner-verified activation. Exclusive volume creation (D1) and
  per-target removal re-observation (D4). The helper deadline across uninterruptible kernel waits
  (the residual above). The C2 probe through the stage loop: after the route echo's unknown closure
  the adapter halts. The probe and the C4 steps are dispatched only under the test-only closure simulation,
  never in production. C0's real run for a fresh token
  is refused until the helper bounds are demonstrated; the C0 origin is exercised by a test run only, so
  the `token32` route has not run natively. The guest closure rule for multi-effect stages, which C2 needs
  before its probe can be dispatched.

## Remaining gates

- **RecipeE and 686**: G1 and G2 wait on the Linux recipe and its artifacts.
- **Native proof of the executor**: the controls are injected and local. Running any
  admitted stage needs its own owner GO, and the executor has not run natively.
- **Colima `docker` child**: the owned link is the enforcement. Every Colima step re-checks
  the link's target and refuses a different one.
- **Nerdctl `--provenance=mode=max`**: the guest `build --help` check proves the flag
  name exists, not that the `mode=max` value is accepted. The planner checks the retained
  help text at C4 activation.
- **Activation**: each stage needs its own fresh GO with its hashes.

## Validation

Controls run through `effigy test:qualification:workspace-private:macos`
(`macos_preparation_boundaries.py`: 104 cases, injected only, no native action). Docs run
through `effigy qa:docs`.
The executor runs through `effigy test:qualification:workspace-private:macos-executor`
(`macos_stage_executor_tests.py`: 103 cases, injected local children, no native action; execution runs only under the test-only seams described in the Executor section).
Its shared primitive change is covered by `effigy test:qualification:linux-preparation`.
