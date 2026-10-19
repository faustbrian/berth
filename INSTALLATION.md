# Installation

## Requirements

- macOS or Linux.
- Docker Desktop, Docker Engine (rootful or rootless) or Podman 4.9 and
  later (`--engine podman`, `BERTH_ENGINE=podman` or `container_engine:
  podman`). The agent acceptance run passes in CI on Linux with rootful
  Docker, rootless Docker and rootless Podman, and locally on Docker
  Desktop; rootful Podman on Linux is not tested.
- A Rust toolchain with Cargo, to build berth.
- `curl` on the host (readiness probes).

## Install a release

```bash
cargo install --git https://github.com/faustbrian/berth.git --tag 10.1.1 --bin berth --locked
```

## Install the 10.x branch

```bash
cargo install --git https://github.com/faustbrian/berth.git --branch 10.x --bin berth --locked
```

## Install from a checkout

```bash
just install    # cargo install --path . --locked
```

## Verify

```bash
berth --version
berth doctor
```

## Upgrade

Re-run the install command. berth keeps its state under `~/.berth`
(`BERTH_HOME` moves it): the installation ID that labels every object it
creates, instance leases and credentials, shared-server admin secrets and
the gateway's CA. Upgrading keeps all of it.

## Shell completions

```bash
berth completions zsh > ~/.zsh/completions/_berth
```

## Optional daemon

`berth daemon watch` and `berth daemon service install` keep 7.0.0's
file-watching convenience for humans. No command needs it, and agents should
not use it.

## Migrating from helm 6 or stackctl 7

berth is the successor of helm 6 and stackctl 7.0.0: the same explicit
commands, selectors, `--env` namespaces, swarm, hooks, presets (except
`minio`), lock file, `share` (for a published port), `dump`/`restore` and
`env-scrub`.

1. Install berth and remove the old `helm` or `stackctl` binary.
2. Convert the project file once, in each project:
   `berth config migrate`. It reads `.helm.toml`, `.helm.yaml`,
   `.stackctl.toml`, `.stackctl.yaml` or `.berth.toml` and writes
   `.berth.yaml` (schema 9), printing every value it derived or dropped.
   berth never reads the old files and fails with a message naming
   `config migrate` while only an old file is present. A checkout holding
   both a helm and a stackctl file is refused: delete the one you no longer
   use first.
3. Rename environment variables to `BERTH_*`.

What behaves differently:

- **Each checkout is its own instance.** Containers are
  `berth_{project}_{instance}_{service}`; `container_prefix` and
  `container_name` are gone. `--instance` overrides the derived ID.
- **Databases and caches are shared by default.** PostgreSQL, MySQL, MariaDB,
  Redis and Valkey become tenants of a shared server. Set `isolation:
  dedicated` for a container of the instance's own: superuser-only
  extensions (PostGIS, TimescaleDB), `DEFINER` restores and `FLUSHDB` need it.
- **Apps are routed, not mapped to host ports.** URLs are
  `https://{instance}-{project}[-{service}].berth.localhost`, served by one
  gateway container instead of a host Caddy, with no `/etc/hosts` edits. Host
  ports are opt-in: `publish: true`, or `berth up -P`.
- **`.env` is never rewritten.** The managed environment is injected into
  containers; `berth env --json` and `berth env generate --output FILE` give
  it to host tools. The old `env --sync` family fails with a message.
- **PHP app containers run as your user** (FrankenPHP, Horizon, scheduler,
  Reverb), with every capability dropped except `NET_BIND_SERVICE`. On
  Linux, a worktree that 7.0.0's root containers wrote into fails `up` with
  the exact `chown` to run. An app serving TLS itself on container port 443
  runs as the container's root (its CA must persist) with a reduced
  capability set.
- **`berth ls` lists instances**; the old service listing is
  `berth ls --services`.
- **`down --purge`** deletes an instance's data, and leases (`up --ttl`,
  `--owner-pid`) let berth reap abandoned instances.
- **The `minio` preset is gone,** since MinIO no longer publishes open-source
  images. Use `rustfs` or `garage`; `config migrate` refuses a `minio`
  service.

## Migrating from stackctl 8

stackctl 8 was built around a required daemon. berth drops it and keeps
8's good parts: shared servers, the `.localhost` gateway, the managed
environment, the strict schema, least-privilege containers and ownership
labels.

1. Install berth and uninstall the stackctl 8 daemon service and binary.
2. Run `berth config migrate` in each project; it converts schema 8
   `.stackctl.yaml` files.
3. Start environments explicitly: `berth up` in the checkout. Nothing is
   discovered from watched roots.

What behaves differently:

- **No daemon, no discovery, no SQLite.** Docker labels and small files under
  `~/.berth` (leases, locks, credentials) are the only state.
- **Identity is per checkout,** not per project name, so parallel copies of
  one project all run.
- **Deletion is one call:** `berth down --purge`. There is no retention,
  backup or prune-token ceremony.
- **Routes** are `{instance}-{project}[-{service}].berth.localhost`.
- **Commands run directly against Docker,** with exact exit codes, stdin and
  TTY; `artisan test` runs isolated, ParaTest included.
- **Not carried over (yet):** host trust-store installation and the `trust`
  commands, the shared Mailpit server (the `mailhog` preset runs per
  instance), the shared Gotenberg server, the `minio` preset and the strict
  digest lock. See
  [`docs/v9/ledger.md`](docs/v9/ledger.md) for every decision.
