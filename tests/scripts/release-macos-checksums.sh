#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
helper="$repo_root/scripts/release-assets.sh"
workflow="$repo_root/.github/workflows/release-binaries.yml"
tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/effigy-macos-checksums.XXXXXX")
trap 'rm -rf "$tmp_root"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

expect_failure() {
  local label=$1
  shift
  if "$@" >"$tmp_root/expected-failure.log" 2>&1; then
    fail "$label unexpectedly succeeded"
  fi
}

independent_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

grep -Fq './scripts/release-assets.sh generate artifacts' "$workflow" ||
  fail 'workflow does not generate sidecars after artifact download'
grep -Fq './scripts/release-assets.sh verify artifacts' "$workflow" ||
  fail 'workflow does not verify sidecars before release creation'
grep -Fq './scripts/release-assets.sh publish generated "$TAG" "$VERSION" artifacts' "$workflow" ||
  fail 'generated-notes path does not use the release asset helper'
grep -Fq './scripts/release-assets.sh publish changelog "$TAG" "$VERSION" artifacts release-notes.md' "$workflow" ||
  fail 'changelog-notes path does not use the release asset helper'

artifact_dir="$tmp_root/artifacts"
mkdir -p "$artifact_dir"
arm=effigy-aarch64-apple-darwin
x86=effigy-x86_64-apple-darwin
printf 'arm macOS fixture bytes\n' > "$artifact_dir/$arm"
printf 'x86 macOS fixture bytes\n' > "$artifact_dir/$x86"
printf 'arm Linux fixture bytes\n' > "$artifact_dir/effigy-aarch64-unknown-linux-gnu"
printf 'x86 Linux fixture bytes\n' > "$artifact_dir/effigy-x86_64-unknown-linux-gnu"

"$helper" generate "$artifact_dir"
for binary in "$arm" "$x86"; do
  digest=$(independent_sha256 "$artifact_dir/$binary")
  printf '%s  %s\n' "$digest" "$binary" > "$tmp_root/expected-$binary.sha256"
  cmp -s "$tmp_root/expected-$binary.sha256" "$artifact_dir/$binary.sha256" ||
    fail "$binary sidecar does not contain the independently computed exact line"
done

printf '%s\n' \
  "$artifact_dir/$arm.sha256" \
  "$artifact_dir/$x86.sha256" > "$tmp_root/expected-sidecars.txt"
find "$artifact_dir" -maxdepth 1 -type f -name '*.sha256' -print | sort > "$tmp_root/actual-sidecars.txt"
cmp -s "$tmp_root/expected-sidecars.txt" "$tmp_root/actual-sidecars.txt" ||
  fail 'sidecar generation did not create exactly the two macOS sidecars'
"$helper" verify "$artifact_dir"
printf 'ok: exact sidecar names and content\n'

printf 'mutated bytes\n' >> "$artifact_dir/$arm"
expect_failure 'verification after binary mutation' "$helper" verify "$artifact_dir"
printf 'arm macOS fixture bytes\n' > "$artifact_dir/$arm"
"$helper" verify "$artifact_dir"
printf 'ok: mutated binary rejected\n'

printf 'malformed sidecar\n' > "$artifact_dir/$x86.sha256"
expect_failure 'malformed sidecar verification' "$helper" verify "$artifact_dir"
"$helper" generate "$artifact_dir"
rm "$artifact_dir/$arm"
expect_failure 'missing macOS binary generation' "$helper" generate "$artifact_dir"
printf 'arm macOS fixture bytes\n' > "$artifact_dir/$arm"
: > "$artifact_dir/$x86"
expect_failure 'empty macOS binary generation' "$helper" generate "$artifact_dir"
printf 'x86 macOS fixture bytes\n' > "$artifact_dir/$x86"
"$helper" generate "$artifact_dir"
rm "$artifact_dir/$arm.sha256"
expect_failure 'missing checksum sidecar verification' "$helper" verify "$artifact_dir"
"$helper" generate "$artifact_dir"
printf 'ok: missing, empty, malformed, and mismatched inputs rejected\n'

"$helper" list "$artifact_dir" > "$tmp_root/actual-assets.txt"
printf '%s\n' \
  "$artifact_dir/effigy-aarch64-apple-darwin" \
  "$artifact_dir/effigy-aarch64-unknown-linux-gnu" \
  "$artifact_dir/effigy-x86_64-apple-darwin" \
  "$artifact_dir/effigy-x86_64-unknown-linux-gnu" \
  "$artifact_dir/effigy-aarch64-apple-darwin.sha256" \
  "$artifact_dir/effigy-x86_64-apple-darwin.sha256" > "$tmp_root/expected-assets.txt"
cmp -s "$tmp_root/expected-assets.txt" "$tmp_root/actual-assets.txt" ||
  fail 'asset list is not the four raw binaries plus two macOS sidecars'

mkdir "$tmp_root/bin"
cat > "$tmp_root/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'CALL\n' >> "$GH_CAPTURE"
printf '%s\n' "$@" >> "$GH_CAPTURE"
EOF
chmod +x "$tmp_root/bin/gh"
printf 'fixture release notes\n' > "$tmp_root/release-notes.md"
PATH="$tmp_root/bin:$PATH" GH_CAPTURE="$tmp_root/gh-calls.txt" \
  "$helper" publish generated v9.8.7 9.8.7 "$artifact_dir"
PATH="$tmp_root/bin:$PATH" GH_CAPTURE="$tmp_root/gh-calls.txt" \
  "$helper" publish changelog v9.8.7 9.8.7 "$artifact_dir" "$tmp_root/release-notes.md"

cat > "$tmp_root/expected-gh-calls.txt" <<EOF
CALL
release
create
v9.8.7
--title
Effigy 9.8.7
--generate-notes
$artifact_dir/effigy-aarch64-apple-darwin
$artifact_dir/effigy-aarch64-unknown-linux-gnu
$artifact_dir/effigy-x86_64-apple-darwin
$artifact_dir/effigy-x86_64-unknown-linux-gnu
$artifact_dir/effigy-aarch64-apple-darwin.sha256
$artifact_dir/effigy-x86_64-apple-darwin.sha256
CALL
release
create
v9.8.7
--title
Effigy 9.8.7
--notes-file
$tmp_root/release-notes.md
$artifact_dir/effigy-aarch64-apple-darwin
$artifact_dir/effigy-aarch64-unknown-linux-gnu
$artifact_dir/effigy-x86_64-apple-darwin
$artifact_dir/effigy-x86_64-unknown-linux-gnu
$artifact_dir/effigy-aarch64-apple-darwin.sha256
$artifact_dir/effigy-x86_64-apple-darwin.sha256
EOF
cmp -s "$tmp_root/expected-gh-calls.txt" "$tmp_root/gh-calls.txt" ||
  fail 'the two publish paths did not pass exactly six intended assets to the gh stub'
for binary in \
  effigy-aarch64-apple-darwin \
  effigy-aarch64-unknown-linux-gnu \
  effigy-x86_64-apple-darwin \
  effigy-x86_64-unknown-linux-gnu; do
  [[ -x "$artifact_dir/$binary" ]] || fail "$binary was not marked executable"
done
for sidecar in "$arm.sha256" "$x86.sha256"; do
  [[ ! -x "$artifact_dir/$sidecar" ]] || fail "$sidecar was marked executable"
done
printf 'ok: both notes paths select six assets through a captured gh stub\n'
