# Berth 9.x architecture

Berth 9.x keeps the explicit-invocation model of helm v6 / stackctl 7.0.0:
every command runs in the caller's process and drives the Docker CLI
directly. There is no required daemon. The design target is many AI agents
creating disposable environments in parallel git worktrees of one project.

## Identity

- **Project**: the `project` field of `.berth.yaml` (lowercase DNS label, at
  most 24 bytes). It is the same in every worktree of a repository.
- **Instance**: one environment of a project. Its ID is
  `{slug}-{hash6}`, derived from the canonical project root:
  - `slug`: the root directory's basename, lowercased, every run of
    characters outside `[a-z0-9]` replaced by one `-`, trimmed, truncated to
    16 bytes and trimmed again (`i` when nothing remains);
  - `hash6`: the first 6 hex digits of SHA-256 over the canonical root path.
  The ID is at most 23 bytes and is stable for a given path. Symlinked and
  real paths resolve to the same ID because the root is canonicalized first.
  `--instance <id>` (or `BERTH_INSTANCE`) overrides it; an invalid value is
  rejected with the input echoed, never normalized. The requested instance
  applies to the project the command runs in; every reload of that project in
  the same process uses it, while swarm dependencies (other projects) always
  use their own path-derived instances and child processes do not inherit
  `BERTH_INSTANCE`. The optional `daemon watch` supervises each discovered
  project's path-derived instance.
- **Runtime environment** (`--env`, from 7.0.0) stays a namespace inside an
  instance.

Container names are `berth_{project}_{instance}_{service}` (plus the 7.0.0
`--env` suffix). Project names, instance IDs and service names are lowercase
DNS labels that may contain `-` but never `_`, so a name always decodes into
its parts. Named volumes of non-application services are namespaced the same
way; application mounts are host paths relative to the project root and are
never renamed. Routes are `{instance}-{project}[-{service}].berth.localhost`;
the service segment is omitted for the service named `app`. Name lengths are
validated so the route's first label never exceeds 63 bytes.

## Ownership

Every Docker object berth creates carries `dev.berth.*` labels: `managed`,
`installation` (a random ID stored in `~/.berth/installation-id`), `project`,
`instance`, `service`, `kind`, `root` (canonical project root) and `schema`.
Berth only stops, removes or connects objects whose labels prove they belong
to the current installation. Unlabeled and foreign containers are never
touched.

Local files under `~/.berth` (directories `0700`, files `0600`, symlinks
refused) hold only leases, locks, credentials and generated gateway state.

## Leases, TTL and reaping

- Each instance has `~/.berth/instances/{project}/{instance}/lease.json` with
  its owning checkout, an optional TTL and expiry, and an optional owner
  process. `up --ttl 2h` sets the TTL; `up --owner-pid <pid>` (or
  `BERTH_OWNER_PID`) names the owner, recorded with its start time so a
  reused PID never counts. Every berth command that touches Docker renews the
  lease when it starts and again when it ends.
- There is no default TTL: an instance without `--ttl` is reaped only when its
  checkout is gone or its owner process exited. (The ledger first planned a
  24-hour default; reaping is a purge, so a default would delete a human's
  idle database the first time another project's command ran.)
- An instance is not owned by a berth process: `berth up` exits once the
  environment is healthy. While any berth command runs in an instance it holds
  a shared `flock` on `locks/instance-{project}-{instance}.lock`, and the
  reaper skips instances whose lock it cannot take, so a test run longer than
  its TTL is never deleted underneath it.
- Docker-touching commands run the reaper at most every 30 seconds per
  installation (`berth ls` too; `berth gc` always), one process at a time. It
  never reaps the instance the command runs in, and only touches this
  installation's labeled objects.
- Reaping an instance is `down --purge`; its lease is deleted last, only
  after its tenants and Docker objects are gone, so an instance that could not
  be deleted completely is retried. Ordinary commands report a reaping
  failure as a warning and carry on (one stuck leftover never blocks other
  agents); `berth gc` fails with it. Leftover test-slot containers whose slot
  `flock` is free (their run died) are removed too, holding the slot meanwhile.
- "Checkout gone" means the recorded root path no longer exists: moving a
  checkout (`git worktree move`) or unmounting its volume makes berth delete
  the instance's data. Run `berth down` before moving one.
- An owner process counts as dead only when the system says the PID is gone
  or belongs to a process started at another time (Linux start ticks;
  elsewhere `ps -o lstart` in UTC with the C locale); an unreadable probe
  keeps the instance.
- `berth ls` lists every instance across projects from labels and berth
  state; `berth gc` reaps without waiting, then removes shared servers that no
  instance network uses and that hold no tenant.

## Test runs

`berth artisan test` keeps the 7.0.0 lease semantics: a bounded per-workspace
slot pool (8–32 slots, each an `flock` under `~/.berth/locks/test-slots`)
limits concurrency, and each run is isolated. Each run's shared services are
a tenant of the slot's own runtime environment, dropped at the start and end
of the run (ParaTest's `_test_*` databases included); dedicated services keep
7.0.0's per-run containers. A killed run frees its slot at once, and the next
run on that slot resets its tenant. Tenant roles may create their own
`{db}_test_*` databases so Pest/ParaTest `--parallel` works.

## Networking

Each instance gets its own bridge network `berth_{project}_{instance}` with an
explicit `/24` from `10.128.0.0/9`, chosen deterministically and probed
forward under a host lock on collision (FI-59). Services join it under their
service name as alias, so applications reach them by name. Shared servers and
the gateway are attached to the instance networks they serve and nothing
else. Host ports are published only when a service opts in (`publish: true`
for an allocated port, or `-P`/`--no-publish-all` for every service);
`--env` namespaces and test leases keep 7.0.0's published ports because they
have no routes. Whether a service is published is read from its container's
port binding (`HostConfig.PortBindings`, which survives a stop), so every
later command, `recreate` and a stopped container's restart keep the port an
earlier `up -P` assigned. A container keeps its publication until it is
removed with `down`. Unpublished services are health-probed from inside the
gateway; `health` never starts the gateway to do so.

Networks are created on demand by any command that starts a container and
removed by `down` (and after each test run) once no container, running or
stopped, references them; a stopped container keeps its network's ID, so its
network is never removed under it. Docker does not check host routes for an
explicit subnet, so a host whose VPN routes `10.128.0.0/9` may see conflicts;
Podman rejects such subnets and berth probes past them.

## Shared servers

PostgreSQL, MySQL, MariaDB, Redis and Valkey run as shared servers keyed by a
compatibility fingerprint. Gotenberg and the mail catcher stay per instance
(ledger L062 deferred, L057 dropped: a shared mail catcher would show every
instance's mail to every agent). Each instance gets a logical tenant (database plus role, or Redis ACL
user with a key prefix). Credentials are per instance and stable until purge.
Creation and tenant provisioning run under a per-server host lock; a Docker
`409` name conflict is treated as "created concurrently" and adopted after an
ownership check. `berth gc` removes shared servers that no instance network
uses and that hold no tenant, and the gateway container once no instance
network holds it (its CA and state stay under `~/.berth/gateway`).

Redis and Valkey specifics: a tenant is an ACL user named like the SQL
tenants, allowed only keys and channels under `{tenant}:`; berth injects
`REDIS_USERNAME`, `REDIS_PASSWORD`, `REDIS_PREFIX={tenant}:` and
`HORIZON_PREFIX={tenant}:horizon:`, so Laravel's keys, queues, broadcast
channels and Horizon stay inside it. Users live in an ACL file on the
server's volume, seeded with only the admin's password hash before first
start. Known consequences: `KEYS`, `FLUSHDB` and `FLUSHALL` are denied, so
`artisan cache:clear` fails against a shared server (set `isolation:
dedicated` when a project needs it); `SCAN` is allowed and lists other
tenants' key names (never values); tenants may `SELECT` other logical
databases because Laravel's cache defaults to database 1, and key prefixes
still isolate them; purges and test-lease resets delete the tenant's keys in
every logical database. Server-wide script state (`SCRIPT FLUSH`, `SCRIPT
KILL`, `FUNCTION`) is denied. Shared servers need Redis 7 or later (Valkey
reports a Redis 7 version). Tools that publish outside the prefix, such as
Reverb's scaling channel, and projects whose `config/database.php` Redis
connections predate Laravel's `username` key (they authenticate as `default`
and are refused) need `isolation: dedicated` or the key added.

Lifecycle commands act on the tenant, never the server: `down`, `stop`,
`kill` and `rm` remove the service's name from the server's endpoint on the
instance network (two shared services of one instance share an endpoint
carrying both names); `pause`, `unpause` and `attach` refuse; `restart` and `recreate`
re-provision the tenant; `db reset` drops and re-provisions it; `down
--purge` drops every tenant of the instance on every shared server of the
installation (each runtime environment's and ParaTest's `_test_N` databases,
and any database a tenant role owns, included), detaches the servers from
all of the instance's networks, and removes its credentials; each step runs
even when an earlier one fails. Each
`--env` namespace and each `artisan test` lease gets a tenant of its own on the
same server. A shared tenant cannot publish a host port, take authored
`database`, `username` or `password` values (its credentials are generated),
or carry settings for a container of its own (`volumes`, `env`, `port`,
`container_port`, `restart`, hooks, which would run inside the server every
instance uses); each is a configuration error that points at `isolation:
dedicated`, which `config migrate` sets for 7.0.0 services with any of them. A lost credentials
file gives the tenant a new password on the next `up`.

PostgreSQL specifics: tenant roles have `CREATEDB` (for ParaTest) and
nothing else, so extensions that need a superuser (PostGIS, TimescaleDB) and
restores of dumps that set other owners need `isolation: dedicated`. Since
`CREATEDB` lets a tenant create any database name, provisioning drops
databases in an instance's namespace (`{project}__{instance}`, then `_…`)
owned by a role outside it before the instance uses them. Shared servers need
PostgreSQL 13 or later, run their admin SQL on stdin over TCP inside the
container with the admin password in the environment (so readiness only
passes once the image's temporary init server is gone), close `template1` to
tenants, and keep data in a labeled volume mounted at the image's `PGDATA`.
Databases that ParaTest creates accept connections from other roles
(PostgreSQL's default `CONNECT` grant); their tables remain private to the
owning tenant, but before PostgreSQL 15 another tenant could also create
objects in their `public` schema. Tenants share one user's machine, so this
guards against accidents, not a hostile neighbour.

MySQL and MariaDB specifics: a tenant is a schema and a user
`{instance hash}_{tenant hash}` (25 bytes, within MySQL's 32-byte user names)
granted everything on its schema and on `{schema}_test_%` and nothing else;
grant patterns escape `_`, which would otherwise match any character. MySQL
and MariaDB images get separate servers. Shared MySQL servers run with
`--skip-log-bin`: MySQL 8's default binary log would deny non-`SUPER`
tenants triggers and stored routines. Restoring a dump that needs more than a
tenant has (`DEFINER` clauses naming another account, `SET
@@GLOBAL.GTID_PURGED`) needs `isolation: dedicated`. Admin SQL runs as
`root` over TCP with `MYSQL_PWD` set; MySQL does not trust the local socket,
so a lost admin secret can only be restored, not reset.

An authored `command` replaces the image's server command and keys a server
of its own.

Secrets never appear in a docker argument: berth passes a secret
`-e NAME=value` as `-e NAME` with the value in the docker CLI's environment.

## Gateway

One Caddy container per installation (`berth_gateway_{installation[..12]}`) terminates TLS with Caddy's internal CA,
whose data directory is bind-mounted from `~/.berth/gateway/data` so the CA
survives gateway recreation. It publishes on `127.0.0.1` only, on configurable
ports (default HTTPS `8443`, HTTP `8080`); an occupied port fails with its
owner named and is never replaced by another port. The complete route set is
derived from container labels and applied as a whole document under
`~/.berth/gateway/.publish.lock`, so concurrent `up` and `down` calls cannot
lose each other's routes. A document is persisted only after the running
gateway accepts it. The gateway listens on the same ports inside its container
as on the host and is attached to each instance network with that network's
routes as aliases, so an app's own route names resolve to the gateway from
inside its container. Clients built on libcurl (PHP's curl extension, Guzzle,
Laravel `Http::`) resolve every `*.localhost` name to loopback themselves and
cannot use routes from inside containers; cross-project calls get in-network
endpoints instead (ledger L205, L206).

Only the default runtime environment of an instance has routes. `--env`
namespaces, including the per-run `artisan test` leases, keep 7.0.0's direct,
port-offset local URLs. Every routed service always has its unique instance
route, which is also its `APP_URL`; an authored `domain`/`domains` adds extra
routes. A host claimed by two running containers (an authored domain committed
to a project with two worktrees up, say) is withheld from both with a warning
while every other route keeps being published (8.x FI-53), so committed config
never needs editing per worktree. When all but one claimant go down, the
shared domain serves the remaining one again.

The gateway ports are an installation setting: the first gateway records them
in `~/.berth/gateway/ports` (from `BERTH_GATEWAY_HTTPS_PORT`/`_HTTP_PORT` or the
defaults), and every later process uses the recorded ports. A variable that
names a different port fails early instead of moving the gateway. The ports
are recorded only once a gateway has started on them, so a first start that
fails on a busy port can be retried on another.
Routable services are web apps, Mailpit's UI and WebSocket servers; workers,
schedulers, Horizon, RabbitMQ, Gotenberg and Selenium have no route.

## Least privilege

PHP app containers (the FrankenPHP app and its Horizon, scheduler and Reverb
processes, which mount the checkout) run as the invoking user's UID:GID, so
files they write into the worktree belong to the agent's user. They drop every
capability except `NET_BIND_SERVICE` (FrankenPHP's binary is
capability-marked), run with `no-new-privileges`, and get writable
`noexec,nosuid` tmpfs at `/data` and `/config` (Caddy's state, 16 MiB each)
and at `/home/berth`, their `HOME` for Composer, npm, Bun and Playwright caches
(capped at 1 GiB; tmpfs holds only what is written). Derived images install
JavaScript runtimes and version managers under `/usr/local`, never `/root`, and version managers' homes are writable by the container user.
berth's own setup inside a running app container (the MySQL client wrappers)
runs as root with the container's capability set (`NET_BIND_SERVICE` only).
The user mapping follows the engine: `--user UID:GID` on rootful Docker,
nothing on rootless Docker (container root already is the host user),
`--userns=keep-id` plus `--user` on Podman; under `sudo`, `SUDO_UID`/`SUDO_GID`.
Before starting, berth fails if `storage/`, `bootstrap/cache/`, `vendor/` or
`node_modules/` hold paths that user does not own (root-owned leftovers of
7.0.0's root containers) and prints the `chown` that fixes them. An app that
terminates TLS itself (`localhost_tls` or container port 443) keeps 7.0.0's
container, because its CA must persist in the image's `/data`. The gateway
runs with the same capability options. Images that bring their own user
(Selenium, Gotenberg, Mailpit, Soketi) keep their defaults.

## Commands and exit codes

- `exec`, `artisan`, `composer`, PHP tools, `node`, `bun` and `deno` run via
  `docker exec`, inheriting stdio. `-i` is always passed so piped stdin
  reaches the command; `-t` is passed only when stdin and stdout are both
  terminals, unless `--tty` or `--no-tty` decides explicitly.
- The command's exit code is berth's exit code. A command killed by a signal
  exits `128 + signal`.
- Berth's own failures (invalid config, unreachable Docker, missing instance)
  exit `125` with the error on stderr, so agents can tell them from a failing
  test.
- stdout carries only command output and machine-readable data; berth's own
  progress and diagnostics go to stderr.
