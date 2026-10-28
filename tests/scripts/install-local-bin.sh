#!/bin/sh
# Fixture tests for the local installer:
#   scripts/build-local-bin.rhai       (release build, staged activation, rollback,
#                                       local code signing)
#   scripts/local-codesign.sh          (stable macOS signing identity)
#   scripts/install-local-bin-links.rhai (launcher disposition)
#
# Everything lives under a fresh mktemp root: a private HOME, a private fixture
# checkout, a private keychain file, and stub `oxide`, `cargo`, `uname`,
# `security`, `codesign`, `cp`, `mv` and `chmod` programs that record their argv
# and delegate to the real tools. Nothing outside the fixture is written: no real
# ~/.local/bin, no real .local-install, no real build, no host install, no
# keychain identity, no host code signature and no global permission change.
#
# The signing identity itself is real: /usr/bin/openssl generates the key and
# certificate, and the stub `security` reads the certificate back out of the
# import's PKCS#12 container, so the fixture proves the identity it "imports" is
# the one the script asked for. `-P` values are redacted from the event log.
#
# Usage: sh tests/scripts/install-local-bin.sh [effigy-binary]
# Without an argument the script builds this checkout's `target/debug/effigy`
# through the selector, so the task is self-contained. The fixture tasks run
# that binary against fixture copies of the production Rhai scripts.
set -u

here=$(cd "$(dirname "$0")/../.." && pwd -P)
fx=""
cleanup() {
  [ -n "$fx" ] && rm -rf "$fx"
}
trap cleanup EXIT HUP INT TERM
fails=0
fail() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }
ok() { echo "ok: $*"; }
check_eq() { # label actual expected
  if [ "$2" = "$3" ]; then ok "$1"; else fail "$1: expected [$3], got [$2]"; fi
}
check_file_eq() { # label file expected-content
  if [ ! -e "$2" ]; then fail "$1: missing $2"; return; fi
  if [ "$(cat "$2")" = "$3" ]; then ok "$1"; else fail "$1: content mismatch in $2"; fi
}
check_contains() { # label file fixed-string
  if [ ! -e "$2" ]; then fail "$1: missing $2"; return; fi
  if grep -F -q -- "$3" "$2"; then ok "$1"; else fail "$1: missing [$3] in $2"; fi
}
check_absent() { # label file fixed-string
  if [ ! -e "$2" ]; then ok "$1"; return; fi
  if grep -F -q -- "$3" "$2"; then fail "$1: unexpected [$3] in $2"; else ok "$1"; fi
}
check_mode() { # label path octal
  if [ ! -e "$2" ]; then fail "$1: missing $2"; return; fi
  mode=$(file_mode "$2")
  if [ "$mode" = "$3" ]; then ok "$1"; else fail "$1: expected mode $3, got $mode for $2"; fi
}

file_hash() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}
file_mode() {
  if stat -f %Lp "$1" >/dev/null 2>&1; then
    stat -f %Lp "$1"
  else
    stat -c %a "$1"
  fi
}

# ---- driver binary -----------------------------------------------------------
binary=${1:-}
if [ -z "$binary" ]; then
  driver_log=$(mktemp "${TMPDIR:-/tmp}/effigy-install-local-driver.XXXXXX") || exit 2
  if ! sh "$here/scripts/oxide-selector.sh" build --bin effigy >"$driver_log" 2>&1; then
    echo "FAIL: could not build the driver effigy binary (see $driver_log)" >&2
    exit 2
  fi
  rm -f "$driver_log"
  binary=$here/target/debug/effigy
fi
if [ ! -x "$binary" ]; then
  echo "FAIL: driver effigy binary is not executable: $binary" >&2
  exit 2
fi
binary=$(cd "$(dirname "$binary")" && pwd -P)/$(basename "$binary")

real_cp=$(command -v cp)
real_mv=$(command -v mv)
real_chmod=$(command -v chmod)

wrapper_for() {
  printf '#!/usr/bin/env bash\nset -euo pipefail\nexec "%s" "$@"\n' "$1"
}

# ---- fixture ----------------------------------------------------------------
new_fixture() {
  [ -n "$fx" ] && rm -rf "$fx"
  fx=$(mktemp -d "${TMPDIR:-/tmp}/effigy-install-local.XXXXXX") || exit 2
  fx=$(cd "$fx" && pwd -P)
  repo="$fx/checkout"
  home="$fx/home"
  stubs="$fx/stubs"
  log="$fx/events.log"
  out="$fx/out.log"
  launcher="$home/.local/bin/effigy"
  keychain="$home/keychain"
  active="$repo/.local-install/bin/effigy"
  active_version="$repo/.local-install/bin/effigy.active-version"
  previous="$repo/.local-install/bin/effigy.previous"
  previous_version="$repo/.local-install/bin/effigy.previous.version"
  mkdir -p "$repo/scripts" "$home" "$stubs"
  cp "$here/scripts/build-local-bin.rhai" \
    "$here/scripts/local-codesign.sh" \
    "$here/scripts/install-local-bin-links.rhai" \
    "$here/scripts/oxide-selector.sh" "$repo/scripts/"
  cat >"$repo/Cargo.toml" <<'TOML'
[workspace]
members = []

[workspace.package]
version = "1.2.3"
TOML
  cat >"$repo/effigy.toml" <<'TOML'
[catalog]
alias = "effigy"

[tasks]
"install:local" = { visibility = "internal", group = "Maintenance", run = [
  { rhai = "scripts/build-local-bin.rhai" },
  { rhai = "scripts/install-local-bin-links.rhai" },
] }
TOML
  : >"$log"
  : >"$out"
  : >"$keychain"
  write_stubs
}

write_stubs() {
  cat >"$stubs/oxide" <<'STUB'
#!/bin/sh
printf 'oxide %s\n' "$*" >>"$FIXTURE_LOG"
if [ "${STUB_OXIDE_FAIL:-}" = "1" ]; then
  echo "stub oxide: refusing to build" >&2
  exit 5
fi
if [ "${STUB_OXIDE_OMIT_ARTIFACT:-}" = "1" ]; then
  exit 0
fi
mkdir -p target/release
printf '%s\n' "${STUB_RELEASE_BODY:-release-1}" >target/release/effigy
chmod 0755 target/release/effigy
STUB
  cat >"$stubs/cargo" <<'STUB'
#!/bin/sh
printf 'cargo %s\n' "$*" >>"$FIXTURE_LOG"
echo "stub cargo: the local installer must not fall back to plain Cargo" >&2
exit 87
STUB
  cat >"$stubs/uname" <<'STUB'
#!/bin/sh
printf 'uname %s\n' "$*" >>"$FIXTURE_LOG"
printf '%s\n' "${STUB_UNAME:-Darwin}"
STUB
  cat >"$stubs/security" <<'STUB'
#!/bin/sh
# The fixture's private keychain is a file listing the common names it holds.
# `-P` values are redacted, so no passphrase, not even the throwaway one that
# guards the transient PKCS#12 container, reaches the event log.
prev=""
logged=""
for arg in "$@"; do
  if [ "$prev" = "-P" ]; then
    arg="<redacted>"
  fi
  logged="$logged $arg"
  prev=$arg
done
printf 'security%s\n' "$logged" >>"$FIXTURE_LOG"
case ${1:-} in
  find-identity)
    count=0
    while IFS= read -r name; do
      [ -n "$name" ] || continue
      count=$((count + 1))
      printf '  %s) 0123456789ABCDEF0123456789ABCDEF01234567 "%s" (CSSMERR_TP_NOT_TRUSTED)\n' "$count" "$name"
    done <"$FIXTURE_KEYCHAIN"
    printf '     %s identities found\n' "$count"
    ;;
  import)
    if [ "${STUB_SECURITY_IMPORT_FAIL:-}" = "1" ]; then
      echo "stub security: refusing the import" >&2
      exit 7
    fi
    p12=""
    pass=""
    prev=""
    for arg in "$@"; do
      if [ "$prev" = "-P" ]; then
        pass=$arg
      fi
      case $arg in
        *.p12) p12=$arg ;;
      esac
      prev=$arg
    done
    [ -n "$p12" ] || { echo "stub security: no PKCS#12 container to import" >&2; exit 7; }
    # Prove the container really carries the identity that was requested.
    name=$(openssl pkcs12 -in "$p12" -passin "pass:$pass" -nokeys -clcerts 2>/dev/null \
      | openssl x509 -noout -subject 2>/dev/null \
      | sed -n 's/.*CN *= *\([^,/]*\).*/\1/p')
    [ -n "$name" ] || { echo "stub security: the container carries no readable certificate" >&2; exit 7; }
    printf '%s\n' "$name" >>"$FIXTURE_KEYCHAIN"
    echo "1 identity imported."
    ;;
  *)
    echo "stub security: unsupported subcommand ${1:-}" >&2
    exit 64
    ;;
esac
STUB
  cat >"$stubs/codesign" <<'STUB'
#!/bin/sh
printf 'codesign %s\n' "$*" >>"$FIXTURE_LOG"
if [ "${STUB_CODESIGN_FAIL:-}" = "1" ]; then
  echo "stub codesign: refusing to sign" >&2
  exit 9
fi
exit 0
STUB
  cat >"$stubs/cp" <<'STUB'
#!/bin/sh
printf 'cp %s\n' "$*" >>"$FIXTURE_LOG"
if [ -n "${STUB_CP_FAIL_MATCH:-}" ]; then
  for arg in "$@"; do
    case $arg in *"$STUB_CP_FAIL_MATCH"*) echo "stub cp: refusing $arg" >&2; exit 7 ;; esac
  done
fi
if [ -n "${STUB_CP_CORRUPT_MATCH:-}" ]; then
  for arg in "$@"; do dest=$arg; done
  case $dest in
    *"$STUB_CP_CORRUPT_MATCH"*)
      printf 'corrupted-copy\n' >"$dest"
      exit 0
      ;;
  esac
fi
exec "$REAL_CP" "$@"
STUB
  cat >"$stubs/mv" <<'STUB'
#!/bin/sh
printf 'mv %s\n' "$*" >>"$FIXTURE_LOG"
if [ -n "${STUB_MV_FAIL_MATCH:-}" ]; then
  for arg in "$@"; do
    case $arg in *"$STUB_MV_FAIL_MATCH"*) echo "stub mv: refusing $arg" >&2; exit 7 ;; esac
  done
fi
exec "$REAL_MV" "$@"
STUB
  cat >"$stubs/chmod" <<'STUB'
#!/bin/sh
printf 'chmod %s\n' "$*" >>"$FIXTURE_LOG"
if [ -n "${STUB_CHMOD_FAIL_MATCH:-}" ]; then
  for arg in "$@"; do
    case $arg in *"$STUB_CHMOD_FAIL_MATCH"*) echo "stub chmod: refusing $arg" >&2; exit 7 ;; esac
  done
fi
exec "$REAL_CHMOD" "$@"
STUB
  chmod 0755 "$stubs/oxide" "$stubs/cargo" "$stubs/uname" "$stubs/security" "$stubs/codesign" \
    "$stubs/cp" "$stubs/mv" "$stubs/chmod"
}

run_install() { # [env assignment ...]
  # The fixture HOME is private, so name a pool explicitly: the selector would
  # otherwise ask the stub `oxide` for `status` and the fixture log would carry
  # an extra call that no install step made.
  (cd "$repo" && env -u GITHUB_ACTIONS -u EFFIGY_OXIDE_ESCAPE \
    HOME="$home" OXIDE_POOL="$fx/pool" FIXTURE_LOG="$log" FIXTURE_KEYCHAIN="$keychain" \
    GIT_CEILING_DIRECTORIES="$fx" \
    REAL_CP="$real_cp" REAL_MV="$real_mv" REAL_CHMOD="$real_chmod" \
    PATH="$stubs:/usr/bin:/bin:/usr/sbin:/sbin" \
    "$@" "$binary" install:local) >"$out" 2>&1
}

# Run the signing helper itself, against the same private keychain.
# Usage: run_helper [env assignment ...] sh scripts/local-codesign.sh <args>
run_helper() {
  (cd "$repo" && env HOME="$home" FIXTURE_LOG="$log" FIXTURE_KEYCHAIN="$keychain" \
    REAL_CP="$real_cp" REAL_MV="$real_mv" REAL_CHMOD="$real_chmod" \
    PATH="$stubs:/usr/bin:/bin:/usr/sbin:/sbin" \
    "$@") >"$out" 2>&1
}

set_version() {
  cat >"$repo/Cargo.toml" <<TOML
[workspace]
members = []

[workspace.package]
version = "$1"
TOML
}

snapshot_install() { # writes $fx/snapshot
  {
    if [ -e "$active" ]; then file_hash "$active"; else echo absent; fi
    if [ -e "$active_version" ]; then cat "$active_version"; else echo absent; fi
    if [ -e "$launcher" ]; then file_hash "$launcher"; else echo absent; fi
  } >"$fx/snapshot"
}
check_snapshot_unchanged() { # label
  {
    if [ -e "$active" ]; then file_hash "$active"; else echo absent; fi
    if [ -e "$active_version" ]; then cat "$active_version"; else echo absent; fi
    if [ -e "$launcher" ]; then file_hash "$launcher"; else echo absent; fi
  } >"$fx/snapshot.after"
  if cmp -s "$fx/snapshot" "$fx/snapshot.after"; then
    ok "$1"
  else
    fail "$1: active install changed ($(cat "$fx/snapshot" | tr '\n' ' ') -> $(cat "$fx/snapshot.after" | tr '\n' ' '))"
  fi
}

# ---- 1. fresh install: release artifact, argv, wrapper ----------------------
new_fixture
run_install
rc=$?
check_eq "fresh install exits 0" "$rc" 0
check_contains "fresh install builds the release binary through Oxide" "$log" "oxide build --release --bin effigy"
check_absent "fresh install never falls back to Cargo" "$log" "cargo "
check_file_eq "fresh install activates the release artifact" "$active" "release-1"
check_file_eq "fresh install writes the active version receipt" "$active_version" "v1.2.3+local.unknown"
check_file_eq "fresh install writes the maintained wrapper" "$launcher" "$(wrapper_for "$active")"
check_mode "fresh wrapper is executable" "$launcher" 755
check_mode "fresh active binary is executable" "$active" 755
[ -e "$repo/.local-install/bin/effigy.new" ] && fail "fresh install left a staged binary" || ok "fresh install consumes the staged binary"
[ -e "$active_version.new" ] && fail "fresh install left a staged version receipt" || ok "fresh install consumes the staged version receipt"

# ---- 2. upgrade: previous binary and version receipts -----------------------
new_fixture
run_install
set_version 1.2.4
: >"$log"
run_install STUB_RELEASE_BODY=release-2
rc=$?
check_eq "upgrade exits 0" "$rc" 0
check_file_eq "upgrade activates the new release artifact" "$active" "release-2"
check_file_eq "upgrade records the new active version" "$active_version" "v1.2.4+local.unknown"
check_file_eq "upgrade preserves the previous binary" "$previous" "release-1"
check_file_eq "upgrade preserves the previous version receipt" "$previous_version" "v1.2.3+local.unknown"
check_mode "previous binary stays owner-only" "$previous" 700
check_mode "previous version receipt stays owner-only" "$previous_version" 600
check_file_eq "upgrade leaves the current wrapper in place" "$launcher" "$(wrapper_for "$active")"
check_absent "current wrapper is not rewritten for a known current target" "$log" "activate stable channel wrapper"

# ---- 3. customized regular launcher is preserved byte-for-byte --------------
new_fixture
run_install
printf '#!/bin/sh\n# operator launcher\nexec /opt/effigy/stable "$@"\n' >"$launcher"
chmod 0750 "$launcher"
custom_content=$(cat "$launcher")
custom_hash=$(file_hash "$launcher")
: >"$log"
run_install STUB_RELEASE_BODY=release-2
rc=$?
check_eq "custom launcher install exits 0" "$rc" 0
check_eq "custom launcher hash is preserved" "$(file_hash "$launcher")" "$custom_hash"
check_file_eq "custom launcher bytes are preserved" "$launcher" "$custom_content"
check_mode "custom launcher mode is preserved" "$launcher" 750
check_contains "custom launcher disposition is explained" "$out" "preserved existing custom launcher"
check_file_eq "custom launcher does not block the binary upgrade" "$active" "release-2"

# ---- 4. non-text launcher content is preserved and explained ----------------
new_fixture
run_install
printf '\377\376\000binary-launcher' >"$launcher"
custom_hash=$(file_hash "$launcher")
: >"$log"
run_install
rc=$?
check_eq "binary launcher install exits 0" "$rc" 0
check_eq "binary launcher hash is preserved" "$(file_hash "$launcher")" "$custom_hash"
check_contains "binary launcher disposition is explained" "$out" "preserved existing custom launcher"

# ---- 5. custom symlink launcher is preserved --------------------------------
new_fixture
run_install
rm -f "$launcher"
ln -s "$fx/custom/effigy" "$launcher"
: >"$log"
run_install
rc=$?
check_eq "symlink launcher install exits 0" "$rc" 0
[ -h "$launcher" ] && ok "custom symlink survives" || fail "custom symlink was replaced"
check_eq "custom symlink target is preserved" "$(readlink "$launcher")" "$fx/custom/effigy"
check_contains "symlink disposition is explained" "$out" "preserved existing symlink launcher"

# ---- 6. generated wrapper for a known target is refreshed -------------------
new_fixture
run_install
wrapper_for "$fx/old-checkout/.local-install/bin/effigy" >"$launcher"
: >"$log"
run_install
rc=$?
check_eq "stale wrapper install exits 0" "$rc" 0
check_file_eq "generated wrapper is refreshed to this checkout" "$launcher" "$(wrapper_for "$active")"
check_contains "generated wrapper refresh is reported" "$out" "refreshed generated stable channel wrapper"

# ---- 7. unsupported launcher path is refused without clobber -----------------
new_fixture
run_install
rm -f "$launcher"
mkdir -p "$launcher"
run_install
rc=$?
[ "$rc" -ne 0 ] && ok "directory launcher is refused" || fail "directory launcher install exited 0"
[ -d "$launcher" ] && ok "directory launcher survives" || fail "directory launcher was replaced"
check_contains "directory launcher refusal is reported" "$out" "refusing to replace unsupported path"

# ---- 8. build failure leaves the active install untouched ------------------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_OXIDE_FAIL=1
rc=$?
[ "$rc" -ne 0 ] && ok "build failure fails the install" || fail "build failure exited 0"
check_contains "build failure is reported" "$out" "build --release --bin effigy through Oxide failed"
check_snapshot_unchanged "build failure leaves the active install untouched"
check_absent "build failure stages no copy" "$log" "cp "
check_absent "build failure renames nothing" "$log" "mv "

# ---- 9. missing release artifact fails instead of reusing a stale one -------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_OXIDE_OMIT_ARTIFACT=1
rc=$?
[ "$rc" -ne 0 ] && ok "missing release artifact fails the install" || fail "missing release artifact exited 0"
check_contains "missing release artifact is reported" "$out" "missing built effigy release binary"
check_contains "missing release artifact names the Cargo escape" "$out" "EFFIGY_OXIDE_ESCAPE=cargo effigy install:local"
check_absent "missing release artifact never falls back to Cargo" "$log" "cargo "
check_snapshot_unchanged "missing release artifact leaves the active install untouched"

# ---- 10. staging copy failure leaves the active install untouched -----------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_CP_FAIL_MATCH=effigy.new
rc=$?
[ "$rc" -ne 0 ] && ok "staging copy failure fails the install" || fail "staging copy failure exited 0"
check_contains "staging copy failure is reported" "$out" "stage local effigy binary failed"
check_snapshot_unchanged "staging copy failure leaves the active install untouched"
check_absent "staging copy failure never activates" "$log" "effigy.new "

# ---- 11. corrupted staged copy is refused ----------------------------------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_CP_CORRUPT_MATCH=effigy.new
rc=$?
[ "$rc" -ne 0 ] && ok "corrupted staged copy fails the install" || fail "corrupted staged copy exited 0"
check_contains "corrupted staged copy is reported" "$out" "is not a byte-identical copy of the release binary"
check_snapshot_unchanged "corrupted staged copy leaves the active install untouched"
[ -e "$repo/.local-install/bin/effigy.new" ] && fail "corrupted staged copy was left behind" || ok "corrupted staged copy is removed"
check_absent "corrupted staged copy never activates" "$log" "effigy.new "

# ---- 12. activation rename failure leaves the active install untouched ------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_MV_FAIL_MATCH=effigy.new
rc=$?
[ "$rc" -ne 0 ] && ok "activation rename failure fails the install" || fail "activation rename failure exited 0"
check_contains "activation rename failure is reported" "$out" "activate local effigy binary failed"
check_snapshot_unchanged "activation rename failure leaves the active install untouched"

# ---- 13. executable-bit failure leaves the active install untouched ---------
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_CHMOD_FAIL_MATCH=effigy.new
rc=$?
[ "$rc" -ne 0 ] && ok "staging chmod failure fails the install" || fail "staging chmod failure exited 0"
check_contains "staging chmod failure is reported" "$out" "mark staged local effigy binary executable failed"
check_snapshot_unchanged "staging chmod failure leaves the active install untouched"

# ---- 14. symlinked active binary is refused before any copy ------------------
new_fixture
run_install
printf 'operator-binary\n' >"$fx/operator-effigy"
rm -f "$active"
ln -s "$fx/operator-effigy" "$active"
snapshot_install
: >"$log"
run_install
rc=$?
[ "$rc" -ne 0 ] && ok "symlinked active binary is refused" || fail "symlinked active binary install exited 0"
check_contains "symlinked active binary refusal is reported" "$out" "refusing to replace non-regular active effigy binary"
[ -h "$active" ] && ok "symlinked active binary survives" || fail "symlinked active binary was replaced"
check_eq "symlinked active binary target is preserved" "$(readlink "$active")" "$fx/operator-effigy"
check_file_eq "symlink destination bytes are untouched" "$fx/operator-effigy" "operator-binary"
check_absent "symlinked active binary stages no copy" "$log" "cp "

# ---- 15. macOS signing: two builds select the one stable identity -----------
new_fixture
run_install
rc=$?
check_eq "first signed install exits 0" "$rc" 0
check_file_eq "first install provisions one local signing identity" "$keychain" "Effigy Local Signing"
check_contains "first install signs the staged binary" "$log" "codesign --force --sign Effigy Local Signing --identifier com.effigy.local"
check_contains "first install verifies the signature it applied" "$log" "codesign --verify --strict"
check_contains "provisioning is explained" "$out" 'provisioned the local code-signing identity "Effigy Local Signing"'
check_file_eq "signed install still activates the release artifact bytes" "$active" "release-1"
: >"$log"
run_install STUB_RELEASE_BODY=release-2
rc=$?
check_eq "second signed install exits 0" "$rc" 0
check_file_eq "second install reuses that same identity" "$keychain" "Effigy Local Signing"
check_absent "second install imports no identity" "$log" "security import"
check_contains "second install signs with the same identity" "$log" "codesign --force --sign Effigy Local Signing --identifier com.effigy.local"
check_contains "reuse is explained" "$out" 'reusing the local code-signing identity "Effigy Local Signing"'
check_file_eq "signed install activates the new release artifact" "$active" "release-2"
check_mode "signed active binary is executable" "$active" 755

# ---- 16. a refused signature leaves the active install and rollback intact --
new_fixture
run_install
snapshot_install
: >"$log"
run_install STUB_CODESIGN_FAIL=1
rc=$?
[ "$rc" -ne 0 ] && ok "signing failure fails the install" || fail "signing failure exited 0"
check_contains "signing failure is reported" "$out" "sign staged local effigy binary failed"
check_contains "signing failure names the signing opt-out" "$out" "EFFIGY_LOCAL_CODESIGN=off"
check_snapshot_unchanged "signing failure leaves the active install and launcher untouched"
check_file_eq "signing failure leaves the rollback binary" "$previous" "release-1"
[ -e "$repo/.local-install/bin/effigy.new" ] && fail "signing failure left the staged binary" || ok "signing failure consumes the staged binary"
check_absent "signing failure never activates" "$log" "effigy.new $active"

# ---- 17. a host that is not macOS selects nothing and signs nothing --------
new_fixture
run_install STUB_UNAME=Linux
rc=$?
check_eq "non-macOS install exits 0" "$rc" 0
check_absent "non-macOS install touches no keychain" "$log" "security "
check_absent "non-macOS install signs nothing" "$log" "codesign "
check_file_eq "non-macOS install activates the release artifact" "$active" "release-1"
check_contains "non-macOS skip is explained" "$out" "this host is not macOS"
[ -s "$keychain" ] && fail "non-macOS install recorded a keychain identity" || ok "non-macOS install leaves the keychain empty"

# ---- 18. EFFIGY_LOCAL_CODESIGN=off skips signing on macOS ------------------
new_fixture
run_install EFFIGY_LOCAL_CODESIGN=off
rc=$?
check_eq "signing opt-out install exits 0" "$rc" 0
check_absent "signing opt-out touches no keychain" "$log" "security "
check_absent "signing opt-out signs nothing" "$log" "codesign "
check_file_eq "signing opt-out activates the release artifact" "$active" "release-1"
check_contains "signing opt-out is explained" "$out" "skipped: EFFIGY_LOCAL_CODESIGN=off"

# ---- 19. EFFIGY_LOCAL_CODESIGN_IDENTITY pins the identity ------------------
new_fixture
run_install EFFIGY_LOCAL_CODESIGN_IDENTITY="Apple Development: Fixture (TEAMID1234)"
rc=$?
check_eq "pinned identity install exits 0" "$rc" 0
check_absent "pinned identity discovers nothing" "$log" "security find-identity"
check_absent "pinned identity provisions nothing" "$log" "security import"
check_contains "pinned identity is the identity used" "$log" "codesign --force --sign Apple Development: Fixture (TEAMID1234)"
check_file_eq "pinned identity install activates the release artifact" "$active" "release-1"

# ---- 20. a refused identity provision fails before the build ---------------
new_fixture
run_install
snapshot_install
: >"$keychain"
: >"$log"
run_install STUB_SECURITY_IMPORT_FAIL=1
rc=$?
[ "$rc" -ne 0 ] && ok "identity provision failure fails the install" || fail "identity provision failure exited 0"
check_contains "identity provision failure is reported" "$out" "local effigy signing identity selection failed"
check_contains "identity provision failure names the signing opt-out" "$out" "EFFIGY_LOCAL_CODESIGN=off"
check_absent "identity provision failure never builds" "$log" "oxide "
check_snapshot_unchanged "identity provision failure leaves the active install untouched"

# ---- 21. the signing helper explains its own refusals ----------------------
new_fixture
printf 'unsigned-binary\n' >"$fx/unsigned-binary"
: >"$out"
run_helper STUB_CODESIGN_FAIL=1 sh scripts/local-codesign.sh sign "Effigy Local Signing" "$fx/unsigned-binary"
rc=$?
[ "$rc" -ne 0 ] && ok "helper refuses a signature it cannot apply" || fail "helper exited 0 for a refused signature"
check_contains "helper reports the refused signature" "$out" "codesign could not sign"
check_contains "helper names the signing opt-out" "$out" "EFFIGY_LOCAL_CODESIGN=off"
run_helper STUB_SECURITY_IMPORT_FAIL=1 sh scripts/local-codesign.sh select
rc=$?
[ "$rc" -ne 0 ] && ok "helper refuses an identity it cannot import" || fail "helper exited 0 for a refused import"
check_contains "helper reports the refused import" "$out" "could not import the local signing identity"
check_contains "helper names the Keychain Access path" "$out" 'certificate named "Effigy Local Signing"'
[ -s "$keychain" ] && fail "a refused import recorded a keychain identity" || ok "a refused import records no identity"

# ---- 22. an unknown subcommand is a usage error ---------------------------
new_fixture
run_helper sh scripts/local-codesign.sh sign
rc=$?
[ "$rc" -ne 0 ] && ok "helper refuses an incomplete sign invocation" || fail "incomplete sign invocation exited 0"
check_contains "helper usage is reported" "$out" "usage: local-codesign.sh sign <ID> <BINARY>"
check_absent "usage error signs nothing" "$log" "codesign "
run_helper sh scripts/local-codesign.sh
rc=$?
[ "$rc" -ne 0 ] && ok "helper refuses an unknown subcommand" || fail "bare helper invocation exited 0"
check_contains "helper top-level usage is reported" "$out" "usage: local-codesign.sh select | sign <ID> <BINARY>"

echo
if [ "$fails" -eq 0 ]; then
  echo "all install:local fixture checks passed"
  exit 0
fi
echo "$fails install:local fixture check(s) failed" >&2
exit 1
