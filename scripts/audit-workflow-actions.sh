#!/usr/bin/env bash

set -euo pipefail

readonly workflow_root="${1:-.github/workflows}"

if [[ ! -d "${workflow_root}" ]]; then
  printf 'workflow action audit root is not a directory: %s\n' \
    "${workflow_root}" >&2
  exit 1
fi

failed=0
declarations=0
set +e
matches="$(
  grep -R -nE \
    --include='*.yml' \
    --include='*.yaml' \
    '^[[:space:]]*(-[[:space:]]+)?uses:' \
    "${workflow_root}"
)"
grep_status=$?
set -e
if (( grep_status > 1 )); then
  printf 'workflow action audit could not scan: %s\n' "${workflow_root}" >&2
  exit 1
fi

if [[ -n "${matches}" ]]; then
  while IFS=: read -r file line_number declaration; do
    declarations=$((declarations + 1))
    reference="${declaration#*uses:}"
    reference="${reference%%#*}"
    reference="${reference#"${reference%%[![:space:]]*}"}"

    if [[ "${reference}" == ./* ]]; then
      continue
    fi

    revision="${reference##*@}"
    if [[ ! "${revision}" =~ ^[0-9a-f]{40}$ ]]; then
      printf '%s:%s: workflow action must use an immutable 40-character commit: %s\n' \
        "${file}" "${line_number}" "${reference}" >&2
      failed=1
    fi
  done <<<"${matches}"
fi

if (( declarations == 0 )); then
  printf 'workflow action audit found no action declarations under: %s\n' \
    "${workflow_root}" >&2
  exit 1
fi

if (( failed != 0 )); then
  exit 1
fi

printf 'workflow action audit passed\n'
