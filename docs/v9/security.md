# berth 9 security model

berth runs project code from many checkouts at once, often on behalf of
agents that edit those checkouts. The project file in a worktree is therefore
untrusted input, and no instance may reach another instance's data. This
document carries stackctl 8's threat-model controls (SEC-01 to SEC-06,
`docs/v8/security.md` on the 8.x branch) into the command-driven model and
names the surfaces 9.x adds.

## Assets and boundaries

- **Instance data:** tenant databases, Redis keys, dedicated volumes and the
  generated credentials under `~/.berth/instances/{project}/{instance}`.
- **Shared-server admin secrets** under `~/.berth/shared/{server}/admin`.
- **The gateway CA** under `~/.berth/gateway/data`.
- **The host:** the Docker engine and the user's files.

Trust boundaries: the project file and everything else in a checkout are
untrusted; the berth binary, its home directory and Docker are trusted.

## Controls

### SEC-01: cross-instance network reachability (mitigated)

Each instance (and each `--env` namespace and test run) has its own Docker
network with an explicit `/24` from `10.128.0.0/9`. Apps, workers and
dedicated services join only that network. A shared server joins each
consumer's network under the consumer's service name and holds a separate
tenant per instance: its own database and role (PostgreSQL, PUBLIC revoked,
`template1` closed), schema and user (MySQL/MariaDB, grants on escaped name
patterns only) or ACL user confined to a key prefix with dangerous commands
denied (Redis/Valkey). Swarm consumers join a network shared only with the
source app, never the source's instance network. The gateway joins instance
networks to proxy routes and is detached before a network is removed.

### SEC-02: project source writes from root containers (mitigated)

PHP app containers (FrankenPHP, Horizon, scheduler, Reverb) run as the
invoking user's UID:GID (mapped for rootless Docker, Podman and sudo), so
files in the worktree belong to the agent's user. A checkout holding
root-owned leftovers fails `up` with the exact `chown`, instead of serving
500s. Exception: an app that terminates TLS itself (container port 443) runs
as the container's root, because its CA must persist in the image's
root-owned `/data` (SEC-05 lists the capabilities it keeps).

### SEC-03: listener-only readiness (mitigated)

`up --wait` returns only after the app's route answers through the gateway and
a Laravel app boots (`php artisan about`); a fatal boot fails `up` with 125
and the fatal line.

### SEC-04: unbounded or hostile project YAML (mitigated)

The project file is read with a size bound, refusing symlinks and non-regular
files, and parsed strictly: one document, no aliases, anchors or merge keys,
no duplicate keys, no unknown fields, YAML 1.2 scalars only.

### SEC-05: unnecessary workload privileges (mitigated)

No field can make a container privileged or add capabilities, devices, host
namespaces, security options or a user (`privileged`, `cap_add`, `devices`,
`network_mode`, `pid`, `security_opt`, `user` do not exist). Non-root PHP
app containers and the gateway drop every capability except
`NET_BIND_SERVICE` and run with `no-new-privileges`, with `noexec,nosuid`
tmpfs for runtime state. A port-443 app, which runs as root, keeps
`NET_BIND_SERVICE` plus `CHOWN`, `DAC_OVERRIDE`, `FOWNER` and `FSETID` (root
must write the user's checkout) and drops the rest of Docker's default set. Images that bring their own user
(Selenium, Gotenberg, Mailpit, Soketi) and dedicated data services keep
Docker's defaults. A bind mount that reaches a container engine socket (the socket, a
symlink to it, or a directory above it) is refused at parse time.

### SEC-06: publication of generated files (mitigated, narrowed)

berth 9 never writes into a checkout implicitly: no lock file, no `.env`.
The only generated config is the gateway's, written atomically under a host
lock into a private directory and applied through Caddy's validated reload.
`berth lock` (explicit) and `env generate` (to a new file only) refuse
symlinked destinations.

## New surfaces in 9.x

- **The berth home** (`~/.berth`, or `BERTH_HOME`): directories are 0700 and
  secrets 0600, created by staging and hard-linking so concurrent first uses
  agree; symlinked directories are refused.
- **Host locks** (`~/.berth/locks/*.lock`, flock): serialize shared-server
  creation and tenant provisioning, gateway publication, network allocation,
  reaping and test-slot leases. Locks are advisory: they order berth's own
  processes, not other programs.
- **Ownership labels** (`dev.berth.installation`, `project`, `instance`,
  `role`, …): reaping, purge and `gc` act only on objects carrying this
  installation's ID; a foreign container under a berth name is never adopted.
- **Secrets in argv:** passwords reach `docker run` and `docker exec` as
  `-e NAME` with the value in the environment, never in the command line, and
  known secrets are redacted from every message.
- **Leases and reaping:** an instance is deleted only for a reason proven on
  this host (an expired lease, an owner PID matched with its start time that
  no longer runs, a checkout that definitely no longer exists); unreadable
  state counts as present.

## Not covered

- `berth restore` into a shared-backed database streams the dump to the
  client inside the shared server, so a dump containing a shell escape
  (`\!`) reaches the server; restore only dumps you trust.
- Host trust-store installation (deferred): the CA stays inside the berth
  home.
- A shared server is one process: a tenant can observe server-wide resource
  use (connections, memory) and other tenants' names (Redis key names via
  `SCAN`, PostgreSQL database names in the catalog), though not their data.
