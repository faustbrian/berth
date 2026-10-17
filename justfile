run: fmt
    cargo run --release

run-dev: fmt
    cargo run

install:
    cargo install --path . --locked

build: fmt
    cargo build --release

build-dev: fmt
    cargo build

lint:
    ./scripts/test-policy-audits.sh
    rustup run nightly cargo fmt --check
    cargo clippy --all-targets --all-features

fmt:
    rustup run nightly cargo fmt

parity-smoke engine="docker":
    ./scripts/runtime-parity-smoke.sh {{engine}}

parity-full engine="docker":
    ./scripts/runtime-parity-full.sh {{engine}}

audit-supply-chain:
    ./scripts/audit-supply-chain.sh
