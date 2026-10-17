# berth

`berth` gives every checkout of a project its own disposable, isolated Docker
environment, from one command and with no daemon. It is built for AI agents
that create environments in parallel (thirty agents in thirty worktrees of
one project), and works the same for the humans next to them.

- **One instance per checkout.** The instance ID is derived from the checkout
  path, so worktrees, dot-directories and nested checkouts never collide and
  committed config never needs editing per agent.
- **Shared database servers, isolated tenants.** PostgreSQL, MySQL, MariaDB,
  Redis and Valkey run once per engine version; each instance and each test
  run gets its own database, role or ACL user.
- **Routed, not port-mapped.** Apps answer at
  `https://{instance}-{project}.berth.localhost` through one local gateway;
  host ports are opt-in.
- **Agent-grade contract.** `up --wait --json` returns only when the app
  boots, commands pass exit codes through exactly, berth's own failures exit
  125, and every inspection command prints one JSON document.
- **Crash recovery without a daemon.** Instances carry a lease (`--ttl`,
  `--owner-pid`); any berth command that touches Docker reaps expired or
  orphaned ones.

## Quick start for agents

```bash
cd "$WORKTREE"
berth up --wait --json --owner-pid $$ > up.json   # URLs, credentials, environment
curl -sk "$(jq -r '.urls[0]' up.json)"
berth artisan --no-tty --non-interactive test tests/Feature/FooTest.php
berth artisan --no-tty --non-interactive test --parallel
berth down --purge                                # nothing retained
```

The contract (JSON shapes, exit codes, leases, test isolation) is in
[`docs/v9/agents.md`](docs/v9/agents.md).

## Quick start for humans

```bash
berth init        # write a .berth.yaml
berth up          # start the instance for this checkout
berth url         # where the app answers
berth ls          # every instance, across projects
berth down
```

A minimal `.berth.yaml`:

```yaml
schema_version: 9
project: shop
project_type: project
services:
  db:
    preset: postgres
  cache:
    preset: redis
  app:
    preset: laravel
```

## Documentation

- Installation, upgrades and migration from helm 6 and stackctl 7/8:
  [`INSTALLATION.md`](INSTALLATION.md)
- Project file and every command and flag: [`USAGE.md`](USAGE.md)
- Agent contract: [`docs/v9/agents.md`](docs/v9/agents.md)
- How it works: [`docs/v9/architecture.md`](docs/v9/architecture.md),
  security model: [`docs/v9/security.md`](docs/v9/security.md)
- What was kept, ported or dropped from stackctl 8:
  [`docs/v9/ledger.md`](docs/v9/ledger.md)
- Release changes: [`CHANGELOG.md`](CHANGELOG.md)

## License

`berth` is licensed under MIT. See [`LICENSE.md`](LICENSE.md).
