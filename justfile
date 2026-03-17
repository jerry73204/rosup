profile := "dev-release"

default:
    @just --list

# Build all targets
build:
    cargo build --all-targets --profile {{profile}}

# Remove build artifacts
clean:
    cargo clean

# Format code (requires nightly)
format:
    cargo +nightly fmt

# Run format and clippy checks
check:
    cargo +nightly fmt --check
    cargo clippy --all-targets -- -D warnings

# Run tests with nextest
test:
    cargo nextest run --no-fail-fast

# Run check then test (for CI)
ci: check test

# Install required dev tools
setup:
    rustup toolchain install nightly
    cargo install cargo-nextest
