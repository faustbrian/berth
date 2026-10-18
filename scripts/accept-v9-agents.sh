#!/usr/bin/env bash
# Agent acceptance for berth 9.x against a real engine (BERTH.md
# "Verification"): Docker by default, Podman with BERTH_ENGINE=podman.
#
# Creates N parallel git worktrees of a pinned public Laravel fixture. Every
# worktree, in parallel, runs what an agent runs: `berth up --wait --json`,
# an HTTPS request to its route, a focused `artisan test`, a ParaTest
# `--parallel` run and `down --purge`. The script then asserts:
#   - every step succeeded;
#   - no instance saw another instance's data;
#   - exit codes are preserved (a deliberately failing test exits 1);
#   - a SIGKILLed agent is reaped by a later berth invocation;
#   - no berth-labeled container, volume or network remains apart from shared
#     servers, which `berth gc` then removes.
# It records time-to-ready (p50/p95) and peak memory in the output directory.
#
# usage: scripts/accept-v9-agents.sh <berth-binary> <new-output-directory> [N]
#
# Everything runs under a private berth home (its own installation ID, so
# every object it creates is labeled and nothing else is touched) and its own
# gateway ports. All of it is removed on exit, success or failure.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if (( $# < 2 || $# > 3 )); then
  printf 'usage: %s <berth-binary> <new-output-directory> [N]\n' "$0" >&2
  exit 64
fi

BERTH_INPUT="$1"
if [[ ! -x "$BERTH_INPUT" ]]; then
  printf 'berth binary is not executable: %s\n' "$BERTH_INPUT" >&2
  exit 66
fi
BERTH="$(cd "$(dirname "$BERTH_INPUT")" && pwd)/$(basename "$BERTH_INPUT")"
readonly BERTH
readonly OUTPUT_INPUT="$2"
readonly AGENTS="${3:-${BERTH_ACCEPT_AGENTS:-30}}"
# The engine berth drives and this script inspects with.
ENGINE="${BERTH_ENGINE:-docker}"
case "$ENGINE" in
  docker|podman) ;;
  *) printf 'BERTH_ENGINE must be docker or podman, got %s\n' "$ENGINE" >&2; exit 64 ;;
esac
export BERTH_ENGINE="$ENGINE"
readonly ENGINE
# Rootless Docker maps the daemon's root to the invoking user, so berth sets
# no --user there; rootful Docker and Podman (keep-id) run apps as UID:GID.
if [[ "$ENGINE" == docker ]] && docker info --format '{{json .SecurityOptions}}' | grep -q rootless; then
  APP_USER=""
else
  APP_USER="$(id -u):$(id -g)"
fi
readonly APP_USER
# How this script runs its own one-off container as the invoking user.
if [[ -z "$APP_USER" ]]; then
  RUN_AS=()
elif [[ "$ENGINE" == podman ]]; then
  RUN_AS=(--userns=keep-id --user "$APP_USER")
else
  RUN_AS=(--user "$APP_USER")
fi
readonly RUN_AS
if ! [[ "$AGENTS" =~ ^[1-9][0-9]*$ ]]; then
  printf 'N must be a positive integer, got %s\n' "$AGENTS" >&2
  exit 64
fi

# Pinned inputs: the laravel/laravel skeleton commit of the 8.x acceptance
# fixture, plus a committed composer.json/composer.lock (the skeleton ships no
# lock) that add ParaTest and fix every dependency version.
readonly LARAVEL_COMMIT="e196bfdfc96903f2e10219749fcbca7c0aefe99f"
readonly COMPOSER_IMAGE="composer@sha256:5946476338742b200bb9ff88f8be56275ddae4b3949c72305cb0dbf10cfcb760"
FIXTURE_FILES="$(pwd)/scripts/fixtures/accept-v9"
readonly FIXTURE_FILES

for tool in curl "$ENGINE" git jq perl; do
  command -v "$tool" >/dev/null || { printf 'required tool is unavailable: %s\n' "$tool" >&2; exit 69; }
done
if [[ -e "$OUTPUT_INPUT" ]]; then
  printf 'output already exists: %s\n' "$OUTPUT_INPUT" >&2
  exit 73
fi
mkdir -p "$OUTPUT_INPUT"
OUTPUT="$(cd "$OUTPUT_INPUT" && pwd)"
readonly OUTPUT

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/berth-accept.XXXXXX")"
readonly ROOT
readonly FIXTURE="$ROOT/fixture"
readonly WORKTREES="$ROOT/worktrees"
export BERTH_HOME="$ROOT/berth-home"
export BERTH_GATEWAY_HTTPS_PORT="${BERTH_ACCEPT_HTTPS_PORT:-28443}"
export BERTH_GATEWAY_HTTP_PORT="${BERTH_ACCEPT_HTTP_PORT:-28080}"
unset BERTH_INSTANCE BERTH_OWNER_PID

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d ' ' -f 1; else shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}
# Stops a process before its children so none forks a next step or reacts
# to a child's death, then kills them all: an abrupt death of the whole tree.
stop_tree() {
  local pid="$1" child
  kill -STOP "$pid" 2>/dev/null || return 0
  for child in $(pgrep -P "$pid" 2>/dev/null); do
    stop_tree "$child"
  done
}
kill_tree() {
  local pid="$1" child
  for child in $(pgrep -P "$pid" 2>/dev/null); do
    kill_tree "$child"
  done
  kill -KILL "$pid" 2>/dev/null || true
}
abrupt_kill() { stop_tree "$1"; kill_tree "$1"; }
# Stops this shell's still-running background jobs (agents, the sampler) and
# their children. Only live jobs of this shell: a PID already waited for may
# have been reused by an unrelated process.
stop_background() {
  local pid
  for pid in $(jobs -p); do
    abrupt_kill "$pid"
  done
  for pid in $(jobs -p); do
    wait "$pid" 2>/dev/null || true
  done
}

now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$OUTPUT/run.log" >&2; }
fail() { log "FAIL: $*"; exit 1; }

installation() { cat "$BERTH_HOME/installation-id" 2>/dev/null || true; }

# A container's user, effective capabilities (sorted, without `CAP_`) and
# security options, as one line: `USER ["CAP",…] ["OPT",…]`. Docker reports
# `--cap-drop ALL --cap-add X` as given; Podman reports both relative to its
# default set, so its `EffectiveCaps` is the comparable value.
security_of() {
  if [[ "$ENGINE" == podman ]]; then
    podman inspect --format '{{.Config.User}}|{{json .EffectiveCaps}}|{{json .HostConfig.SecurityOpt}}' "$1"
  else
    docker inspect --format '{{.Config.User}}|{{json .HostConfig.CapDrop}}|{{json .HostConfig.CapAdd}}|{{json .HostConfig.SecurityOpt}}' "$1" |
      awk -F'|' '{ if ($2 ~ /"(CAP_)?ALL"/) print $1 "|" $3 "|" $4; else print $1 "|dropped " $2 "|" $4 }'
  fi 2>&1 | awk -F'|' '{print $1; print $2; print $3}' |
    { read -r user; read -r caps; read -r options
      printf '%s %s %s' "$user" "$(jq -c 'map(sub("^CAP_"; "")) | sort' <<< "$caps" 2>/dev/null || printf '%s' "$caps")" "$options"; }
}

# Every Docker object this run created carries its installation label.
# Fails (rather than printing nothing) when the ID or Docker is unavailable,
# so a leftover check can never pass vacuously.
labeled_objects() {
  local id containers volumes networks
  id="$(installation)"
  [[ -n "$id" ]] || { printf 'no installation ID under %s\n' "$BERTH_HOME" >&2; return 1; }
  containers="$("$ENGINE" ps -aq --filter "label=dev.berth.installation=$id")" || return 1
  volumes="$("$ENGINE" volume ls -q --filter "label=dev.berth.installation=$id")" || return 1
  networks="$("$ENGINE" network ls -q --filter "label=dev.berth.installation=$id")" || return 1
  [[ -z "$containers" ]] || printf 'container %s\n' $containers
  [[ -z "$volumes" ]] || printf 'volume %s\n' $volumes
  [[ -z "$networks" ]] || printf 'network %s\n' $networks
}
# The `--format` term for an object's dev.berth.role label in `ps` and
# `volume ls`: Podman 4.9's listings have no `.Label` function. Networks are
# inspected instead, where `.Labels` is a map on both engines.
role_template() {
  if [[ "$ENGINE" == podman ]]; then
    printf '%s' '{{index .Labels "dev.berth.role"}}'
  else
    printf '%s' '{{.Label "dev.berth.role"}}'
  fi
}

count_labeled_objects() {
  local objects
  objects="$(labeled_objects)" || { printf 'unknown'; return; }
  [[ -z "$objects" ]] && printf '0' || printf '%s\n' "$objects" | wc -l | tr -d ' '
}

cleanup() {
  local status=$?
  set +e
  stop_background
  local id attempt
  id="$(installation)"
  if [[ -n "$id" ]] && (( status != 0 )); then
    # What a failed run leaves for diagnosis: every labeled container's
    # state and last log lines, and each network's members.
    mkdir -p "$OUTPUT/engine"
    "$ENGINE" ps -a --filter "label=dev.berth.installation=$id" --format '{{.Names}}' |
      while read -r container; do
        "$ENGINE" inspect --format '{{json .State}}' "$container" > "$OUTPUT/engine/$container.state" 2>&1
        "$ENGINE" logs --tail 200 "$container" > "$OUTPUT/engine/$container.log" 2>&1
      done
    "$ENGINE" network ls -q --filter "label=dev.berth.installation=$id" |
      while read -r network; do
        "$ENGINE" network inspect "$network" > "$OUTPUT/engine/network-$network.json" 2>&1
      done
  fi
  if [[ -n "$id" ]]; then
    # Containers first (they hold networks and volumes); repeat until
    # nothing labeled is left or a few passes did not help.
    for attempt in 1 2 3; do
      "$ENGINE" ps -aq --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" rm -f -v >/dev/null 2>&1
      "$ENGINE" network ls -q --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" network rm >/dev/null 2>&1
      "$ENGINE" volume ls -q --filter "label=dev.berth.installation=$id" | xargs -r "$ENGINE" volume rm >/dev/null 2>&1
      [[ "$(count_labeled_objects)" == 0 ]] && break
    done
    printf 'cleanup_leftover_objects=%s\n' "$(count_labeled_objects)" >> "$OUTPUT/metadata.txt"
  fi
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
  printf 'agents=%s\n' "$AGENTS"
  printf 'berth=%s\n' "$("$BERTH" --version)"
  if [[ "$ENGINE" == podman ]]; then
    # A local (non-remote) Podman has no server section.
    printf 'engine=podman %s\n' "$(podman info --format '{{.Version.Version}}')"
    printf 'engine_cpus=%s\n' "$(podman info --format '{{.Host.CPUs}}')"
    printf 'engine_memory_bytes=%s\n' "$(podman info --format '{{.Host.MemTotal}}')"
    printf 'engine_rootless=%s\n' "$(podman info --format '{{.Host.Security.Rootless}}')"
  else
    printf 'engine=docker %s\n' "$(docker version --format '{{.Server.Version}}')"
    printf 'engine_cpus=%s\n' "$(docker info --format '{{.NCPU}}')"
    printf 'engine_memory_bytes=%s\n' "$(docker info --format '{{.MemTotal}}')"
    printf 'engine_rootless=%s\n' "$([[ -z "$APP_USER" ]] && echo true || echo false)"
  fi
  printf 'laravel_skeleton_commit=%s\ncomposer_image=%s\n' "$LARAVEL_COMMIT" "$COMPOSER_IMAGE"
  printf 'laravel_framework=%s\n' "$(jq -r '.packages[] | select(.name == "laravel/framework") | .version' "$FIXTURE_FILES/composer.lock")"
  printf 'composer_lock_sha256=%s\n' "$(sha256 "$FIXTURE_FILES/composer.lock")"
} > "$OUTPUT/metadata.txt"
# Create the private installation now and record its ID, so anything left
# after an interrupted run can still be found by its label.
"$BERTH" ls --json > /dev/null
[[ -n "$(installation)" ]] || fail "berth created no installation ID under $BERTH_HOME"
printf 'installation_id=%s\n' "$(installation)" >> "$OUTPUT/metadata.txt"

# --- Fixture ---------------------------------------------------------------
log "preparing the Laravel fixture"
mkdir -p "$FIXTURE"
git -C "$FIXTURE" init --quiet
git -C "$FIXTURE" remote add origin https://github.com/laravel/laravel.git
git -C "$FIXTURE" fetch --quiet --depth 1 origin "$LARAVEL_COMMIT"
git -C "$FIXTURE" checkout --quiet --detach FETCH_HEAD
git -C "$FIXTURE" checkout --quiet -b acceptance
cp "$FIXTURE_FILES/composer.json" "$FIXTURE_FILES/composer.lock" "$FIXTURE/"
"$ENGINE" run --rm ${RUN_AS[@]+"${RUN_AS[@]}"} --env COMPOSER_HOME=/tmp/composer \
  --volume "$FIXTURE:/app" --workdir /app "$COMPOSER_IMAGE" \
  install --no-interaction --prefer-dist \
  > "$OUTPUT/fixture.txt" 2>&1 || fail "composer could not install the fixture (see fixture.txt)"

cat > "$FIXTURE/.berth.yaml" <<'YAML'
schema_version: 9
project: accept
project_type: project
services:
  db:
    preset: postgres
  cache:
    preset: redis
  app:
    preset: laravel
    env:
      CACHE_STORE: redis
      SESSION_DRIVER: redis
      QUEUE_CONNECTION: sync
YAML

cat > "$FIXTURE/database/migrations/2026_01_01_000000_create_markers_table.php" <<'PHP'
<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('markers', function (Blueprint $table) {
            $table->id();
            $table->string('agent');
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('markers');
    }
};
PHP

mkdir -p "$FIXTURE/tests/Feature"
cat > "$FIXTURE/tests/Feature/IsolationTest.php" <<'PHP'
<?php

namespace Tests\Feature;

use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Tests\TestCase;

class IsolationTest extends TestCase
{
    use RefreshDatabase;

    public function test_each_run_starts_from_its_own_empty_database(): void
    {
        // phpunit.xml defaults to in-memory SQLite and the array cache; the
        // run must be on its own shared-server tenants instead.
        $this->assertSame('pgsql', DB::connection()->getDriverName());
        $this->assertStringStartsWith('accept__', DB::connection()->getDatabaseName());
        // The script checks that no other agent's run used this database.
        file_put_contents(storage_path('app/db-names'), DB::connection()->getDatabaseName().PHP_EOL, FILE_APPEND);
        $this->assertSame(0, DB::table('markers')->count());
        // The script checks that this file belongs to the invoking user.
        file_put_contents(storage_path('app/owner-probe'), 'written by the app container');
        DB::table('markers')->insert(['agent' => 'test-run']);
        $this->assertSame(1, DB::table('markers')->count());
    }

    public function test_the_cache_belongs_to_this_run(): void
    {
        $this->assertSame('redis', config('cache.default'));
        $this->assertStringStartsWith('accept__', (string) getenv('REDIS_PREFIX'));
        $this->assertNull(Cache::get('accept-owner'));
        Cache::put('accept-owner', 'test-run', 60);
        $this->assertSame('test-run', Cache::get('accept-owner'));
    }
}
PHP
for index in 1 2 3 4; do
  cat > "$FIXTURE/tests/Feature/ParallelSlice${index}Test.php" <<PHP
<?php

namespace Tests\Feature;

use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\DB;
use Tests\TestCase;

class ParallelSlice${index}Test extends TestCase
{
    use RefreshDatabase;

    public function test_each_parallel_process_has_its_own_database(): void
    {
        // ParaTest gives each process its own \`{database}_test_{token}\`.
        \$this->assertSame('pgsql', DB::connection()->getDriverName());
        \$this->assertMatchesRegularExpression('/^accept__.*_test_[0-9]+\$/', DB::connection()->getDatabaseName());
        file_put_contents(storage_path('app/db-names'), DB::connection()->getDatabaseName().PHP_EOL, FILE_APPEND);
        \$this->assertSame(0, DB::table('markers')->count());
        DB::table('markers')->insert(['agent' => 'slice-${index}']);
        \$this->assertSame(1, DB::table('markers')->count());
    }
}
PHP
done
# Probes live outside phpunit.xml's suites, so ParaTest never runs them; they
# run only when named by path.
mkdir -p "$FIXTURE/tests/Probe"
cat > "$FIXTURE/tests/Probe/DeliberateFailureTest.php" <<'PHP'
<?php

namespace Tests\Probe;

use Tests\TestCase;

class DeliberateFailureTest extends TestCase
{
    public function test_fails_on_purpose(): void
    {
        $this->assertTrue(false, 'deliberate failure');
    }
}
PHP
cat > "$FIXTURE/tests/Probe/SlowTest.php" <<'PHP'
<?php

namespace Tests\Probe;

use Tests\TestCase;

class SlowTest extends TestCase
{
    public function test_runs_long_enough_to_be_killed(): void
    {
        // The worktree is bind-mounted: the script waits for this file to
        // know the test is running before it kills the agent.
        touch(base_path('storage/slow-test-running'));
        sleep(600);
        $this->assertTrue(true);
    }
}
PHP
# A route that answers with the database it is connected to, so the HTTPS
# probe proves the route reaches this instance's own app.
cat >> "$FIXTURE/routes/web.php" <<'PHP'

Route::get('/berth-instance', fn () => Illuminate\Support\Facades\DB::connection()->getDatabaseName());
PHP
# An artisan command that exits with the code it is given: a distinctive
# code (3) proves exit codes pass through `berth artisan` exactly.
cat >> "$FIXTURE/routes/console.php" <<'PHP'

Artisan::command('berth:exit {code}', fn (string $code) => (int) $code);
PHP
printf '/vendor\n' >> "$FIXTURE/.gitignore"
git -C "$FIXTURE" add -A
# A conventional message: commit-msg hooks the host enforces globally apply
# to this repository too.
git -C "$FIXTURE" -c user.name=accept -c user.email=accept@invalid commit --quiet \
  -m "test: add the berth acceptance fixture" -m "Impact: none"

# --- Worktrees --------------------------------------------------------------
log "creating $AGENTS worktrees"
mkdir -p "$WORKTREES"
make_worktree() {
  local name="$1"
  git -C "$FIXTURE" worktree add --quiet -b "$name" "$WORKTREES/$name" acceptance
  # `.env` is gitignored; a checkout's usual first step. berth injects every
  # connection setting, so the copied defaults never reach the services.
  cp "$WORKTREES/$name/.env.example" "$WORKTREES/$name/.env"
  # Vendor is not committed; a copy-on-write clone where the filesystem has
  # one (APFS `cp -c`, GNU `--reflink=auto`).
  if [[ "$(uname -s)" == Darwin ]]; then
    cp -cR "$FIXTURE/vendor" "$WORKTREES/$name/vendor"
  else
    cp -a --reflink=auto "$FIXTURE/vendor" "$WORKTREES/$name/vendor"
  fi
}
for index in $(seq 1 "$AGENTS"); do
  make_worktree "agent-$index"
done
make_worktree "agent-killed"

# --- Memory sampler ----------------------------------------------------------
# Prints the summed memory of this run's running containers, or nothing when
# none runs or `"$ENGINE" stats` failed (a container removed between listing and
# sampling); without IDs `"$ENGINE" stats` would report every container on the
# host.
memory_bytes() {
  local ids usage
  ids="$("$ENGINE" ps -q --filter "label=dev.berth.installation=$(installation)")" || return 0
  [[ -n "$ids" ]] || return 0
  usage="$("$ENGINE" stats --no-stream --format '{{.MemUsage}}' $ids 2>/dev/null)" || return 0
  printf '%s\n' "$usage" \
    | awk '{
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
  while true; do
    memory_bytes >> "$OUTPUT/memory-samples.txt"
    sleep 2
  done
}

# --- One agent ---------------------------------------------------------------
# Runs every step in its worktree and records each step's exit code and the
# time `up` took. Steps keep going after a failure so the report shows them
# all; the assertions below decide.
agent() {
  set +e
  local name="$1" dir="$WORKTREES/$1" out="$OUTPUT/agents/$1"
  mkdir -p "$out"
  cd "$dir" || { printf 'missing worktree %s\n' "$dir" > "$out/error"; return; }
  local start end status url code
  start=$(now_ms)
  # This agent's shell owns the instance (bash 3.2 has no $BASHPID).
  local owner
  owner="$(exec sh -c 'echo "$PPID"')"
  "$BERTH" up --wait --json --ttl 2h --owner-pid "$owner" > "$out/up.json" 2> "$out/up.err"
  status=$?
  end=$(now_ms)
  printf '%s\n' "$status" > "$out/up.status"
  printf '%s\n' "$(( end - start ))" > "$out/up.ms"

  # The app container as Docker runs it: user and capability sets (L022).
  local app
  app="$(jq -r '.services[] | select(.name == "app") | .container' "$out/up.json" 2>/dev/null)"
  security_of "$app" > "$out/app-security.txt"
  url="$(jq -r '.urls[0] // empty' "$out/up.json" 2>/dev/null)"
  # Verified against the gateway's CA, not `-k`.
  local ca="$BERTH_HOME/gateway/data/caddy/pki/authorities/local/root.crt"
  code="$(curl -s --cacert "$ca" -o "$out/https.body" -w '%{http_code}' --max-time 30 "$url" 2>/dev/null || true)"
  printf '%s\n' "$code" > "$out/https.code"
  curl -s --cacert "$ca" --max-time 30 "$url/berth-instance" > "$out/route.body" 2>/dev/null || true

  "$BERTH" artisan --no-tty --non-interactive migrate --force > "$out/migrate.out" 2>&1
  printf '%s\n' "$?" > "$out/migrate.status"
  "$BERTH" artisan --no-tty --non-interactive tinker \
    --execute="DB::table('markers')->insert(['agent' => '$name']); echo DB::table('markers')->pluck('agent')->join(',');" \
    > "$out/markers.out" 2>&1
  printf '%s\n' "$?" > "$out/markers.status"

  # Tools agents run as the container's (invoking) user: Composer and npm
  # need a writable HOME and caches.
  "$BERTH" composer --no-tty validate --no-check-publish > "$out/composer.out" 2>&1
  printf '%s\n' "$?" > "$out/composer.status"
  # One agent installs the skeleton's frontend dependencies (its npm cache
  # fills HOME); the rest only run npm. `--legacy-peer-deps`: the pinned
  # skeleton's own vite/@tailwindcss/vite peer ranges conflict.
  if [[ "$name" == agent-1 ]]; then
    "$BERTH" node --no-tty --package-manager npm install --no-audit --no-fund --legacy-peer-deps > "$out/npm.out" 2>&1
  else
    "$BERTH" node --no-tty --package-manager npm --version > "$out/npm.out" 2>&1
  fi
  printf '%s\n' "$?" > "$out/npm.status"

  # The tenant's own clients run in throwaway containers on the instance
  # network and reach the shared servers by service name.
  printf 'select current_user;\n' | "$BERTH" exec --no-tty --service db > "$out/psql.out" 2>&1
  printf '%s\n' "$?" > "$out/psql.status"
  printf 'PING\n' | "$BERTH" exec --no-tty --service cache > "$out/redis-cli.out" 2>&1
  printf '%s\n' "$?" > "$out/redis-cli.status"

  "$BERTH" artisan --no-tty --non-interactive test tests/Feature/IsolationTest.php > "$out/test.out" 2>&1
  printf '%s\n' "$?" > "$out/test.status"
  "$BERTH" artisan --no-tty --non-interactive test --parallel --processes=4 \
    > "$out/paratest.out" 2>&1
  printf '%s\n' "$?" > "$out/paratest.status"
  "$BERTH" artisan --no-tty --non-interactive berth:exit 3 > "$out/exit3.out" 2>&1
  printf '%s\n' "$?" > "$out/exit3.status"
  "$BERTH" artisan --no-tty --non-interactive test tests/Probe/DeliberateFailureTest.php \
    > "$out/failing.out" 2>&1
  printf '%s\n' "$?" > "$out/failing.status"

  "$BERTH" down --purge > "$out/down.out" 2>&1
  printf '%s\n' "$?" > "$out/down.status"
}

log "starting the memory sampler"
sample_memory &

log "running $AGENTS agents in parallel"
run_started=$(date +%s)
pids=()
for index in $(seq 1 "$AGENTS"); do
  ( agent "agent-$index" ) &
  pids+=("$!")
done

# The killed agent: its shell owns its instance. Once its slow test is
# running it is SIGKILLed with its children (the other agents are still
# working), and a later berth invocation must reap the instance.
readonly KILLED_MARKER="$WORKTREES/agent-killed/storage/slow-test-running"
(
  cd "$WORKTREES/agent-killed" || exit 3
  exec bash -c '
    "$0" up --wait --json --ttl 2h --owner-pid "$$" > /dev/null || exit 3
    "$0" artisan --no-tty --non-interactive test tests/Probe/SlowTest.php
  ' "$BERTH" > "$OUTPUT/killed-agent.out" 2>&1
) &
killed_pid=$!
for _ in $(seq 1 900); do
  [[ -e "$KILLED_MARKER" ]] && break
  kill -0 "$killed_pid" 2>/dev/null || break
  sleep 1
done
[[ -e "$KILLED_MARKER" ]] || fail "the agent to kill never started its slow test (see killed-agent.out)"
# Its test-lease app container, as Docker runs it, while the test runs.
lease="$("$ENGINE" ps --filter "label=dev.berth.installation=$(installation)" --format '{{.Names}}' | grep 'agent-killed' | grep '_app-testing-pool' | head -n 1)"
security_of "$lease" > "$OUTPUT/lease-security.txt"
log "SIGKILLing the agent mid-test"
abrupt_kill "$killed_pid"
wait "$killed_pid" 2>/dev/null || true

for pid in "${pids[@]}"; do
  wait "$pid" || true
done
run_seconds=$(( $(date +%s) - run_started ))
printf 'parallel_run_seconds=%s\n' "$run_seconds" >> "$OUTPUT/metadata.txt"
log "all agents finished in ${run_seconds}s"

stop_background
samples="$(wc -l < "$OUTPUT/memory-samples.txt" | tr -d ' ')"
printf 'memory_samples=%s\n' "$samples" >> "$OUTPUT/metadata.txt"

# --- Assertions --------------------------------------------------------------
failures=0
file_owner() {
  if [[ "$(uname -s)" == Darwin ]]; then stat -f %u "$1" 2>/dev/null; else stat -c %u "$1" 2>/dev/null; fi
}
# Test output without ANSI colors.
plain() { sed $'s/\x1b\\[[0-9;]*m//g' "$1"; }
check() {
  local description="$1" actual="$2" expected="$3"
  if [[ "$actual" != "$expected" ]]; then
    log "  $description: expected '$expected', got '$actual'"
    failures=$((failures + 1))
  fi
}
log "checking every agent"
for index in $(seq 1 "$AGENTS"); do
  name="agent-$index"
  out="$OUTPUT/agents/$name"
  check "$name up" "$(cat "$out/up.status")" 0
  check "$name up document" "$(jq -r '.status' "$out/up.json" 2>/dev/null)" ready
  check "$name https" "$(cat "$out/https.code")" 200
  check "$name route reaches its own instance" "$(cat "$out/route.body")" \
    "$(jq -r '.environment.DB_DATABASE' "$out/up.json")"
  check "$name exit code 3 passes through artisan" "$(cat "$out/exit3.status")" 3
  check "$name app runs least-privileged" "$(cat "$out/app-security.txt")" \
    "$APP_USER [\"NET_BIND_SERVICE\"] [\"no-new-privileges\"]"
  check "$name migrate" "$(cat "$out/migrate.status")" 0
  check "$name markers" "$(cat "$out/markers.status")" 0
  # Its development database holds only its own marker: no other agent's
  # data is visible.
  check "$name sees only its own data" "$(tail -n 1 "$out/markers.out" | tr -d '[:space:]')" "$name"
  check "$name composer as the invoking user" "$(cat "$out/composer.status")" 0
  check "$name npm as the invoking user" "$(cat "$out/npm.status")" 0
  check "$name psql as its tenant" "$(cat "$out/psql.status") $(grep -c "$(jq -r '.credentials.db.username' "$out/up.json")" "$out/psql.out")" "0 1"
  check "$name redis-cli as its tenant" "$(cat "$out/redis-cli.status") $(tr -d '\r' < "$out/redis-cli.out" | grep -cx PONG)" "0 1"
  check "$name focused test" "$(cat "$out/test.status")" 0
  # Files the app writes into the checkout belong to the agent's user.
  check "$name app writes files as the invoking user" \
    "$(file_owner "$WORKTREES/$name/storage/app/owner-probe")" "$(id -u)"
  # Exit codes alone do not prove the tests ran: count them.
  check "$name focused test ran both tests" \
    "$(plain "$out/test.out" | grep -cE 'Tests: +2 passed')" 1
  check "$name paratest --parallel" "$(cat "$out/paratest.status")" 0
  check "$name paratest ran the eight suite tests" \
    "$(plain "$out/paratest.out" | grep -cE '^OK \(8 tests')" 1
  check "$name deliberate failure keeps its exit code" "$(cat "$out/failing.status")" 1
  check "$name failure came from the deliberate assertion" \
    "$(plain "$out/failing.out" | grep -q 'deliberate failure' && echo yes)" yes
  check "$name down --purge" "$(cat "$out/down.status")" 0
done

check "test-lease app runs least-privileged" "$(cat "$OUTPUT/lease-security.txt")" \
  "$APP_USER [\"NET_BIND_SERVICE\"] [\"no-new-privileges\"]"

log "checking that no two agents' runs shared a database"
: > "$OUTPUT/db-names.txt"
for index in $(seq 1 "$AGENTS"); do
  name="agent-$index"
  dev="$(jq -r '.environment.DB_DATABASE' "$OUTPUT/agents/$name/up.json")"
  names="$WORKTREES/$name/storage/app/db-names"
  if [[ ! -s "$names" ]]; then
    log "  $name recorded no test databases"
    failures=$((failures + 1))
    continue
  fi
  grep -qx "$dev" "$names" && { log "  $name's tests ran in its development database"; failures=$((failures + 1)); }
  sort -u "$names" | sed "s/^/$name /" >> "$OUTPUT/db-names.txt"
done
shared="$(awk '{print $2}' "$OUTPUT/db-names.txt" | sort | uniq -d | wc -l | tr -d ' ')"
check "databases used by more than one agent" "$shared" 0

log "checking that the killed agent is reaped"
# Reaping runs at most every 30 seconds per installation.
sleep 31
"$BERTH" ls --json > "$OUTPUT/ls-after-kill.json" 2> "$OUTPUT/ls-after-kill.err" || true
check "killed agent reaped" \
  "$(jq '[.instances[] | select(.root | endswith("/agent-killed"))] | length' "$OUTPUT/ls-after-kill.json")" 0
# Whichever berth invocation ran next (another agent's or this `ls`) reaped
# it and said why.
grep -rqE "Reaped accept/agent-killed-[0-9a-f]+ because its owner process exited" "$OUTPUT" \
  || { log "  no berth invocation reported reaping the killed agent"; failures=$((failures + 1)); }
# Its instance containers and its interrupted test run's slot containers.
check "killed agent's containers removed" \
  "$("$ENGINE" ps -a --filter "label=dev.berth.installation=$(installation)" --format '{{.Names}}' | grep -c 'agent-killed')" 0

log "checking the gateway"
gateway="$("$ENGINE" ps -q --filter "label=dev.berth.installation=$(installation)" --filter label=dev.berth.role=gateway)"
check "gateway runs least-privileged" "$(security_of "$gateway")" \
  "$APP_USER [\"NET_BIND_SERVICE\"] [\"no-new-privileges\"]"

log "checking an app that terminates TLS itself on container port 443"
# The one case that runs as the container's root, with a reduced capability
# set (L022): it must still bind 443, serve HTTPS on its published port and
# write the checkout, on rootful Linux Docker as much as elsewhere. Such an
# app is a direct TLS target: the gateway does not route it.
make_worktree tls-443
perl -0pi -e 's/  app:\n    preset: laravel\n/  app:\n    preset: laravel\n    container_port: 443\n    localhost_tls: true\n    publish: true\n/' \
  "$WORKTREES/tls-443/.berth.yaml"
tls_out="$OUTPUT/tls-443"
mkdir -p "$tls_out"
tls_status=0
(cd "$WORKTREES/tls-443" && "$BERTH" up --wait --json > "$tls_out/up.json" 2> "$tls_out/up.err") || tls_status=$?
check "443 app up" "$tls_status" 0
tls_app="$(jq -r '.services[] | select(.name == "app") | .container' "$tls_out/up.json" 2>/dev/null)"
check "443 app capabilities" "$(security_of "$tls_app")" \
  ' ["CHOWN","DAC_OVERRIDE","FOWNER","FSETID","NET_BIND_SERVICE"] ["no-new-privileges"]'
# Its certificate comes from the app's own Caddy CA, kept in the container.
tls_url="$(jq -r '.services[] | select(.name == "app") | .url // empty' "$tls_out/up.json" 2>/dev/null)"
check "443 app answers on its published port" "$(
  curl -sk -o /dev/null -w '%{http_code}' --max-time 30 "$tls_url/up" 2>/dev/null || true
)" 200
tls_write=0
(cd "$WORKTREES/tls-443" && "$BERTH" artisan --no-tty --non-interactive tinker \
  --execute="file_put_contents(storage_path('app/tls-probe'), 'root with reduced capabilities');" \
  > "$tls_out/write.out" 2>&1) || tls_write=$?
check "443 app writes the checkout" "$tls_write $([[ -s "$WORKTREES/tls-443/storage/app/tls-probe" ]] && echo written)" "0 written"
tls_down=0
(cd "$WORKTREES/tls-443" && "$BERTH" down --purge > "$tls_out/down.out" 2>&1) || tls_down=$?
check "443 app down --purge" "$tls_down" 0

log "checking the MySQL, MariaDB and Valkey tenant clients"
# Each runs in a throwaway container on the instance network and logs in
# as the tenant (agents cover PostgreSQL and Redis).
make_worktree clients
cat > "$WORKTREES/clients/.berth.yaml" <<'YAML'
schema_version: 9
project: clients
project_type: project
services:
  my:
    preset: mysql
  maria:
    preset: mariadb
  kv:
    preset: valkey
YAML
clients_out="$OUTPUT/clients"
mkdir -p "$clients_out"
clients_status=0
(cd "$WORKTREES/clients" && "$BERTH" up --wait > "$clients_out/up.out" 2>&1) || clients_status=$?
check "tenant-client instance up" "$clients_status" 0
for service in my maria; do
  status=0
  (cd "$WORKTREES/clients" && printf 'select current_user();\n' |
    "$BERTH" exec --no-tty --service "$service" > "$clients_out/$service.out" 2>&1) || status=$?
  check "$service client as its tenant" \
    "$status $(grep -cE '^[0-9a-f]{12}_[0-9a-f]+@%$' "$clients_out/$service.out")" "0 1"
  check "$service client prints no warning" "$(grep -ci warning "$clients_out/$service.out")" 0
done
status=0
(cd "$WORKTREES/clients" && printf 'PING\n' |
  "$BERTH" exec --no-tty --service kv > "$clients_out/kv.out" 2>&1) || status=$?
check "kv client as its tenant" "$status $(tr -d '\r' < "$clients_out/kv.out" | grep -cx PONG)" "0 1"
clients_down=0
(cd "$WORKTREES/clients" && "$BERTH" down --purge > "$clients_out/down.out" 2>&1) || clients_down=$?
check "tenant-client instance down --purge" "$clients_down" 0

log "checking leftovers"
leftover_instances="$(jq '.instances | length' "$OUTPUT/ls-after-kill.json")"
check "instances left" "$leftover_instances" 0
non_shared="unknown"
if id="$(installation)" && [[ -n "$id" ]] && listing="$(
  "$ENGINE" ps -a --filter "label=dev.berth.installation=$id" --format "{{.Names}} $(role_template)" &&
  "$ENGINE" volume ls --filter "label=dev.berth.installation=$id" --format "{{.Name}} $(role_template)" &&
  "$ENGINE" network ls -q --filter "label=dev.berth.installation=$id" |
    while read -r network; do
      "$ENGINE" network inspect --format '{{.Name}} {{index .Labels "dev.berth.role"}}' "$network"
    done
)"; then
  printf '%s\n' "$listing" > "$OUTPUT/objects-before-gc.txt"
  non_shared="$(printf '%s\n' "$listing" | awk 'NF && $2 != "shared-server" && $2 != "gateway"' | wc -l | tr -d ' ')"
fi
check "objects left besides shared servers and the gateway" "$non_shared" 0
gc_status=0
"$BERTH" gc --json > "$OUTPUT/gc.json" 2> "$OUTPUT/gc.err" || gc_status=$?
check "berth gc" "$gc_status" 0
check "objects left after gc" "$(count_labeled_objects)" 0

# The sampler must have covered the run, or the peak below is not the run's
# peak. A sample takes 2 s plus however long the engine's `stats` needs, which grows
# with the container count; one per 20 s (and at least three) is the floor.
run_seconds="$(sed -n 's/^parallel_run_seconds=//p' "$OUTPUT/metadata.txt")"
if (( samples < 3 || samples * 20 < run_seconds )); then
  log "  only $samples memory samples over ${run_seconds}s"
  failures=$((failures + 1))
fi

# --- Benchmarks --------------------------------------------------------------
sort -n "$OUTPUT"/agents/agent-*/up.ms > "$OUTPUT/ready-ms.txt"
percentile() {
  awk -v p="$1" '{ values[NR] = $1 } END { index_ = int((p / 100) * NR + 0.999999); if (index_ < 1) index_ = 1; print values[index_] }' "$OUTPUT/ready-ms.txt"
}
{
  printf 'time_to_ready_p50_ms=%s\n' "$(percentile 50)"
  printf 'time_to_ready_p95_ms=%s\n' "$(percentile 95)"
  printf 'peak_memory_bytes=%s\n' "$(sort -n "$OUTPUT/memory-samples.txt" | tail -n 1)"
  printf 'failures=%s\n' "$failures"
} | tee "$OUTPUT/results.txt"

if (( failures > 0 )); then
  fail "$failures checks failed (see $OUTPUT)"
fi
log "PASS: $AGENTS agents"
