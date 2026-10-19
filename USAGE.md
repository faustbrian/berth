# Usage

This document describes the `.berth.yaml` project file, the concepts every
command shares, and every command and flag. Agents should read
[`docs/v9/agents.md`](docs/v9/agents.md) first: it is the exact contract
(commands, JSON shapes, exit codes, leases).

## CLI shape

```bash
berth [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]
```

## Global options

These apply to every command.

- `-q, --quiet`: less output.
- `--no-color`: no colored output.
- `--dry-run`: print what would change; commands that cannot honor it
  (`gc`, `up --json`, `logs --json`) refuse it.
- `--config <PATH>`: use this `.berth.yaml`.
- `--project-root <DIR>`: find `.berth.yaml` in this directory.
- `--env <NAME>`: a runtime namespace inside the instance (its own containers
  and network; for example `test`).
- `--instance <ID>`: use this instance ID instead of the one derived from the
  checkout path (`BERTH_INSTANCE` sets it too).
- `--engine <docker|podman>`: the container engine for this invocation
  (`BERTH_ENGINE` sets it too, and either wins over `container_engine` from
  the project file; the default is `docker`).
- `--repro`: reproducible mode (checks the lock file).
- `--non-interactive`: no prompts, no browser, no TTY.
- `--docker-max-heavy-ops <N>`, `--docker-max-build-ops <N>`,
  `--docker-retry-budget <N>`, `--test-runtime-pool-size <N>`: concurrency and
  retry tuning.

## Project file

berth reads exactly one file, `.berth.yaml`, at the project root. Older
`.helm.toml`, `.helm.yaml`, `.berth.toml` and `.stackctl.yaml` files are never
read: berth fails and names `berth config migrate`, which converts them once.
`berth config validate [--json]` checks a project file without touching a
container engine.

```yaml
schema_version: 9
project: shop            # lowercase letters, digits, inner hyphens; ≤ 24 bytes
project_type: project    # or library (else read from composer.json)
container_engine: docker # optional: docker or podman
services:
  db:
    preset: postgres     # shared server by default
  cache:
    preset: redis        # shared server by default
  app:
    preset: laravel
    env:
      QUEUE_CONNECTION: sync
  search:
    preset: opensearch   # dedicated container per instance
```

The YAML is strict: one document, no duplicate keys, no unknown fields, and
every error names the line. Service names are DNS labels (`a-z`, `0-9`, inner
`-`, ≤ 63 bytes) and become part of container and route names, so they are
never rewritten.

Service fields: `preset`, `kind`, `driver`, `image`, `host`, `port`,
`database`, `username`, `password`, `bucket`, `access_key`, `secret_key`,
`api_key`, `region`, `scheme`, `domain`, `domains`, `routes`, `container_port`,
`smtp_port`, `volumes`, `env`, `command`, `depends_on`, `seed_file`, `hook`,
`health_path`, `health_statuses`, `restart`, `localhost_tls`, `octane`,
`octane_workers`, `octane_max_requests`, `php_extensions`,
`trust_container_ca`, `env_mapping`, `javascript`, `publish` and
`isolation`.

Rules that protect a worktree's project file:

- No container privileges: `privileged`, `cap_add`, `devices`, `network_mode`,
  `pid`, `security_opt` and `user` do not exist as fields.
- No bind mount may reach a container engine socket (the socket, a symlink to
  it, or a directory above it such as `/var/run`).
- `env` keys are portable names (a letter or `_`, then letters, digits, `_`
  or `.`).
- `container_name` and `name` are not settable: containers are named
  `berth_{project}_{instance}_{service}` so parallel instances never collide.

### Shared and dedicated services

PostgreSQL, MySQL, MariaDB, Redis and Valkey services are tenants of one
shared server per engine version (an own database and role, or an ACL user
with a key prefix, per instance). Every other preset runs a dedicated
container per instance. `isolation: dedicated` gives a shared-capable
service its own container; `isolation: shared` is the default for those
engines. A shared service cannot set `publish`, authored credentials or
container fields (`volumes`, `env`, `port`, `container_port`, `restart`,
`hook`). See [`docs/v9/architecture.md`](docs/v9/architecture.md).

### Routing and host ports

Every routable app gets `https://{instance}-{project}[-{service}].berth.localhost`
through one local gateway (port 8443 by default, `BERTH_GATEWAY_HTTPS_PORT`).
Services reach each other by service name inside the instance network. Host
ports are opt-in: `publish: true` on a dedicated service, or `up -P`.

An app that serves several sites on separate domains, like production, lists
them under `routes`: each label gets its own route for every instance, and the
named env var receives its URL.

```yaml
services:
  app:
    preset: laravel
    routes:
      ee: APP_EE_URL   # https://{instance}-{project}-ee.berth.localhost:8443
      fi: APP_FI_URL
```

Labels are DNS labels, and the whole route label stays within 63 bytes. A
route that is also another route of the instance, a var that berth already
sets (`APP_URL`, `ASSET_URL`, `APP_KEY`, service endpoints), sets in the
service's `env` or sets for another route, and `routes` on a service without
a gateway route (including `localhost_tls`) all fail validation. A var a
swarm workspace injects into the project fails before the app starts, and so
does an app when another running instance already serves one of its routes.

An app with `routes` gets no `ASSET_URL`, so each site loads its assets from
its own host. Laravel then builds asset URLs from the request, so the app must
trust the gateway's forwarded headers (`trustProxies(at: '*')` with
`X-Forwarded-Host`, `-Port` and `-Proto`); otherwise its pages link `http://`
assets on an `https` port. Under `--env` and in `artisan test` runs there are no routes,
and the vars are left to the app's own env files. Adding or changing
`routes` on a running instance takes effect after `berth recreate`.

### Service hooks

Per-service hooks run in phases `post_up`, `pre_down` and `post_down`.

```yaml
services:
  app:
    preset: laravel
    hook:
      - name: seed-dev-user
        phase: post_up
        on_error: fail    # fail | warn
        run:
          type: exec      # inside the service container; `script` runs a host script
          argv: [php, artisan, db:seed, --class=DevUserSeeder]
```

Hook output goes to stderr, so `up --json` keeps stdout for its document.

### Restart policy

Long-lived containers get Docker's `unless-stopped` unless `restart` says
otherwise (`no`, `on-failure`, `always`, `unless-stopped`).

### Swarm workspaces

A workspace `.berth.yaml` lists projects and the values each one injects into
another:

```yaml
schema_version: 9
project: ws
project_type: project
swarm:
  - name: api
    root: api
    depends_on: [location]
    inject_env:
      - env: LOCATION_URL
        from: location
        value: ":url"     # :url, :base_url, :host or :port of the source app
  - name: location
    root: location
```

Injected endpoints name the source app's container, reachable from the
consumer's containers; sources must be in `depends_on` and running.

## Value enums

- `KIND`: `database`, `cache`, `object-store`, `search`, `app`.
- `DRIVER`: `postgres`, `mongodb`, `mysql`, `sqlserver`, `redis`, `valkey`,
  `dragonfly`, `memcached`, `garage`, `localstack`, `rustfs`,
  `opensearch`, `elasticsearch`, `meilisearch`, `typesense`, `frankenphp`,
  `reverb`, `horizon`, `scheduler`, `dusk`, `gotenberg`, `mailhog`,
  `rabbitmq`, `soketi`.
- Pull policy (`--pull`): `always`, `missing` (default), `never`.
- Port strategy (`--port-strategy`): `random` (default), `stable` (with
  `--port-seed`).
- Node package manager (`--package-manager`): `npm`, `pnpm`, `yarn`.
- Node version manager (`--version-manager`): `system` (default), `fnm`,
  `nvm`, `volta`.
- JavaScript runtime (`javascript.runtime`): `node` (default), `bun`, `deno`.

## Command reference

Generated from `berth <command> --help`; global options are listed above.

### `berth init`

Initialize a new .berth.yaml project file

```text
berth init [OPTIONS]
```

### `berth config`

Print resolved configuration

```text
berth config [OPTIONS] [COMMAND]
```

Options:

- `--format <FORMAT>`: [default: toml]

#### `berth config migrate`

Convert helm, stackctl and .berth.toml project files into .berth.yaml

```text
berth config migrate [OPTIONS]
```

#### `berth config validate`

Check the project file without touching Docker: exits 0 when it is valid, 125 with the precise error otherwise

```text
berth config validate [OPTIONS]
```

Options:

- `--json`: Print one JSON document: the project, this checkout's instance and its services

### `berth preset`

Inspect available service presets

```text
berth preset [OPTIONS] <COMMAND>
```

#### `berth preset list`

List all available preset names

```text
berth preset list [OPTIONS]
```

#### `berth preset show`

Show resolved default values for one preset

```text
berth preset show [OPTIONS] <NAME>
```

Arguments:

- `<NAME>`

Options:

- `--format <FORMAT>`: [default: toml]

### `berth profile`

Inspect profile groupings

```text
berth profile [OPTIONS] <COMMAND>
```

#### `berth profile list`

List built-in profile names

```text
berth profile list [OPTIONS]
```

#### `berth profile show`

Show services included by a profile

```text
berth profile show [OPTIONS] <NAME>
```

Arguments:

- `<NAME>`

Options:

- `--format <FORMAT>`: [default: table]

### `berth doctor`

Validate local setup and config health

```text
berth doctor [OPTIONS]
```

Options:

- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)
- `--fix`: Attempt to fix detected issues where possible
- `--reachability`: Probe app URLs and health endpoints for runtime reachability

### `berth lock`

Manage workspace lockfile for reproducible image resolution

```text
berth lock [OPTIONS] <COMMAND>
```

#### `berth lock images`

Resolve configured service images to immutable digests

```text
berth lock images [OPTIONS]
```

#### `berth lock verify`

Verify lockfile is present and in sync with current config

```text
berth lock verify [OPTIONS]
```

#### `berth lock diff`

Show the lockfile changes that would be written

```text
berth lock diff [OPTIONS]
```

### `berth daemon`

Optional file-watching daemon for humans; no command needs it

```text
berth daemon [OPTIONS] <COMMAND>
```

#### `berth daemon start`

Start a per-project Berth daemon

```text
berth daemon start [OPTIONS] --path <DIR>
```

Options:

- `--path <DIR>`: Project directory or nested path inside a Berth project

#### `berth daemon watch`

Watch directories for Berth projects and ensure daemons are running

```text
berth daemon watch [OPTIONS] --dir <DIR>
```

Options:

- `--dir <DIR>`: Directory to scan for Berth projects
- `--exclude-dir <DIR>`: Directory subtree to exclude from project discovery
- `--max-projects <N>`: Maximum number of projects to auto-start from one watch pass
- `--once`: Run one discovery pass and exit
- `--interval <INTERVAL>`: Seconds between discovery scans [default: 30]

#### `berth daemon service`

Install or inspect a login-time daemon watch service

```text
berth daemon service [OPTIONS] <COMMAND>
```

##### `berth daemon service install`

Install and start a login-time daemon watch service

```text
berth daemon service install [OPTIONS] --dir <DIR>
```

Options:

- `--dir <DIR>`: Directory to scan for Berth projects
- `--exclude-dir <DIR>`: Directory subtree to exclude from project discovery
- `--max-projects <N>`: Maximum number of projects to auto-start from one watch pass
- `--interval <INTERVAL>`: Seconds between discovery scans [default: 30]

##### `berth daemon service status`

Show service installation state

```text
berth daemon service status [OPTIONS]
```

##### `berth daemon service print`

Print the rendered service definition without installing it

```text
berth daemon service print [OPTIONS] --dir <DIR>
```

Options:

- `--dir <DIR>`: Directory to scan for Berth projects
- `--exclude-dir <DIR>`: Directory subtree to exclude from project discovery
- `--max-projects <N>`: Maximum number of projects to auto-start from one watch pass
- `--interval <INTERVAL>`: Seconds between discovery scans [default: 30]

##### `berth daemon service uninstall`

Stop and remove the installed service definition

```text
berth daemon service uninstall [OPTIONS]
```

#### `berth daemon status`

Show daemon status for a project

```text
berth daemon status [OPTIONS] --path <DIR>
```

Options:

- `--path <DIR>`: Project directory or nested path inside a Berth project

#### `berth daemon stop`

Stop a per-project Berth daemon

```text
berth daemon stop [OPTIONS] --path <DIR>
```

Options:

- `--path <DIR>`: Project directory or nested path inside a Berth project

#### `berth daemon logs`

Show daemon log output for a project

```text
berth daemon logs [OPTIONS] --path <DIR>
```

Options:

- `--path <DIR>`: Project directory or nested path inside a Berth project

### `berth setup`

Prepare service(s)

```text
berth setup [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--timeout <TIMEOUT>`: Optional cap in seconds on the readiness wait; without it berth waits as long as the database container is alive
- `--parallel <PARALLEL>`: [default: 4]

### `berth start`

Run doctor, start services, bootstrap app runtime, then open app URLs

```text
berth start [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Start a named service profile (full, infra, data, app, web, api)
- `--no-wait`: Skip health waits during startup (default behavior for `start`)
- `--wait`: Wait for service(s) to accept connections
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)
- `--pull <PULL>`: Pull image before start [default: missing] [possible values: always, missing, never]
- `--force-recreate`: Recreate containers even if their configuration and image haven't changed
- `--no-open`: Disable automatic browser launch + URL summary
- `--health-path <HEALTH_PATH>`: Override app health path shown by `open`
- `--no-deps`: Skip starting workspace swarm dependencies
- `--parallel <PARALLEL>`: Number of services to process in parallel [default: 4]

### `berth up`

Create or converge this checkout's instance (`--wait --json` for agents)

```text
berth up [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--ttl <TTL>`: Lease the instance for this long (`90m`, `2h`, `1d`): every berth command in the checkout renews it, and once it lapses any berth command deletes the instance as `down --purge` does. `none` clears a TTL set earlier; without `--ttl` the recorded TTL is kept
- `--owner-pid <OWNER_PID>`: Record this process (usually the agent's shell, `$$`) as the instance's owner: once it exits, any berth command deletes the instance as `down --purge` does. `BERTH_OWNER_PID` sets it too [env: BERTH_OWNER_PID=]
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Start a named service profile (full, infra, data, app, web, api)
- `--wait`: Wait for service(s) to accept connections (enabled by default for `up`)
- `--no-wait`: Skip health waits during startup
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)
- `--json`: Print one JSON document on stdout: URLs, per-service status, credentials and the managed environment (or the error on failure)
- `--pull <PULL>`: Pull image before start [default: missing] [possible values: always, missing, never]
- `--force-recreate`: Recreate containers even if their configuration and image haven't changed
- `-P, --publish-all`: Publish every service on random host ports (off by default: services are reached inside the instance network and through gateway routes)
- `--no-publish-all`: Publish every service on its configured host port
- `--port-strategy <PORT_STRATEGY>`: Strategy for assigning ports when randomization is enabled [default: random] [possible values: random, stable]
- `--port-seed <PORT_SEED>`: Seed used by `--port-strategy stable`
- `--no-deps`: Skip starting workspace swarm dependencies
- `--seed`: Apply configured data seed files after startup
- `--parallel <PARALLEL>`: [default: 4]

### `berth apply`

Converge services and apply configured data seeds

```text
berth apply [OPTIONS]
```

Options:

- `--no-deps`: Skip starting workspace swarm dependencies

### `berth update`

Pull latest images and restart selected services

```text
berth update [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Update a named service profile (full, infra, data, app, web, api)
- `--force-recreate`
- `--no-build`: Do not build images before starting containers
- `--wait`
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)

### `berth down`

Remove the instance's containers (`--purge` also deletes its data)

```text
berth down [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Down a named service profile (full, infra, data, app, web, api)
- `--no-deps`: Skip stopping workspace swarm dependencies
- `-f, --force`: Allow downing shared workspace dependencies
- `--timeout <TIMEOUT>`: Timeout in seconds for stop before remove [default: 30]
- `--parallel <PARALLEL>`: [default: 4]
- `--purge`: Also delete the instance's data: database volumes, logical databases and credentials. No confirmation; nothing is retained

### `berth stop`

Stop service container(s)

```text
berth stop [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Stop a named service profile (full, infra, data, app, web, api)
- `--timeout <TIMEOUT>`: Timeout in seconds for docker stop [default: 30]
- `--parallel <PARALLEL>`: [default: 4]

### `berth rm`

Remove service container(s)

```text
berth rm [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Remove a named service profile (full, infra, data, app, web, api)
- `-f, --force`
- `--parallel <PARALLEL>`: [default: 4]

### `berth recreate`

Destroy and recreate service container(s) from scratch

```text
berth recreate [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Recreate a named service profile (full, infra, data, app, web, api)
- `--wait`: Wait for service(s) to accept connections after recreating (enabled by default)
- `--no-wait`: Skip health waits during recreate
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)
- `-P, --publish-all`: Publish all exposed ports to random host ports after start
- `--no-deps`: Skip recreating workspace swarm dependencies
- `--env-output`: Write inferred service vars to local `.env`
- `--parallel <PARALLEL>`: [default: 4]

### `berth restart`

Restart service container(s)

```text
berth restart [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Restart a named service profile (full, infra, data, app, web, api)
- `--wait`
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)
- `--parallel <PARALLEL>`: [default: 4]

### `berth relabel`

Recreate containers to apply current Berth ownership labels

```text
berth relabel [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Relabel a named service profile (full, infra, data, app, web, api)
- `--wait`: Wait for service(s) to accept connections after relabeling
- `--wait-timeout <WAIT_TIMEOUT>`: Optional cap in seconds on readiness waits. Without it berth waits as long as the service is alive and fails only on a real error (the container stops, the framework hits a fatal error)
- `--parallel <PARALLEL>`: [default: 4]

### `berth url`

Print connection URL(s)

```text
berth url [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--driver <DRIVER>`
  - `mongodb`: `MongoDB`
  - `memcached`: `Memcached`
  - `postgres`: `PostgreSQL`
  - `mysql`: `MySQL`
  - `sqlserver`: `SQL Server`
  - `redis`: `Redis`
  - `valkey`: `Valkey`
  - `dragonfly`: `Dragonfly`
  - `garage`: `Garage`
  - `rustfs`: `RustFS`
  - `localstack`: `LocalStack`
  - `opensearch`: `OpenSearch`
  - `elasticsearch`: `Elasticsearch`
  - `meilisearch`: `Meilisearch`
  - `typesense`: `Typesense`
  - `frankenphp`: App runtime using `FrankenPHP`
  - `reverb`: Laravel Reverb WebSocket server
  - `horizon`: Laravel Horizon queue worker process
  - `scheduler`: Laravel scheduler worker process
  - `dusk`: Selenium standalone Chrome container for Laravel Dusk
  - `gotenberg`: Gotenberg document conversion API
  - `mailhog`: `MailHog` local email testing server
  - `rabbitmq`: `RabbitMQ` broker
  - `soketi`: `Soketi` WebSocket server

### `berth restore`

Restore a SQL file into a database service

```text
berth restore [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--file <FILE>`
- `--reset`: Drop and recreate the database before restoring
- `--migrate`: Run `php artisan migrate` after restore
- `--schema-dump`: Run `php artisan schema:dump` after restore
- `--gzip`: Treat input as gzip-compressed SQL

### `berth dump`

Dump a database service to a SQL file

```text
berth dump [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--file <FILE>`
- `--stdout`
- `--gzip`: Write gzip-compressed SQL output

### `berth ps`

List service runtime status

```text
berth ps [OPTIONS]
```

Options:

- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--driver <DRIVER>`
  - `mongodb`: `MongoDB`
  - `memcached`: `Memcached`
  - `postgres`: `PostgreSQL`
  - `mysql`: `MySQL`
  - `sqlserver`: `SQL Server`
  - `redis`: `Redis`
  - `valkey`: `Valkey`
  - `dragonfly`: `Dragonfly`
  - `garage`: `Garage`
  - `rustfs`: `RustFS`
  - `localstack`: `LocalStack`
  - `opensearch`: `OpenSearch`
  - `elasticsearch`: `Elasticsearch`
  - `meilisearch`: `Meilisearch`
  - `typesense`: `Typesense`
  - `frankenphp`: App runtime using `FrankenPHP`
  - `reverb`: Laravel Reverb WebSocket server
  - `horizon`: Laravel Horizon queue worker process
  - `scheduler`: Laravel scheduler worker process
  - `dusk`: Selenium standalone Chrome container for Laravel Dusk
  - `gotenberg`: Gotenberg document conversion API
  - `mailhog`: `MailHog` local email testing server
  - `rabbitmq`: `RabbitMQ` broker
  - `soketi`: `Soketi` WebSocket server

### `berth about`

Show project runtime overview

```text
berth about [OPTIONS]
```

Options:

- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)

### `berth health`

Check if service(s) are ready to accept connections

```text
berth health [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Check a named service profile (full, infra, data, app, web, api)
- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)
- `--timeout <TIMEOUT>`: Timeout in seconds [default: 30]
- `--interval <INTERVAL>`: Seconds between health checks [default: 2]
- `--retries <RETRIES>`: Maximum check attempts before failing
- `--parallel <PARALLEL>`: [default: 4]

### `berth env`

Print the managed environment containers receive (`.env` is never rewritten)

```text
berth env [OPTIONS] [COMMAND]
```

Options:

- `--service <SERVICE>`
- `--json`: Print the environment as JSON: `environment` (what app containers receive) and `host` (endpoints a host tool can reach)

#### `berth env generate`

Write the managed environment to a new file (never an existing one)

```text
berth env generate [OPTIONS] --output <OUTPUT>
```

Options:

- `--output <OUTPUT>`
- `--host`: Write the host view (endpoints a host tool can reach) instead of what app containers receive

### `berth logs`

Show container logs

```text
berth logs [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Tail logs for a named service profile (full, infra, data, app, web, api)
- `--all`
- `--prefix`: Prefix each log line with service name
- `-f, --follow`: Follow log output
- `--tail <TAIL>`
- `--since <SINCE>`
- `--until <UNTIL>`
- `-t, --timestamps`
- `--access`: Tail local Caddy access logs instead of container logs
- `--json`: Print one JSON object per line: `{"service", "stream", "line"}`

### `berth top`

Show running processes in container(s)

```text
berth top [OPTIONS] [ARGS]...
```

Arguments:

- `[ARGS]...`: Optional ps args passed to `<engine> top`

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)

### `berth stats`

Show a live stream of container resource usage

```text
berth stats [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--no-stream`: Disable streaming and show a single snapshot
- `--format <FORMAT>`

### `berth inspect`

Show low-level details for container(s)

```text
berth inspect [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--format <FORMAT>`
- `--output <OUTPUT>`: [default: table] [possible values: table, json]
- `--json`: Emit structured JSON array output
- `--size`
- `--type <OBJECT_TYPE>`

### `berth attach`

Attach local standard input/output/error streams to a running container

```text
berth attach [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--no-stdin`
- `--sig-proxy`
- `--detach-keys <DETACH_KEYS>`

### `berth cp`

Copy files/folders between host and container

```text
berth cp [OPTIONS] <SOURCE> <DESTINATION>
```

Arguments:

- `<SOURCE>`: Source path (`service:/path` or a host path)
- `<DESTINATION>`: Destination path (`service:/path` or a host path)

Options:

- `-L, --follow-link`: Follow symlinks in SRC_PATH
- `-a, --archive`: Copy all uid/gid information

### `berth kill`

Force-stop running container(s)

```text
berth kill [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--signal <SIGNAL>`
- `--parallel <PARALLEL>`: [default: 4]

### `berth pause`

Pause all processes in container(s)

```text
berth pause [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--parallel <PARALLEL>`: [default: 4]

### `berth unpause`

Unpause all processes in container(s)

```text
berth unpause [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--parallel <PARALLEL>`: [default: 4]

### `berth wait`

Block until container(s) stop and print exit status

```text
berth wait [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--condition <CONDITION>`
- `--parallel <PARALLEL>`: [default: 4]

### `berth events`

Stream container runtime events (Berth scope by default)

```text
berth events [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--since <SINCE>`
- `--until <UNTIL>`
- `--format <FORMAT>`: Runtime template passed to `<engine> events --format`
- `--json`: Emit newline-delimited JSON objects (`--format {{json .}}`)
- `--all`: Stream all daemon events instead of filtering to Berth containers
- `--allow-empty`: Return success when no services match current filters
- `--filter <FILTER>`
- `berth events`
- `berth events --service db`
- `berth events --all --filter type=container`

### `berth port`

List port mappings for container(s)

```text
berth port [OPTIONS] [PRIVATE_PORT]
```

Arguments:

- `[PRIVATE_PORT]`: Optional private port/protocol, for example `80/tcp`

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--format <FORMAT>`: [default: table] [possible values: table, json]
- `--json`: Emit structured JSON array output

### `berth prune`

Remove stopped Berth service containers (or all with --all)

```text
berth prune [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--parallel <PARALLEL>`: [default: 4]
- `--all`: Prune all stopped Docker containers (not only Berth-managed services), except another berth installation's
- `-f, --force`: Required with --all; confirms intentional global prune
- `--filter <FILTER>`: Additional Docker prune filters (global mode only; `label=` and `until=` only)
- `berth prune`
- `berth prune --kind database`
- `berth prune --all --force`

### `berth pull`

Pull latest service image(s)

```text
berth pull [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Pull a named service profile (full, infra, data, app, web, api)
- `--parallel <PARALLEL>`: [default: 4]

### `berth exec`

Run a command inside a service container

```text
berth exec [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Command and arguments to run

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth app-create`

Bootstrap Laravel app runtime (key, cache clear, migrate, storage link)

```text
berth app-create [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--no-migrate`
- `--seed`
- `--no-storage-link`

### `berth artisan`

Run php artisan inside a serve container

```text
berth artisan [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Artisan command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--browser`: Enable Playwright bootstrap for browser-style artisan test runs
- `--tty`
- `--no-tty`

### `berth composer`

Run composer inside an app container

```text
berth composer [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Composer command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth phpstan`

Run phpstan inside an app container

```text
berth phpstan [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth ecs`

Run ECS inside an app container

```text
berth ecs [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth php-cs-fixer`

Run php-cs-fixer inside an app container

```text
berth php-cs-fixer [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth psalm`

Run Psalm inside an app container

```text
berth psalm [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth pint`

Run Pint inside an app container

```text
berth pint [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth pest`

Run Pest inside an app container

```text
berth pest [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth phpunit`

Run PHPUnit inside an app container

```text
berth phpunit [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth rector`

Run Rector inside an app container

```text
berth rector [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Tool command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--tty`
- `--no-tty`

### `berth node`

Run Node package manager commands inside an app container

```text
berth node [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Package manager command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--package-manager <PACKAGE_MANAGER>`: [possible values: npm, pnpm, yarn]
- `--version-manager <VERSION_MANAGER>`: [possible values: system, fnm, nvm, volta]
- `--node-version <NODE_VERSION>`
- `--tty`
- `--no-tty`

### `berth bun`

Run Bun inside an app container

```text
berth bun [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Bun command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--bun-version <BUN_VERSION>`
- `--tty`
- `--no-tty`

### `berth deno`

Run Deno inside an app container

```text
berth deno [OPTIONS] [COMMAND]...
```

Arguments:

- `[COMMAND]...`: Deno command and arguments

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--deno-version <DENO_VERSION>`
- `--tty`
- `--no-tty`

### `berth task`

Run opinionated internal task workflows

```text
berth task [OPTIONS] <COMMAND>
```

#### `berth task deps`

Run dependency-management workflows

```text
berth task deps [OPTIONS] <COMMAND>
```

##### `berth task deps bump`

Bump app dependency manifests and refresh locks

```text
berth task deps bump [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--package-manager <PACKAGE_MANAGER>`: Override inferred Node package manager [possible values: npm, pnpm, yarn]
- `--version-manager <VERSION_MANAGER>`: Override the Node version manager used for Node workflows [possible values: system, fnm, nvm, volta]
- `--node-version <NODE_VERSION>`: Override the Node version used for Node workflows
- `--tty`
- `--no-tty`
- `--composer`: Run the Composer dependency bump workflow
- `--node`: Run the Node dependency bump workflow
- `--bun`: Run the Bun dependency bump workflow
- `--deno`: Run the Deno dependency bump workflow
- `--all`: Run all dependency bump workflows

##### `berth task deps audit`

Audit app dependencies for known vulnerabilities

```text
berth task deps audit [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--package-manager <PACKAGE_MANAGER>`: Override inferred Node package manager [possible values: npm, pnpm, yarn]
- `--version-manager <VERSION_MANAGER>`: Override the Node version manager used for Node workflows [possible values: system, fnm, nvm, volta]
- `--node-version <NODE_VERSION>`: Override the Node version used for Node workflows
- `--tty`
- `--no-tty`
- `--composer`: Run the Composer dependency bump workflow
- `--node`: Run the Node dependency bump workflow
- `--bun`: Run the Bun dependency bump workflow
- `--deno`: Run the Deno dependency bump workflow
- `--all`: Run all dependency bump workflows

##### `berth task deps normalize`

Normalize dependency manifests and lockfiles

```text
berth task deps normalize [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--package-manager <PACKAGE_MANAGER>`: Override inferred Node package manager [possible values: npm, pnpm, yarn]
- `--version-manager <VERSION_MANAGER>`: Override the Node version manager used for Node workflows [possible values: system, fnm, nvm, volta]
- `--node-version <NODE_VERSION>`: Override the Node version used for Node workflows
- `--tty`
- `--no-tty`
- `--composer`: Run the Composer dependency bump workflow
- `--node`: Run the Node dependency bump workflow
- `--bun`: Run the Bun dependency bump workflow
- `--deno`: Run the Deno dependency bump workflow
- `--all`: Run all dependency bump workflows

##### `berth task deps install`

Install app dependencies from declared manifests

```text
berth task deps install [OPTIONS]
```

Options:

- `--service <SERVICE>`
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--package-manager <PACKAGE_MANAGER>`: Override inferred Node package manager [possible values: npm, pnpm, yarn]
- `--version-manager <VERSION_MANAGER>`: Override the Node version manager used for Node workflows [possible values: system, fnm, nvm, volta]
- `--node-version <NODE_VERSION>`: Override the Node version used for Node workflows
- `--tty`
- `--no-tty`
- `--composer`: Run the Composer dependency bump workflow
- `--node`: Run the Node dependency bump workflow
- `--bun`: Run the Bun dependency bump workflow
- `--deno`: Run the Deno dependency bump workflow
- `--all`: Run all dependency bump workflows

### `berth ls`

List every instance this installation owns, across projects (`--services` lists the current project's services)

```text
berth ls [OPTIONS]
```

Options:

- `--format <FORMAT>`: `table` or `json` [default: table] [possible values: table, json]
- `--json`: Print JSON (the same as `--format json`)
- `--services`: List the current project's configured services instead of instances (berth 7's `ls`)
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--driver <DRIVER>`
  - `mongodb`: `MongoDB`
  - `memcached`: `Memcached`
  - `postgres`: `PostgreSQL`
  - `mysql`: `MySQL`
  - `sqlserver`: `SQL Server`
  - `redis`: `Redis`
  - `valkey`: `Valkey`
  - `dragonfly`: `Dragonfly`
  - `garage`: `Garage`
  - `rustfs`: `RustFS`
  - `localstack`: `LocalStack`
  - `opensearch`: `OpenSearch`
  - `elasticsearch`: `Elasticsearch`
  - `meilisearch`: `Meilisearch`
  - `typesense`: `Typesense`
  - `frankenphp`: App runtime using `FrankenPHP`
  - `reverb`: Laravel Reverb WebSocket server
  - `horizon`: Laravel Horizon queue worker process
  - `scheduler`: Laravel scheduler worker process
  - `dusk`: Selenium standalone Chrome container for Laravel Dusk
  - `gotenberg`: Gotenberg document conversion API
  - `mailhog`: `MailHog` local email testing server
  - `rabbitmq`: `RabbitMQ` broker
  - `soketi`: `Soketi` WebSocket server

### `berth gc`

Reap expired and orphaned instances, then remove unused shared servers

```text
berth gc [OPTIONS]
```

Options:

- `--json`: Print one JSON document describing what was removed

### `berth swarm`

Run a berth command across configured swarm targets

```text
berth swarm [OPTIONS] <COMMAND>...
```

Arguments:

- `<COMMAND>...`: Command to run, e.g. `up`, `down`, `ps --format json`

Options:

- `--only <ONLY>`: Restrict to a comma-delimited subset of swarm target names
- `--no-deps`: Run only selected targets and skip swarm dependencies
- `-f, --force`: Allow downing dependencies also required by non-selected targets
- `--parallel <PARALLEL>`: Number of projects to process in parallel [default: 4]
- `--fail-fast`: Stop after first failure; in parallel mode remaining targets are cancelled
- `--port-strategy <PORT_STRATEGY>`: Strategy for assigning ports during `swarm up` [default: random] [possible values: random, stable]
- `--port-seed <PORT_SEED>`: Seed used by `--port-strategy stable`
- `--env-output`: Write inferred service vars to local `.env` during `swarm up`

### `berth completions`

Generate shell completions

```text
berth completions [OPTIONS] <SHELL>
```

Arguments:

- `<SHELL>`: [possible values: bash, elvish, fish, powershell, zsh]

### `berth serve`

Start an app serve target and publish its gateway route

```text
berth serve [OPTIONS]
```

Options:

- `--service <SERVICE>`: App service name to serve
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--recreate`: Recreate the app container before starting serve
- `--detached`: Run foreground serve process in detached container mode
- `--env-output`: Write inferred service vars to local `.env`
- `--trust-container-ca`: Trust the inner serve container CA in system trust store

### `berth open`

Print/open serve URL and show app/database health summary

```text
berth open [OPTIONS]
```

Options:

- `--service <SERVICE>`: App service name to inspect/open
- `--kind <KIND>`
  - `database`: SQL databases
  - `cache`: Cache backends
  - `object-store`: S3-compatible object storage
  - `search`: Search engines
  - `app`: Application runtimes (web apps, workers, tooling)
- `--profile <PROFILE>`: Select a service profile (full, infra, data, app, web, api)
- `--all`
- `--health-path <HEALTH_PATH>`
- `--no-browser`
- `--database`: Open database connection URL(s) via the platform opener
- `--json`: Print machine-readable JSON summary

### `berth share`

Share an app service through an external tunnel provider

```text
berth share [OPTIONS] <COMMAND>
```

#### `berth share start`

Start a sharing tunnel for an app service

```text
berth share start [OPTIONS]
```

Options:

- `--service <SERVICE>`: App service name to expose
- `--provider <PROVIDER>`: Share provider runtime [possible values: cloudflare, expose, tailscale]
- `--cloudflare`: Shorthand for `--provider cloudflare`
- `--expose`: Shorthand for `--provider expose`
- `--tailscale`: Shorthand for `--provider tailscale`
- `--detached`: Keep process running in background and persist session metadata
- `--timeout <TIMEOUT>`: Max seconds to wait for public URL verification before failing [default: 30]
- `--json`: Print machine-readable JSON output

#### `berth share status`

Show tracked sharing tunnel sessions

```text
berth share status [OPTIONS]
```

Options:

- `--service <SERVICE>`: Filter by app service name
- `--provider <PROVIDER>`: Filter by provider [possible values: cloudflare, expose, tailscale]
- `--cloudflare`: Shorthand for `--provider cloudflare`
- `--expose`: Shorthand for `--provider expose`
- `--tailscale`: Shorthand for `--provider tailscale`
- `--json`: Print machine-readable JSON output

#### `berth share stop`

Stop tracked sharing tunnel sessions

```text
berth share stop [OPTIONS]
```

Options:

- `--service <SERVICE>`: Filter by app service name
- `--provider <PROVIDER>`: Filter by provider [possible values: cloudflare, expose, tailscale]
- `--cloudflare`: Shorthand for `--provider cloudflare`
- `--expose`: Shorthand for `--provider expose`
- `--tailscale`: Shorthand for `--provider tailscale`
- `--all`: Stop all tracked sessions
- `--json`: Print machine-readable JSON output

### `berth env-scrub`

Scrub sensitive .env values with safe local placeholders

```text
berth env-scrub [OPTIONS]
```

Options:

- `--env-file <ENV_FILE>`

