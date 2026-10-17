#!/usr/bin/env bash
# helm v6 baseline for docs/v9/benchmarks.md: N agents bring up the same
# Laravel fixture (Postgres, Redis, app) in parallel worktrees the way agents
# used v6, one `--env` namespace each, and the script records time-to-ready
# (p50/p95 of `helm up` until it returns) and peak memory of the run's
# containers.
#
# usage: scripts/benchmark-v6-baseline.sh <helm-6-binary> <new-output-directory> [N]
#
# Isolation: a private HOME (helm state), a container prefix unique to this run
# (every container and volume name starts with it), and `localhost_tls` apps so
# helm never configures the host Caddy or /etc/hosts. Only objects carrying
# both helm's label and that prefix are ever removed.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if (( $# < 2 || $# > 3 )); then
  printf 'usage: %s <helm-6-binary> <new-output-directory> [N]\n' "$0" >&2
  exit 64
fi
HELM="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
readonly HELM
[[ -x "$HELM" ]] || { printf 'helm binary is not executable: %s\n' "$1" >&2; exit 66; }
readonly OUTPUT_INPUT="$2"
readonly AGENTS="${3:-30}"
[[ "$AGENTS" =~ ^[1-9][0-9]*$ ]] || { printf 'N must be a positive integer\n' >&2; exit 64; }
[[ ! -e "$OUTPUT_INPUT" ]] || { printf 'output already exists: %s\n' "$OUTPUT_INPUT" >&2; exit 73; }
for tool in docker git jq perl; do
  command -v "$tool" >/dev/null || { printf 'required tool is unavailable: %s\n' "$tool" >&2; exit 69; }
done

readonly LARAVEL_COMMIT="e196bfdfc96903f2e10219749fcbca7c0aefe99f"
readonly COMPOSER_IMAGE="composer@sha256:5946476338742b200bb9ff88f8be56275ddae4b3949c72305cb0dbf10cfcb760"
FIXTURE_FILES="$(pwd)/scripts/fixtures/accept-v9"
readonly FIXTURE_FILES

mkdir -p "$OUTPUT_INPUT"
OUTPUT="$(cd "$OUTPUT_INPUT" && pwd)"
readonly OUTPUT
ROOT="$(mktemp -d "${TMPDIR:-/tmp}/helm-bench.XXXXXX")"
readonly ROOT
readonly FIXTURE="$ROOT/fixture"
readonly WORKTREES="$ROOT/worktrees"
PREFIX="hb$(od -An -N3 -tx1 /dev/urandom | tr -d ' \n')"
readonly PREFIX
export DOCKER_CONFIG="${DOCKER_CONFIG:-$HOME/.docker}"
export HOME="$ROOT/home"
mkdir -p "$HOME"
background_pids=''

now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$OUTPUT/run.log" >&2; }
fail() { log "FAIL: $*"; exit 1; }

# Only this run's objects: helm's label and the run's unique name prefix.
own_containers() {
  docker ps -a --filter label=com.helm.managed=true --format '{{.ID}} {{.Names}}' \
    | awk -v p="$PREFIX" 'index($2, p) == 1 { print $1 }'
}
own_volumes() {
  docker volume ls --format '{{.Name}}' | awk -v p="$PREFIX" 'index($1, p) == 1'
}
kill_tree() {
  local pid="$1" child
  for child in $(pgrep -P "$pid" 2>/dev/null); do kill_tree "$child"; done
  kill -KILL "$pid" 2>/dev/null || true
}
cleanup() {
  local status=$? pid
  set +e
  for pid in $background_pids; do kill_tree "$pid"; done
  for pid in $background_pids; do wait "$pid" 2>/dev/null; done
  own_containers | xargs -r docker rm -f -v >/dev/null 2>&1
  own_volumes | xargs -r docker volume rm >/dev/null 2>&1
  printf 'cleanup_leftover_objects=%s\n' "$(( $(own_containers | wc -l) + $(own_volumes | wc -l) ))" \
    >> "$OUTPUT/metadata.txt"
  if [[ -d "$FIXTURE/.git" ]]; then
    git -C "$FIXTURE" worktree list --porcelain 2>/dev/null | awk '/^worktree /{print $2}' | while read -r tree; do
      [[ "$tree" == "$FIXTURE" ]] || git -C "$FIXTURE" worktree remove --force "$tree" >/dev/null 2>&1
    done
  fi
  rm -rf "$ROOT"
  exit "$status"
}
trap cleanup EXIT

{
  printf 'agents=%s\nhelm=%s\nprefix=%s\n' "$AGENTS" "$("$HELM" --version)" "$PREFIX"
  printf 'docker=%s\n' "$(docker version --format '{{.Server.Version}}')"
  printf 'docker_cpus=%s\ndocker_memory_bytes=%s\n' \
    "$(docker info --format '{{.NCPU}}')" "$(docker info --format '{{.MemTotal}}')"
  printf 'laravel_skeleton_commit=%s\n' "$LARAVEL_COMMIT"
} > "$OUTPUT/metadata.txt"

log "preparing the Laravel fixture"
mkdir -p "$FIXTURE"
git -C "$FIXTURE" init --quiet
git -C "$FIXTURE" remote add origin https://github.com/laravel/laravel.git
git -C "$FIXTURE" fetch --quiet --depth 1 origin "$LARAVEL_COMMIT"
git -C "$FIXTURE" checkout --quiet --detach FETCH_HEAD
git -C "$FIXTURE" checkout --quiet -b bench
cp "$FIXTURE_FILES/composer.json" "$FIXTURE_FILES/composer.lock" "$FIXTURE/"
docker run --rm --user "$(id -u):$(id -g)" --env COMPOSER_HOME=/tmp/composer \
  --volume "$FIXTURE:/app" --workdir /app "$COMPOSER_IMAGE" \
  install --no-interaction --prefer-dist > "$OUTPUT/fixture.txt" 2>&1 \
  || fail "composer could not install the fixture (see fixture.txt)"
cat > "$FIXTURE/.helm.toml" <<TOML
schema_version = 1
project_type = "project"
container_engine = "docker"
container_prefix = "$PREFIX"

[[service]]
preset = "postgres"
name = "db"

[[service]]
preset = "redis"
name = "cache"

[[service]]
preset = "laravel"
name = "app"
localhost_tls = true
container_port = 443
depends_on = ["db", "cache"]
TOML
printf '/vendor\n' >> "$FIXTURE/.gitignore"
git -C "$FIXTURE" add -A
git -C "$FIXTURE" -c user.name=bench -c user.email=bench@invalid commit --quiet \
  -m "test: add the helm v6 benchmark fixture" -m "Impact: none"

log "creating $AGENTS worktrees"
mkdir -p "$WORKTREES"
for index in $(seq 1 "$AGENTS"); do
  git -C "$FIXTURE" worktree add --quiet -b "agent-$index" "$WORKTREES/agent-$index" bench
  cp "$WORKTREES/agent-$index/.env.example" "$WORKTREES/agent-$index/.env"
  if [[ "$(uname -s)" == Darwin ]]; then
    cp -cR "$FIXTURE/vendor" "$WORKTREES/agent-$index/vendor"
  else
    cp -a --reflink=auto "$FIXTURE/vendor" "$WORKTREES/agent-$index/vendor"
  fi
done

memory_bytes() {
  local ids usage
  ids="$(own_containers)" || return 0
  [[ -n "$ids" ]] || return 0
  usage="$(docker stats --no-stream --format '{{.MemUsage}}' $ids 2>/dev/null)" || return 0
  printf '%s\n' "$usage" | awk '{
      value=$1; unit=value; sub(/[0-9.]+/, "", unit); sub(/[A-Za-z]+/, "", value);
      factor=1;
      if (unit=="KiB") factor=1024; else if (unit=="MiB") factor=1048576; else if (unit=="GiB") factor=1073741824;
      else if (unit=="kB") factor=1000; else if (unit=="MB") factor=1000000; else if (unit=="GB") factor=1000000000;
      total += value * factor
    } END { printf "%d\n", total }'
}
sample_memory() {
  set +e
  : > "$OUTPUT/memory-samples.txt"
  while true; do memory_bytes >> "$OUTPUT/memory-samples.txt"; sleep 2; done
}

agent() {
  set +e
  local name="$1" out="$OUTPUT/agents/$1" start end
  mkdir -p "$out"
  cd "$WORKTREES/$name" || return
  start=$(now_ms)
  "$HELM" --env "$name" up --wait --wait-timeout 300 > "$out/up.out" 2>&1
  printf '%s\n' "$?" > "$out/up.status"
  end=$(now_ms)
  printf '%s\n' "$(( end - start ))" > "$out/up.ms"
}

sample_memory &
background_pids="$!"
log "running $AGENTS agents in parallel"
started=$(date +%s)
pids=()
for index in $(seq 1 "$AGENTS"); do
  ( agent "agent-$index" ) &
  pids+=("$!")
  background_pids="$background_pids $!"
done
for pid in "${pids[@]}"; do wait "$pid" || true; done
printf 'parallel_run_seconds=%s\n' "$(( $(date +%s) - started ))" >> "$OUTPUT/metadata.txt"
# Peak while everything is up, then tear down.
sleep 6
for index in $(seq 1 "$AGENTS"); do
  # An env that never came up may fail its `down`; cleanup sweeps the rest.
  ( cd "$WORKTREES/agent-$index" && "$HELM" --env "agent-$index" down > "$OUTPUT/agents/agent-$index/down.out" 2>&1 ) || true
done

ok="$(cat "$OUTPUT"/agents/*/up.status | grep -c '^0$' || true)"
# Time-to-ready counts only agents whose `up` succeeded.
for status in "$OUTPUT"/agents/*/up.status; do
  [[ "$(cat "$status")" == 0 ]] && cat "$(dirname "$status")/up.ms"
done | sort -n > "$OUTPUT/ready-ms.txt"
percentile() {
  awk -v p="$1" '{ v[NR] = $1 } END { i = int((p / 100) * NR + 0.999999); if (i < 1) i = 1; print v[i] }' "$OUTPUT/ready-ms.txt"
}
{
  printf 'succeeded=%s\n' "$ok"
  printf 'time_to_ready_p50_ms=%s\n' "$(percentile 50)"
  printf 'time_to_ready_p95_ms=%s\n' "$(percentile 95)"
  printf 'peak_memory_bytes=%s\n' "$(sort -n "$OUTPUT/memory-samples.txt" | tail -n 1)"
  printf 'memory_samples=%s\n' "$(wc -l < "$OUTPUT/memory-samples.txt" | tr -d ' ')"
} | tee "$OUTPUT/results.txt"
