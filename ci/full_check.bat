cargo fmt --all --check || exit /b 1

cargo clippy --all-targets || exit /b 1

cargo clippy --no-default-features || exit /b 1

cargo clippy --all-features --all-targets || exit /b 1

cargo test --all-features --all-targets || exit /b 1
cargo test --all-features --doc || exit /b 1

cargo doc --no-deps || exit /b 1