#!/usr/bin/env bash
# Verifies that docs/v9/ledger.md accounts for every 8.x source directory and
# every commit between the 7.0.0 base and the 8.0.0 tag.
#
# Directories must be named in the ledger's row tables (above the
# "## Commit coverage" heading). Commits may be referenced anywhere in the
# ledger by their abbreviated hash. Every row must carry exactly one decision.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
ledger="${repo_root}/docs/v9/ledger.md"
base_ref="${BERTH_LEDGER_BASE_REF:-7.0.0}"
v8_ref="${BERTH_LEDGER_V8_REF:-8.0.0}"

fail() {
    printf 'check-v9-ledger: %s\n' "$1" >&2
    exit 1
}

[[ -f "${ledger}" ]] || fail "missing ${ledger}"
git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null ||
    fail "base ref '${base_ref}' is unavailable; fetch tags first"
git rev-parse --verify --quiet "${v8_ref}^{commit}" >/dev/null ||
    fail "8.x ref '${v8_ref}' is unavailable; fetch tags first"

rows_section="$(awk '/^## Commit coverage/ { exit } { print }' "${ledger}")"
[[ -n "${rows_section}" ]] || fail "ledger has no row section"

missing=0

while IFS= read -r directory; do
    if ! grep -Fq -- "\`${directory}\`" <<<"${rows_section}"; then
        printf 'unreferenced 8.x directory: %s\n' "${directory}" >&2
        missing=$((missing + 1))
    fi
done < <(git ls-tree -r -d --name-only "${v8_ref}" -- src)

while IFS= read -r commit; do
    if ! grep -Eq -- "(^|[^0-9a-f])${commit}([^0-9a-f]|$)" "${ledger}"; then
        printf 'unreferenced 8.x commit: %s\n' "${commit}" >&2
        missing=$((missing + 1))
    fi
done < <(git log --format='%h' --abbrev=7 "${base_ref}..${v8_ref}")

# Every table row with an ID must have exactly one recognized decision cell.
bad_rows="$(grep -E '^\| L[0-9]{3} ' "${ledger}" |
    awk -F'|' '{
        count = 0
        for (i = 2; i < NF; i++) {
            cell = $i
            gsub(/^[ \t]+|[ \t]+$/, "", cell)
            if (cell ~ /^(Port|Reimplement|Drop|Defer)$/) count++
        }
        if (count != 1) print $2
    }')"
if [[ -n "${bad_rows}" ]]; then
    printf 'rows without exactly one decision:%s\n' "$(tr '\n' ' ' <<<" ${bad_rows}")" >&2
    missing=$((missing + 1))
fi

# With --final, the last cell (status) of every row must be terminal.
if [[ "${1:-}" == "--final" ]]; then
    open_rows="$(grep -E '^\| L[0-9]{3} ' "${ledger}" |
        awk -F'|' '{
            status = $(NF - 1)
            gsub(/^[ \t]+|[ \t]+$/, "", status)
            if (status !~ /^(Done|Dropped|Deferred)$/) print $2
        }')"
    if [[ -n "${open_rows}" ]]; then
        printf 'rows without a terminal status:%s\n' "$(tr '\n' ' ' <<<" ${open_rows}")" >&2
        missing=$((missing + 1))
    fi
fi

row_count="$(grep -cE '^\| L[0-9]{3} ' "${ledger}" || true)"
[[ "${row_count}" -gt 0 ]] || fail "ledger contains no L### rows"

if [[ "${missing}" -gt 0 ]]; then
    fail "${missing} coverage problem(s)"
fi

printf 'check-v9-ledger: %s rows cover all 8.x directories and %s commits\n' \
    "${row_count}" "$(git rev-list --count "${base_ref}..${v8_ref}")"
