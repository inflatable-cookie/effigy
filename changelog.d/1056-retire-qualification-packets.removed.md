Removed the qualification packets and replaced them with one small proof per
supported backend. `docs/qualification/` now indexes the retired
ordinary-Compose, Acow, workspace-subset, private-workspace, task-engine signal
and consumer packets by reviewed commit and keeps only the changelog migration
record; the private workspace proof language retires with the guest-to-host
channel. Each backend proof (`effigy qualification:backend:colima`, `:desktop`,
`:linux`) brings up a real workspace with ordinary Effigy commands, runs one
representative task, tears the stack down, checks the engine inventory returned
to its starting state and writes one verdict file below
`/Volumes/Stash/scratch/effigy/evidence/<run>/`. Consumer continuity is the
owner-run checklist in `docs/qualification/consumer-continuity-checklist.md`,
real-engine signal settlement stays with `test:task-engine:kill-matrix`, and the
`signal-qualification-fixture` and `qualification-observer` Cargo features are
gone.
