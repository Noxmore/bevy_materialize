#!/bin/bash
set -e

echo cargo fmt --all --check
cargo fmt --all --check

echo cargo clippy --all-targets
cargo clippy --all-targets

echo cargo clippy --no-default-features
cargo clippy --no-default-features

echo cargo clippy --all-features --all-targets
cargo clippy --all-features --all-targets

echo cargo test --all-features --all-targets
cargo test --all-features --all-targets