# berth for agents

The exact contract an agent (or a script driving many of them) can rely on.
Every shape below was captured from a real run; field order may differ.

## The loop

```bash
cd "$WORKTREE"                                  # any directory: dot-dirs, nested worktrees
berth config validate --json                    # optional preflight, never touches Docker
berth up --wait --json --owner-pid $$ > up.json # create or converge; exits when the app boots
url=$(jq -r '.urls[0]' up.json)
curl -sk "$url"                                  # HTTPS through the gateway
berth artisan --no-tty --non-interactive test tests/Feature/FooTest.php
berth artisan --no-tty --non-interactive test --parallel
berth down --purge                              # containers, databases, volumes, credentials
```

Thirty agents can run this at once in thirty worktrees of one project: each
checkout is its own instance, with its own containers, network, tenants and
route, and nothing is configured per agent.

## Instances

An instance is one checkout of one project. Its ID is derived from the
canonical checkout path: a readable slug of the directory name plus a short
hash (`agent-17-4499c8`), stable for that path and different for every other
path, including symlinked and real paths to the same checkout (they resolve
to one ID). `--instance <ID>` or `BERTH_INSTANCE` overrides it. Container,
network and route names embed project and instance:
`berth_{project}_{instance}_{service}` and
`https://{instance}-{project}[-{service}].berth.localhost:{port}`. The port is
8443 unless `BERTH_GATEWAY_HTTPS_PORT` says otherwise when the gateway is
first created; berth records it, later processes use the recorded port, and a
different value fails. Port 443 is left out of URLs.

## Exit codes

- A command berth runs inside a container (`exec`, `artisan`, `composer`,
  the PHP tools, `node`, `bun`, `deno`) exits with that command's exact code;
  a command killed by signal *n* exits `128 + n`. stdin is passed through.
- berth's own failures (invalid config, Docker errors, a failed `up`, usage
  errors) exit **125**, with the message on stderr only.
- `--help` and `--version` exit 0.
- Without a terminal on stdin, commands run without a TTY by default;
  `--no-tty` disables it even with one, and `--tty` asks for one (honored
  only when stdin is a terminal). Errors fail fast: berth never retries for
  minutes (image pulls retry transient registry errors at most three times).

## `up --wait --json`

stdout holds exactly one JSON document; progress, hook output and pulls go
to stderr. With `--wait` (the default) `up` returns after every selected
service accepts connections and a Laravel app has booted (`php artisan
about`); `--wait-timeout` (default 120 s) bounds it.

```json
{
  "schema": 1,
  "project": "shop",
  "instance": "shop-613c1c",
  "root": "/work/shop",
  "status": "ready",
  "urls": ["https://shop-613c1c-shop.berth.localhost:8443"],
  "services": [
    {"name": "db", "kind": "database", "driver": "postgres",
     "container": "berth_shared_postgres_24c918af7910", "state": "running",
     "health": "healthy", "endpoint": "db:5432", "url": null, "published": false},
    {"name": "app", "kind": "app", "driver": "frankenphp",
     "container": "berth_shop_shop-613c1c_app", "state": "running",
     "health": "healthy", "endpoint": "app:80",
     "url": "https://shop-613c1c-shop.berth.localhost:8443", "published": false}
  ],
  "credentials": {
    "db": {"database": "shop__shop-613c1c__db", "username": "shop__shop-613c1c__db", "password": "…"},
    "cache": {"username": "shop__shop-613c1c__cache", "password": "…"}
  },
  "environment": {"DB_HOST": "db", "DB_DATABASE": "shop__shop-613c1c__db", "…": "…"},
  "host_environment": {"…": "…"}
}
```

- `health` is `healthy` (probed this run), `unchecked` (running, no probe),
  `not_running`, or `not_checked` (`--no-wait`). It is never guessed.
- `environment` is what app containers receive; `host_environment` holds
  endpoints a host tool can reach (only published services have any).
- On failure `up --json` still prints one document and exits 125:
  `{"schema":1,"project":…,"instance":…,"root":…,"status":"failed","error":"…"}`.
  Known secrets in the error are redacted. Three failures come before there
  is a project to describe and print nothing on stdout (exit 125, the error
  on stderr): a project file that cannot be loaded, an instance that cannot
  be marked in use (an unwritable berth home), and `--dry-run` (refused).

## Commands in the instance

- `berth artisan …`, `berth composer …`,
  `berth phpstan|ecs|php-cs-fixer|psalm|pint|pest|phpunit|rector …`,
  `berth node …`, `berth bun …` and `berth deno …` run through `docker exec`
  in the app container (or a one-off container when it is not running), as
  the invoking user, with the managed environment.
- `berth exec [--service S] -- CMD…` runs in the service's running
  container; when it is not running, `exec` fails with 125 and says to run
  `berth up` first. On a shared-backed service, `berth exec --service db`
  without a command opens the tenant's own client in a throwaway container
  on the instance network (stdin and its exit code pass through); a command
  of your own, `cp` and `inspect` are refused (125), because they would act
  on the server every instance shares. `berth cp` accepts only this
  instance's services (`db:/path`) and host paths.
- Inside a container, reach other services by name (`db:5432`, the
  `endpoint` field) and other projects through swarm `inject_env` values.
  Route URLs (`*.berth.localhost`) are for the host: libcurl resolves every
  `*.localhost` name to the container's own loopback, so an app calling its
  own `APP_URL` from inside its container is refused.
- The app reads its configuration from the injected environment; `.env` is
  never written. The host's cached Laravel config is bypassed
  (`APP_CONFIG_CACHE` points inside the container).

## `artisan test`

Each `berth artisan test …` run takes a slot from a pool keyed by the
workspace (8, 16 or 32 slots, sized from the engine's CPUs and memory;
`--test-runtime-pool-size` or `BERTH_TEST_RUNTIME_POOL_SIZE` sets it) and runs
in fresh, disposable containers with its own tenants on the shared servers,
so concurrent test runs in one checkout never see each other's data. Under
`--parallel` (Pest or ParaTest) each process gets its own database,
`{tenant}_test_{N}`, which the run's role may create and drop. Everything the
run created is removed when it ends; a run that was killed has its slot
containers removed by the next berth invocation. A test run's own app URL is
not reachable from the host (tests fake HTTP).

## `down` and `down --purge`

`down` removes the instance's containers and network and keeps its data
(dedicated volumes, tenant databases, credentials). `down --purge` deletes
that data too, in one call, with no confirmation and nothing retained.
`down --purge --service S` purges one service.

## Leases, TTL and crash recovery

No daemon is needed. Every berth command that touches Docker (at most every
30 s per installation; `berth ls` and `gc` too) reaps, as `down --purge`
does, any instance of this installation whose:

- lease expired (`up --ttl 2h`, cleared by `up --ttl none`; every berth
  command in the checkout that touches Docker renews it, which `init`, `config`, `preset`, `profile`,
  `env`, `lock`, `completions`, `daemon` and `ls --services` do not; there is
  no default TTL), or
- owner process exited (`up --owner-pid $$`, or `BERTH_OWNER_PID`; the PID is
  matched with its start time, so a reused PID is not the owner), or
- checkout no longer exists (moving or deleting a checkout purges its
  instance; run `berth down` before moving one).

An instance a berth command is currently running in is never reaped. A reap
prints `Reaped {project}/{instance} because …` on stderr. Reap failures are
warnings, except in `berth gc`, which exits 125.

## Inspection

Every inspection command prints exactly one JSON document on stdout with
`--json` (or `--format json`; `--format` accepts only `table` or `json`):

- `berth ls --json`: every instance of this installation, across projects:
  `{"instances":[{"project","instance","root","root_exists","status",
  "containers","networks","running","ttl_seconds","expires_at",
  "expires_in_seconds","expired","lease_error","owner_pid","owner_alive"}]}`.
  `status` is `running`, `partial`, `stopped` or `down`.
- `berth ps --json`: `[{"name","kind","driver","image","port","status"}]`.
- `berth url --json`: `[{"name","url","published"}]`.
- `berth health --json`: `[{"name","ok","error"}]`.
- `berth about --json`: `{"application","environment","services"}`.
- `berth doctor --json`: `{"ok","error","checks"}`.
- `berth env --json`: `{"environment","host"}`; `berth env generate --output
  F` writes it to a new file.
- `berth logs --json`: one object per line,
  `{"service","stream":"stdout"|"stderr","line"}`; `--all` skips services
  without a container.
- `berth config validate --json`: `{"valid":true,"project","instance",
  "services":[{"name","kind","shared"}]}`, or exit 125.
- `berth gc --json`: `{"reaped_instances","removed_test_containers",
  "removed_test_slot_locks","servers":[{"server","engine","removed","reason"}],"gateway":{"removed",
  "container"|"reason"}}`. If reaping itself fails, `gc` exits 125 with the
  error on stderr and prints no document.

## Shared servers: what a tenant can and cannot do

PostgreSQL, MySQL, MariaDB, Redis and Valkey run as one shared server per
engine version; each instance (and each test run) is a tenant. Set
`isolation: dedicated` on the service when the project needs:

- Redis or Valkey `FLUSHDB`/`FLUSHALL` (`cache:clear` on the Redis store), or
  an older Laravel `database.redis` config that sends no username;
- PostgreSQL extensions only a superuser can create (PostGIS, TimescaleDB
  `CREATE EXTENSION`), or restores that set object owners;
- MySQL restores with `DEFINER` clauses or GTID statements.

A tenant cannot read or write another tenant's data, but it can see other
tenants' names: Redis `SCAN` lists every key name (never values), and
PostgreSQL's catalog lists every database name. `pause` and `unpause` on a
shared-backed service refuse with 125, because the server belongs to every
instance; `stop`, `kill`, `rm` and `down` detach the instance from it and
leave the server running.

## TLS

The gateway terminates TLS with one local CA that stays stable across gateway
recreation (`~/.berth/gateway/data/caddy/pki/authorities/local/root.crt`).
berth does not install it into host trust stores; use `curl -k`, or trust that
file explicitly (`curl --cacert …`).

## Swarm endpoints

In a swarm workspace, values injected from another project (`:url`,
`:base_url`, `:host`, `:port`) name that project's app container, which the
consumer's containers reach over a network shared only with that app. Sources
must be in `depends_on` and running.

## Known limits

- berth never stops, kills, restarts or removes a container that another
  berth installation (another `BERTH_HOME`) labeled: such a command fails
  with 125 naming it, and `berth prune --all` prunes every stopped container
  except another installation's. Two homes using the same project and instance names
  still derive the same container names, so only one of them can run that
  instance at a time.
- When an authored `domain` is another instance's route, the route serves
  that instance and the authored domain is withheld, with a warning.
- The acceptance run (`BERTH_ENGINE=docker|podman`) passes on Docker
  Desktop and, in CI on Linux, on rootful Docker, rootless Docker and
  rootless Podman 4.9, including the app that terminates TLS on port 443.
  Rootful Podman on Linux is not part of that run.
