run: fmt
    RUSTFLAGS="-Awarnings" cargo run --release

run-dev: fmt
    RUSTFLAGS="-Awarnings" cargo run

install:
    cargo install --path .

build: fmt
    cargo build --release

build-dev: fmt
    cargo build

lint:
    rustup run nightly cargo fmt --check

fmt:
    rustup run nightly cargo fmt

parity-smoke engine="docker":
    ./scripts/runtime-parity-smoke.sh {{engine}}

parity-full engine="docker":
    ./scripts/runtime-parity-full.sh {{engine}}

audit-supply-chain:
    ./scripts/audit-supply-chain.sh
