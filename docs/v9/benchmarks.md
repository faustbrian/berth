# berth benchmarks

Time-to-ready and peak memory of parallel agents, berth 9 against helm 6
(the v6 baseline), on one machine.

## Setup

- **Host:** Apple silicon Mac, Docker Desktop 29.8.2 VM with 16 CPUs and
  31.3 GiB.
- **Fixture (both tools):**
  - the laravel/laravel skeleton at `e196bfd`, with laravel/framework v13.37.0
    from the committed lock;
  - services: PostgreSQL, Redis and a Laravel app;
  - one git worktree per agent;
  - images already present locally.
- **berth 9:** `scripts/accept-v9-agents.sh target/release/berth <out> N`.
  - It is built from commit `e09074d8`.
  - **Time-to-ready** is the wall time of `berth up --wait --json`. That call
    returns once the app answers through the gateway and Laravel boots.
  - The rest of each agent's work runs concurrently with the other agents'
    `up` calls: HTTPS checks, migrations, a focused test, ParaTest with
    4 processes and a failing test.
- **helm 6:** `scripts/benchmark-v6-baseline.sh <helm-6.0.0> <out> N`.
  - Each agent runs `helm --env agent-N up --wait --wait-timeout 300`.
  - **Time-to-ready** is the time until that returns after its `/up` health
    check passes.
  - The apps use `localhost_tls`, so helm never touches the host Caddy.
  - Nothing else runs.
- **Peak memory:** the largest sum of `docker stats` memory over each run's
  own containers, sampled every few seconds.

## Results

| Run | Agents up | Time-to-ready p50 | p95 | Peak memory |
|---|---|---|---|---|
| berth 9, N=30 | 30 of 30 | 23.3 s | 34.0 s | 4.67 GiB |
| helm 6, N=30 | 12 of 30 | 14.4 s | 17.2 s | 3.03 GiB |
| berth 9, N=12 | 12 of 12 | 16.1 s | 16.9 s | 3.01 GiB |
| helm 6, N=12 | 11 of 12 | 12.3 s | 13.8 s | 2.79 GiB |

For helm, the time-to-ready columns cover only the agents that came up.

- **helm 6 cannot run more than 11 parallel environments of this project.**
  - Its `--env` namespace offsets every port by 1,000 per environment index.
  - PostgreSQL's base port, 54320, leaves no room past `agent-11`: `agent-12`
    onwards fail with `cannot apply env port offset (+12000) to port for
    service 'db' (base port 54320)`.
  - Neighbouring checkouts also share helm's project-name identity unless
    each one passes a distinct `--env`.
- **berth 9 runs all 30**, and its full acceptance run passes:
  - every instance, test run and ParaTest process gets its own database;
  - exit codes pass through;
  - a SIGKILLed agent is reaped;
  - nothing labeled is left.
- **Per-instance time-to-ready is higher with berth.**
  - berth waits for the routed HTTPS request through the gateway and a
    Laravel boot (`artisan about`).
  - In its runs, early agents' test suites (ParaTest included) compete with
    later agents' `up`.
  - helm's runs started nothing else.
  - Between N=12 and N=30, berth's p95 grows from 16.9 s to 34.0 s. Most of
    that growth is the concurrent test work, not `up` itself.
- **Memory grows slowly with berth.**
  - Databases and caches run once per engine (shared servers with a tenant
    per instance).
  - helm runs a PostgreSQL and a Redis container per environment.
  - At N=12, berth uses about the same memory as helm while 12 test suites
    are running.
  - At N=30, berth holds 30 apps, their test-run containers and the shared
    servers in 4.67 GiB.

## Raw records

Each run writes `metadata.txt` and `results.txt`. The berth record also has
`ready-ms.txt`, the per-agent `up --json` documents and the step logs. For
berth at N=30, the ready times ranged from 16.0 s to 34.3 s, the whole run
took 271 s, and the memory sampler took 33 samples. The helm N=12 run took
14 s and has only 4 memory samples, so its peak is approximate.
