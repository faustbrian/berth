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

cat >"$TMP_DIR/.berth.toml" <<EOF_CONF
schema_version = 1
container_engine = "$ENGINE"
container_prefix = "parity"

[[service]]
preset = "redis"
name = "cache"
EOF_CONF

run_berth() {
  cargo run --release --manifest-path "$ROOT/Cargo.toml" -- \
    --project-root "$TMP_DIR" \
    "$@"
}

step() {
  echo "==> [$ENGINE] $*"
  run_berth "$@"
}

# Runtime and config resolution checks
step about >/dev/null
step config >/dev/null
step ls >/dev/null

# Lifecycle bootstrap
step setup --service cache
step up --service cache --wait --wait-timeout 30 --no-deps

# Running-container operations
step ps
step health --service cache
step url --service cache
step inspect --service cache --format '{{.Name}}' >/dev/null
step port --service cache >/dev/null
step logs --service cache --tail 5
step exec --service cache -- redis-cli ping
step top --service cache aux >/dev/null
step stats --service cache --no-stream
step pull --service cache

# Copy operations (host -> container -> host)
echo "parity-copy" > "$TMP_DIR/host-copy.txt"
step cp "$TMP_DIR/host-copy.txt" cache:/tmp/host-copy.txt
step cp cache:/tmp/host-copy.txt "$TMP_DIR/container-copy.txt"
if [[ "$(cat "$TMP_DIR/container-copy.txt")" != "parity-copy" ]]; then
  echo "copy parity validation failed" >&2
  exit 1
fi

# Runtime control operations
step pause --service cache
step unpause --service cache
step restart --service cache --wait --wait-timeout 30
step relabel --service cache --wait --wait-timeout 30

# Commands that are interactive/streaming by nature (validate command path)
step attach --service cache --dry-run
step events --service cache --dry-run

# Kill/wait/start flows
step kill --service cache
step wait --service cache --condition not-running --dry-run
step start --service cache --no-open --no-deps --no-wait

# Recreate/update/apply paths
step update --service cache --wait --wait-timeout 30
step recreate --service cache --wait --wait-timeout 30
step apply --no-deps

# Shutdown and cleanup paths
step stop --service cache
step rm --service cache
step up --service cache --wait --wait-timeout 30 --no-deps
step down --service cache --no-deps
step prune --service cache
step prune --all --force --dry-run

echo "full runtime parity for '$ENGINE' passed"
