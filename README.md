# stackctl

> **Deprecated.** stackctl 8 is superseded by **berth 9.0.0**, released from
> the [`9.x` branch](https://github.com/faustbrian/berth/tree/9.x) (tag
> [`9.0.0`](https://github.com/faustbrian/berth/releases/tag/9.0.0)). berth
> continues helm 6 and stackctl 7 without a required daemon: one isolated
> instance per checkout, shared database servers with a tenant per instance,
> and an exact command and JSON contract for agents. 8.x receives no further
> changes; see berth's `INSTALLATION.md` for migrating from stackctl 8
> (`berth config migrate` converts `.stackctl.yaml`).

Stackctl v8 is a clean-slate local development control plane for macOS and
Linux. One per-user daemon reconciles strict `.stackctl.yaml` projects into
Linux containers through the Engine API. It owns shared infrastructure, local
HTTPS routing, credentials, recovery, and lifecycle state without installing
PHP, databases, Caddy, or nginx on the host.

## Quick start

Start Docker Desktop on macOS or Docker Engine on Linux, then install the
current checkout:

```bash
cargo install --path . --locked
stackctl --version
```

Create a project configuration:

```yaml
schema_version: 8
project: bill
services:
  app:
    preset: laravel
  db:
    preset: postgres
    version: "18"
```

Perform the one-time setup:

```bash
stackctl setup --dir ~/Developer
stackctl open
```

Setup validates the roots and `.localhost` resolution before host mutation,
installs the singleton CA trust, and starts the login service transactionally.
The daemon discovers the project and creates its missing immutable artifact
lock through the selected Docker-compatible Engine before any workload
mutation. Optional diagnostics remain available through `stackctl doctor`,
`stackctl status`, `stackctl logs`, and `stackctl url`.
`stackctl open` waits for the selected project route to finish reconciling, so
normal startup does not require a manual reconcile or a second open command.
`stackctl config validate .stackctl.yaml` is available as an optional
read-only diagnostic; normal setup and reconciliation do not require it.

V8 does not upgrade, migrate, adopt, or execute pre-v8 project configuration.
Use a fresh installation and new `.stackctl.yaml` files.

## Documentation

- [V8 design and operations](docs/v8/README.md)
- [Installation](INSTALLATION.md)
- [Command reference](USAGE.md)
- [Release changes](CHANGELOG.md)

## License

Stackctl is licensed under MIT. See [LICENSE.md](LICENSE.md).
