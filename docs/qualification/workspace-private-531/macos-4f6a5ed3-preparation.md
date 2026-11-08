# macOS 4f6a5ed3 preparation: Colima and Desktop stages

Preparation only. It arms nothing and gives no proof credit. The executable plan is
[macos-4f6a5ed3-stages.json](macos-4f6a5ed3-stages.json), manifest revision r3. Each stage
needs its own fresh GO under standing decision `63c338e2-1c50-435e-9783-e78e9d8558e6` v2,
naming the stage, the manifest, source and input hashes, the engine identity and a window
of at most 1200 seconds. The standing decision is not a blanket authority, and no GO is
recorded here.

## Revision r4 (supersedes r3 for D1 and D4)

r3 refused D1 because `docker volume create` is idempotent and the name was caller-derived, so exclusive
creation could not be established. r4 names no volume in the create. The engine generates the name, the
create prints it, and the receipt is the inspect of that printed name with this occurrence's label. D4
re-observes the engine and each exact target before its own removal, compares it with the receipt that
produced the target, and includes the complete image inventory in the before and after foreign lists.
The contract is source-qualified by controlled transports. Native execution remains under a separate
owner GO and a Nucleus proof-only launch. D2 and D3 remain held, so D4 is not admitted. Details are in the
D1, D4 and Executor sections below.

The D1 window request names the original D0 receipt (`prior_receipt`, exact path and sha256). It verifies
that receipt's complete chain at plan time and at launch. It also pins the control root and the two Docker
directories to the identities the original D0 helper recorded. D0 is not rerun and is not relabelled.

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
- **D1** owned fixture volume, named by the engine (r4). The create names no volume:
  `docker volume create --label effigy.qual=<token>` prints one engine-generated 64-hex name. Before it, a
  label observation (`volume ls --filter label=effigy.qual=<token>`) must be empty. After it, `volume inspect`
  of that exact printed name must carry the same name and this occurrence's label. Both streams are retained.
  Not a receipt unless the inspect closed. Window 300 s, reserve 60 s, storage cap 1 MiB, download 0.
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
- **D4** cleanup by creation receipt only (r4). Observations: engine (`docker info`), `ps -a`, `volume ls`,
  the complete image inventory (`image ls --all --no-trunc`), and the selected image's inspect by ID. The
  helper `select_cleanup_targets` selects only receipted creations on this engine. Each class (containers,
  volumes, images) then runs, only if it selected exactly one target: a fresh `docker info`, an exact
  `inspect` of that target, the helper `check_cleanup_target`, and the removal. The check compares every
  immutable fact the receipt kept (name, driver, scope, mountpoint, creation time, labels for volumes;
  ID, repo tags, creation time, OS, architecture, labels for images; ID, name and label for containers).
  Any drift, missing fact, foreign label or replacement refuses before that removal. After the removals:
  `ps -a`, `volume ls` and the complete image inventory. Foreign lists must equal the lists observed
  before the first removal. Removals are `docker rm`, `docker volume rm` and `docker image rm` by exact
  ID or name, never `-f`, never prune. Stage budget 600 s, reserve 60 s, step total 460 s. The pre-removal
  check is not an atomic conditional delete: the engine can change between the check and the removal, and
  the post-compare is the only detection after that. Each class with no selection is recorded as skipped.


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

- Manifest r4 changes, D1 and D4 only. C0 and D0 are byte-identical to r2 and r3. D1 replaces
  `d1-volume-absent` with the label observation `d1-label-absent` and takes the name from the create's
  output, so no argv carries a caller-chosen name. D4 gains per-class observation, inspect and check steps
  before each removal, `d4-observe-images` and `d4-post-images`, and a budget of 600 s (step total 460 s).
  Each stage's step total stays under its budget.
- Why the engine-named create (r4). Docker's daemon chooses `stringid.GenerateRandomID()` when the name is
  empty, and the CLI prints the name the API returns. The API has no 'created' flag: the store's internal
  `created` boolean is not sent, and an existing volume with the requested name is returned without a
  label comparison. Attribution therefore rests on three things, not on the API alone: the engine-generated
  64-hex name; label absence observed before the create; and the inspect of that exact name carrying this
  occurrence's label. A name collision needs a random 256-bit collision, and a label collision needs a
  same-user process that already holds this token. The caller never chooses the name. Source reads:
  moby `volume/service/service.go` (`GenerateRandomID` for an empty name), `volume/service/store.go`
  (`create` returns an existing volume with `created=false`, no label comparison), and docker/cli
  `cli/command/volume/create.go` (prints `res.Volume.Name`). Native confirmation that the daemon's create
  prints a 64-hex name is a D1 proof obligation; a different shape refuses at the create post-check.

## Executor (source only)

`macos_stage_executor.py` is the stage runner. This source preparation starts no native backend.

### Desktop readiness before D1

The prospective `plan-ready BINDINGS OPTIONS` request binds the original D0 receipt, its three
directory identities, the expected engine, the exact installed Desktop bundle and launcher, the
source-reviewed Docker CLI, and the freshly observed identity of the existing `Docker.raw`. It
composes a readiness prefix with the existing D1 window. The proof launch has one monotonic deadline
of at most 900 seconds, including the 60-second reserve; readiness has at most 600 seconds and D1
has at most 300 seconds or the remaining allowance. The readiness and app-start claims are distinct
from D1's claims. A spent or uncertain claim is not replayable. The original D1 creation and
inspection receipt still determines whether a volume is attributable.

The fixed CLI context-name query is the sole default-config exception. Its normal config loader may
internally decode stored auths. It does not retrieve credential values, call helpers, authenticate or
contact a registry. The adapter does not parse, copy or expose the global config. The exact request's
`requiresOperatorGo` discloses this loader behaviour and both possible effects: starting Desktop and
creating an empty volume. A later Nucleus qualification must pin that request and explicitly obtain
GO for that scope; this source preparation grants no native action. The pinned CLI is Docker 29.8.0,
sha256 `4fb51be4a4e9389b942ecbf52901a962bdccdaa38275e6de4d353df33981d600`, built from
[`88096ef`](https://github.com/docker/cli/tree/88096ef00576baf72a9cb45caa45c0544c40e0a7).
Its [context show](https://github.com/docker/cli/blob/88096ef00576baf72a9cb45caa45c0544c40e0a7/cli/command/context/show.go)
prints the selected name; its [config loader](https://github.com/docker/cli/blob/88096ef00576baf72a9cb45caa45c0544c40e0a7/cli/config/configfile/file.go)
can decode auth fields. A synthetic config and helper sentinel exercise this exact CLI without a
daemon or real credentials. The shared stage supervisor has one narrowly restricted context-query
mode: it retains the original name and empty stderr only when the command succeeds with a valid name.
It suppresses all output on other results, since the CLI can include configuration aliases in errors;
the process, waited exit and suppression disposition remain recorded. This avoids writing unexpected
config text into evidence while keeping the existing responsive supervisor and its bounds.

The context is read before and after readiness. Startup is refused unless it is already
`desktop-linux`, because [Docker documents that Desktop selects that context at startup](https://docs.docker.com/desktop/setup/install/mac-permission-requirements/).
The already-ready path skips startup and requires the selection to remain unchanged. Neither check
prevents concurrent external changes. The fixed launcher opens only `/Applications/Docker.app`;
its exit does not establish readiness. An initial bounded engine read makes the startup decision:
a matching engine skips startup, while only the two recognised local connection errors permit a
separately claimed start, including when the socket exists but refuses connections. Other failures
and changed engine facts refuse. The recognised error requires exit 1 and the exact stderr; stdout
is retained and may contain client facts, as in the spent 702 attempt. Nested helper timers preserve
the earlier outer deadline. The owned
socket and a matching engine response establish readiness. Drift, missing telemetry, unexpected
failed commands and exhausted reserve stop before D1.

The held, no-alias Desktop disk is sampled before, during and after the operation against a 512 MiB
allocated-growth threshold, alongside the private-root monitor. Samples are not a hard filesystem
quota. The launcher process group does not contain the app or daemon's entire lifetime; Desktop may
remain running after a timeout. No descendant-closure or complete resource certificate is claimed.
Fixture simulations are labelled and their D1 receipts cannot be consumed as native chain evidence.

- **Activation.** The ordinary `execute_stage` entry refuses. The separate
  `qualification:macos:proof-only -- REQUEST SHA256` entry is only for an actual Nucleus proof-only
  launch at the pinned commit and inputs, after native operator GO. Nucleus must record that execution;
  the program cannot independently establish its provenance against another process of the same user.
  A request file grants no authority. The entry checks its bindings and makes an exclusive input-hash claim.
- **Prospective proof binding.** A later Nucleus qualification must pin the accepted commit, this exact
  selector and the SHA-256 of the read-only request. The request has exactly `stage`, `activation_path`,
  `activation_sha256`, `bindings`, `evidence_root`, `inputs_sha256`, `manifest_sha256` and
  `source_sha256`, plus `inputs`. `inputs_sha256` is the canonical hash of `inputs`: exact reviewed
  repository-relative paths and content hashes, plus canonical machine paths and hashes for the interpreter
  and both tools. The executor independently derives and compares the entire set before claiming. It retains
  the actual canonical runtime input map in the stage receipt and rehashes it before each launch. The planner still
  needs a fresh token and private roots, exact request and activation bytes and their hashes, current
  tool hashes and an expiry window. The operator question must name the selected stage's native effects
  (both claim and create private files and directories; C0 invokes Colima's start help, while D0 reads
  the Desktop endpoint and engine inventory). No question,
  GO or proof execution is created by this source task.
- **Preparing a D0 binding without native effects.** From the exact merged preparation checkout, choose
  a fresh eight-hex token. Create a read-only bindings JSON under the preparation root using
  `macos_preparation.control_paths(token)` filtered to `SLOTS`, plus `desktop_host=DESKTOP_HOST`,
  `token`, `check_source=CHECK_SOURCE`, and `route_echo=ROUTE_ECHO`. For example, after checking
  `a19b3c4d` is fresh and selecting a private preparation directory, make that file with:

  ```sh
  python3 -I -B -c 'import json,sys; sys.path.insert(0,"docs/qualification/workspace-private-531"); import macos_preparation as p; t="a19b3c4d"; b={k:v for k,v in p.control_paths(t).items() if k in p.SLOTS}; b.update(desktop_host=p.DESKTOP_HOST,token=t,check_source=p.CHECK_SOURCE,route_echo=p.ROUTE_ECHO); print(json.dumps(b,sort_keys=True))' > /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  chmod 400 /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  effigy qualification:macos:proof-only -- plan D0 /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  ```

  The paths and token are examples to be checked for collisions, not a reservation. Run
  `effigy qualification:macos:proof-only -- plan D0 /absolute/path/to/bindings.json`.
  This only reads the checkout, interpreter and pinned tool files; it prints the stable `inputs`,
  hashes, candidate head and bindings hash, and creates no roots, claims or activation. Copy those
  exact fields into the request and activation: the request uses `stage`, `bindings`, `inputs`,
  `inputs_sha256`, `manifest_sha256`, `source_sha256`, `activation_path`, `activation_sha256`, and
  `evidence_root`. The activation uses the printed hashes, `candidate_head` and `bindings_sha256`,
  plus `decision`, `decision_version`, `stage`, a unique `activation_id`, `issued_utc`, `expires_utc`,
  and `scope: single-activation` as checked by `macos_preparation.check_go`. Hash the read-only
  activation first, then place that hash in the read-only request and hash the request. The future
  selector is `qualification:macos:proof-only -- /absolute/path/to/request.json REQUEST_SHA256`;
  neither file names the future checkout. The pinned candidate head remains H; Nucleus separately pins
  the exact merged preparation commit K and selector/request hashes and records the real operator answer.
  D0 requires its token's control directory to exist privately, its `d/docker-config` and `d/ctx`
  paths to be absent, a private evidence root and private activation and claims directories under the
  configured scratch root. The Desktop endpoint, engine identity, tool bytes, all input rechecks,
  one-use claims and other runtime gates still apply. A real operator GO and Nucleus proof-only launch
  remain necessary; this plan grants neither.
- **Launch-time window recipe.** The legacy exact-file route above remains supported. For a delayed
  approval, provision the private activation, claims and evidence directories and the C0 parent or D0
  control root first. Create an options JSON containing exactly `activation_id` (fresh, 8–64 safe
  characters), `duration_seconds` (integer greater than the stage reserve and at most its stage
  seconds and 1200), and canonical `evidence_root`. With the bindings JSON described above, run:

  ```sh
  effigy qualification:macos:proof-only -- plan-window D0 /absolute/path/to/bindings.json /absolute/path/to/options.json
  ```

  Use C0 in place of D0 when appropriate. Save the output unchanged as a read-only request under
  scratch, retain its exact bytes and SHA-256, and pin the merged K, selector
  `qualification:macos:proof-only -- /absolute/path/to/request.json REQUEST_SHA256` and request hash
  in Nucleus. Null root identities are provisioning preconditions: rerun the plan after provisioning,
  before freezing the request. The D0 `absent_paths` are fixed creation destinations and must remain
  absent through launch; existing directories and symlinks refuse before the request claim. Check all
  named destinations are unused. The native operator answer and
  actual proof-only launch remain necessary. At launch the request claim is spent even on an uncertain
  or partial write; retain the claim, activation, proof evidence and stage evidence, and never retry it.
  The runtime timestamps come from launch, not from the preparation file. Source-test simulations confer
  no native proof credit.
- **Static admission.** C0 and D0 have no remaining static helper refusal. Their actual execution still
  needs the external proof-only precondition and all runtime checks. Other stages remain refused:
  - C1, C4 and D2: the download cap is payload bytes, wire traffic is not counted, and the manifest
    records no pinned image size.
  - C2: a multi-effect guest stage. The guest closure is unknown after the route echo, so the probe
    is not reachable through this adapter.
  - C3: `guest_spec` derives from the C2 probe output and the `token32` that C0 mints.
  - D1 is statically admitted (r4). Its ordinary execution still refuses at the owner mechanism; its
    proof-only window names the original D0 receipt (see Executor).
  - D4: its per-target contract is source-qualified, but its predecessor D3 needs D2 (download cap),
    so D4 cannot run until D2 and D3 qualify.
  - D3, C5 and C6: unadmitted because their predecessor is.
- **C0 (fresh tokens).** Its handlers are `verify_tool_sha` (the live colima and docker binaries
  against the sha256 the pinned 688 C0 record names) and `verify_tranche_summary` (the tranche
  summary against the sha the same record names). C0 creates its own roots under the scratch root
  and refuses on any collision. For the recorded token it refuses: that C0 is historical, and C1
  binds it. Each claimed C0 activation mints one
  `token32` (32 lowercase hex), written exclusively as `token32.json` and recorded by hash in the receipt
  as `origin`. It is the chain's origin: `origin_token32` reads it back from the verified C0 receipt before
  C3 and C4 claim. A historical C0 minted none, so C3 and C4 refuse for the recorded token. That receipt also
  supplies the owned roots a fresh chain's later C stages must re-observe (effigy#698): its creation step's
  evidence, identity rows and pinned inventory are the binding, with no caller path and no 688 record.
- **C1 binds the pinned 688 C0 record, by identity.** The record is
  `macos-4f6a5ed3-activation-7deaf1b3/C0.stage.json` (sha256 `15a5373e…`) and its tranche summary
  (`macos-metadata-rfzuiadz/summary.json`, sha256 `cdfb7fb6…`). C1 accepts it only when the bytes
  match the pinned sha, the record is a passed, zero-share, safe C0 for the recorded token, the live
  colima and docker binaries match the recorded tools, and the recorded roots match the recorded
  identities. This is a binding, not a receipt: C1 claims its own activation, and nothing is
  replayed, adopted or recreated. A fresh token takes no part of this: its C1 consumes its own verified
  executor C0 receipt, and the 688 records are never consulted for it.
- **Plan.** `plan` is pure. Each stage resolves to its argv, pinned env, timeouts, caps, runtime
  preconditions, the sources it derives at run time (`runtime_derived`), and both its global and
  static admission reasons. Derived and unpinned slots render as explicit placeholders.
- **Bindings.** Checked against the reviewed paths for the token, the pinned socket URL and the
  pinned check program. A caller who supplies a derived slot or an unpinned source refuses.
- **Inputs.** Program files, selector configuration, manifest, interpreter, recipe and tool binaries
  are bound by the activation. The fixed helper checks its input hashes before and after its work.
- **Claims.** `macos-stage-claims/<activation_id>.claim`, exclusive, under a provisioned directory.
  A missing directory refuses. The executor never creates one under the scratch root.
- **Pins.** The evidence, claims, activation and owned directories, and for D1 to D4 the private
  Docker config and context directories, are pinned by canonical path, owner, mode 0700, device and
  inode. Each is rechecked before every step. C0 pins its roots once it creates them.
- **Helper supervision.** C0 and D0 use a fixed child entry through the shared stage supervisor.
  The parent samples the same absolute deadline and live counter while the child runs, including a
  zero limit. A cap, timeout, lost counter or invalid result stops the stage. Polling is observation,
  not a physical quota. The parent verifies and holds created directory descriptors and records both
  original streams, the observed process group, spawn time and waited exit. Cleanup waits only through
  the stage reserve; an unreaped child remains unknown with its identity recorded. An uninterruptible
  kernel wait is not promised to terminate. Later-stage helpers remain behind their existing refusals.
- **After an unknown guest closure.** A guest step whose closure is unknown halts later effects. The
  only helper that may still run is the C2 route-echo comparison, a read of the captured bytes against
  the reviewed constant before any payload. Every later step is recorded as `not_dispatched`.
- **Verification of a preceding receipt** is field-level. The receipt must be a closed executor
  receipt on the same chain, with the expected PRIOR stage and a genuine origin. Its claim and
  activation copy must match. Its step inventory and kinds must equal the manifest's. Each step's
  receipt and both streams must match recorded hashes. Each creation receipt must equal, field for
  field (kind, id, name, label, engine), what its producing stage's receipt, recorded argv and
  original streams establish.
- **Recorded identities.** C1 onward binds the owned roots by device, inode, owner, mode 0700 and (at C1
  only) their first-use entries. Each root is opened no-follow once: that descriptor supplies the comparison and
  then becomes the stage's held pin, so no path is re-observed between them and a same-user swap in that gap is
  pinned as the opened directory and refused at the next recheck before any step. A fresh chain's roots come
  from its own verified C0 creation record: the paths of the token's supported shape, and the identities and
  first-use entries that record's `make_owned_dir` evidence and pinned inventory name (effigy#698). The
  recorded token instead keeps the three pinned 688 roots (token `7deaf1b3`) by count; that route never
  receives a minted token32 or a fresh binding, and a fresh chain never adopts the 688 identities. D1 to D4
  require the chain engine to be `ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd`, the engine 690 observed. An empty or
  default `docker info` template refuses.
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
  caller-supplied: the plan shows placeholders, and the run derives the values from the chain. C2
  remains unadmitted, so these paths are not activated by this source change.
- **Colima and Desktop.** Colima children resolve `docker` to `/usr/local/bin/docker` through an owned
  `bin/docker` link at the front of `PATH`, re-checked on every Colima step. Desktop runs with its
  pinned socket, `DOCKER_HOST`, an owned `DOCKER_CONFIG`, no `HOME` and no `DOCKER_CONTEXT`.
- **Guest.** Any waited exit of a guest step, zero or not, is its client's exit. The guest's own closure
  is unknown, so the step is `guest_unknown` and the stage stops there. A timeout, cap or monitor refusal is
  `unknown_retained`.
- **Test seams (tests only).** `TEST_SEAM` lifts the owner refusal and the static reasons inside the
  test process only, and `simulate_guest_closure` treats a clean guest exit as closed so the loop can run
  past guest steps (its receipts are labelled and refused by the chain verifier). Production defaults are off. Tests that replace predecessor verification (C1, C2, C3, C4, C5
  and C6), the C0 origin walk for C3 and C4, or the historical record (`HISTORICAL_C0`) say so. The fresh-chain
  root binding (`FreshChainRootTests`) replaces none of them: its C0, C1 and C2 runs are genuine receipts, and
  only the local children that stand in for the tools are controlled.
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
- **Not proven.** Native execution or an actual Nucleus proof execution and native GO. Native confirmation
  that the engine's volume create prints one 64-hex name and that its inspect carries the anonymous and occurrence
  labels as the source reads predict (D1). Native per-target re-observation and removal (D4). Termination of an
  uninterruptible kernel wait. The C2 probe through the stage loop: after the route echo's unknown closure
  the adapter halts. The probe and the C4 steps are dispatched only under the test-only closure simulation,
  never in production. The C0 origin is exercised by a test run only, so
  the `token32` route has not run natively. The fresh-chain root binding is proven against local fixture C0
  receipts only: no native C0 has created a root that a later C stage then re-observed. The guest closure rule
  for multi-effect stages, which C2 needs before its probe can be dispatched.
- **Create-unknown volumes.** A D1 create whose closure is unknown, or whose inspect failed, leaves an
  engine-named volume with the occurrence label and no receipt. D4 never removes it. The stage stops
  and retains the create stream, so the volume is reported for manual removal.


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
(`macos_preparation_boundaries.py`: 109 cases, injected only, no native action). Docs run
through `effigy qa:docs`.
The executor runs through `effigy test:qualification:workspace-private:macos-executor`
(`macos_stage_executor_tests.py`: 166 cases, injected local children, no native action; execution runs only under the test-only seams described in the Executor section).
Its shared primitive change is covered by `effigy test:qualification:linux-preparation`.
