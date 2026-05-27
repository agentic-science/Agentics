import 'common.just'

# Check Rust formatting
[working-directory: repo]
fmt-check:
    cargo fmt --all -- --check

# Lint Rust
[working-directory: repo]
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
