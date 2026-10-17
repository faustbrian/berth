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
cleanup() {
  if [[ -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

cat >"$TMP_DIR/.berth.toml" <<EOF
schema_version = 1
container_engine = "$ENGINE"
container_prefix = "parity"
swarm = []

[[service]]
preset = "redis"
name = "cache"
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
