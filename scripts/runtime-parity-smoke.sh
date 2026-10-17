#!/usr/bin/env bash
set -euo pipefail

ENGINE="${1:-docker}"
if [[ "$ENGINE" != "docker" && "$ENGINE" != "podman" ]]; then
  echo "usage: $0 [docker|podman]" >&2
  exit 2
fi

if ! command -v "$ENGINE" >/dev/null 2>&1; then
  echo "runtime '$ENGINE' is not installed or not on PATH" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
# A private berth home: its own installation ID labels everything this run
# creates, so cleanup removes exactly that, even after a failure.
export BERTH_HOME="$TMP_DIR/berth-home"
cleanup() {
  local id
  id="$(cat "$BERTH_HOME/installation-id" 2>/dev/null || true)"
  if [[ -n "$id" ]]; then
    "$ENGINE" ps -aq --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" rm -f -v >/dev/null 2>&1 || true
    "$ENGINE" network ls -q --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" network rm >/dev/null 2>&1 || true
    "$ENGINE" volume ls -q --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" volume rm >/dev/null 2>&1 || true
  fi
  if [[ -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

# A dedicated container: the container commands below need one (a shared
# Redis tenant has none).
cat >"$TMP_DIR/.berth.yaml" <<EOF
schema_version: 9
project: parity
project_type: project
container_engine: $ENGINE
services:
  cache:
    preset: redis
    isolation: dedicated
EOF

run_berth() {
  cargo run --release --manifest-path "$ROOT/Cargo.toml" -- \
    --project-root "$TMP_DIR" \
    "$@"
}

echo "==> [$ENGINE] config load"
run_berth about >/dev/null

echo "==> [$ENGINE] up"
run_berth up --service cache --wait --wait-timeout 30 --no-deps

echo "==> [$ENGINE] status"
run_berth ps

echo "==> [$ENGINE] inspect"
run_berth inspect --service cache --format '{{.Name}}'

echo "==> [$ENGINE] port"
run_berth port --service cache

echo "==> [$ENGINE] logs"
run_berth logs --service cache --tail 5

echo "==> [$ENGINE] exec"
run_berth exec --service cache -- redis-cli ping

echo "==> [$ENGINE] top"
run_berth top --service cache aux

echo "==> [$ENGINE] down"
run_berth down --service cache --no-deps

echo "runtime parity smoke for '$ENGINE' passed"
