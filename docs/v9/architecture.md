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
  its root, TTL and expiry. Every berth command that touches the instance
  renews the expiry. The default TTL is 24 hours; `--ttl` sets it per
  instance (`0` disables expiry).
- An instance is not owned by a process: `berth up` exits immediately after
  the environment is healthy. Instances are reaped only when their lease has
  expired or their project root no longer exists (an agent deleted its
  worktree).
- Every berth invocation runs a throttled reaper. It never touches an
  instance whose operation lock (`flock` on the instance directory) is held,
  and never touches another installation's objects.
- Reaping an instance is `down --purge`.

## Test runs

`berth artisan test` keeps the 7.0.0 lease semantics: a bounded per-instance
slot pool limits concurrency, and each run is isolated. Each run gets fresh,
uniquely named logical databases (and a key prefix) in the shared servers,
created before and dropped after the run. Run databases whose lease lock is no
longer held are dropped by the reaper, so a killed run never collides with a
later one. Tenant roles may create and drop their own `{db}_test_*` databases
so Pest/ParaTest `--parallel` works.

## Networking

Each instance gets its own bridge network `berth_{project}_{instance}` with an
explicit `/24` from `10.128.0.0/9`, chosen deterministically and probed
forward under a host lock on collision (FI-59). Services join it under their
service name as alias, so applications reach them by name. Shared servers and
the gateway are attached to the instance networks they serve and nothing
else. Host ports are published only when a service opts in.

Networks are created on demand by any command that starts a container and
removed by `down` (and after each test run) once no container, running or
stopped, references them; a stopped container keeps its network's ID, so its
network is never removed under it. Docker does not check host routes for an
explicit subnet, so a host whose VPN routes `10.128.0.0/9` may see conflicts;
Podman rejects such subnets and berth probes past them.

## Shared servers

PostgreSQL, MySQL, MariaDB, Redis and Valkey run as shared servers keyed by a
compatibility fingerprint, plus stateless Gotenberg and attribution-only
Mailpit. Each instance gets a logical tenant (database plus role, or Redis ACL
user with a key prefix). Credentials are per instance and stable until purge.
Creation and tenant provisioning run under a per-server host lock; a Docker
`409` name conflict is treated as "created concurrently" and adopted after an
ownership check. `berth gc` removes shared servers that no instance network
uses and that hold no tenant.

Known consequence: shared Redis/Valkey denies `FLUSHDB`/`FLUSHALL`, so
`artisan cache:clear` fails against it; set `isolation: dedicated` on the
service when a project needs it. Tenants may `SELECT` other logical databases
because Laravel's cache defaults to database 1; key prefixes still isolate
tenants. PostgreSQL databases that ParaTest creates accept connections from
other roles (PostgreSQL's default `CONNECT` grant), but their tables remain
private to the owning tenant.

## Gateway

One Caddy container (`berth-gateway`) terminates TLS with Caddy's internal CA,
whose data directory is bind-mounted from `~/.berth/gateway/data` so the CA
survives gateway recreation. It publishes on `127.0.0.1` only, on configurable
ports (default HTTPS `8443`, HTTP `8080`); an occupied port fails with its
owner named and is never replaced by another port. The complete route set is
derived from container labels and applied as a whole document under
`~/.berth/gateway/.publish.lock`, so concurrent `up` and `down` calls cannot
lose each other's routes.

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
