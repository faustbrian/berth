# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Added

- Added instances: every checkout of a project is its own environment. The
  instance ID (`{slug}-{hash6}`) is derived from the canonical project root,
  so parallel git worktrees never share containers or named volumes, and
  `--instance <id>` or `BERTH_INSTANCE` selects an explicit ID for the
  current project (swarm dependencies keep their own instances). Containers
  are named `berth_{project}_{instance}_{service}`, and service, app and
  one-off containers carry `dev.berth.*` ownership labels for their project,
  instance and root.
- Added the berth gateway: one Caddy container per installation serves every
  instance at `https://{instance}-{project}[-{service}].berth.localhost:8443`
  (ports via `BERTH_GATEWAY_HTTPS_PORT`/`BERTH_GATEWAY_HTTP_PORT`, bound to
  `127.0.0.1` only, never replaced by another port). Routes are rebuilt from
  container labels under a host lock, unknown hosts get a 404, HTTP redirects
  to HTTPS, the CA persists in `~/.berth/gateway/data` across gateway
  recreation, and route names resolve to the gateway inside instance networks.
  Only an instance's default environment is routed (`--env` namespaces and
  test leases keep direct local URLs), every routed service keeps a unique
  instance route (also its `APP_URL`) with authored domains as extra routes, a
  host claimed twice is withheld from both claimants without affecting other
  routes, the gateway ports are recorded once per installation, and `down`
  never starts the gateway. `logs --access` shows the gateway's log.
- Added per-instance Docker networks: every instance (and every `--env`
  runtime namespace in it) gets its own bridge network with an explicit `/24`
  from `10.128.0.0/9`, allocated under a host lock so 30 parallel instances
  never exhaust Docker's default address pools. Services join under their
  service name, the app reaches them by name and container port
  (`DB_HOST=db`), instances cannot reach each other, and `down` removes the
  network once no container uses it.
- Added the berth home directory (`~/.berth`, or `BERTH_HOME`) with a random
  installation ID that labels berth's networks, and host file locks that the
  kernel releases when a berth process dies.
- Added `docs/v9/architecture.md`, the berth 9.x design for instances,
  ownership, leases, shared servers, the gateway and exit codes.
- Added the berth 9.x ledger (`docs/v9/ledger.md`), which records one Port,
  Reimplement, Drop or Defer decision for every stackctl 8.x behavior, and
  `scripts/check-v9-ledger.sh`, which fails when any 8.x source directory or
  commit is left unaccounted for.

- Added `berth config migrate`, which converts helm `.helm.toml`/`.helm.yaml`,
  stackctl `.stackctl.toml`/`.stackctl.yaml` (including 8.x
  `schema_version: 8` files) and their runtime-environment overrides into
  berth 9 files. It runs before any project file is loaded, refuses to
  overwrite existing files, writes nothing unless every conversion succeeds,
  keeps service declaration order (which decides default ports and the
  primary `DB_*` service), and rejects fields that have no exact berth 9
  equivalent, including `domain_strategy = "random"` and unknown schema
  versions. 8.x preset-only services are pinned to the exact image the 8.x
  catalog resolved (for example `postgres:18`), so migration never changes a
  database major version. Legacy lockfiles are reported for regeneration, not
  converted.
- App services without an explicit `domain` now always get a generated
  directory-based domain; projects that relied on having no generated domain
  (7.0 configs without `domain_strategy`) see a new `APP_URL`.

- Added a supply-chain gate (`just audit-supply-chain`): `cargo audit` and
  `cargo deny` (advisories, licenses, duplicate versions and sources) must
  pass with warnings denied.

### Changed

- YAML is now parsed with the maintained, safe-Rust `serde-saphyr` instead of
  the deprecated `serde_yaml`/`unsafe-libyaml`. Project files and lockfiles
  must be regular files of at most 1 MiB (verified by inode after opening),
  and anchors, aliases, merge keys, duplicate keys, tags, multiple documents,
  nesting deeper than 32 levels, documents over 10,000 nodes and YAML 1.1
  boolean words such as `yes`/`off` are rejected. `config migrate` reads
  legacy YAML with YAML 1.2 rules, so unquoted values like `off`, `no` and
  `0022` keep their exact text. The lockfile is written atomically and never
  through a symbolic link.
- Removed the unused `tabled` dependency (and its unmaintained
  `proc-macro-error2`) and updated locked dependencies past the
  `crossbeam-epoch` and `anyhow` advisories.
- Berth reads only `.berth.yaml` project files using the strict
  `schema_version: 9` schema: a required `project` name (lowercase DNS label
  of at most 24 bytes) replaces `container_prefix`, services are a `services`
  map keyed by name, and unknown fields or duplicate keys are rejected.
  Runtime overrides are `.berth.<env>.yaml` and the lockfile is
  `.berth.lock.yaml`. A legacy helm, stackctl or TOML project file fails with
  an error naming `berth config migrate` instead of being read.
- `berth init` writes a `schema_version: 9` `.berth.yaml` starter file.
- Commands berth runs in containers (`exec`, `artisan`, `composer`, PHP
  tools, `node`, `bun`, `deno`, one-off commands) now set berth's exit code to
  the command's exact exit code, or `128 + signal` when it was killed; a
  failing `artisan test` keeps its code even when test-runtime cleanup also
  fails, and `swarm` exits with the first failing target's code. Berth's own
  failures exit `125`, including usage errors, `exec` against a stopped or
  missing instance and failing internal steps such as hooks; errors are
  printed to stderr.
- Progress and log events are written to stderr, so stdout carries only
  command output and machine-readable data such as `--format json`.
- `artisan` adds `--ansi` only when the command runs with a TTY and keeps an
  explicit `--ansi` or `--no-ansi`, so agents reading pipes get plain text;
  log lines are colored only when stderr is a terminal.
- `container_name` is no longer accepted in project files (berth names
  containers per instance), service names must be lowercase DNS labels, and
  named volumes of non-application services are namespaced per instance;
  host-path mounts are unchanged. Migrated 6/7 projects therefore start with
  fresh data volumes, which `config migrate` reports.
- Renamed the project from Stackctl to Berth: the crate and binary are now
  `berth`, configuration files use the `.berth.*` prefix, environment
  variables use `BERTH_*`, Docker labels use the `dev.berth.*` namespace, and
  generated service-manager units, domains and state paths use `berth`.

### Removed

- Removed the host Caddy integration, `/etc/hosts` editing and
  `domain_strategy`-generated `.berth` domains; authored `domain`/`domains`
  must now be one label under `berth.localhost`, and `doctor --fix` no longer
  installs a host Caddy CA.

- Removed `up --save-ports`, `recreate --save-ports` and
  `env --sync --persist-runtime`, which wrote runtime ports back into the
  committed project file and would conflict across parallel worktrees.

### Fixed

- `--dry-run` no longer probes application health URLs, which made a dry-run
  `up` wait 30 seconds and then fail.
- Isolated the swarm bootstrap test's fixture repository from the developer's
  global Git configuration so global commit hooks no longer fail the test.
- Official PostgreSQL images tagged with major 18 or later now get their
  data volume at `/var/lib/postgresql`; mounting `/var/lib/postgresql/data`
  made their entrypoint refuse to start. Untagged, `latest` and digest-only
  references keep the previous mount so existing data is never hidden.
- Made test-only container engine overrides thread-local so tests no longer
  observe another test's Podman engine selection intermittently.

## [7.0.0] - 2026-07-12

### Added

- Added dual-format Stackctl config support so project discovery, loading, and
  saving now autodetect `.stackctl.toml` and `.stackctl.yaml` files while still
  rejecting the `.yml` extension.
- Added top-level `stackctl phpstan`, `stackctl ecs`, `stackctl php-cs-fixer`, and
  `stackctl psalm` wrappers so common PHP quality tools can run directly inside
  the selected app container without going through `stackctl exec`.
- Added top-level `stackctl pint`, `stackctl pest`, `stackctl phpunit`, and
  `stackctl rector` wrappers so common PHP formatting, test, and refactor tools
  can run directly inside the selected app container.
- Added `opensearch` and `elasticsearch` search service presets with local
  single-node defaults, inferred Scout env wiring, named data volumes, and
  shared runtime handling alongside existing search backends.
- Added project-wide `domain_strategy` config with `directory` and `random`
  modes so app services can resolve `.stackctl` domains automatically without
  repeating explicit per-service `domain` entries.
- Added service-level `restart` config with Docker-compatible policies so
  projects can opt out of or override Stackctl's default restart behavior.
- Added a `stackctl daemon` command group with `start`, `status`, `stop`, and
  `logs` subcommands that resolve target projects from an explicit `--path`
  before normal config loading.
- Added per-project daemon session persistence under `~/.config/stackctl/daemon/`
  so daemon commands can track pid and log metadata across separate CLI runs.
- Added `stackctl daemon watch` with repeatable `--dir` roots, `--once`, and
  `--interval` so Stackctl can discover `.stackctl.toml` projects and start missing
  per-project daemons automatically.
- Added `stackctl daemon service <install|status|print|uninstall>` so users can
  install a login-time watch service through `launchd` or `systemd --user`
  without hand-writing unit definitions.
- Added watch policy flags for `stackctl daemon watch` and installed daemon watch
  services so users can exclude subtrees and cap the number of auto-started
  projects under broad parent directories.

### Changed

- Renamed the project, CLI, config files, env vars, Docker labels, and
  user-facing documentation to `stackctl`.
- Changed Rust dependencies and CI workflow action pins to their latest
  available releases, including `tabled 0.21`, refreshed transitive lockfile
  versions, and `actions/checkout@v5` in the GitHub Actions pipeline.
- Changed `stackctl init` to write `domain_strategy = "directory"` and rely on
  automatic `.stackctl` domain generation for app services instead of emitting an
  explicit `domain = "...localhost"` entry in new configs.
- Changed Docker `run` generation to default Stackctl-managed services to
  `--restart unless-stopped`, improving recovery after Docker restarts
  and laptop sleep without requiring a manual `stackctl up`.
- Changed the daemon child path to bootstrap projects through the existing
  `start` flow, then supervise service containers by polling for non-running
  containers and retrying recovery with exponential backoff.
- Changed daemon project discovery to deduplicate overlapping watch roots,
  skip nested child projects once a valid parent project is managed, and
  report invalid `.stackctl.toml` files without stopping sibling discovery.

### Fixed

- Fixed CI `clippy` failures in daemon and artisan runtime helpers by
  removing panic-prone `expect()` usage from production code and marking
  the long-running daemon supervisor loop as intentional.
- Fixed repeated `stackctl artisan test --browser` Playwright browser downloads by
  caching browser binaries under `.stackctl/cache/playwright` in the mounted
  workspace and reusing that cache across reset test-runtime containers.
- Fixed repeated `stackctl artisan test --browser` system dependency installs by
  baking Playwright's `install-deps chromium` step into the cached derived app
  image used for browser test runtimes instead of rerunning it inside each
  fresh test container.
- Fixed `stackctl artisan test --browser` mixed-service projects such as
  `app + mailhog` so browser-runtime targeting stays scoped to FrankenPHP app
  services and does not route other app presets through PHP module inspection.
- Fixed Stackctl's default PHP memory limit for derived app images and
  `stackctl artisan test` runs to `4096M`, reducing coverage-run failures
  caused by the previous `2048M` ceiling.
- Fixed `stackctl artisan test` runtime planning to automatically include the
  `pcov` PHP extension in derived app images so Laravel coverage runs have a
  driver available without repeating `php_extensions = ["pcov"]` in app
  config.
- Fixed app-service startup waits to stop forcing HTTP `/up` probes onto
  non-HTTP worker presets such as Horizon, while adding default HTTP health
  paths for `frankenphp`, `reverb`, and `soketi` so `start --wait` uses the
  correct readiness checks for each app preset.
- Fixed `stackctl restore --service <db> --migrate` and `--schema-dump` to
  resolve and pass the restored service's Laravel database connection
  into `stackctl artisan`, and clarified post-restore logs to print the exact
  `--database=...` connection used for each restored service.
- Fixed Docker heavy-operation slot scheduling to reclaim stale PID lock
  files and wait long enough for slow runtime cleanup, preventing random
  `stackctl artisan test` setup failures while `docker rm` or `docker volume rm`
  is queued behind abandoned or long-running heavy ops.
- Fixed `stackctl artisan test` pooled-runtime stale-slot probing to silence
  expected `kill -0` stderr for dead PIDs, preventing random `kill: <pid>:
  No such process` noise during normal slot reclamation.
- Fixed standalone lifecycle commands such as `stackctl up` to treat missing
  workspace swarm context as a no-op for project dependencies, and fixed
  `--no-deps` to skip workspace swarm injected-env resolution entirely,
  so non-workspace projects no longer fail unless they actually opt into
  swarm wiring.
- Fixed app serve/open/down routing decisions to fall back to the local
  published endpoint when no domain is configured, so legacy projects can
  keep app domains optional instead of failing through the Caddy path.
- Fixed swarm child command construction to append `--no-deps` for nested
  `up`, `recreate`, `start`, and `down` invocations, preventing child Stackctl
  processes from re-resolving workspace dependencies that the parent swarm
  run already planned.
- Fixed object-store bucket bootstrap to prefer running the AWS CLI helper in
  the target container's network namespace and reach the service over
  `localhost`, while falling back to the legacy host-gateway path for
  published-port compatibility and runtimes that do not support
  `--network container:...`.
- Fixed random-port startup planning to reuse the published host port of an
  already running container, preventing `up` and `recreate` flows from
  reporting a new random port that the live service is not actually using.
- Fixed HTTP status probing to parse the trailing curl status line after
  preserving the response body, so doctor and open-summary checks can read
  status codes reliably without discarding probe output.
- Fixed app health-check timeout errors to include the last observed HTTP
  status, response body, or probe transport error, so `stackctl swarm recreate`
  now reports actionable diagnostics for failures such as `502 Bad Gateway`.
- Fixed Docker build-slot scheduling to wait long enough for real derived
  image builds, preventing sibling swarm targets from failing after a short
  slot-acquisition timeout while another app image is still building.
- Fixed swarm target stream forwarding to drop blank child-output lines, and
  clarified derived-image build queue logging so queued targets report that
  they are waiting for a Docker build slot before the actual build begins.
- Fixed Gotenberg health validation to evaluate the raw JSON response body
  instead of a truncated display summary, preventing healthy `200 {"status":
  "up"}` readiness responses from being misclassified as failures.
- Fixed app health-check timeout diagnostics for proxy-backed `.stackctl` URLs to
  explain that `502` means Caddy reached the public route but the upstream app
  container was not yet serving valid HTTP.
- Fixed Caddy reload fallback to stop any existing process before starting a
  replacement, preventing duplicate `caddy run` instances from keeping stale
  upstream port mappings alive after app ports change.
- Fixed parallel swarm Caddy apply races by serializing route-state writes and
  reload/start operations through one lock, preventing concurrent targets from
  spawning overlapping `caddy run` processes on `:443`.
- Fixed Caddy apply locking to recover stale lock files and wait long enough
  for queued updates, preventing abandoned `apply.lock` files or normal
  contention from causing downstream swarm targets to fail.
- Fixed Caddy apply to stage and validate config changes before replacing
  live files, then roll back and recover interrupted backup state so failed
  or crashed updates do not leave Caddy stopped with stale routes or a
  half-written config directory.
- Fixed Caddy command execution to capture output through temporary files
  instead of inherited pipes, preventing `stackctl ... recreate` from hanging
  when `caddy reload` or `caddy start` leaves descendant processes holding
  stdout or stderr open.
- Refactored Caddy temporary-file subprocess capture into a dedicated
  module so lifecycle code stays focused on reload, start, and trust
  behavior without changing runtime behavior.
- Fixed CI stability for Docker host-gateway args tests by pinning their
  runtime engine explicitly, and replaced panicking Node toolchain
  `expect()` paths with actionable errors that satisfy strict Clippy
  `expect_used` checks.

## [6.0.0] - 2026-03-06

### Added

- Added service-level JavaScript toolchain configuration via
  `[service.javascript]`
  with `runtime`, `package_manager`, `version_manager`, and `version`
  fields so app runtimes can resolve Node, Bun, or Deno execution
  explicitly.
- Added `stackctl node --package-manager`, `--version-manager`, and
  `--node-version`, plus matching overrides for `stackctl task deps bump`,
  so callers can choose package-manager and version-manager behavior per
  invocation.
- Added `stackctl bun` with `--bun-version`, plus Bun runtime detection from
  `bun.lock`, `bun.lockb`, and `package.json.packageManager`.
- Added `stackctl deno` with `--deno-version`, plus Deno runtime detection
  from `deno.json`, `deno.jsonc`, and `deno.lock`.
- Added explicit `stackctl task deps bump --node`, `--bun`, and `--deno`
  selectors so dependency workflows map directly to the runtime being
  managed.
- Added `stackctl task deps audit`, `stackctl task deps normalize`, and
  `stackctl task deps install` so app dependency maintenance now covers
  vulnerability checks, manifest normalization, and install workflows
  across Composer plus the explicit Node, Bun, and Deno runtime
  selectors already used by `stackctl task deps bump`.
- Added project-file Node inference for `package.json.packageManager`,
  `package.json.volta.node`, `.nvmrc`, `.node-version`, and
  `package.json.engines.node` so Node workflows can derive toolchain
  settings from existing repo metadata.

### Changed

- Changed app runtime image generation to install the selected Node
  version-manager, Bun runtime, or Deno runtime instead of always
  installing a hardcoded NodeSource LTS toolchain under the hood.
- Changed Node package-manager execution to resolve from config and
  project metadata instead of defaulting `stackctl node` to `bun`.
- Changed JavaScript runtime naming from `JsRuntime` to
  `JavaScriptRuntime`, and split Bun into its own `stackctl bun` command
  instead of treating it as a Node package-manager option.
- Changed the shared runtime domain module from `src/node/` to
  `src/javascript/` so internal naming matches the broader
  JavaScript-runtime scope.
- Changed the service config section name from `[service.node]` to
  `[service.javascript]` so configuration naming matches Bun and Deno
  support. This is a breaking config change.
- Changed Node-related CLI flags by removing `--manager` in favor of the
  explicit `--package-manager` name. This is a breaking CLI change.

### Fixed

- Fixed flaky Rust tests that mutate Docker runtime test state by
  serializing dry-run, engine, and fake-command overrides through one
  shared test scope.
- Fixed inferred Laravel app runtime env to inject
  `LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK=local` by default so HTTPS app domains
  avoid mixed-content failures during Livewire temporary uploads when using
  local S3-compatible object stores.
- Fixed object-store startup reliability by automatically ensuring configured
  buckets exist during `up` flows for object-store services.
- Fixed `stackctl artisan` with no explicit subcommand to run `php artisan list`
  by default, so command discovery works without requiring `-- <command>`.
- Fixed `stackctl artisan` runtime env composition to include swarm
  `inject_env` dependency values, so injected `*_API_BASE_URL` overrides are
  applied consistently in artisan command execution.
- Fixed package-manager wrapper defaults so `stackctl composer` now runs
  `composer list` when no subcommand is provided, and `stackctl node` now
  executes the selected package manager without forcing explicit args.

## [5.0.0] - 2026-03-09

### Added

- Added `stackctl task deps bump` for opinionated dependency maintenance
  workflows in app containers, with `--composer`, `--node`, and `--all`
  targets, lockfile-based Node package-manager inference, and documented
  manifest-skip behavior when `composer.json` or `package.json` is absent.

### Changed

- Changed `recreate` default behavior to wait for healthy services before
  finishing, with explicit opt-out available via `--no-wait`.
- Changed config project-mode resolution to require an explicit
  `project_type` outcome from `.stackctl.toml` or `composer.json` `type`
  (`project`/`library`), with hard failure when neither source resolves.
- Changed Laravel-only workflows to respect project mode: `start` skips
  Laravel bootstrap for `library` projects, and `artisan`/`app-create` are
  rejected unless `project_type = "project"`.

### Fixed

- Fixed object-store bucket bootstrap on Linux by adding host-gateway mapping
  for bootstrap helper containers.
- Fixed `stackctl swarm recreate` hangs after healthy targets by bounding Caddy
  reload/start command execution and output-drain waits, so swarm runs now
  complete instead of blocking indefinitely on stuck child process I/O.
- Fixed object-store startup flakiness by improving readiness handling around
  bucket bootstrap timing and service health dependencies.
- Fixed `stackctl artisan test` ZPL conversion runtime dependencies by installing
  `ghostscript` (`gs`) in default derived app images.
- Fixed derived image cache invalidation by hashing rendered Dockerfile
  content, so runtime dependency/template changes rebuild automatically.
- Fixed Linux `/etc/hosts` update reliability by hardening privileged append
  escalation paths (`sudo`/`pkexec`) and retry behavior for permission-related
  failures including read-only filesystem edge cases.
- Fixed app runtime env defaults to always provide `APP_NAME` when absent, to
  prevent runtime metadata and health regressions in app services that require
  it.
- Fixed serve health checks for Gotenberg by preserving and validating
  response body content instead of status-only probes.
- Fixed serve app container local networking by mapping configured local
  domains and injected local peer domains to host-gateway, preventing
  container-local loopback resolution failures for self and swarm calls.

## [4.0.0] - 2026-02-24

### Added

- Added container runtime engine selection via `.stackctl.toml` using
  `container_engine = "docker"` or `container_engine = "podman"`.
- Added global CLI override `--engine <docker|podman>` to select runtime
  engine for the current invocation.

### Changed

- Changed runtime command execution to resolve the container CLI binary from
  selected engine (`docker` or `podman`) through shared command runners.
- Changed host-loopback alias resolution to be engine-aware:
  Docker uses `host.docker.internal` and Podman uses
  `host.containers.internal`.

### Migration Notes

- Existing configs remain valid. If `container_engine` is omitted, Stackctl
  defaults to `docker`.
- Podman support targets core Docker-compatible CLI flows. Some advanced
  behavior may still differ across host/network/runtime setups.

## [3.6.0] - 2026-02-23

### Added

- Added automatic Playwright bootstrap for `stackctl artisan test` when
  `pestphp/pest-plugin-browser` is detected in `composer.json`, including:
  `npm install playwright@latest`, `npx playwright install-deps`, and
  `npx playwright install`.

### Changed

- Changed adaptive default test runtime pool sizing from small fixed tiers to
  host-aware `8/16/32` slot tiers, using Docker CPU/memory hints when
  available.
- Changed Laravel app preset PHP extension defaults to include `sockets` so
  Pest browser plugin socket helpers are available in test containers.

### Fixed

- Fixed `stackctl artisan test` browser test failures caused by missing PHP
  `sockets` extension in app test runtime containers.
- Fixed `PlaywrightOutdatedException` in containerized browser tests by
  ensuring Playwright package and browser/runtime dependencies are provisioned
  before test execution.
- Fixed `stackctl artisan test` config loading to prefer `.stackctl.testing.toml`
  when present, with fallback to `.stackctl.toml`, so test runtimes no longer
  start services that were excluded from testing config.

## [3.5.0] - 2026-02-22

### Added

- Added `garage` object-store preset as a MinIO alternative for local
  S3-compatible storage workflows.

### Changed

- Changed object-store preset and runtime wiring to include Garage driver
  support across preset expansion, env inference, and runtime defaults.

## [3.4.0] - 2026-02-21

### Added

- Added adaptive `stackctl artisan test` runtime pool sizing based on available
  host resources with optional Docker resource hints.
- Added deterministic workspace-scoped runtime naming for pooled test runs to
  isolate concurrent workspaces safely.

### Changed

- Changed test runtime pool-size resolution precedence to:
  explicit override, then `STACKCTL_TEST_RUNTIME_POOL_SIZE`, then adaptive sizing.
- Changed pooled runtime lock wait behavior to allow longer acquisition under
  heavier concurrent test load.

### Fixed

- Fixed pooled runtime container-name collisions across different workspaces
  by including workspace identity in runtime environment names.

## [3.3.0] - 2026-02-20

### Added

- Added env-scoped config discovery for `--env <name>` so Stackctl now prefers
  `.stackctl.<env>.toml` when present and falls back to `.stackctl.toml`.
- Added Docker runtime policy controls for operation scheduling:
  `--docker-max-heavy-ops`, `--docker-max-build-ops`,
  and `--docker-retry-budget`.
- Added `--test-runtime-pool-size` for `stackctl artisan test` runtime pooling.

### Changed

- Changed `stackctl artisan test` runtime namespace allocation to use pooled
  runtime leases, reducing unbounded parallel test runtime churn.
- Changed heavy Docker execution paths to use scheduler gating for build and
  cleanup operations.
- Changed CLI bootstrap/config loading to support env-specific config file
  selection while preserving existing fallback behavior.

### Fixed

- Fixed derived-image lockfile write race conditions by adding lock-file
  coordination and atomic lockfile replacement.

## [3.2.0] - 2026-02-20

### Added

- Added selector parity across operational command families by supporting
  `--profile` on `pull`, `health`, `logs`, `relabel`, `top`, `stats`,
  `inspect`, `kill`, `pause`, `unpause`, `wait`, and `port`.
- Added repeatable `--service` selection across the same operational command
  families so multiple explicit targets can be selected in one invocation.
- Added output-format parity flags in docker-ops output flows:
  `stackctl port --format json` and `stackctl inspect --output json`.

### Changed

- Changed command dispatch and handler selection wiring to route operational
  command execution through shared service-scope filtering with profile and
  multi-service support.
- Changed JSON output handling for `port` and `inspect` to keep existing
  `--json` behavior while accepting the new format-style flags.
- Changed release metadata to `3.2.0`.

### Migration Notes

- `port` JSON output:
  - previous: `stackctl port --json`
  - now also supported: `stackctl port --format json`
- `inspect` JSON output:
  - previous: `stackctl inspect --json`
  - now also supported: `stackctl inspect --output json`
- Profile-based selection on ops commands:
  - previous: commands commonly required explicit `--service`/`--kind`
  - now also supported: `--profile <infra|data|app|web|api|full|all>`
- Multi-target service selection:
  - previous: single `--service <name>`
  - now also supported: repeat `--service` (for example
    `--service db --service cache`)

## [3.1.0] - 2026-02-20

### Added

- Added global `--non-interactive` mode to disable interactive behaviors in
  automation contexts, including browser-open and TTY-dependent command paths.
- Added JSON output support for diagnostics commands:
  `stackctl doctor --format json`, `stackctl health --format json`, and
  `stackctl about --format json`.
- Added lifecycle selector parity by supporting `--profile` on
  `stackctl down`, `stackctl stop`, `stackctl rm`, `stackctl recreate`, and
  `stackctl restart`.
- Added repeatable `--service` selection for lifecycle and diagnostics paths
  so multiple explicit services can be targeted in one invocation.
- Added `stackctl logs --since <VALUE>` and `stackctl logs --until <VALUE>` to align
  with Docker log time-window filtering behavior.
- Added stop-timeout controls for teardown flows via
  `stackctl stop --timeout <SECONDS>` and `stackctl down --timeout <SECONDS>`.
- Added selector parity (`--kind` and `--profile`) across app/runtime command
  families: `exec`, `artisan`, `composer`, `node`, `serve`, and `open`.

### Changed

- Changed shared service-selection internals to route more command families
  through common filter/profile resolution, reducing per-command selector
  drift.
- Changed command dispatch wiring and usage docs to keep new selector and
  output-format capabilities consistent and discoverable.

## [3.0.0] - 2026-02-17

### Added

- Added `stackctl share` tunnel management with `start`, `status`, and `stop`
  subcommands for app services, including provider-backed session tracking,
  persisted runtime metadata, and JSON/text output modes.
- Added Expose client sharing support to `stackctl share` via
  `--provider expose` and `--expose`.
- Added share provider shorthand flags `--cloudflare`, `--expose`, and
  `--tailscale` as alternatives to
  `--provider <cloudflare|expose|tailscale>` across share flows.

### Changed

- Changed random-port allocation and runtime host normalization to scope
  allocation, remapping, and conflict tracking by bind host, improving
  mixed-host and wildcard host behavior.
- Changed doctor port validation coverage to include SMTP listeners and use
  shared host-aware checks across startup and recreate flows.
- Changed swarm target argument validation to enforce consistent parallel
  execution guard behavior and remove duplicate forwarding paths.

### Fixed

- Fixed release build breakage after refactors by repairing visibility
  regressions in internal modules.
- Fixed `stackctl artisan test` runtime isolation so test startup now derives
  injected env values after test-port remapping and keeps app targets on
  localhost TLS, preventing test containers from corrupting active dev
  runtime networking (for example Redis host/port reachability).
- Fixed `stackctl artisan test` runtime cleanup to force-remove prior test
  containers and purge reusable named volumes before startup, ensuring each
  run starts from fresh service state.
- Fixed random-port runtime flow failures in CLI orchestration by repairing
  compilation and remap control paths used for service startup.
- Fixed SMTP remap handling to avoid duplicate host-port collisions and ensure
  doctor conflict checks include SMTP service ports.
- Fixed wildcard IPv6 host detection in config/runtime normalization so host
  binding behavior remains consistent across port allocation paths.
- Fixed swarm clone target execution to preserve `git clone` repository and
  destination argument order.
- Fixed serve-mode trust-info emission to use the selected options target,
  ensuring status output is routed to the correct service context.

## [2.0.0] - 2026-02-14

### Added

- Added automatic host-gateway alias injection
  (`host.docker.internal:host-gateway`) for service/container runs that
  require host-loopback reachability.
- Added default persistent named-volume mounts for stateful backends when no
  explicit volume mapping is configured.
- Added host-level port occupancy checks to `stackctl doctor` so startup conflicts
  are detected before container launch.
- Added Laravel runtime bootstrap to `stackctl start` for app targets:
  `storage:link`, `migrate`, and conditional `key:generate` when `APP_KEY`
  is missing.

### Changed

- Changed startup behavior to prioritize first-run app readiness as part of
  the default `stackctl start` flow.
- Changed zero-config persistence defaults so common stateful services survive
  routine stop/down/recreate cycles without manual volume setup.
- Changed Linux compatibility for host-service access by making loopback host
  alias wiring automatic in Docker run paths.

### Fixed

- Fixed `stackctl start --env <name>` key bootstrap detection to inspect the
  effective runtime env file (`.env.<name>`) instead of always reading `.env`.
- Fixed duplicate Laravel bootstrap execution across multi-app profiles by
  targeting only the primary web app container (FrankenPHP app target with no
  custom command).
- Fixed Laravel bootstrap migration commands to run with `--isolated`,
  preventing concurrent migration execution when multiple app processes start
  in parallel.
- Fixed doctor host-port validation to probe the configured bind host directly
  without fallback to `127.0.0.1`, preventing false negatives on non-loopback
  host bindings.

## [1.9.0] - 2026-02-13

### Added

- Added `dragonfly` cache preset with DragonflyDB image defaults and
  Redis-compatible runtime behavior.
- Added `sqlserver` database preset with `mssql` alias and SQL Server image
  defaults.
- Added `localstack` object-store preset with S3-oriented default settings.
- Added SQL Server (`sqlsrv://`) connection URL support for service runtime
  summaries and tooling output.

### Changed

- Changed driver/runtime mapping coverage to include `dragonfly`,
  `sqlserver`, and `localstack` across preset expansion, inferred env output,
  default ports, and health checks.
- Changed SQL driver gating so SQL Server is treated as a database backend,
  while dump/restore SQL-admin operations remain scoped to Postgres/MySQL.

### Fixed

- Fixed SQL restore failure handling to consume stderr with
  `wait_with_output`, preventing potential deadlocks and preserving actionable
  restore error output (including non-UTF8 payloads).
- Fixed RabbitMQ preset default coverage to assert zero-config broker creds via
  `username`/`password` defaults (`guest`/`guest`), matching runtime env
  injection behavior.
- Fixed serve teardown reporting to fail on real `docker stop`/`docker rm`
  errors instead of always reporting success.
- Fixed serve teardown behavior for already-absent containers by treating
  `No such container` as a non-fatal state.
- Fixed managed env update behavior to error when the target env file is
  missing, instead of silently no-oping.
- Fixed env file writers to escape quotes, backslashes, and control characters
  so generated `.env` entries remain valid.
- Fixed service connection URL construction to percent-encode credentials and
  path components.
- Fixed service connection URL host formatting to bracket raw IPv6 literals.

## [1.8.0] - 2026-02-13

### Fixed

- Fixed `stackctl artisan test` runtime startup to assign random host ports by
  default (matching `stackctl up`) and recreate remapped services automatically,
  preventing host-port collision failures.

## [1.7.0] - 2026-02-13

### Fixed

- Fixed swarm dependency `inject_env` `:port`/`:url` resolution to use live
  Docker host-port bindings when available, with fallback to configured ports.
- Fixed `stackctl open` database status URLs to report runtime published ports
  instead of static config ports.
- Fixed Caddy reverse-proxy forwarding for app routes to always send HTTPS
  `X-Forwarded-*` headers, preventing login/form redirects from downgrading
  to `http://`.
- Fixed serve container env precedence so inferred HTTPS `APP_URL` and
  `ASSET_URL` cannot be downgraded by explicit `service.env` `http://`
  overrides.
- Fixed `stackctl up` app env merge precedence so swarm/project dependency
  injected values cannot downgrade inferred HTTPS `APP_URL`/`ASSET_URL` to
  `http://`.

## [1.6.0] - 2026-02-13

### Added

- Added service lifecycle hooks in `.stackctl.toml` via `[[service.hook]]` with
  `post_up`, `pre_down`, and `post_down` phases.
- Added hook run modes for container `exec` commands and host `script`
  commands with optional `on_error` behavior (`fail` or `warn`).
- Added lifecycle integration so configured hooks run during `stackctl up`,
  `stackctl apply`, and `stackctl down` for selected targets.
- Added usage and init-template examples for hook configuration.

### Fixed

- Fixed `stackctl recreate --publish-all` to support `--parallel > 1`.
- Fixed `stackctl swarm recreate` to work with default random-port publishing
  under parallel target execution.

## [1.5.0] - 2026-02-13

### Added

- Added `pgsql` as a PostgreSQL preset alias (alongside `postgres` and `pg`).
- Added Laravel queue worker app preset:
  `queue-worker` (with `queue` alias), using
  `php artisan queue:work` defaults.

## [1.4.0] - 2026-02-13

### Added

- Added Laravel worker/runtime presets:
  `reverb`, `horizon`, `scheduler`, and `dusk`.
- Added app-service presets and aliases for broader Laravel local stacks:
  `selenium`, `mailpit`, `rabbitmq`, and `soketi`.
- Added infrastructure presets for common Laravel-adjacent backends:
  `mongodb` and `memcached`.
- Added Scout-oriented env defaults for search presets so `meilisearch` and
  `typesense` inject expected Laravel variables by default.

### Changed

- Changed startup defaults and usage documentation to align generated config
  and command behavior.
- Changed preset coverage and docs to reflect new Laravel service options,
  including Mailpit and standalone Selenium naming.

### Removed

- Removed `laravel-full` preset alias because it duplicated `laravel`
  behavior without adding distinct defaults.
- Removed `laravel-minimal` preset because it no longer matched active
  project usage.

## [1.2.0] - 2026-02-13

### Added

- Added formal Stackctl ownership labels to created containers:
  `com.stackctl.managed`, `com.stackctl.service`, `com.stackctl.kind`, and
  `com.stackctl.container`.
- Added `stackctl relabel` command to migrate existing containers by recreating
  selected services and applying Stackctl ownership labels.
- Added label-aware ownership checks before scoped prune removes containers.
- Added command polish flags for Docker parity:
  `stackctl cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added command polish flags for Docker parity:
  `stackctl inspect` now supports `--size` and `--type`.
- Added command polish flags for Docker parity:
  `stackctl attach` now supports `--detach-keys`.
- Added structured JSON output modes:
  `stackctl inspect --json`, `stackctl port --json`, and `stackctl events --json`.
- Added `stackctl events --allow-empty` to explicitly allow empty service
  selections without failing.

### Changed

- Changed default `stackctl events` scoping from name filters to label-based Stackctl
  ownership filters.
- Changed `stackctl events` behavior to return non-zero when no services match,
  unless `--allow-empty` is supplied.
- Changed `stackctl events --all` behavior to print an explicit warning before
  streaming global daemon events.
- Changed default `stackctl prune` behavior to enforce Stackctl-scoped cleanup
  semantics with ownership validation.
- Changed `stackctl prune --all` behavior to require an explicit `--force` guard
  before global Docker prune execution, with warning output when used.
- Changed global prune dry-run behavior to preview candidate stopped containers
  before execution.
- Changed command help text to call out scoped-vs-global behavior for `events`
  and `prune`.
- Changed `events` and `prune` help output to include concrete usage examples.

## [1.1.0] - 2026-02-13

### Added

- Added Docker passthrough commands for service containers:
  `top`, `stats`, `inspect`, `attach`, `cp`, `kill`, `pause`, `unpause`,
  `wait`, `events`, `port`, and `prune`.
- Added `service:/path` shorthand resolution for `stackctl cp` endpoints.
- Added Docker-compatible flags:
  `stackctl cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added Docker-compatible flags:
  `stackctl inspect` now supports `--size` and `--type`.
- Added Docker-compatible flags:
  `stackctl attach` now supports `--detach-keys`.
- Added scoped selectors for event/prune workflows:
  `stackctl events` now supports `--service` and `--kind`.
- Added scoped selectors for event/prune workflows:
  `stackctl prune` now supports `--service`, `--kind`, and `--parallel`.

### Changed

- Changed restore file-progress output from an in-place status bar to
  persistent incremental log events with percent and MiB totals.
- Changed `stackctl events` default behavior to Stackctl container scope by applying
  container filters from resolved service names.
- Changed `stackctl events` UX to require explicit `--all` for global daemon
  event streaming.
- Changed `stackctl prune` default behavior to remove only stopped
  Stackctl-configured service containers (safe by default).
- Changed `stackctl prune` UX to require explicit `--all` for global
  `docker container prune` behavior.

## [1.0.0] - 2026-02-12

### Added

- Stable `1.0.0` release line for Stackctl.
- `INSTALLATION.md` with explicit install and verification steps.
- Release changelog baseline.

### Changed

- Refined release README messaging around zero-BS, low-config Laravel
  orchestration.
- Updated `stackctl init` defaults to a smaller preset-driven config.
- Fixed generated `stackctl init` template output so it writes valid TOML.
- Updated package metadata and versioning to `1.0.0`.
- Improved `stackctl artisan test` failure reporting to include the app
  container exit code when `docker exec` returns non-zero after test
  output has already been streamed.
- Fixed `stackctl artisan` serve-container failures to report the actual
  `docker exec` exit code instead of the generic container error.
