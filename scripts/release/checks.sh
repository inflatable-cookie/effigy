#!/bin/sh
# Effigy's own release checks (contract 054 §4.4).
#
# These were `effigy release validate|preflight|check-binary|proof|verify-install|evidence`
# until v0.15 retired them. They are Effigy-repository policy, not a product
# surface, so they live here and the release declares them as repo tasks in
# `config/tasks.toml`:
#
#   release:metadata           metadata   – version, changelog, lockfile, records
#   release:preflight          preflight  – metadata + docs (+ smoke on request)
#   release:check-binary       check-binary – GLIBC floor of a Linux binary
#   release:proof              proof      – artifact proof of a release asset dir
#   release:verify:install     verify-install – tagged-source install proof
#   release:evidence:validate  evidence-validate – release asset evidence
#   release:evidence:closeout  evidence-closeout – closeout record
#   release:publish:binaries   publish-binaries – dispatch release-binaries.yml
#   release:check:binaries     check-binaries – idempotency probe for that target
#   release:verify:binaries    verify-binaries – GitHub Release, Homebrew, install
#
# Nothing here publishes a release on its own: `publish-binaries` only
# dispatches the existing workflow, and the rest read local artifacts or the
# published release to check it.
set -eu
export LC_ALL=C

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release/checks.sh metadata [--tag TAG]
  scripts/release/checks.sh preflight [--tag TAG] [--skip-docs] [--skip-smoke] [--output PATH]
  scripts/release/checks.sh check-binary BIN --glibc-floor VER
  scripts/release/checks.sh proof [--artifacts-dir DIR]
  scripts/release/checks.sh verify-install [--tag TAG] [--repo-url URL]
  scripts/release/checks.sh evidence-validate --artifacts-dir DIR [--expect-homebrew]
  scripts/release/checks.sh evidence-closeout --tag TAG --artifacts-dir DIR [--output PATH]
  scripts/release/checks.sh publish-binaries
  scripts/release/checks.sh check-binaries
  scripts/release/checks.sh verify-binaries [--repo-url URL]
USAGE
}

fail() {
  printf '[error] %s\n' "$*" >&2
  exit 1
}

ok() {
  printf '[ok] %s\n' "$*"
}

effigy_bin() {
  if [ -n "${EFFIGY_EXECUTABLE:-}" ] && [ -x "${EFFIGY_EXECUTABLE}" ]; then
    printf '%s\n' "$EFFIGY_EXECUTABLE"
    return 0
  fi
  command -v effigy || fail "no effigy binary on PATH; set EFFIGY_EXECUTABLE"
}

repo_root() {
  git rev-parse --show-toplevel 2>/dev/null || pwd
}

require_release_input() {
  [ -n "${EFFIGY_RELEASE_INPUT:-}" ] || fail "this verb runs as a release task: \$EFFIGY_RELEASE_INPUT is not set"
  [ -f "$EFFIGY_RELEASE_INPUT" ] || fail "release input is missing: $EFFIGY_RELEASE_INPUT"
  command -v jq >/dev/null 2>&1 || fail "jq is required to read the release input"
}

release_input() {
  jq -r "$1" "$EFFIGY_RELEASE_INPUT"
}

# Write the contract 054 §6 output document. `committed` is true, false or
# "unknown"; no file after exit 0 means unknown, so every definite answer is
# written here.
write_release_output() {
  committed=$1
  shift
  destinations='[]'
  for destination in "$@"; do
    destinations=$(printf '%s' "$destinations" | jq --arg d "$destination" '. + [$d]')
  done
  [ -n "${EFFIGY_RELEASE_OUTPUT:-}" ] || fail "\$EFFIGY_RELEASE_OUTPUT is not set"
  jq -n \
    --argjson committed "$committed" \
    --argjson destinations "$destinations" \
    '{schema: "effigy.release.output.v1", schema_version: 1, committed: $committed, destinations: $destinations}' \
    > "$EFFIGY_RELEASE_OUTPUT"
}

release_tag() {
  count=$(release_input '.identities | length')
  [ "$count" = "1" ] || fail "Effigy's release has one identity; the input carries $count"
  release_input '.identities[0].tag'
}

release_version() {
  count=$(release_input '.identities | length')
  [ "$count" = "1" ] || fail "Effigy's release has one identity; the input carries $count"
  release_input '.identities[0].identity'
}

sha256_of() {
  digest_file=$1
  if command -v sha256sum >/dev/null 2>&1; then
    digest_output=$(sha256sum "$digest_file")
  elif command -v shasum >/dev/null 2>&1; then
    digest_output=$(shasum -a 256 "$digest_file")
  else
    fail "no SHA-256 utility found (need sha256sum or shasum)"
  fi
  printf '%s\n' "${digest_output%% *}"
}

glibc_versions_of() {
  binary=$1
  for probe in readelf objdump strings; do
    command -v "$probe" >/dev/null 2>&1 || continue
    case "$probe" in
      readelf) probe_output=$(readelf --version-info "$binary" 2>&1 || true) ;;
      objdump) probe_output=$(objdump -T "$binary" 2>&1 || true) ;;
      *) probe_output=$(strings "$binary" 2>&1 || true) ;;
    esac
    found=$(printf '%s\n' "$probe_output" | sed -n 's/.*GLIBC_\([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | sort -t. -k1,1n -k2,2n -u)
    if [ -n "$found" ]; then
      printf '%s\n' "$found"
      return 0
    fi
  done
  return 0
}

glibc_at_most() {
  required=$1
  floor=$2
  required_major=${required%%.*}
  required_minor=${required#*.}
  floor_major=${floor%%.*}
  floor_minor=${floor#*.}
  case "$required_major$required_minor$floor_major$floor_minor" in
    *[!0-9]*) return 1 ;;
  esac
  [ "$required_major" -lt "$floor_major" ] && return 0
  [ "$required_major" -gt "$floor_major" ] && return 1
  [ "$required_minor" -le "$floor_minor" ]
}

cargo_manifest_version() {
  awk '
    /^\[workspace\.package\]/ { section = 1; next }
    /^\[/ { section = 0 }
    section && /^version[[:space:]]*=/ {
      line = $0
      sub(/^version[[:space:]]*=[[:space:]]*"/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' "$1"
}

lockfile_version() {
  awk -v wanted="effigy" '
    $0 == "name = \"" wanted "\"" { found = 1; next }
    found && /^version = / {
      line = $0
      sub(/^version = "/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' "$1"
}

verb=${1:-}
if [ $# -gt 0 ]; then shift; fi

case "$verb" in
  metadata)
    tag=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --tag) tag=${2:?--tag needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    root=$(repo_root)
    cargo_toml="$root/Cargo.toml"
    lockfile="$root/Cargo.lock"
    changelog_file="$root/CHANGELOG.md"
    version=$(cargo_manifest_version "$cargo_toml")
    [ -n "$version" ] || fail "Cargo.toml has no [workspace.package] version"
    case "$version" in
      [0-9]*.[0-9]*.[0-9]*) ;;
      *) fail "workspace version is not semver-like: $version" ;;
    esac
    locked=$(lockfile_version "$lockfile")
    [ "$locked" = "$version" ] || fail "Cargo.lock records effigy $locked but Cargo.toml declares $version; run the release sync step"
    grep -q "^## \[$version\]" "$changelog_file" || fail "CHANGELOG.md has no \`## [$version]\` section"
    if [ -n "$tag" ]; then
      expected="v$version"
      [ "$tag" = "$expected" ] || fail "tag $tag does not match the workspace version $expected"
    fi
    for record in README.md CHANGELOG.md docs/knowledge/contracts/release.md docs/knowledge/contracts/054-release-platform-contract.md .github/workflows/release-binaries.yml config/release.toml; do
      [ -f "$root/$record" ] || fail "release record is missing: $record"
    done
    workflow="$root/.github/workflows/release-binaries.yml"
    grep -q '^on:' "$workflow" || fail "release-binaries.yml has no trigger block"
    grep -q 'workflow_dispatch:' "$workflow" || fail "release-binaries.yml must stay explicitly dispatched, not triggered by a tag push"
    grep -q 'git cat-file -t "refs/tags/\$RELEASE_TAG"' "$workflow" || fail "release-binaries.yml must keep the annotated-tag check"
    if ! grep -Eq 'release (check-binary|:check-binary)|scripts/release/checks\.sh check-binary' "$workflow"; then
      fail "release-binaries.yml lost its Linux GLIBC guard; restore a check-binary step before the release"
    fi
    if grep -q 'release check-binary' "$workflow"; then
      printf '%s\n' \
        "[warn] .github/workflows/release-binaries.yml still runs the retired \`effigy release check-binary\`." \
        "[warn] Change that step to ./scripts/release/checks.sh check-binary ./effigy-\${{ matrix.target }} --glibc-floor 2.35" \
        "[warn] before publishing: a workflow edit needs operator authority, and until it lands the binary workflow stops at that step." >&2
    fi
    ok "release metadata for v$version"
    ;;

  preflight)
    tag=""
    skip_docs=false
    skip_smoke=false
    output_path=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --tag) tag=${2:?--tag needs a value}; shift 2 ;;
        --skip-docs) skip_docs=true; shift ;;
        --skip-smoke) skip_smoke=true; shift ;;
        --output) output_path=${2:?--output needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    root=$(repo_root)
    effigy=$(effigy_bin)
    metadata_status=passed
    docs_status=skipped
    smoke_status=skipped
    if ! sh "$root/scripts/release/checks.sh" metadata ${tag:+--tag "$tag"}; then
      metadata_status=failed
    fi
    if [ "$skip_docs" = false ]; then
      docs_status=passed
      ( cd "$root" && "$effigy" qa:docs ) || docs_status=failed
    fi
    if [ "$skip_smoke" = false ]; then
      smoke_status=passed
      ( cd "$root" && "$effigy" smoke:release ) || smoke_status=failed
    fi
    if [ -n "$output_path" ]; then
      {
        printf 'schema=effigy.release.preflight.v1\n'
        printf 'tag=%s\n' "${tag:-}"
        printf 'metadata=%s\n' "$metadata_status"
        printf 'docs=%s\n' "$docs_status"
        printf 'smoke=%s\n' "$smoke_status"
      } > "$output_path"
      ok "wrote preflight summary: $output_path"
    fi
    [ "$metadata_status" = passed ] || fail "preflight metadata slice failed"
    [ "$docs_status" != failed ] || fail "preflight docs slice failed"
    [ "$smoke_status" != failed ] || fail "preflight smoke slice failed"
    ok "release preflight passed"
    ;;

  check-binary)
    binary=""
    floor=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --glibc-floor) floor=${2:?--glibc-floor needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        -*) usage; fail "unknown argument: $1" ;;
        *)
          [ -z "$binary" ] || fail "unexpected argument: $1"
          binary=$1
          shift
          ;;
      esac
    done
    [ -n "$binary" ] || fail "check-binary needs the binary path"
    [ -f "$binary" ] || fail "binary not found: $binary"
    [ -n "$floor" ] || fail "check-binary needs --glibc-floor"
    versions=$(glibc_versions_of "$binary")
    required=$(printf '%s\n' "$versions" | tail -1)
    if [ -z "$required" ]; then
      ok "no dynamic GLIBC symbol requirements found: $binary"
      exit 0
    fi
    glibc_at_most "$required" "$floor" || fail "$binary requires GLIBC_$required but the release floor is GLIBC_$floor"
    ok "$binary GLIBC floor is compatible (requires GLIBC_$required, max GLIBC_$floor)"
    ;;

  proof)
    artifacts_dir=".effigy/release/artifacts"
    while [ $# -gt 0 ]; do
      case "$1" in
        --artifacts-dir) artifacts_dir=${2:?--artifacts-dir needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    root=$(repo_root)
    sh "$root/scripts/release-assets.sh" verify "$artifacts_dir"
    for binary in effigy-aarch64-unknown-linux-gnu effigy-x86_64-unknown-linux-gnu; do
      if [ -f "$artifacts_dir/$binary" ]; then
        sh "$root/scripts/release/checks.sh" check-binary "$artifacts_dir/$binary" --glibc-floor 2.35
      fi
    done
    if [ -x "$artifacts_dir/effigy-$(uname -m)-apple-darwin" ] && [ "$(uname -s)" = Darwin ]; then
      smoke_status=$("$artifacts_dir/effigy-$(uname -m)-apple-darwin" smoke:release "$artifacts_dir/effigy-$(uname -m)-apple-darwin" 2>&1) \
        || fail "release binary smoke failed: $smoke_status"
      ok "release binary smoke passed"
    fi
    ok "release artifact proof passed for $artifacts_dir"
    ;;

  verify-install)
    tag=""
    repo_url=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --tag) tag=${2:?--tag needs a value}; shift 2 ;;
        --repo-url) repo_url=${2:?--repo-url needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    if [ -z "$tag" ] && [ -n "${EFFIGY_RELEASE_INPUT:-}" ] && [ -f "${EFFIGY_RELEASE_INPUT:-}" ]; then
      tag=$(release_tag)
    fi
    [ -n "$tag" ] || fail "verify-install needs --tag <TAG> or a release input"
    case "$tag" in
      v[0-9]*.[0-9]*.[0-9]*) ;;
      *) fail "tag must be vX.Y.Z: $tag" ;;
    esac
    root=$(repo_root)
    [ "$(awk '/^\[package\]/{s=1;next} /^\[/{s=0} s && /^name[[:space:]]*=/ {gsub(/[",]/,"",$0); sub(/^name[[:space:]]*=[[:space:]]*/,"",$0); print; exit}' "$root/Cargo.toml")" = "effigy" ] \
      || fail "verify-install proves Effigy's own tagged binary only; this checkout's root package is not \`effigy\`. A library or service repository uses its own consumer smoke instead"
    if [ -z "$repo_url" ]; then
      repo_url=$(git -C "$root" remote get-url origin 2>/dev/null || true)
    fi
    [ -n "$repo_url" ] || fail "verify-install needs --repo-url <URL> or an origin remote"
    version=${tag#v}
    temp_root=$(mktemp -d "${TMPDIR:-/tmp}/effigy-release-verify-install.XXXXXX")
    install_root="$temp_root/install-root"
    fixture_dir="$temp_root/fixture"
    mkdir -p "$fixture_dir"
    printf '[catalog]\nalias = "catalog_a"\n\n[tasks]\nnoop = "echo noop"\n' > "$fixture_dir/effigy.toml"
    cleanup() {
      rm -rf "$temp_root"
    }
    trap cleanup EXIT INT TERM
    printf '[release] installing %s from %s\n' "$tag" "$repo_url"
    cargo install --locked --git "$repo_url" --tag "$tag" --root "$install_root" --force effigy \
      || fail "cargo install from $repo_url at $tag failed"
    installed="$install_root/bin/effigy"
    [ -x "$installed" ] || fail "installed binary is missing or not executable: $installed"
    version_output=$("$installed" version 2>&1) || fail "installed binary \`version\` failed: $version_output"
    printf '%s\n' "$version_output" | grep -q "v$version" \
      || fail "expected version output to contain \`v$version\`, got \`$version_output\`"
    ok "installed binary version check"
    tasks_output=$("$installed" tasks --repo "$fixture_dir" 2>&1) || fail "installed binary \`tasks\` failed: $tasks_output"
    printf '%s\n' "$tasks_output" | grep -q "noop" \
      || fail "expected \`tasks\` to list the fixture task, got \`$tasks_output\`"
    ok "installed binary tasks fixture check"
    prefixed_output=$("$installed" catalog_a/tasks --repo "$fixture_dir" 2>&1) || fail "installed binary prefixed builtin \`tasks\` failed: $prefixed_output"
    printf '%s\n' "$prefixed_output" | grep -q "noop" \
      || fail "expected prefixed \`tasks\` to list the fixture task, got \`$prefixed_output\`"
    ok "installed binary prefixed builtin tasks check"
    help_output=$("$installed" --json help 2>&1) || fail "installed binary \`--json help\` failed: $help_output"
    printf '%s\n' "$help_output" | jq -e '(.schema == "effigy.command.v1") and (.result.schema == "effigy.help.v1")' >/dev/null 2>&1 \
      || fail "expected the effigy help JSON envelope from \`--json help\`, got \`$help_output\`"
    ok "installed binary JSON help envelope check"
    ok "tagged-source install of $tag verified"
    ;;

  evidence-validate)
    artifacts_dir=""
    expect_homebrew=false
    while [ $# -gt 0 ]; do
      case "$1" in
        --artifacts-dir) artifacts_dir=${2:?--artifacts-dir needs a value}; shift 2 ;;
        --expect-homebrew) expect_homebrew=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    [ -n "$artifacts_dir" ] || fail "evidence-validate needs --artifacts-dir"
    root=$(repo_root)
    sh "$root/scripts/release-assets.sh" verify "$artifacts_dir"
    for binary in effigy-aarch64-apple-darwin effigy-aarch64-unknown-linux-gnu effigy-x86_64-apple-darwin effigy-x86_64-unknown-linux-gnu; do
      [ -s "$artifacts_dir/$binary" ] || fail "release asset is missing or empty: $artifacts_dir/$binary"
    done
    if [ "$expect_homebrew" = true ]; then
      ls "$artifacts_dir"/*homebrew* >/dev/null 2>&1 \
        || fail "expected a captured Homebrew channel log under $artifacts_dir"
    fi
    ok "release artifact evidence validated for $artifacts_dir"
    ;;

  evidence-closeout)
    tag=""
    artifacts_dir=""
    output_path=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --tag) tag=${2:?--tag needs a value}; shift 2 ;;
        --artifacts-dir) artifacts_dir=${2:?--artifacts-dir needs a value}; shift 2 ;;
        --output) output_path=${2:?--output needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    [ -n "$tag" ] || fail "evidence-closeout needs --tag"
    [ -n "$artifacts_dir" ] || fail "evidence-closeout needs --artifacts-dir"
    [ -d "$artifacts_dir" ] || fail "artifact directory does not exist: $artifacts_dir"
    root=$(repo_root)
    if [ -z "$output_path" ]; then
      output_path="$artifacts_dir/closeout-$tag.md"
    fi
    {
      printf '# Effigy %s release closeout\n\n' "$tag"
      printf 'Recorded: %s\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      printf '## Release assets\n\n'
      for asset in effigy-aarch64-apple-darwin effigy-aarch64-unknown-linux-gnu effigy-x86_64-apple-darwin effigy-x86_64-unknown-linux-gnu; do
        if [ -s "$artifacts_dir/$asset" ]; then
          printf -- '- `%s` sha256 `%s`\n' "$asset" "$(sha256_of "$artifacts_dir/$asset")"
        else
          printf -- '- `%s` missing\n' "$asset"
        fi
      done
      printf '\n## Captured logs\n\n'
      captured=0
      for log in "$artifacts_dir"/*.log; do
        [ -f "$log" ] || continue
        printf -- '- `%s`\n' "$(basename "$log")"
        captured=1
      done
      [ "$captured" = 1 ] || printf -- '- none captured\n'
      printf '\nThe release Run receipt is the durable record: `effigy release receipt <run>`.\n'
    } > "$output_path"
    sh "$root/scripts/release/checks.sh" evidence-validate --artifacts-dir "$artifacts_dir" || true
    ok "wrote closeout record: $output_path"
    ;;

  publish-binaries)
    require_release_input
    tag=$(release_tag)
    command -v gh >/dev/null 2>&1 || fail "gh is required to dispatch release-binaries.yml"
    gh workflow run release-binaries.yml -f "tag=$tag" \
      || fail "dispatching release-binaries.yml for $tag failed"
    run_url=""
    attempts=0
    while [ "$attempts" -lt 10 ] && [ -z "$run_url" ]; do
      attempts=$((attempts + 1))
      run_url=$(gh run list --workflow release-binaries.yml --limit 1 --json url --jq '.[0].url // ""' 2>/dev/null || true)
      [ -n "$run_url" ] || sleep 3
    done
    if [ -n "$run_url" ]; then
      ok "dispatched release-binaries.yml for $tag: $run_url"
      write_release_output true "workflow:$run_url" "github-release:https://github.com/inflatable-cookie/effigy/releases/tag/$tag"
    else
      ok "dispatched release-binaries.yml for $tag"
      write_release_output true "workflow:release-binaries.yml" "github-release:https://github.com/inflatable-cookie/effigy/releases/tag/$tag"
    fi
    ;;

  check-binaries)
    require_release_input
    tag=$(release_tag)
    command -v gh >/dev/null 2>&1 || fail "gh is required to probe the GitHub Release"
    if assets=$(gh release view "$tag" --json assets --jq '.assets[].name' 2>/dev/null); then
      missing=0
      for asset in effigy-aarch64-apple-darwin effigy-aarch64-unknown-linux-gnu effigy-x86_64-apple-darwin effigy-x86_64-unknown-linux-gnu; do
        printf '%s\n' "$assets" | grep -qx "$asset" || missing=1
      done
      if [ "$missing" = 0 ]; then
        ok "release $tag already publishes every binary asset"
        write_release_output true "github-release:https://github.com/inflatable-cookie/effigy/releases/tag/$tag"
        exit 0
      fi
      ok "release $tag exists without every binary asset"
      write_release_output false
      exit 0
    fi
    if gh release view "$tag" >/dev/null 2>&1; then
      write_release_output false
      exit 0
    fi
    probe=$(gh release view "$tag" 2>&1 || true)
    case "$probe" in
      *"release not found"*|*"Not Found"*)
        ok "release $tag is not published yet"
        write_release_output false
        ;;
      *)
        printf '%s\n' "$probe" >&2
        write_release_output '"unknown"'
        ;;
    esac
    ;;

  verify-binaries)
    repo_url=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --repo-url) repo_url=${2:?--repo-url needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; fail "unknown argument: $1" ;;
      esac
    done
    require_release_input
    tag=$(release_tag)
    root=$(repo_root)
    command -v gh >/dev/null 2>&1 || fail "gh is required to verify the GitHub Release"
    assets=$(gh release view "$tag" --json assets --jq '.assets[].name') \
      || fail "GitHub Release $tag is missing"
    for asset in effigy-aarch64-apple-darwin effigy-aarch64-unknown-linux-gnu effigy-x86_64-apple-darwin effigy-x86_64-unknown-linux-gnu effigy-aarch64-apple-darwin.sha256 effigy-x86_64-apple-darwin.sha256; do
      printf '%s\n' "$assets" | grep -qx "$asset" || fail "GitHub Release $tag is missing asset $asset"
    done
    ok "GitHub Release $tag publishes every binary and macOS checksum asset"
    brew_formula=$(brew info --json=v2 inflatable-cookie/effigy/effigy 2>/dev/null | jq -r '.formulae[0].versions.stable // ""' || true)
    if [ -n "$brew_formula" ]; then
      [ "$brew_formula" = "${tag#v}" ] || fail "Homebrew tap publishes $brew_formula, expected ${tag#v}"
      ok "Homebrew tap publishes $brew_formula"
    else
      printf '[warn] Homebrew tap is not installed on this host; the tap check is unproven\n' >&2
    fi
    sh "$root/scripts/release/checks.sh" verify-install --tag "$tag" ${repo_url:+--repo-url "$repo_url"}
    ok "release $tag verified"
    ;;

  ""|-h|--help) usage; [ -z "$verb" ] && exit 2 || exit 0 ;;
  *) usage; fail "unknown verb: $verb" ;;
esac
