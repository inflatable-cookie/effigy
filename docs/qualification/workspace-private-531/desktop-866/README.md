# Desktop session evidence, effigy#866

The one unsealed session at harness head `e66ba085c` passed the new CLI and
Compose preflights and **applied Compose successfully**. Its owned container
was running. It then failed during workspace publication/selection, before
committing a selected session. The 12.174-second attempt ran on 2026-11-11 at
00:28:43–00:28:55 UTC. No full-session or sealed qualification credit is claimed.

Fresh D0–D3 preparation closed on token `4636d9b7`. Both preparation and session
recorded the existing `/usr/local/bin/docker` symlink, its real bundle target
and SHA-256. Their fresh private configs referenced Desktop's existing plugin
directory. Docker 29.8.0 and Compose v5.5.1 both returned zero under those private
environments. No Docker CLI copy, global config edit or context change was used.

`result.json` retains the original apply event (exit 0), artifact and evidence
hashes, test/build results, failure stage and cleanup summary. The original
apply-child stderr is unavailable because its required `Terminal::Discard`
closes the stream before exec. The retained activation CLI stderr is empty;
`selector-stderr.txt` preserves the entry's failure. After review `5851771381`,
the operator amended the Nucleus-held brief to version 2
(`ff8cb7e59ecbb42e654436d1a7dda156ca2fb6fb5703b3f264286953ff73ec29`).
It explicitly preserves Discard and replaces raw apply-child stderr with the
apply exit receipt, exact caller stderr, selector stderr, CLI/Compose preflight
receipts and backend-window observation. Those records describe the original
attempt; the empty caller file is never presented as the child's stream.
The additional read-only reproduction applies to an apply failure; apply
succeeded here, and the later publication/selection failure is recorded
separately. No matching Desktop lock-directory/backend lines occurred in the
session window. This amendment authorizes no replay; none was performed.

The exact retained supervisor's stop request remained pending after 30 seconds
(`stop-refusal.json`). Its boot/PID/start identity was checked twice before
SIGKILL. The session container, two mask volumes and network were verified by
scope/checkout labels and removed by exact identity. D4 closed and removed only
its receipted image and volume. Complete final shared-engine inventories match
task start; all four journalled owners are closed with complete cleanup, and
none of 12 recorded process identities is live. There was no session retry,
engine restart, settings change or Colima profile action.

Bulk evidence remains at
`/Volumes/Stash/scratch/effigy/feeafc9c/desktop-866-SMRTjCd1`; preparation is its
sibling `desktop-smoke-xncvg97e`. Nucleus owns this run root's later removal.
The committed files preserve the concise result independently of that cleanup.
