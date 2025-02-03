# Runs a bunch of commands to make sure everything is good. This is my bootleg replacement for CI :)

#!/bin/bash
set -e

echo cargo clippy --all-targets
cargo clippy --all-targets

echo cargo clippy --no-default-features
cargo clippy --no-default-features

echo cargo clippy --all-features --all-targets
cargo clippy --all-features --all-targets

echo cargo fmt --all --check
cargo fmt --all --check

echo cargo test --all-features
cargo test --all-features