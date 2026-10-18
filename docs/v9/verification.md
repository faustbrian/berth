# Verifying berth

Who can prove what, and how a failure is written down (carried from 8.x's
verification discipline, ledger L161).

## Levels of evidence

| Claim | Proven by | Run by |
|---|---|---|
| Parsing, naming, policy, planning | unit tests (`cargo test`) | every developer, CI on four platforms |
| Command behavior against an engine (argv, ordering, exit codes, JSON documents, labels, purge, reaping) | integration tests in `tests/` with a fake `docker` that records calls | `cargo test`, CI |
| Real Docker behavior (shared tenants, gateway TLS, least privilege, ownership, isolation, ParaTest, reaping a killed agent, zero leftovers) | `scripts/accept-v9-agents.sh <berth> <out> [N]` | a developer with Docker (N=30), CI on Linux with rootful Docker, rootless Docker and rootless Podman (N=4, cold image cache) |
| v6 command parity | `scripts/runtime-parity-smoke.sh`, `scripts/runtime-parity-full.sh` | a developer with Docker |
| Lint, supply chain, workflow pinning | `just lint`, `scripts/audit-supply-chain.sh`, `scripts/test-policy-audits.sh` | every developer, CI |
| Ledger coverage of 8.x | `scripts/check-v9-ledger.sh [--final]` | every developer, CI |

CI runs the suite with `--test-threads=1`: many tests write a fake `docker`
or `curl` and run it, and on Linux a fork by another test thread while such a
file is still open for writing makes its exec fail with ETXTBSY.

A fake-engine test proves what berth asks Docker to do, never what Docker
does. Claims about Docker behavior need the acceptance script or an equally
real run, and a passing narrower check never stands in for a broader claim.

## Recording a failure

Every defect found by a review, CI or a live run is recorded with:

1. **Symptom:** what was observed, with the exact output.
2. **Root cause:** the first point where behavior diverged from the
   contract.
3. **Missing layer:** which test level should have caught it, and the test
   added there. A fix without a test that fails before it is not done.

Example (2026-10-18): test-lease app containers ran as root on Linux.

- Symptom: CI's acceptance job saw a file written during `artisan test`
  owned by UID 0.
- Root cause: test leases set `localhost_tls` on port 80, and the
  least-privilege exclusion for in-container TLS matched `localhost_tls`.
- Missing layer: a real-Docker check of the lease container. The acceptance
  script now inspects a live lease's user and capabilities, and the unit test
  pins the exclusion to port 443.

## Before a release

On the exact commit to be tagged: `just build`, `cargo test`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`scripts/audit-supply-chain.sh`, `scripts/test-policy-audits.sh`,
`scripts/check-v9-ledger.sh --final`, `scripts/accept-v9-agents.sh` at
N=30, an independent review of the complete diff, and green CI.
