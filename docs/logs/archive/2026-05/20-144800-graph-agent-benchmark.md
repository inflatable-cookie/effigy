# Graph Agent Benchmark

Date: 2026-05-20
Roadmap: `g07.076`
Batch card: `1026`

## Scope

Built a repo-owned benchmark for practical graph adoption across fixture and
live repo shapes.

## What changed

- added `perf:graph-agent-benchmark`
- added fixture-backed benchmark repos for:
  - split owner vs test-owner
  - redirect behavior ownership
  - migration validation ownership
- added optional live targets for:
  - Effigy
  - `~/Dev/projects/underlay-reference`
  - `~/Dev/legacy/sites/brains`
- emitted:
  - `.effigy/perf/graph-agent-benchmark/README.md`
  - `.effigy/perf/graph-agent-benchmark/summary.json`

## Result

Fixtures:

- split-owner behavior query resolved via graph with the expected edit target
  and test hints
- exact token lookup stayed `rg`-preferred, which is the intended result
- redirect and migration-validation fixtures resolved via graph without fallback

Live repos:

- Effigy shell-exit prompt query resolved via graph to
  `src/runner/container_command/closeout.rs`
- Underlay admin validation query resolved via graph to
  `acme-client/src/commands/admin/validation-commands.ts`
- decodelabs brains codebase-hook query resolved via graph to
  `legacy/directory/front/hooks/_nodes/HttpCodebase.php`

## Notes

- the benchmark does not claim percentage wins
- optional live repos are skip-safe
- the fixture lane remains runnable on machines without private local repos
- exact-token searches are still explicitly allowed to stay `rg`-first

## Validation

- `effigy perf:graph-agent-benchmark /tmp/effigy-runner-1025/debug/effigy`
- reviewed:
  - `.effigy/perf/graph-agent-benchmark/README.md`
  - `.effigy/perf/graph-agent-benchmark/summary.json`

## Vision Target Delta

- primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- moved: no practical cross-repo proof -> fixture-backed and skip-safe benchmark
  with machine-readable output
- remains open: `1027` skill and active-doc guidance update, then `1028`
  closeout
