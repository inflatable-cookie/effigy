#!/usr/bin/env bash
#
# Maintained heavy-admission entry point for authorized in-place release
# preparation, reached through the `effigy release:prepare` selector.
#
# The selector is the admission surface: `config/tasks.toml` declares the task
# `admission = "heavy"`, so the host-run scheduler admits the whole operation
# before any compilation or file mutation. This script only forwards a
# validated subset of `effigy release prepare` options to the Effigy built from
# the invocation checkout. It never selects `release execute`, `release
# resume`, tag, or publish; it never accepts a `--repo` override; and it never
# evaluates an arbitrary command.
#
# Usage (via the selector):
#   effigy release:prepare --plan                       # outer, no-write task plan
#   effigy release:prepare -- --plan                    # inner `release prepare --plan`
#   effigy release:prepare --yes --version 0.14.0       # mutating preparation
#
# The outer `--plan` is the runner's task plan: it resolves and prints the task
# command without running it. The inner plan must be separated with `--`, like
# any forwarded task argument.
#
# See docs/knowledge/contracts/release.md and
# docs/guides/051-release-orchestration.md.

set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
release:prepare forwards to `effigy release prepare` and accepts:
  --yes                 explicitly request a mutating preparation (requires --version)
  --version <SEMVER>    explicit target version for the preparation
  --plan, --dry-run     preview the prepare plan without mutations
  --check-gates         accepted; a mutating preparation always enforces it
  --json                request the built-in JSON document (pass as `-- --json`)

The subcommand is fixed to `release prepare`. `execute`, `resume`, tag,
publish and `--repo` are never forwarded. `effigy release:prepare --plan` is
the outer, no-write task plan; pass `-- --plan` to run the inner prepare plan.
USAGE
}

refuse() {
  printf 'release:prepare: %s\n' "$*" >&2
  exit 2
}

plan=false
yes=false
json=false
check_gates=false
version=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --yes)
      yes=true
      ;;
    --plan|--dry-run)
      plan=true
      ;;
    --check-gates)
      check_gates=true
      ;;
    --json)
      json=true
      ;;
    --version)
      [ "$#" -ge 2 ] || refuse "--version requires a value"
      version=$2
      shift
      ;;
    --repo|--repo=*)
      refuse "--repo is not accepted: preparation always targets the invocation checkout"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      # The runner strips the passthrough delimiter before `{args}`, so a
      # literal `--` here would be forwarded to the built-in and rejected.
      refuse "unexpected \`--\` argument; pass options directly"
      ;;
    --*)
      refuse "unknown option: $1"
      ;;
    *)
      refuse "unexpected argument: $1"
      ;;
  esac
  shift
done

if [ "$plan" = true ] && [ "$yes" = true ]; then
  refuse "--plan/--dry-run and --yes are mutually exclusive"
fi
if [ "$plan" = false ] && [ "$yes" = false ]; then
  refuse "no operation requested; pass --yes --version <SEMVER> to prepare or --plan to inspect"
fi
if [ "$yes" = true ] && [ -z "$version" ]; then
  refuse "a mutating preparation requires an explicit --version <SEMVER>"
fi
if [ -n "$version" ]; then
  case "$version" in
    -*) refuse "--version value must not start with '-'" ;;
  esac
fi

# The invocation checkout is the current directory's Git work tree. No `--repo`
# override is accepted, and the entry point must belong to that same checkout
# so a copied script cannot redirect preparation elsewhere.
repo_root=$(git rev-parse --show-toplevel 2>/dev/null) ||
  refuse "not inside a Git work tree"
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
script_root=$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null) ||
  refuse "entry point is not inside a Git work tree"
if [ "$script_root" != "$repo_root" ]; then
  refuse "entry point belongs to $script_root, not the invocation checkout $repo_root"
fi

branch=$(git -C "$repo_root" symbolic-ref --quiet --short HEAD 2>/dev/null) ||
  refuse "a checked-out branch is required; a detached HEAD is not a release source"
if [ "$branch" != "main" ]; then
  refuse "release preparation requires branch 'main', found '$branch'"
fi

if [ -n "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=no)" ]; then
  refuse "release preparation requires a clean tracked working tree; commit or discard pending changes"
fi

if ! git -C "$repo_root" rev-parse --verify --quiet refs/remotes/origin/main >/dev/null; then
  refuse "origin/main is required to verify the prepared source is pushed"
fi
head_sha=$(git -C "$repo_root" rev-parse HEAD)
origin_sha=$(git -C "$repo_root" rev-parse refs/remotes/origin/main)
if [ "$head_sha" != "$origin_sha" ]; then
  refuse "release preparation requires the pushed main tip: HEAD $head_sha != origin/main $origin_sha"
fi

prepare_args=(release prepare)
if [ "$yes" = true ]; then
  intent="mutating ${version}"
  prepare_args+=(--yes)
else
  intent="plan"
  prepare_args+=(--plan)
fi
# A mutating preparation always runs the configured gates; an inner plan only
# runs them when the caller explicitly asked for gate checking.
if [ "$yes" = true ] || [ "$check_gates" = true ]; then
  prepare_args+=(--check-gates)
fi
if [ -n "$version" ]; then
  prepare_args+=(--version "$version")
fi
if [ "$json" = true ]; then
  prepare_args+=(--json)
fi

printf 'release:prepare: admitted heavy preparation (%s) of %s at %s\n' \
  "$intent" "$branch" "$head_sha" >&2

# Build and run Effigy from this checkout. Never an installed or PATH Effigy,
# which may predate the release-preparation repairs under review.
exec cargo run --quiet --bin effigy -- "${prepare_args[@]}"
