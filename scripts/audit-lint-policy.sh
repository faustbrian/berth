#!/usr/bin/env bash

set -euo pipefail

ROOT_DIRECTORY=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT_DIRECTORY"

failures=0

if ! grep -qE '^unused = \{ level = "deny", priority = -1 \}$' Cargo.toml; then
  printf '%s\n' 'Cargo.toml must deny unused production and test code' >&2
  failures=1
fi

if ! grep -qE '^all = \{ level = "deny", priority = -1 \}$' Cargo.toml; then
  printf '%s\n' 'Cargo.toml must deny the clippy::all group' >&2
  failures=1
fi

report_matches() {
  local description=$1
  local pattern=$2
  shift 2

  local path
  local matches
  local grep_status
  for path in "$@"; do
    set +e
    matches="$(grep -R -nE "$pattern" "$path")"
    grep_status=$?
    set -e
    if (( grep_status == 0 )); then
      printf '%s\n%s\n' "$description" "$matches" >&2
      failures=1
    elif (( grep_status > 1 )); then
      printf 'lint policy audit could not scan: %s\n' "$path" >&2
      failures=1
    fi
  done
}

# Crate-level attributes may span lines and hide in `expect` or `cfg_attr`;
# scan each file whole. `cfg_attr(test, …)` keeps test-only allows legal.
report_crate_attributes() {
  local description=$1
  local lints=$2
  local matches
  if ! matches="$(find src -name '*.rs' -print0 | xargs -0 perl -0777 -ne '
      while (/#!\[\s*(?:cfg_attr\s*\((?!\s*test\s*,)[^]]*?)?(?:allow|expect)\s*\(([^]]*?)\)\s*\)?\s*\]/sg) {
        my ($block, $at, $attribute) = ($1, $-[0], $&);
        $attribute =~ s/\s+/ /g;
        if ($block =~ /'"${lints}"'/) {
          my $line = 1 + (() = substr($_, 0, $at) =~ /\n/g);
          print "$ARGV:$line: $attribute\n";
        }
      }')"; then
    printf 'lint policy audit could not scan: src\n' >&2
    failures=1
    return
  fi
  if [[ -n "$matches" ]]; then
    printf '%s\n%s\n' "$description" "$matches" >&2
    failures=1
  fi
}

report_crate_attributes \
  'crate-wide compiler warning suppression is forbidden:' \
  '\b(dead_code|unused_imports|unused|warnings)\b'

report_crate_attributes \
  'broad Clippy group suppression is forbidden:' \
  'clippy::(all|pedantic|nursery|cargo|correctness|suspicious|style|complexity|perf)\b'

report_matches \
  'build and run recipes must not disable compiler warnings:' \
  'RUSTFLAGS=.*-A ?warnings|RUSTFLAGS=.*--allow[= ]warnings' \
  justfile scripts .github

if (( failures != 0 )); then
  exit 1
fi

printf '%s\n' 'lint policy audit passed'
