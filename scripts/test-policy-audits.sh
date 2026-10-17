#!/usr/bin/env bash
# Proves each policy audit can fail (L155): every audit runs on a passing
# input and on a planted violation, under a minimal PATH (FI-26).

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

ROOT="$(mktemp -d)"
trap 'rm -rf "${ROOT}"' EXIT INT TERM

expect_failure() {
  local description="$1"
  shift
  if PATH=/usr/bin:/bin "$@" >/dev/null 2>&1; then
    printf '%s\n' "policy audit accepted ${description}" >&2
    exit 1
  fi
}

mkdir -p "${ROOT}/valid" "${ROOT}/mutable" "${ROOT}/empty"
cat >"${ROOT}/valid/ci.yml" <<'YAML'
steps:
  - name: Checkout
    uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd
  - name: Local action
    uses: ./local-action
YAML
cat >"${ROOT}/mutable/ci.yml" <<'YAML'
steps:
  - name: Checkout
    uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd
  - uses: actions/cache@v4
YAML
PATH=/usr/bin:/bin ./scripts/audit-workflow-actions.sh "${ROOT}/valid"
PATH=/usr/bin:/bin ./scripts/audit-workflow-actions.sh
expect_failure 'a mutable action reference' ./scripts/audit-workflow-actions.sh "${ROOT}/mutable"
expect_failure 'an empty workflow scan' ./scripts/audit-workflow-actions.sh "${ROOT}/empty"

PATH=/usr/bin:/bin ./scripts/audit-lint-policy.sh
PATH=/usr/bin:/bin ./scripts/audit-install-workflow.sh

# Planted violations run against a copy of the repository's policy inputs.
copy="${ROOT}/repository"
mkdir -p "${copy}/scripts" "${copy}/src" "${copy}/.github"
cp scripts/audit-lint-policy.sh scripts/audit-install-workflow.sh "${copy}/scripts/"
cp Cargo.toml justfile "${copy}/"
cp -R src/. "${copy}/src/"
cp -R .github/. "${copy}/.github/"
# The unmodified copy passes, so each failure below is the planted one.
PATH=/usr/bin:/bin "${copy}/scripts/audit-lint-policy.sh" >/dev/null
PATH=/usr/bin:/bin "${copy}/scripts/audit-install-workflow.sh" >/dev/null

for planted in '#![allow(clippy::pedantic)]' \
  $'#![allow(\n    clippy::mod_module_files,\n    clippy::all,\n)]' \
  '#![expect(clippy::nursery)]' \
  '#![cfg_attr(not(test), allow(clippy::pedantic))]' \
  '#![allow(dead_code)]'; do
  printf '%s\n' "$planted" >> "${copy}/src/main.rs"
  expect_failure "the crate attribute ${planted}" "${copy}/scripts/audit-lint-policy.sh"
  cp src/main.rs "${copy}/src/main.rs"
done

# Built in two parts so this script itself never matches the audit.
planted='RUSTFLAGS="-A'
planted+='warnings"'
sed -i.bak "s/^    cargo build --release\$/    ${planted} cargo build --release/" "${copy}/justfile"
expect_failure 'a recipe that disables warnings' "${copy}/scripts/audit-lint-policy.sh"
cp justfile "${copy}/justfile"

sed -i.bak 's/^    cargo install --path \. --locked$/    cargo install --path ./' "${copy}/justfile"
expect_failure 'an unlocked install recipe' "${copy}/scripts/audit-install-workflow.sh"

printf '%s\n' 'policy audit regressions passed'
