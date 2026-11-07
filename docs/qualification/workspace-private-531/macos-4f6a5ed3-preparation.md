# macOS 4f6a5ed3 preparation: Colima and Desktop stages

Preparation only. It arms nothing and gives no proof credit. Each stage in
[macos-4f6a5ed3-stages.json](macos-4f6a5ed3-stages.json) needs its own fresh GO
under standing decision `63c338e2-1c50-435e-9783-e78e9d8558e6` v2, naming the
stage, manifest/source/input hashes and a window of at most 1200 seconds. The
standing decision is not a blanket authority, and no GO is recorded here.

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

This is local metadata. It is not a running-engine observation, not a daemon
observation, and not guest identity. Desktop reachability and the Desktop engine
baseline are unknown until the D0 stage runs under a GO.

## Paths (fresh, owned, pure-checked)

Control root `/Volumes/Stash/scratch/effigy/mac-4f6a-<token>`, where the token is
eight lowercase hex minted at C0. Colima home is `<control>/c`, and the shared
root `<control>/s` is empty and planned as the only permitted share.

Profile `effigy-4f6a-qual`. The boundary test computes these socket paths:

- `ha.sock` and `ga.sock`: 87 bytes each.
- `docker.sock`: 78 bytes.

The limit is below 104 bytes, so the margin is 16 bytes. A collision, an alias,
a Builds path, or a socket at or over the limit refuses before any effect. Adopting
`effigy`, `effigy-release` or any historical 531 resource is refused.

## Stage order

Colima (backend `colima`):

- **C0** admission: `colima start --help` to confirm the zero-share form, the tool
  hash check, and the pure path/profile checks.
- **C1** start: `colima start -p effigy-4f6a-qual --runtime containerd --vm-type vz
  --arch aarch64 --cpu 2 --memory 2 --disk 20 --root-disk 20 --mount none`, with
  `COLIMA_HOME=<control>/c` and `LIMA_HOME`, `DOCKER_HOST` and `DOCKER_CONTEXT` unset.
- **C2** identity: `colima status --json` and a read-only guest probe. The guest
  parent identity is observed, not guessed.
- **C3** stage Dockerfile: the reviewed `image_context.stage` helper, with the exact
  recipe bytes on stdin. It writes the file O_EXCL at 0600 and returns a receipt.
- **C4** build: one local-path `nerdctl build` against the owned guest context.
  Docker config is an uncreated child of that context. The stage uses no `-`
  context, no login and no secret. C3's receipt is required first.
- **C5** disposable check: `nerdctl run --pull=never --rm --network none`.
- **C6** observe only. A VM stop is not in this manifest and needs its own GO.

Desktop (backend `desktop`), using the local `desktop-linux` context:

- **D0** baseline: context inspect, `ps -a`, network and volume lists. Each resource
  is split by exact `effigy.qual=<token>` label and name prefix into owned and
  foreign. Foreign resources are never touched.
- **D1** owned volume `effigy-4f6a-qual-<token>`, labelled.
- **D2** runtime image into the Desktop engine, built from the same Dockerfile bytes
  as C3. Host staging is helper-only and uses a receipt.
- **D3** disposable check, labelled and `--rm`.
- **D4** cleanup by exact owned IDs only. No prune, and no foreign or unlabelled removal.

Gated, not activatable here: **G1** E14 source placement (`candidate-4f6a5ed3`,
tree `dcb4c791`), **G2** artifact, sidecar and producer receipt placement, and
**G3** the proof run. These wait on RecipeE (686 Linux preparation) and on a
genuine byte-identical receipt. No producer receipt is synthesized.

## Closure, retention and failure

- Every command is an argv list, never a shell string. Timeouts are numeric and the
  output cap is 256 KiB. Retries are zero.
- Closed means a waited exit 0, with no deadline or cap. A deadline, a cap or a
  missing waited exit is **Unknown and retained**. Unknown never replays. A host
  kill does not prove remote closure.
- Each intent id is claimed once. A reused intent refuses before effects.
- Old 531 markers and consumed activations stay history, and the boundary test
  refuses them.
- Retain intent, GO copy, every stream, PID, waited exit, and hashes under the
  run's private child. Keep receipts outside Queue cleanup.

## Decisions

- Zero host shares is the preferred C1 form. The shared root is empty and used
  only if `--mount none` is not expressible in 0.10.3, and then the GO must name it.
- The Colima build is a local-path build from a helper-staged file, not a tar-stdin
  context. The installed nerdctl 2.3.1 rejects tar-stdin contexts, so this
  avoids that refused form. No builder switch is made.
- A new Docker context is not an isolated engine. D0 records the Desktop baseline
  and does not treat the context name as proof of isolation.
- `docker context inspect` reads context metadata, and the CLI may read its own
  `config.json` at startup. The tranche does not parse `config.json`, and it does
  not retain TLS or credential contents.

## Remaining gates

- **RecipeE and 686**: G1 and G2 wait on the Linux recipe and its artifacts.
- **Flag form**: the zero-share `--mount none` is checked at C0 against
  `colima start --help`.
- **Activation**: each stage needs a fresh GO, recorded with its own hashes.

## Validation

Controls run through `effigy test:qualification:workspace-private:macos`
(`macos_preparation_boundaries.py`: 28 cases, injected only, no native action).
Docs run through `effigy qa:docs`.
