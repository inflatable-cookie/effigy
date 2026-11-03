# Task-engine real-container signal packet r1

Queue task effigy#579. Status: **prepared, not executed.** Armed: **false.**

This packet is the smallest later-executable shape for the genuine missing
real-engine cells in `matrix.json`: real-container × INT/TERM/HUP/KILL
through production `RemoteExecAdapter` / `ComposeRemote::<HostCli>`, plus
strict foreign/unknown refusal, on one bounded run. It does not qualify
those cells. Boundary doubles prove packet admission only.

## Subject

- Adapter: production `ComposeRemote::<HostCli>`
  (`crates/effigy-containers/src/environment/remote.rs`).
- Engine call path: `src/runner/task_engine/remote.rs` `run_remote`.
- Identity: `engine:full-id@context` (`RemoteContainer::encode`). Full id
  is 64 ASCII hex digits. Engine is `colima-nerdctl` or `docker-compose`.
- Deadlines (original, not relaxed): ACK 60s, CALL 20s, GRACE 3s,
  SIGNAL_EXIT 10s.
- Fixture: an ordinary `run_in = "container"` hold task that leaves a
  background grandchild in its group. Not Discard. Not workspace-private
  apply. Not a fake backend substituted for the engine.
- Closure: in-container `/proc` helper rechecks boot/ns/start ticks/nonce
  before signal. Never infer closure from owner exit, no output, label
  match, reused PID, empty list without completeness, or a fake transport.
- One bounded run. Pre/post exact-owned inventory and cleanup/error
  receipts under `/Volumes/Builds/scratch/effigy/579`.

## GO contract

The operator-copied receipt is a regular 0600 file at exactly
`/Volumes/Builds/scratch/effigy/579/go.txt` (tests pass a disposable
scratch root). Required lines:

```
packet <bundle-sha256>
GO
Tom
task-engine-signals
engine <colima-nerdctl|docker-compose>
engine-id <engine>:<64-hex>@<context>
context <context>
image-digest sha256:<64-hex>
binary <absolute path>
binary-sha256 <64-hex>
source <40-hex>
window <nonempty>
inventory <absolute path under the scratch root>
```

The `packet` line is the live bundle SHA-256 of the named helpers listed
in `manifest-r1.json` (sorted name + content). A GO that quotes the wrong
bundle, a missing GO, a symlink, a world-readable mode, or a path other
than the pinned scratch file refuses before effects.

Even a well-formed GO refuses while this prepared packet stays unarmed
(`armed: false` and any `UNARMED` placeholder in the committed manifest).
`--execute` is structurally refused on this packet. Missing image or
runtime is a preparation prerequisite, not permission: the engine named
in GO must already be running and the image digest must already be
present. This packet never starts, stops, or reconfigures a VM or engine,
never pulls or builds an image, never runs `install:local`, never adopts
foreign resources, and never uses Colima profile `effigy` or a #531/#574
GO.

Client exit is never remote closure (K6). Foreign cleanup is refused.
Pattern kills (`pkill`, `killall`, aliases) are refused.

## Plan construction

A correct fully-armed *shape* (in-memory overlay in the admission battery,
or a later armed revision of this packet) constructs the exact owned
command plan in-process and does not spawn. The plan names the four
signal cells, the foreign/unknown cell, the identity format, the original
deadlines, the ordinary container hold task, and the forbidden-token
list. `spawn` is always `false` on this prepared packet.

## What passing the battery is

`test:qualification:task-engine-signals` runs `boundaries.py` against
fake backends and disposable fixture metadata. A pass is packet
admission, not native task closure.
