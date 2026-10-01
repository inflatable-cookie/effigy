#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

macos_binaries=(
  effigy-aarch64-apple-darwin
  effigy-x86_64-apple-darwin
)
release_binaries=(
  effigy-aarch64-apple-darwin
  effigy-aarch64-unknown-linux-gnu
  effigy-x86_64-apple-darwin
  effigy-x86_64-unknown-linux-gnu
)
release_assets=(
  effigy-aarch64-apple-darwin
  effigy-aarch64-unknown-linux-gnu
  effigy-x86_64-apple-darwin
  effigy-x86_64-unknown-linux-gnu
  effigy-aarch64-apple-darwin.sha256
  effigy-x86_64-apple-darwin.sha256
)

usage() {
  cat >&2 <<'EOF'
usage:
  release-assets.sh generate ARTIFACTS_DIR
  release-assets.sh verify ARTIFACTS_DIR
  release-assets.sh list ARTIFACTS_DIR
  release-assets.sh publish generated TAG VERSION ARTIFACTS_DIR
  release-assets.sh publish changelog TAG VERSION ARTIFACTS_DIR NOTES_FILE
EOF
}

require_nonempty_file() {
  local path=$1
  if [[ ! -f "$path" || -L "$path" || ! -s "$path" ]]; then
    printf 'release asset is missing, empty, or not a regular file: %s\n' "$path" >&2
    return 1
  fi
}

sha256_of() {
  local path=$1 output digest
  if command -v sha256sum >/dev/null 2>&1; then
    output=$(sha256sum "$path")
  elif command -v shasum >/dev/null 2>&1; then
    output=$(shasum -a 256 "$path")
  else
    printf 'no SHA-256 utility found (need sha256sum or shasum)\n' >&2
    return 1
  fi
  digest=${output%% *}
  if [[ ${#digest} -ne 64 || "$digest" == *[!0-9a-f]* ]]; then
    printf 'SHA-256 utility returned a malformed digest for %s\n' "$path" >&2
    return 1
  fi
  printf '%s\n' "$digest"
}

require_artifact_dir() {
  if [[ ! -d "$1" ]]; then
    printf 'artifact directory does not exist: %s\n' "$1" >&2
    return 1
  fi
}

generate_sidecars() {
  local artifact_dir=$1 binary path digest
  require_artifact_dir "$artifact_dir"
  for binary in "${macos_binaries[@]}"; do
    require_nonempty_file "$artifact_dir/$binary"
  done
  for binary in "${macos_binaries[@]}"; do
    path="$artifact_dir/$binary"
    digest=$(sha256_of "$path")
    printf '%s  %s\n' "$digest" "$binary" > "$path.sha256"
  done
}

verify_sidecars() {
  local artifact_dir=$1 binary path sidecar digest
  require_artifact_dir "$artifact_dir"
  for binary in "${macos_binaries[@]}"; do
    path="$artifact_dir/$binary"
    sidecar="$path.sha256"
    require_nonempty_file "$path"
    require_nonempty_file "$sidecar"
    digest=$(sha256_of "$path")
    if ! cmp -s "$sidecar" <(printf '%s  %s\n' "$digest" "$binary"); then
      printf 'checksum sidecar is malformed or does not match %s\n' "$binary" >&2
      return 1
    fi
  done
}

validate_release_assets() {
  local artifact_dir=$1 binary
  require_artifact_dir "$artifact_dir"
  for binary in "${release_binaries[@]}"; do
    require_nonempty_file "$artifact_dir/$binary"
  done
  verify_sidecars "$artifact_dir"
}

list_assets() {
  local artifact_dir=$1 asset
  validate_release_assets "$artifact_dir"
  for asset in "${release_assets[@]}"; do
    printf '%s/%s\n' "${artifact_dir%/}" "$asset"
  done
}

publish_release() {
  local mode=$1 tag=$2 version=$3 artifact_dir=$4 notes_file=${5:-} binary asset
  local -a asset_paths
  case "$mode" in
    generated)
      if [[ $# -ne 4 ]]; then usage; return 2; fi
      ;;
    changelog)
      if [[ $# -ne 5 ]]; then usage; return 2; fi
      if [[ ! -f "$notes_file" || ! -s "$notes_file" ]]; then
        printf 'release notes file is missing or empty: %s\n' "$notes_file" >&2
        return 1
      fi
      ;;
    *)
      usage
      return 2
      ;;
  esac

  validate_release_assets "$artifact_dir"
  for binary in "${release_binaries[@]}"; do
    chmod +x "$artifact_dir/$binary"
  done
  for asset in "${release_assets[@]}"; do
    asset_paths+=("$artifact_dir/$asset")
  done

  if [[ "$mode" == generated ]]; then
    gh release create "$tag" \
      --title "Effigy $version" \
      --generate-notes \
      "${asset_paths[@]}"
  else
    gh release create "$tag" \
      --title "Effigy $version" \
      --notes-file "$notes_file" \
      "${asset_paths[@]}"
  fi
}

command_name=${1:-}
if [[ $# -gt 0 ]]; then shift; fi
case "$command_name" in
  generate)
    [[ $# -eq 1 ]] || { usage; exit 2; }
    generate_sidecars "$1"
    ;;
  verify)
    [[ $# -eq 1 ]] || { usage; exit 2; }
    verify_sidecars "$1"
    ;;
  list)
    [[ $# -eq 1 ]] || { usage; exit 2; }
    list_assets "$1"
    ;;
  publish)
    [[ $# -ge 4 ]] || { usage; exit 2; }
    publish_release "$@"
    ;;
  *)
    usage
    exit 2
    ;;
esac
