#!/usr/bin/env bash
# Sourced by the two admitted fixture runners; only recorded child PIDs are killed.
run_test() {
  local test_name=$1
  local status=0
  local deadline

  local listing
  listing=$("$runner" "$test_name" --exact --include-ignored --list)
  if [[ "$listing" != "$test_name: test"* ]]; then
    printf 'required fixture test is absent: %s\n' "$test_name" >&2
    return 2
  fi

  "$runner" "$test_name" --exact --include-ignored --nocapture --test-threads=1 &
  local test_pid=$!
  deadline=$((SECONDS + 180))
  while kill -0 "$test_pid" 2>/dev/null; do
    if (( SECONDS >= deadline )); then
      printf 'fixture test timed out; terminating owned PID %s\n' "$test_pid" >&2
      kill -TERM "$test_pid" 2>/dev/null || true
      for _ in {1..20}; do
        kill -0 "$test_pid" 2>/dev/null || break
        sleep 0.1
      done
      kill -KILL "$test_pid" 2>/dev/null || true
      wait "$test_pid" || true
      return 124
    fi
    sleep 0.1
  done
  wait "$test_pid" || status=$?
  return "$status"
}

