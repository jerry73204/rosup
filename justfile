profile := "dev-release"

default:
    @just --list

# Build all targets
build:
    cargo build --all-targets --profile {{profile}}

# Run the rosup CLI (e.g.: just run build, just run 'add sensor_msgs')
run *args:
    cargo run --profile {{profile}} --bin rosup -- {{args}}

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
ci: check
    cargo build -p shims -p rosup
    cargo nextest run --profile ci

# Install the rosup binary to ~/.cargo/bin
install:
    cargo install --path crates/rosup-cli

# Install required dev tools
setup:
    rustup toolchain install nightly
    cargo install cargo-nextest
