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

# ── Test recipes ──────────────────────────────────────────────────────────────

# Run unit tests only (no external deps needed)
test-unit:
    cargo nextest run --no-fail-fast --workspace --exclude rosup-tests

# Run integration tests (spawns rosup binary, uses shims)
test-integration:
    cargo build -p shims -p rosup
    cargo nextest run --no-fail-fast -p rosup-tests

# Run all fast tests (unit + integration)
test: test-unit test-integration

# Run native build tests that require a sourced ROS 2 environment.
# These are #[ignore]d in normal runs. Requires: source /opt/ros/<distro>/setup.bash
test-ros:
    cargo nextest run --no-fail-fast --run-ignored ignored-only -E 'test(build::task::)'

# Run check then test (for CI — no ROS env needed)
ci: check
    cargo build -p shims -p rosup
    cargo nextest run --profile ci

# Run everything including ROS-dependent tests
test-all: ci test-ros

# ── Workspace testing script ──────────────────────────────────────────────────

# Test rosup against a real workspace (e.g.: just test-workspace ~/repos/autoware/1.5.0-ws --distro humble)
test-workspace *args:
    ./scripts/test-workspace.sh {{args}}

# ── Install / Setup ───────────────────────────────────────────────────────────

# Install the rosup binary to ~/.cargo/bin
install:
    cargo install --path crates/rosup-cli

# Install required dev tools
setup:
    rustup toolchain install nightly
    cargo install cargo-nextest
