# Usage

This document describes every command and flag exposed by `berth`.

## CLI Shape

```bash
berth [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]
```

## Global Options

These apply to all commands.

- `-q, --quiet`
  - Reduce output/noise.
- `--no-color`
  - Disable colored output.
- `--dry-run`
  - Print planned actions without changing containers/config where supported.
- `--config <PATH>`
  - Use an explicit `.berth.toml` or `.berth.yaml` path.
  - Conflicts with `--project-root`.
- `--project-root <DIR>`
  - Resolve `.berth.toml` or `.berth.yaml` from a specific directory.
- `--env <NAME>`
  - Runtime namespace (for example `testing` / `test`).
- `--engine <docker|podman>`
  - Override container runtime engine for this invocation.
  - When omitted, Berth uses `container_engine` from config, then defaults to
    `docker`.
- `--repro`
  - Enable reproducibility mode (lockfile + deterministic checks).
- `--non-interactive`
  - Disable interactive behavior (for example: browser auto-open and TTY usage).

## Runtime Engine

Set a project default engine in `.berth.toml` or `.berth.yaml`:

```toml
container_engine = "docker" # or "podman"
```

Notes:

- Berth defaults to `docker` when `container_engine` is not set.
- Podman support covers core Docker-compatible CLI flows.
- Some advanced runtime/network behavior can differ by engine and host setup.

## Domain Strategy

Set a project default app-domain strategy in `.berth.toml` or
`.berth.yaml`:

```toml
domain_strategy = "directory" # or "random"
```

Rules:

- `directory` uses the kebab-case project directory name as the base label.
- `random` uses a stable project-specific `berth-<hash>` base label.
- Berth only auto-generates domains for app services that do not already define
  `domain` or `domains`.
- The generated app service named `app` uses `<base>.berth`.
- Other app services use `<base>-<service>.berth`.

Example for `/Users/brian/Developer/my-project`:

```toml
domain_strategy = "directory"

[[service]]
preset = "laravel"

[[service]]
preset = "gotenberg"

[[service]]
preset = "mailhog"
```

This resolves to:

- `my-project.berth`
- `my-project-gotenberg.berth`
- `my-project-mailhog.berth`

## Common Selectors

Many commands support these selectors:

- `--service <NAME>`: one named service.
- `--kind <KIND>`: filter by service kind.
- `--driver <DRIVER>`: filter by backend driver.

If `--service` is omitted, commands operate on all matching services.

## Value Enums

### `KIND`

- `database`
- `cache`
- `object-store`
- `search`
- `app`

### `DRIVER`

- `postgres`
- `mongodb`
- `mysql`
- `sqlserver`
- `redis`
- `valkey`
- `dragonfly`
- `memcached`
- `minio`
- `garage`
- `localstack`
- `rustfs`
- `meilisearch`
- `typesense`
- `frankenphp`
- `reverb`
- `horizon`
- `scheduler`
- `dusk`
- `gotenberg`
- `mailhog`
- `rabbitmq`
- `soketi`

### Pull Policy (`--pull`)

- `always`
- `missing` (default)
- `never`

### Restart Policy (`restart`)

- `no`
- `on-failure`
- `always`
- `unless-stopped` (default when `restart` is omitted)

### Port Strategy (`--port-strategy`)

- `random` (default)
- `stable` (uses `--port-seed` if provided)

### Node Package Manager (`berth node --package-manager`)

- `npm`
- `pnpm`
- `yarn`

### Node Version Manager (`berth node --version-manager`)

- `system` (default)
- `fnm`
- `nvm`
- `volta`

### JavaScript Runtime (`[service.javascript].runtime`)

- `node` (default)
- `bun`
- `deno`

### Container Engine (`--engine`)

- `docker` (default)
- `podman`

## Top-Level Commands

### `berth init`

Initialize a new `.berth.toml` in the current directory.

- New configs default `domain_strategy` to `directory`.
- The generated template omits explicit app `domain` entries and relies on the
  configured strategy instead.

### `berth config [--format <toml|json>] [migrate]`

- Without subcommand: print resolved config.
- `--format <FORMAT>`: output format (`toml` default, `json` supported).
- `berth config migrate`: migrate local config schema to latest supported version.

### `berth daemon <start|watch|status|stop|logs>`

Manage a per-project Berth daemon target by explicit path instead of the
current working directory.

Flags:

- `--path <DIR>` (required)

Notes:

- `--path` may point at the Berth project root or any nested directory inside
  that project.
- Daemon commands resolve `.berth.toml` from the explicit path before regular
  config loading, so they do not depend on the caller's current directory.
- `daemon start` persists per-project session metadata and a log path under
  `~/.config/berth/daemon/`.
- `daemon watch --dir <DIR>` scans one or more parent directories for
  `.berth.toml` projects and starts missing per-project daemons.
- `daemon watch --exclude-dir <DIR>` skips specific subtrees inside watched
  roots.
- `daemon watch --max-projects <N>` caps how many discovered projects Berth
  will auto-start from one watch pass.
- `daemon watch --once` runs one discovery pass and exits.
- `daemon watch --interval <SECONDS>` controls the repeat scan delay when
  `--once` is not set.
- `daemon status`, `stop`, and `logs` read that persisted session state instead
  of inferring daemon ownership from the current shell process.
- The daemon child bootstraps the project through Berth's existing `start`
  flow, then polls managed services and reruns `up` if any service container is
  missing or no longer `running`.
- Repeated recovery failures use exponential backoff before retrying.
- Watch mode deduplicates overlapping watch roots, ignores nested child
  projects under an already managed project root, and reports invalid
  `.berth.toml` files without stopping discovery for valid sibling projects.
- When `--max-projects` is set, extra discovered projects are skipped instead
  of being auto-started, which keeps broad watch roots from starting an
  unbounded number of daemons.

### `berth daemon service <install|status|print|uninstall>`

Manage a login-time watch service backed by the local user service manager.

Flags:

- `install --dir <DIR>` (repeatable, required)
- `install --exclude-dir <DIR>` (repeatable)
- `install --max-projects <N>`
- `install --interval <SECONDS>` (default: `30`)
- `print --dir <DIR>` (repeatable, required)
- `print --exclude-dir <DIR>` (repeatable)
- `print --max-projects <N>`
- `print --interval <SECONDS>` (default: `30`)

Notes:

- On macOS Berth installs a `launchd` user agent under
  `~/Library/LaunchAgents/`.
- On Linux Berth installs a `systemd --user` unit under
  `~/.config/systemd/user/`.
- `install` writes the rendered service definition, enables it for the current
  user, and starts it immediately.
- Installed services preserve the same watch policy flags as interactive
  `berth daemon watch`, including exclusions and max project limits.
- `status` reports whether the service definition is currently installed.
- `print` shows the rendered unit/plist without writing it.
- `uninstall` stops the installed service and removes its definition file.

### `berth preset <SUBCOMMAND>`

- `berth preset list`: list available preset names.
- `berth preset show <NAME> [--format <toml|json>]`: show resolved defaults.

### `berth profile <SUBCOMMAND>`

- `berth profile list`: list built-in profile names.
- `berth profile show <NAME> [--format <FORMAT>]`: show services in profile.
  - `json`: structured JSON
  - `markdown`: markdown table
  - other values/default (`table`): plain tab-separated output

Built-in profiles include: `full`, `all`, `infra`, `data`, `app`, `web`, `api`.

### `berth doctor [--fix] [--repro] [--reachability]`

Validate local setup and configuration health.

- `--fix`: attempt automatic fixes where possible.
- `--repro`: run reproducibility checks.
- `--reachability`: probe app URLs and health endpoints.
- `--format <FORMAT>` (`table` default, `json` supported)

### `berth lock <SUBCOMMAND>`

- `berth lock images`: resolve configured images to immutable digests.
- `berth lock verify`: verify lockfile exists and is in sync.
- `berth lock diff`: preview lockfile changes.

### `berth task deps bump`

Run opinionated dependency bump workflows for Composer and selected
JavaScript runtimes.

### `berth task deps audit`

Run dependency vulnerability audits for Composer and selected
JavaScript runtimes.

### `berth task deps normalize`

Normalize dependency manifests and lockfiles for Composer and selected
JavaScript runtimes.

### `berth task deps install`

Install dependencies for Composer and selected JavaScript runtimes.

### `berth setup`

Prepare services before startup.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth start`

Run doctor checks, start selected services, then open app URL summaries.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--no-wait` (conflicts with `--wait`, default behavior)
- `--wait-timeout <SECONDS>` (default: `30`)
- `--pull <always|missing|never>` (default: `missing`)
- `--force-recreate`
- `--no-open`
- `--health-path <PATH>`
- `--no-deps`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth up`

Start service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait` (enabled by default behavior)
- `--no-wait` (conflicts with `--wait`)
- `--wait-timeout <SECONDS>` (default: `30`)
- `--pull <always|missing|never>` (default: `missing`)
- `--force-recreate`
- `-P, --publish-all` (enabled by default behavior)
- `--no-publish-all` (conflicts with `--publish-all`)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--no-deps`
- `--seed`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth apply`

Converge services and apply configured seed files.

Flags:

- `--no-deps`

### `berth update`

Pull and restart selected services.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--force-recreate`
- `--no-build`
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)

### `berth down`

Stop and remove services.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

## Service Hooks

Define per-service lifecycle hooks in `.berth.toml` with `[[service.hook]]`.
Supported phases are `post_up`, `pre_down`, and `post_down`.

```toml
[[service]]
name = "app"
preset = "laravel"

[[service.hook]]
name = "seed-dev-user"
phase = "post_up"
on_error = "fail" # fail | warn

[service.hook.run]
type = "exec"
argv = ["php", "artisan", "db:seed", "--class=DevUserSeeder"]
```

`run.type = "exec"` runs inside the selected service container.
`run.type = "script"` runs a host script (relative paths are resolved from
the Berth project root).

## Service Restart Policy

Berth applies Docker restart policies to long-lived service containers.
When `restart` is omitted, Berth uses `unless-stopped` so services come
back after Docker restarts or laptop sleep without requiring a manual
morning `berth up`.

Override per service in `.berth.toml` when a container should opt out or
use a stricter Docker policy:

```toml
[[service]]
preset = "laravel"
restart = "unless-stopped"

[[service]]
name = "browser"
preset = "dusk"
restart = "no"
```

Berth passes the configured value through to Docker `run --restart ...`.

### `berth stop`

Stop services without removing containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth rm`

Remove service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `-f, --force`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth recreate`

Destroy and recreate service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait` (default behavior)
- `--no-wait` (conflicts with `--wait`)
- `--wait-timeout <SECONDS>` (default: `30`)
- `-P, --publish-all`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth restart`

Restart service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth relabel`

Recreate containers to apply current Berth ownership labels.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth url`

Print service connection URLs.

Flags:

- `--service <NAME>`
- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): plain text URL output
- `--kind <KIND>`
- `--driver <DRIVER>`

### `berth restore`

Restore SQL data into a database service.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--reset`
- `--migrate`
- `--schema-dump`
- `--gzip`

### `berth dump`

Dump a database service to SQL.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--stdout`
- `--gzip`

### `berth ps`

Show runtime status for services.

Flags:

- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): human status view
- `--kind <KIND>`
- `--driver <DRIVER>`

### `berth about`

Show runtime project overview.

Flags:

- `--format <FORMAT>` (`table` default, `json` supported)

### `berth health`

Run health checks against selected services.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>` (`table` default, `json` supported)
- `--timeout <SECONDS>` (default: `30`)
- `--interval <SECONDS>` (default: `2`)
- `--retries <N>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth env [generate]`

Manage `.env` values based on resolved/running services.

Main command flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--env-file <PATH>`
- `--sync`
- `--purge`
- `--persist-runtime` (requires `--sync`)
- `--create-missing`

Subcommands:

- `berth env generate --output <PATH>`
  - Generate a full env file from managed Berth app variables.

### `berth logs`

Show container logs.

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service`, `--kind`, and `--all`)
- `--all` (conflicts with `--service`)
- `--prefix`
- `-f, --follow`
- `--tail <N>`
- `--since <VALUE>`
- `--until <VALUE>`
- `-t, --timestamps`
- `--access` (tail local Caddy access logs instead)

### `berth top [ARGS...]`

Show running processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- Trailing `ARGS...` are passed to `<engine> top` (for example: `aux`).

### `berth stats`

Show a live stream of container resource usage.

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--no-stream` (single snapshot mode)
- `--format <FORMAT>` (passed to Docker stats format)

### `berth inspect`

Show low-level details for container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>`
- `--output <FORMAT>` (`table` default, `json` supported)
- `--json` (structured JSON array)
- `--size`
- `--type <OBJECT_TYPE>`

### `berth attach`

Attach local standard input/output/error streams to a running container.

Flags:

- `--service <NAME>`
- `--no-stdin`
- `--sig-proxy`
- `--detach-keys <KEYS>`

### `berth cp <SOURCE> <DESTINATION>`

Copy files/folders between host and container.

`SOURCE` and `DESTINATION` can be host paths, `service:/path`, or
`container:/path`.

Flags:

- `-L, --follow-link`
- `-a, --archive`

### `berth kill`

Force-stop running container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--signal <SIGNAL>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth pause`

Pause all processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth unpause`

Unpause all processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth wait`

Block until container(s) stop and print exit status.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--condition <CONDITION>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth events`

Stream Docker daemon events (Berth container scope by default).

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--since <VALUE>`
- `--until <VALUE>`
- `--format <FORMAT>`
- `--json` (newline-delimited JSON objects)
- `--all` (disable Berth-only event scoping)
- `--allow-empty`
- `--filter <KEY=VALUE>` (repeatable)

### `berth port [PRIVATE_PORT]`

List port mappings for container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>` (`table` default, `json` supported)
- `--json` (structured JSON array)
- Optional positional `PRIVATE_PORT` (for example `80/tcp`)

### `berth prune`

Remove stopped Berth service containers (or all with `--all`).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))
- `--all` (global Docker prune scope)
- `-f, --force` (required with `--all`)
- `--filter <KEY=VALUE>` (global mode only)

### `berth pull`

Pull service images.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `berth exec [-- <COMMAND...>]`

Run a command inside a service container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing command is optional after flags.
- If no command is provided, Berth opens an interactive shell session.

### `berth app-create`

Bootstrap Laravel runtime tasks.

Flags:

- `--service <NAME>`
- `--no-migrate`
- `--seed`
- `--no-storage-link`

### `berth artisan -- <COMMAND...>`

Run `php artisan` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Artisan command/args.

### `berth composer -- <COMMAND...>`

Run `composer` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Composer command/args.

### `berth phpstan -- <COMMAND...>`

Run `phpstan` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHPStan command/args.

### `berth ecs -- <COMMAND...>`

Run `ecs` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing ECS command/args.

### `berth php-cs-fixer -- <COMMAND...>`

Run `php-cs-fixer` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHP CS Fixer command/args.

### `berth psalm -- <COMMAND...>`

Run `psalm` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Psalm command/args.

### `berth pint -- <COMMAND...>`

Run `pint` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Pint command/args.

### `berth pest -- <COMMAND...>`

Run `pest` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Pest command/args.

### `berth phpunit -- <COMMAND...>`

Run `phpunit` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHPUnit command/args.

### `berth rector -- <COMMAND...>`

Run `rector` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Rector command/args.

### `berth node -- <COMMAND...>`

Run Node package manager commands inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--package-manager <npm|pnpm|yarn>` (optional override)
- `--version-manager <system|fnm|nvm|volta>` (optional override)
- `--node-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing package-manager command/args.

Node package-manager resolution order:

- CLI overrides
- `[service.javascript]` in `.berth.toml`
- Project files:
  `.nvmrc`, `.node-version`, `package.json.packageManager`,
  `package.json.volta.node`, and `package.json.engines.node`
- Berth defaults (`system` version manager)

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "node"
package_manager = "pnpm"
version_manager = "fnm"
version = "22"
```

### `berth deno -- <COMMAND...>`

Run Deno inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--deno-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing Deno command/args.

Deno resolution order:

- CLI `--deno-version`
- `[service.javascript]` in `.berth.toml`
- Project files: `deno.json`, `deno.jsonc`, or `deno.lock`
- Berth default Deno installer version

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "deno"
version = "2.2.3"
```

### `berth bun -- <COMMAND...>`

Run Bun inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--bun-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing Bun command/args.

Bun resolution order:

- CLI `--bun-version`
- `[service.javascript]` in `.berth.toml`
- Project files: `bun.lock`, `bun.lockb`, or `package.json.packageManager`
- Berth default Bun installer version

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "bun"
version = "1.2.5"
```

### `berth task deps <bump|audit|normalize|install>`

Run opinionated dependency workflows inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--composer`
- `--node`
- `--bun`
- `--deno`
- `--all` (runs all workflows; conflicts with `--composer`, `--node`, `--bun`, and `--deno`)
- `--package-manager <npm|pnpm|yarn>` (optional override)
- `--version-manager <system|fnm|nvm|volta>` (optional override)
- `--node-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`

Notes:

- At least one target flag is required: `--composer`, `--node`, `--bun`,
  `--deno`, or `--all`.
- `bump` runs:
  - Composer: `composer bump --dev-only`, `composer bump --no-dev-only`,
    `composer update --ignore-platform-reqs`, then `composer normalize`
  - Node: manager-specific upgrade flow against `package.json`
  - Bun: `bun update --latest`
  - Deno: `deno outdated --update --latest`
- `audit` runs:
  - Composer: `composer audit`
  - Node: manager-specific audit flow
  - Bun: `bun audit`
  - Deno: currently skipped with a warning because Berth does not define a
    Deno dependency-audit equivalent yet
- `normalize` runs:
  - Composer: `composer normalize`
  - Node: manager-specific lockfile normalization flow
  - Bun: `bun install`
  - Deno: currently skipped with a warning because Berth does not define a
    Deno dependency-normalize equivalent yet
- `install` runs:
  - Composer: `composer install`
  - Node: manager-specific install flow
  - Bun: `bun install`
  - Deno: currently skipped with a warning because Berth does not define a
    Deno dependency-install equivalent yet
- Node workflow targets infer the package manager from
  `package.json.packageManager` first, then lockfiles, when
  `--package-manager` is omitted.
- `--all` runs Composer, Node, Bun, and Deno workflows; missing manifests
  and unsupported Deno actions are skipped with a warning.
- Non-system Node version managers require a concrete Node version from
  `--node-version`, `[service.javascript].version`, or project files such as
  `.nvmrc` or `.node-version`.
- Missing `composer.json`, `package.json`, or Deno project files are
  skipped with a warning.

### `berth ls`

List every instance this installation owns, across projects: project,
instance ID, status (`running`, `partial`, `stopped`, `down`), lease expiry
and owning checkout. Runs outside any project.

Flags:

- `--json` or `--format json`: one document,
  `{"instances": [{"project", "instance", "root", "root_exists", "status",
  "containers", "networks", "running", "ttl_seconds", "expires_at",
  "expires_in_seconds", "expired", "lease_error", "owner_pid",
  "owner_alive"}]}`
- `--services`: list the current project's configured services instead
  (one name per line, or a JSON array with `--format json`), with optional
  `--kind <KIND>` and `--driver <DRIVER>` filters

### `berth gc`

Reap now (instances whose lease expired, whose owner process exited or
whose checkout is gone, and leftover test-slot containers), then remove
shared servers that no instance
network uses and that hold no tenant: the container, its data volume and its
admin secret. A server berth cannot inspect is kept and reported. Runs
outside any project, so it uses Docker unless `--engine` says otherwise
(as does `berth ls`). Exits 125 when an instance could not be reaped.

Flags:

- `--json`: one document, `{"reaped_instances", "removed_test_containers",
  "servers": [{"server", "engine", "removed", "reason"}]}`

### `berth swarm -- <COMMAND...>`

Run a Berth command across workspace swarm targets.

Flags:

- `--only <name1,name2,...>`
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))
- `--fail-fast` (parallel-safe fail-fast; remaining targets are cancelled)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--env-output`
- Trailing command is required (examples: `up`, `down`, `ps --format json`).

### `berth completions <SHELL>`

Generate shell completion scripts.

`<SHELL>` is one of clap-complete supported targets (for example `bash`,
`zsh`, `fish`, `powershell`, `elvish`).

### `berth serve`

Start and expose an app service through local HTTPS routing.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--recreate`
- `--detached`
- `--env-output`
- `--trust-container-ca`

### `berth open`

Print or open serve URLs and health summary.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service`, `--kind`, and `--all`)
- `--all` (conflicts with `--service`)
- `--health-path <PATH>`
- `--no-browser`
- `--json`

### `berth share <SUBCOMMAND>`

Expose an app service through a supported tunnel provider.

- `berth share start (--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale) [--service <NAME>] [--detached] [--timeout <SECONDS>] [--json]`
- `berth share status [--service <NAME>] [--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale] [--json]`
- `berth share stop [--all] [--service <NAME>] [--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale] [--json]`

Notes:

- `share start` requires provider binaries on `PATH` (`cloudflared`, `expose`, or `tailscale`).
- `--detached` keeps the provider process running in background.
- Session state is persisted under `~/.config/berth/share/`.

### `berth env-scrub`

Scrub sensitive `.env` values and replace with local-safe placeholders.

Flags:

- `--env-file <PATH>`
