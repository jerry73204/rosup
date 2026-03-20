#!/usr/bin/env bash
# test-workspace.sh — Run common rosup operations against a workspace.
#
# Usage:
#   ./scripts/test-workspace.sh <workspace_dir> [--distro <distro>] [--exclude <pattern>...]
#
# Examples:
#   ./scripts/test-workspace.sh ~/repos/autoware/1.5.0-ws
#   ./scripts/test-workspace.sh ~/repos/AutoSDV --exclude 'src/localization/autoware_isaac_*' '**/tests/**'
#   ./scripts/test-workspace.sh ~/repos/cuda_ndt_matcher --distro humble

set -uo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

pass() { printf "${GREEN}✓${NC} %s\n" "$*"; }
fail() { printf "${RED}✗${NC} %s\n" "$*"; }
info() { printf "${CYAN}→${NC} %s\n" "$*"; }
warn() { printf "${YELLOW}!${NC} %s\n" "$*"; }

# ── Parse args ────────────────────────────────────────────────────────────────

WS_DIR=""
DISTRO="${ROS_DISTRO:-}"
EXCLUDES=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --distro) DISTRO="$2"; shift 2 ;;
        --exclude)
            shift
            while [[ $# -gt 0 && ! "$1" =~ ^-- ]]; do
                EXCLUDES+=("$1")
                shift
            done
            ;;
        -*) echo "Unknown option: $1"; exit 1 ;;
        *) WS_DIR="$1"; shift ;;
    esac
done

if [[ -z "$WS_DIR" ]]; then
    echo "Usage: $0 <workspace_dir> [--distro <distro>] [--exclude <pattern>...]"
    exit 1
fi

WS_DIR="$(realpath "$WS_DIR")"
if [[ ! -d "$WS_DIR" ]]; then
    echo "Directory not found: $WS_DIR"
    exit 1
fi

DISTRO_FLAG=()
if [[ -n "$DISTRO" ]]; then
    DISTRO_FLAG=(--ros-distro "$DISTRO")
    export ROS_DISTRO="$DISTRO"
fi

CLEANUP_FILES=()

cleanup() {
    for f in "${CLEANUP_FILES[@]}"; do
        rm -f "$f" 2>/dev/null
    done
}
trap cleanup EXIT

cd "$WS_DIR"
echo ""
info "Testing workspace: $WS_DIR"
info "ROS distro: ${DISTRO:-<not set>}"
echo ""

# ── Init ──────────────────────────────────────────────────────────────────────

CREATED_TOML=false
if [[ -f rosup.toml ]]; then
    info "rosup.toml already exists"
else
    info "Creating rosup.toml"
    if rosup init --workspace "${DISTRO_FLAG[@]}" --force 2>&1; then
        pass "init --workspace"
        CREATED_TOML=true
        CLEANUP_FILES+=(rosup.toml .gitignore)
    else
        fail "init --workspace"
        exit 1
    fi
fi
echo ""

# ── Excludes ──────────────────────────────────────────────────────────────────

if [[ ${#EXCLUDES[@]} -gt 0 ]]; then
    for pattern in "${EXCLUDES[@]}"; do
        if rosup exclude "$pattern" 2>&1; then
            pass "exclude $pattern"
        else
            fail "exclude $pattern"
        fi
    done
    echo ""
    info "Current exclude list:"
    rosup exclude --list 2>&1
    echo ""
fi

# ── Discovery ─────────────────────────────────────────────────────────────────

info "Package discovery"
COUNT=$(rosup sync --lock --dry-run 2>&1 | grep "^+" | wc -l)
pass "Discovered $COUNT packages"
echo ""

# ── Search ────────────────────────────────────────────────────────────────────

if [[ -n "$DISTRO" ]]; then
    info "Search"
    if rosup search rclcpp --limit 2 2>&1 | head -3; then
        pass "search works"
    else
        fail "search"
    fi
    echo ""
fi

# ── Resolve ───────────────────────────────────────────────────────────────────

info "Dependency resolution (dry-run)"
RESOLVE_OUT=$(rosup resolve --dry-run 2>&1 || true)
RESOLVED=$(echo "$RESOLVE_OUT" | grep -c "^would resolve" || true)
WARNINGS=$(echo "$RESOLVE_OUT" | grep -c "^warning" || true)

if [[ $RESOLVED -gt 0 ]]; then
    pass "Resolved $RESOLVED dependencies"
fi
if [[ $WARNINGS -gt 0 ]]; then
    warn "$WARNINGS unresolved dependencies:"
    echo "$RESOLVE_OUT" | grep "^warning" | head -5
    if [[ $WARNINGS -gt 5 ]]; then
        echo "  ... ($((WARNINGS - 5)) more)"
    fi
fi
echo ""

# ── Add / Remove ──────────────────────────────────────────────────────────────

# Find a package to test add/remove on.
FIRST_PKG=$(rosup sync --lock --dry-run 2>&1 | grep "^+" | head -1 | sed 's/^+ //')
if [[ -n "$FIRST_PKG" ]]; then
    # Extract the package name from package.xml.
    PKG_XML="$WS_DIR/$FIRST_PKG/package.xml"
    if [[ -f "$PKG_XML" ]]; then
        PKG_NAME=$(grep -oP '(?<=<name>)[^<]+' "$PKG_XML" | head -1)
        if [[ -n "$PKG_NAME" ]]; then
            info "Testing add/remove on $PKG_NAME"
            if rosup add __test_dep__ -p "$PKG_NAME" 2>&1 >/dev/null; then
                if rosup remove __test_dep__ -p "$PKG_NAME" 2>&1 >/dev/null; then
                    pass "add -p / remove -p roundtrip"
                else
                    fail "remove -p $PKG_NAME"
                fi
            else
                fail "add -p $PKG_NAME"
            fi
        fi
    fi
    echo ""
fi

# ── Manifest path ─────────────────────────────────────────────────────────────

info "Testing --manifest-path from /tmp"
if (cd /tmp && rosup --manifest-path "$WS_DIR/rosup.toml" exclude --list 2>&1 >/dev/null); then
    pass "--manifest-path works"
else
    fail "--manifest-path"
fi
echo ""

# ── Summary ───────────────────────────────────────────────────────────────────

echo "────────────────────────────────────────"
info "Summary: $COUNT packages, $RESOLVED resolved deps, $WARNINGS unresolved"
if [[ "$CREATED_TOML" == true ]]; then
    info "Cleaning up generated rosup.toml"
fi
