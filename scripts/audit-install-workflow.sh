#!/usr/bin/env bash
# The install recipe must build from Cargo.lock, as CI does (L156, FI-65).

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

recipe="$({
  awk '
    /^install:$/ { inside = 1; next }
    inside && /^[[:alnum:]_-]+:/ { exit }
    inside { print }
  ' justfile
})"

if ! printf '%s\n' "${recipe}" | grep -q -E '^[[:space:]]+cargo install --path \. --locked$'; then
  printf '%s\n' 'install recipe must run `cargo install --path . --locked`' >&2
  exit 1
fi

printf '%s\n' 'install workflow policy passed'
