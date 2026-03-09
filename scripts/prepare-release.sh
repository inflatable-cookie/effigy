#!/usr/bin/env bash
set -euo pipefail

# Reads CHANGELOG.md [Unreleased] section, determines version bump type,
# and optionally applies the version bump to Cargo.toml and CHANGELOG.md.
#
# Usage:
#   ./scripts/prepare-release.sh            # dry-run (default)
#   ./scripts/prepare-release.sh --apply    # apply changes

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="$ROOT_DIR/CHANGELOG.md"
CARGO_TOML="$ROOT_DIR/Cargo.toml"
APPLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply)
      APPLY=true
      shift
      ;;
    -h|--help)
      echo "Usage: prepare-release.sh [--apply]"
      echo ""
      echo "Reads CHANGELOG.md [Unreleased] to determine version bump."
      echo "Without --apply, prints what would happen (dry run)."
      echo "With --apply, updates Cargo.toml and CHANGELOG.md."
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1" >&2
      exit 1
      ;;
  esac
done

if [[ ! -f "$CHANGELOG" ]]; then
  echo "[error] CHANGELOG.md not found at $CHANGELOG" >&2
  exit 1
fi

# Extract current version from Cargo.toml
CURRENT_VERSION="$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "//;s/"//')"
if [[ -z "$CURRENT_VERSION" ]]; then
  echo "[error] could not read version from Cargo.toml" >&2
  exit 1
fi

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Extract [Unreleased] section content (between ## [Unreleased] and the next ## heading)
UNRELEASED="$(sed -n '/^## \[Unreleased\]/,/^## \[/{/^## \[Unreleased\]/d;/^## \[/d;p;}' "$CHANGELOG")"

if [[ -z "$(echo "$UNRELEASED" | grep -v '^$' | grep -v '^### ')" ]]; then
  echo "[info] no unreleased changes found in CHANGELOG.md"
  echo "[info] current version: $CURRENT_VERSION"
  exit 0
fi

# Check if there are entries under ### Breaking
# Look for non-empty lines between ### Breaking and the next ### heading
BREAKING_SECTION="$(sed -n '/^### Breaking/,/^### /{/^### Breaking/d;/^### /d;p;}' <<< "$UNRELEASED")"
HAS_BREAKING=false
if echo "$BREAKING_SECTION" | grep -q '^- '; then
  HAS_BREAKING=true
fi

# Determine bump type and next version
if [[ "$HAS_BREAKING" == "true" ]]; then
  BUMP_TYPE="MINOR"
  NEXT_VERSION="$MAJOR.$((MINOR + 1)).0"
else
  BUMP_TYPE="PATCH"
  NEXT_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
fi

echo "[info] current version: $CURRENT_VERSION"
if [[ "$HAS_BREAKING" == "true" ]]; then
  echo "[info] breaking changes detected"
fi
echo "[info] bump type: $BUMP_TYPE"
echo "[info] next version: $CURRENT_VERSION -> $NEXT_VERSION"

if [[ "$APPLY" != "true" ]]; then
  echo ""
  echo "[dry-run] no changes made. Use --apply to update files."
  exit 0
fi

# Apply: update Cargo.toml version
sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEXT_VERSION\"/" "$CARGO_TOML"
echo "[ok] updated Cargo.toml version to $NEXT_VERSION"

# Apply: move [Unreleased] entries to versioned heading
TODAY="$(date +%Y-%m-%d)"
FRESH_UNRELEASED="## [Unreleased]

### Breaking

### Added

### Changed

### Fixed"

# Replace the [Unreleased] section with fresh + versioned section
# Strategy: replace "## [Unreleased]" line with fresh section + versioned heading,
# keeping the content under the new version heading
{
  # Read up to and including the line before ## [Unreleased]
  sed -n '1,/^## \[Unreleased\]/{/^## \[Unreleased\]/!p;}' "$CHANGELOG"

  # Write fresh unreleased section
  echo "$FRESH_UNRELEASED"
  echo ""

  # Write versioned heading with the old content
  echo "## [$NEXT_VERSION] - $TODAY"

  # Write the old unreleased content
  echo "$UNRELEASED"

  # Write everything after the unreleased section (from the next ## heading onward)
  sed -n '/^## \[Unreleased\]/,/^## \[/{/^## \[Unreleased\]/d;/^## \[/!d;/^## \[/p;}' "$CHANGELOG"
  sed -n '/^## \[Unreleased\]/,/^## \[/!{/^## \[Unreleased\]/!p;}' "$CHANGELOG" | tail -n +2

} > "$CHANGELOG.tmp"
mv "$CHANGELOG.tmp" "$CHANGELOG"
echo "[ok] updated CHANGELOG.md with [$NEXT_VERSION] - $TODAY"

# Sync Cargo.lock
echo "[info] syncing Cargo.lock..."
(cd "$ROOT_DIR" && cargo check --quiet 2>/dev/null)
echo "[ok] Cargo.lock synced"

echo ""
echo "[done] release $NEXT_VERSION prepared"
echo "[next] review changes, then commit: git commit -m 'release: v$NEXT_VERSION'"
