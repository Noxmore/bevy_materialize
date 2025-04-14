#!/bin/bash
set -e
set -x

cargo fmt --all --check

cargo clippy --all-targets

cargo clippy --no-default-features

cargo clippy --all-features --all-targets

cargo test --all-features --all-targets

cargo doc --no-deps --all-features