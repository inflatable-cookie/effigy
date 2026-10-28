#!/bin/sh
# One stable macOS code-signing identity for this checkout's local install.
#
# `effigy install:local` rebuilds the optimized binary on every source change.
# An unsigned rebuild is a new program to macOS: Gatekeeper and the privacy
# prompts key off the code identity, so a verification panel comes back with
# every build. Signing each activated binary with one identity that stays in the
# user's keychain gives every local rebuild the same designated requirement
# (`identifier "com.effigy.local" and certificate root = H"…"`).
#
# Usage:
#   sh scripts/local-codesign.sh select              # resolve or provision it
#   sh scripts/local-codesign.sh sign <ID> <BINARY>  # sign one staged binary
#
# `select` writes the identity to stdout (nothing when signing is skipped) and
# one explanatory line to stderr. `sign` writes nothing on success. Both exit
# non-zero on failure, and the caller fails the install before activation.
#
# Rules, in order:
#   1. `EFFIGY_LOCAL_CODESIGN=off` skips signing for this build;
#   2. a host that is not macOS is untouched: nothing is selected or signed;
#   3. `EFFIGY_LOCAL_CODESIGN_IDENTITY` names the identity to use, by common
#      name or SHA-1 fingerprint, and nothing is discovered or provisioned;
#   4. an existing "Effigy Local Signing" identity is reused;
#   5. otherwise one is provisioned once and kept: a self-signed code-signing
#      certificate and private key made with /usr/bin/openssl and imported into
#      the user's default keychain with `security import -T /usr/bin/codesign`,
#      so codesign reads the key without a keychain prompt. The certificate and
#      key are generated in an owner-only temporary directory that is removed on
#      every exit path; only the keychain items stay. No sudo, no system trust
#      setting, no Apple certificate and no notarization.
#
# This is local-install signing only: release artifacts are signed by the
# release pipeline, not here. Guide 010 documents the operator view.
set -u

identity_name="Effigy Local Signing"
identifier="com.effigy.local"

say() {
  echo "$*" >&2
}

fail() {
  echo "local-codesign: $*" >&2
  exit 1
}

is_macos() {
  [ "$(uname -s 2>/dev/null || echo unknown)" = "Darwin" ]
}

# Identities the keychain offers for code signing, one common name per line.
# `-v` is deliberately absent: it would hide the self-signed identity this
# script provisions and turn every rebuild into a fresh provisioning.
codesigning_identities() {
  security find-identity -p codesigning 2>/dev/null \
    | sed -n 's/^[^"]*"\([^"]*\)".*$/\1/p'
}

# Create the self-signed code-signing identity in the default keychain and keep
# it there. Everything written below `work` is transient key material.
provision_identity() {
  work=$(mktemp -d "${TMPDIR:-/tmp}/effigy-codesign-identity.XXXXXX") \
    || fail "could not create the owner-only directory for the signing identity"
  trap 'rm -rf "$work"' EXIT
  trap 'rm -rf "$work"; exit 1' HUP INT TERM

  chmod 0700 "$work" || fail "could not restrict the signing identity directory: $work"

  cat >"$work/identity.cnf" <<CNF || fail "could not write $work/identity.cnf"
[req]
prompt = no
distinguished_name = identity
x509_extensions = codesign
[identity]
CN = $identity_name
O = Effigy local install
[codesign]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF

  if ! /usr/bin/openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -config "$work/identity.cnf" \
    -keyout "$work/identity.key" -out "$work/identity.crt" \
    >"$work/req.log" 2>&1; then
    cat "$work/req.log" >&2
    say "hint: EFFIGY_LOCAL_CODESIGN=off skips signing this build"
    fail "could not create the local signing certificate with /usr/bin/openssl"
  fi

  # `security import` rejects an empty PKCS#12 passphrase, so this one is random
  # per run. It guards only the transient container below, which is deleted on
  # exit; the retained private key is the keychain item.
  passphrase=$(/usr/bin/openssl rand -hex 24) \
    || fail "could not generate a passphrase for the transient signing container"
  if ! /usr/bin/openssl pkcs12 -export -inkey "$work/identity.key" \
    -in "$work/identity.crt" -name "$identity_name" \
    -out "$work/identity.p12" -passout "pass:$passphrase" \
    >"$work/pkcs12.log" 2>&1; then
    cat "$work/pkcs12.log" >&2
    say "hint: EFFIGY_LOCAL_CODESIGN=off skips signing this build"
    fail "could not package the local signing identity with /usr/bin/openssl"
  fi

  # `-T /usr/bin/codesign` grants codesign access to the imported key, so no
  # keychain prompt stands between a rebuild and its signature.
  if ! security import "$work/identity.p12" -P "$passphrase" -T /usr/bin/codesign \
    >"$work/import.log" 2>&1; then
    cat "$work/import.log" >&2
    say "hint: create a self-signed code-signing certificate named \"$identity_name\""
    say "      in Keychain Access (Certificate Assistant) and re-run install:local,"
    say "      or set EFFIGY_LOCAL_CODESIGN=off to skip signing this build"
    fail "could not import the local signing identity into the default keychain"
  fi

  if [ -z "$(codesigning_identities | grep -F -x -- "$identity_name" | head -n 1)" ]; then
    say "hint: create a self-signed code-signing certificate named \"$identity_name\""
    say "      in Keychain Access (Certificate Assistant) and re-run install:local,"
    say "      or set EFFIGY_LOCAL_CODESIGN=off to skip signing this build"
    fail "the imported identity is not offered for code signing"
  fi

  rm -rf "$work"
  trap - EXIT HUP INT TERM
}

select_identity() {
  if [ "${EFFIGY_LOCAL_CODESIGN:-}" = "off" ]; then
    say "skipped: EFFIGY_LOCAL_CODESIGN=off"
    return 0
  fi

  if ! is_macos; then
    say "skipped: this host is not macOS, so no code signature is applied"
    return 0
  fi

  if [ -n "${EFFIGY_LOCAL_CODESIGN_IDENTITY:-}" ]; then
    say "using EFFIGY_LOCAL_CODESIGN_IDENTITY=\"${EFFIGY_LOCAL_CODESIGN_IDENTITY}\""
    printf '%s\n' "${EFFIGY_LOCAL_CODESIGN_IDENTITY}"
    return 0
  fi

  existing=$(codesigning_identities | grep -F -x -- "$identity_name" | head -n 1)
  if [ -n "$existing" ]; then
    say "reusing the local code-signing identity \"$identity_name\" from the user keychain"
    printf '%s\n' "$existing"
    return 0
  fi

  provision_identity
  say "provisioned the local code-signing identity \"$identity_name\" in the user keychain"
  printf '%s\n' "$identity_name"
}

sign_binary() {
  identity=$1
  binary=$2

  [ -n "$identity" ] || fail "sign needs the identity to sign with"
  [ -f "$binary" ] || fail "missing binary to sign: $binary"

  if ! output=$(codesign --force --sign "$identity" --identifier "$identifier" \
    --timestamp=none "$binary" 2>&1); then
    echo "$output" >&2
    say "hint: EFFIGY_LOCAL_CODESIGN=off skips signing this build"
    fail "codesign could not sign $binary with \"$identity\""
  fi

  if ! output=$(codesign --verify --strict "$binary" 2>&1); then
    echo "$output" >&2
    say "hint: EFFIGY_LOCAL_CODESIGN=off skips signing this build"
    fail "codesign could not verify the signature on $binary"
  fi
}

case ${1:-} in
  select)
    [ $# -eq 1 ] || fail "usage: local-codesign.sh select"
    select_identity
    ;;
  sign)
    [ $# -eq 3 ] || fail "usage: local-codesign.sh sign <ID> <BINARY>"
    sign_binary "$2" "$3"
    ;;
  *)
    fail "usage: local-codesign.sh select | sign <ID> <BINARY>"
    ;;
esac
