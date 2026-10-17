# Installation

## Requirements

- Docker Desktop (or Docker Engine)
- Rust toolchain with Cargo (for install methods below)

## Install From Main Branch (Recommended)

```bash
cargo install --git git@github.com:faustbrian/stackctl.git --bin berth --branch main --locked
```

## Install From Git Tag

```bash
cargo install --git git@github.com:faustbrian/stackctl.git --bin berth --tag v1.0.0 --locked
```

## Install From Local Source

From the repository root:

```bash
cargo install --path . --locked
```

## Verify Installation

```bash
berth --version
```

Expected output includes `berth 1.0.0`.

## Upgrade

Re-run the install command for the target ref/tag.

## Shell Completions (Optional)

```bash
berth completions zsh > ~/.zsh/completions/_berth
```
