# macOS 4f6a5ed3 preparation: Colima and Desktop stages

Preparation only. It arms nothing and gives no proof credit. The executable plan is
[macos-4f6a5ed3-stages.json](macos-4f6a5ed3-stages.json). Each stage needs its own
fresh GO under standing decision `63c338e2-1c50-435e-9783-e78e9d8558e6` v2, naming
the stage, the manifest, source and input hashes, the engine identity and a window of
at most 1200 seconds. The standing decision is not a blanket authority, and no GO is
recorded here.

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

## Paths (fresh, owned, pure-checked)

Control root `/Volumes/Stash/scratch/effigy/mac-4f6a-<token>`, with `<token>` eight
lowercase hex minted at C0. Colima home `<control>/c`. Qualification root
`<control>/s`, empty and 0700, the only permitted host share. Desktop config
`<control>/d/docker-config` and context `<control>/d/ctx`, both owned and empty.

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
- Colima steps pin `COLIMA_HOME` and unset `LIMA_HOME`, `DOCKER_HOST` and `DOCKER_CONTEXT`.
- Desktop steps pin `DOCKER_HOST` to the observed endpoint and `DOCKER_CONFIG` to the
  owned empty config, and unset `DOCKER_CONTEXT`. The steps never name a context, and
  they never read the user's context store or `config.json`. Each step runs against
  the observed socket, not a context name.
- Helper steps name one of a fixed set of host actions, each a pure check or a receipt
  producer. No helper runs a shell.

## Stage order

Colima (`colima`):

- **C0** admission: tool hashes against the tranche, tranche summary check, intent
  claim, owned roots created 0700 and empty, and `colima start --help` to confirm the
  zero-share form.
- **C1** start: `colima start -p effigy-4f6a-qual --runtime containerd --vm-type vz
  --arch aarch64 --cpu 2 --memory 2 --disk 20 --root-disk 20 --mount none`.
- **C2** identity: `colima status --json`, the read-only guest probe (the reviewed
  `image_context.py` probe source, which reports the guest parent binding from the
  guest home), and a route echo. The route echo sends a hostile string through
  `colima ssh` and compares it byte-for-byte, so the JSON and source arguments that
  follow have proven quoting.
- **C3** stage Dockerfile: `render_guest_spec` builds the stage spec from the C2
  parent binding, the name `effigy-531-image-<token32>` and the recipe sha. Then the
  existing helper's `stage` runs with `runtime.Dockerfile` (sha `d290f70f`) on stdin.
- **C4** verify and build: `verify` checks the stage receipt. Then one local-path
  `nerdctl build` against the owned guest context, with `DOCKER_CONFIG` set to an
  uncreated child of that context.
- **C5** disposable check: `nerdctl run --pull=never --rm --network none`, running the
  pinned check source.
- **C6** observe only. A VM stop is not in the manifest and needs its own GO.

Desktop (`desktop`), with the pinned env above:

- **D0** endpoint, owned dirs and baseline. The helper checks the tranche's endpoint,
  then `docker info` gives the engine ID for the GO, and `ps`, `network` and `volume`
  are listed. Owned resources split from foreign by exact label and name.
- **D1** owned volume `effigy-4f6a-qual-<token>`, labelled `effigy.qual=<token>`.
- **D2** runtime image into the Desktop engine, from a receipt-bound Dockerfile
  staged in `<control>/d/ctx`. The image carries the same recipe bytes as C3.
- **D3** disposable check with the same pinned source, labelled and `--rm`.
- **D4** cleanup by exact owned IDs only, with no prune. Each step runs only when its
  owned set is non-empty. A foreign or unlabelled ID is never removed.

A new Docker context is not an isolated engine. D0 records the Desktop baseline and
does not treat the endpoint or context name as proof of isolation.

## Proof-entry bindings (G1–G3, gated)

The frozen 627 artifacts are bound in the manifest by path and sha256. Each file was
verified present with its index sha256 on 2026-11-07. Bound, not run:

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

- Closed means a waited exit 0 with no deadline or cap. A deadline, a cap or a missing
  waited exit is **Unknown and retained**. Unknown never replays. A host kill does not
  prove remote closure.
- Each activation's intent is claimed once. A reused intent refuses before effects.
- Old 531 markers and consumed activations stay history, and the boundary test refuses
  them. The guest name `effigy-531-image-<token32>` is the helper's shape. The token is
  fresh, and no 531 receipt is reused.
- Retain intent, GO copy, every stream, PID, waited exit and hash under the run's
  private child. Receipts stay outside Queue cleanup.

## Decisions

- Zero host shares is the preferred C1 form. The qualification root is the only
  permitted share, used only if `--mount none` is not expressible in 0.10.3, and the
  GO must then name it.
- The Colima build is a local-path build from a helper-staged file, not tar-stdin.
  Installed nerdctl 2.3.1 refuses tar-stdin contexts, so no builder switch is made.
- Desktop pins the observed endpoint with `DOCKER_HOST` and an owned empty
  `DOCKER_CONFIG`, not a context name. This keeps every foreign context and every
  credential out of the stage.
- The guest name stays under the helper's `effigy-531-image-` shape. The helper
  enforces it, so the plan binds the name instead of changing the helper.

## Remaining gates

- **RecipeE and 686**: G1 and G2 wait on the Linux recipe and its artifacts.
- **Flag form**: the zero-share `--mount none` is checked at C0 against `colima start --help`.
- **Nextest fixture path**: G3 waits on the metadata-only proof described above.
- **Activation**: each stage needs its own fresh GO with its hashes.

## Validation

Controls run through `effigy test:qualification:workspace-private:macos`
(`macos_preparation_boundaries.py`: 44 cases, injected only, no native action). Docs run
through `effigy qa:docs`.
