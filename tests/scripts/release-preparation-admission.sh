#!/usr/bin/env bash
#
# Private focused proof for the maintained `effigy release:prepare` selector:
# the manifest heavy admission, the argv-safe entry point, and its refusal of
# wrong checkout, branch, dirty tree and unpushed source.
#
# No build, no compile, no live scheduler and no live release. The entry point
# is exercised against disposable git fixtures with a recording `cargo` stub on
# PATH, which is the only seam this proof needs.

set -euo pipefail
export LC_ALL=C

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
entry="$repo_root/scripts/release-prepare.sh"
manifest="$repo_root/config/tasks.toml"
tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/effigy-release-prepare-admission.XXXXXX")
trap 'rm -rf "$tmp_root"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

# 1) Manifest guard: the selector is heavy-admitted and wired to the reviewed
#    entry point. This is the property the scheduler routes on.
grep -Eq '^"release:prepare"\.admission = "heavy"$' "$manifest" ||
  fail 'release:prepare is not declared admission = "heavy"'
grep -Fq '"release:prepare".run = "bash scripts/release-prepare.sh {args}"' "$manifest" ||
  fail 'release:prepare is not wired to the reviewed entry point'

# 2) Disposable clean, pushed `main` checkout carrying the real entry point.
fixture="$tmp_root/repo"
origin="$tmp_root/origin.git"
git init --quiet --bare "$origin"
git -c init.defaultBranch=main init --quiet "$fixture"
mkdir -p "$fixture/scripts"
cp "$entry" "$fixture/scripts/release-prepare.sh"
printf 'fixture\n' > "$fixture/README.md"
git -C "$fixture" add -A
git -C "$fixture" -c user.email=release@example.invalid -c user.name='Release Fixture' \
  commit --quiet -m init
git -C "$fixture" remote add origin "$origin"
git -C "$fixture" push --quiet -u origin main

# A recording `cargo` stub. The entry point execs `cargo run ...`; the stub
# captures the exact forwarded argv and exits 0, so nothing is compiled.
stub_bin="$tmp_root/bin"
mkdir -p "$stub_bin"
cat > "$stub_bin/cargo" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$EFFIGY_TEST_CARGO_ARGS"
exit 0
STUB
chmod +x "$stub_bin/cargo"

# Run the fixture entry point with the stub cargo first on PATH. GIT stays the
# real git. `$1` names the recording file; the rest are forwarded options.
run_entry() {
  local record="$1"
  shift
  rm -f "$record"
  (
    cd "$fixture"
    EFFIGY_TEST_CARGO_ARGS="$record" PATH="$stub_bin:$PATH" \
      bash "$fixture/scripts/release-prepare.sh" "$@"
  )
}

expect_record() {
  local label="$1" expected="$2"
  shift 2
  local record="$tmp_root/$label.args"
  run_entry "$record" "$@" >"$tmp_root/$label.log" 2>&1 ||
    fail "$label exited non-zero; log: $(cat "$tmp_root/$label.log")"
  cmp -s "$record" "$expected" ||
    fail "$label forwarded $(tr '\n' ' ' < "$record")"
}

expect_refusal() {
  local label="$1"
  shift
  local record="$tmp_root/refusal-$label.args"
  rm -f "$record"
  if (
    cd "$fixture"
    EFFIGY_TEST_CARGO_ARGS="$record" PATH="$stub_bin:$PATH" \
      bash "$fixture/scripts/release-prepare.sh" "$@" >"$tmp_root/$label.log" 2>&1
  ); then
    fail "$label unexpectedly succeeded"
  fi
  [ ! -e "$record" ] || fail "$label recorded a cargo invocation before refusing"
}

# 3) Exact argv forwarding for each admitted mode.
cat > "$tmp_root/expected-yes.args" <<'EOF'
run
--quiet
--bin
effigy
--
release
prepare
--yes
--check-gates
--version
0.14.0
EOF
expect_record yes "$tmp_root/expected-yes.args" --yes --version 0.14.0

cat > "$tmp_root/expected-plan.args" <<'EOF'
run
--quiet
--bin
effigy
--
release
prepare
--plan
EOF
expect_record plan "$tmp_root/expected-plan.args" --plan

cat > "$tmp_root/expected-plan-gates.args" <<'EOF'
run
--quiet
--bin
effigy
--
release
prepare
--plan
--check-gates
--version
0.14.0
EOF
expect_record plan-gates "$tmp_root/expected-plan-gates.args" \
  --plan --check-gates --version 0.14.0

cat > "$tmp_root/expected-json.args" <<'EOF'
run
--quiet
--bin
effigy
--
release
prepare
--yes
--check-gates
--version
0.14.0
--json
EOF
expect_record json "$tmp_root/expected-json.args" --yes --version 0.14.0 --json

for record in "$tmp_root"/*.args; do
  case "$record" in
    */refusal-*) continue ;;
  esac
  if grep -Eq '^(execute|resume|tag|publish|--repo)$' "$record"; then
    fail "forwarded argv in $record contains a forbidden token"
  fi
done
printf 'ok: exact argv forwarding cannot select execute/resume/tag/publish\n'

# 4) Option refusals. A mutating run must be explicit and must name a version.
expect_refusal no-intent
expect_refusal repo-override --yes --version 0.14.0 --repo "$tmp_root/other"
expect_refusal repo-option --yes --version 0.14.0 --repo="$tmp_root/other"
expect_refusal missing-version --yes
expect_refusal conflicting --plan --yes --version 0.14.0
expect_refusal unknown-option --execute
expect_refusal positional execute
expect_refusal missing-value --yes --version
expect_refusal version-dash --yes --version -1
expect_refusal literal-delimiter --yes --version 0.14.0 --
printf 'ok: invalid options refuse before any cargo invocation\n'

# 5) Wrong branch, dirty tracked tree, and unpushed main all refuse.
git -C "$fixture" checkout --quiet -b release-branch
expect_refusal wrong-branch --yes --version 0.14.0
git -C "$fixture" checkout --quiet main

printf 'dirty\n' >> "$fixture/README.md"
expect_refusal dirty-tree --yes --version 0.14.0
git -C "$fixture" checkout --quiet -- README.md

git -C "$fixture" -c user.email=release@example.invalid -c user.name='Release Fixture' \
  commit --quiet --allow-empty -m extra
expect_refusal unpushed --yes --version 0.14.0
git -C "$fixture" reset --quiet --hard origin/main

# 6) A copied entry point cannot prepare a different checkout.
other="$tmp_root/other"
git -c init.defaultBranch=main init --quiet "$other"
printf 'other\n' > "$other/README.md"
git -C "$other" add -A
git -C "$other" -c user.email=release@example.invalid -c user.name='Release Fixture' \
  commit --quiet -m init
if (
  cd "$other"
  EFFIGY_TEST_CARGO_ARGS="$tmp_root/wrong-checkout.args" PATH="$stub_bin:$PATH" \
    bash "$fixture/scripts/release-prepare.sh" --yes --version 0.14.0 \
    >"$tmp_root/wrong-checkout.log" 2>&1
); then
  fail 'wrong-checkout entry point unexpectedly succeeded'
fi
[ ! -e "$tmp_root/wrong-checkout.args" ] ||
  fail 'wrong-checkout entry point invoked cargo'
printf 'ok: wrong branch, dirty tree, unpushed source and foreign checkout refuse\n'

# 7) Production admission precedes the builder and prepare effects. Run the
#    real selector against an isolated, empty host-run root: routing must fail
#    closed at the scheduler (exit 75) without ever reaching the entry point's
#    cargo build. A recording cargo stub makes any accidental execution
#    visible without compiling. Skipped, with an explicit note, when no
#    checkout-built Effigy binary is available; the milestone planner owns the
#    admitted host-run proof.
probe_binary=""
for candidate in \
  "$repo_root/target/worktree-hooks/debug/effigy" \
  "$repo_root/target/debug/effigy" \
  "$repo_root/target/bootstrap-local/debug/effigy"; do
  if [ -x "$candidate" ]; then
    probe_binary=$candidate
    break
  fi
done
if [ -z "$probe_binary" ]; then
  printf 'deferred: no checkout-built effigy binary; admission-before-effects probe needs a milestone host-run\n'
else
  probe_root=$(mktemp -d "$tmp_root/probe.XXXXXX")
  probe_record="$tmp_root/probe-cargo.args"
  probe_log="$tmp_root/admission-probe.log"
  rm -f "$probe_record"
  set +e
  (
    cd "$repo_root"
    unset EFFIGY_SCHEDULER_OVERRIDE HOST_RUN_TOKEN HOST_RUN_ID
    export EFFIGY_HOST_SCHEDULER=1 EFFIGY_HOST_RUN_ROOT="$probe_root"
    EFFIGY_TEST_CARGO_ARGS="$probe_record" PATH="$stub_bin:$PATH" \
      "$probe_binary" release:prepare --yes --version 0.14.0
  ) >"$probe_log" 2>&1
  probe_code=$?
  set -e
  [ "$probe_code" -eq 75 ] ||
    fail "admission probe exited $probe_code, expected 75; log: $(cat "$probe_log")"
  grep -Fq 'scheduler_unreachable' "$probe_log" ||
    fail 'admission probe did not fail closed at the scheduler'
  [ ! -e "$probe_record" ] ||
    fail 'admission did not precede the builder: cargo was invoked'
  printf 'ok: heavy admission precedes builder and prepare effects (exit 75, no build)\n'
fi

printf 'ok: release:prepare heavy admission and argv safety\n'
