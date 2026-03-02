#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: update-homebrew-formula-from-metadata.sh --metadata <path> --formula <path>

Updates a Homebrew formula URL/SHA256 from a homebrew-metadata.json payload.
USAGE
}

METADATA_PATH=""
FORMULA_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --metadata)
      METADATA_PATH="${2:-}"
      shift 2
      ;;
    --formula)
      FORMULA_PATH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$METADATA_PATH" || -z "$FORMULA_PATH" ]]; then
  echo "--metadata and --formula are required" >&2
  usage >&2
  exit 1
fi

if [[ ! -f "$METADATA_PATH" ]]; then
  echo "metadata file not found: $METADATA_PATH" >&2
  exit 1
fi

if [[ ! -f "$FORMULA_PATH" ]]; then
  echo "formula file not found: $FORMULA_PATH" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

TAG="$(jq -r '.tag // empty' "$METADATA_PATH")"
VERSION="$(jq -r '.version // empty' "$METADATA_PATH")"
URL="$(jq -r '.url // empty' "$METADATA_PATH")"
SHA256="$(jq -r '.sha256 // empty' "$METADATA_PATH")"
FORMULA_NAME="$(jq -r '.formula // empty' "$METADATA_PATH")"

if [[ -z "$TAG" || -z "$VERSION" || -z "$URL" || -z "$SHA256" ]]; then
  echo "metadata is missing required keys (tag, version, url, sha256)" >&2
  exit 1
fi

if [[ -n "$FORMULA_NAME" && "$FORMULA_NAME" != "effigy" ]]; then
  echo "unexpected formula name in metadata: $FORMULA_NAME" >&2
  exit 1
fi

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
  echo "invalid tag in metadata: $TAG" >&2
  exit 1
fi

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

awk -v new_url="$URL" -v new_sha256="$SHA256" '
  BEGIN {
    url_updated = 0
    sha_updated = 0
  }
  {
    if (url_updated == 0 && $0 ~ /^[[:space:]]*url[[:space:]]+"/) {
      sub(/"[^"]*"/, "\"" new_url "\"")
      url_updated = 1
    }
    if (sha_updated == 0 && $0 ~ /^[[:space:]]*sha256[[:space:]]+"/) {
      sub(/"[^"]*"/, "\"" new_sha256 "\"")
      sha_updated = 1
    }
    print
  }
  END {
    if (url_updated == 0 || sha_updated == 0) {
      exit 42
    }
  }
' "$FORMULA_PATH" > "$tmp_file" || {
  echo "failed to update formula (url/sha256 keys not found): $FORMULA_PATH" >&2
  exit 1
}

mv "$tmp_file" "$FORMULA_PATH"

printf 'Updated %s\n' "$FORMULA_PATH"
printf '  tag: %s\n' "$TAG"
printf '  version: %s\n' "$VERSION"
printf '  url: %s\n' "$URL"
printf '  sha256: %s\n' "$SHA256"
