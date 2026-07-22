#!/usr/bin/env bash
# =============================================================================
# verify-release.sh  —  Pumpkin Release QA Verification Script
# =============================================================================
#
# This script runs the full quality-assurance suite for a Pumpkin release.
# It is designed to be run both locally (by developers before tagging) and
# in CI (via .github/workflows/qa.yml).
#
# Stages (in order):
#   1. Environment checks (Rust toolchain, required tools)
#   2. Code formatting   (cargo fmt --check)
#   3. Linting           (cargo clippy -- -D warnings)
#   4. Compilation       (cargo check — all targets & features)
#   5. Unit tests        (cargo test)
#   6. Documentation tests (cargo test --doc)
#   7. Benchmarks        (cargo bench — smoke run)
#   8. Protocol fuzz tests (integration tests in pumpkin-protocol)
#   9. Malformed-packet resilience tests
#
# If any stage fails the script exits immediately with a non-zero status.
#
# Usage:
#   ./scripts/verify-release.sh          # full suite
#   ./scripts/verify-release.sh --fast   # skip benchmarks & doc-tests
#
# =============================================================================

set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Colour

FAST_MODE=false
if [[ "${1:-}" == "--fast" ]]; then
    FAST_MODE=true
    echo -e "${YELLOW}Fast mode enabled — skipping benchmarks and doc-tests.${NC}"
fi

# ── Helper ───────────────────────────────────────────────────────────────────
pass() { echo -e "${GREEN}[PASS]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; exit 1; }
info() { echo -e "${CYAN}[INFO]${NC} $*"; }
run() {
    echo -e "${YELLOW}[RUN]${NC} $*"
    if ! "$@"; then
        fail "Command failed: $*"
    fi
}

# ── Stage 0: Environment ─────────────────────────────────────────────────────
info "===== Stage 0: Environment Checks ====="

# Check Rust toolchain
if ! command -v rustc &>/dev/null; then
    fail "rustc not found. Install Rust: https://rustup.rs"
fi
rustc --version
cargo --version

info "Rust toolchain OK."

# Verify we are in the project root (contains Cargo.toml workspace)
if [[ ! -f Cargo.toml ]]; then
    fail "Cargo.toml not found. Run this script from the project root."
fi

# ── Stage 1: Formatting ──────────────────────────────────────────────────────
info ""
info "===== Stage 1: Formatting ====="
run cargo fmt --check
pass "cargo fmt passed."

# ── Stage 2: Clippy ──────────────────────────────────────────────────────────
info ""
info "===== Stage 2: Clippy (deny warnings) ====="
run cargo clippy --all-targets --all-features -- -D warnings
pass "cargo clippy passed."

# ── Stage 3: Compilation ─────────────────────────────────────────────────────
info ""
info "===== Stage 3: Compilation Check ====="
run cargo check --all-targets --all-features
pass "cargo check passed."

# ── Stage 4: Unit Tests ──────────────────────────────────────────────────────
info ""
info "===== Stage 4: Unit Tests ====="
run cargo test --all-features --verbose
pass "cargo test passed."

# ── Stage 5: Documentation Tests ─────────────────────────────────────────────
if [[ "$FAST_MODE" == false ]]; then
    info ""
    info "===== Stage 5: Documentation Tests ====="
    run cargo test --doc --verbose
    pass "cargo doc-test passed."
else
    info ""
    info "===== Stage 5: Documentation Tests (skipped --fast) ====="
fi

# ── Stage 6: Benchmarks (smoke) ──────────────────────────────────────────────
if [[ "$FAST_MODE" == false ]]; then
    info ""
    info "===== Stage 6: Benchmarks (smoke) ====="
    # Run each benchmark group once to verify they don't crash.
    # --warm-up-time 1 --measurement-time 1 makes this fast.
    run cargo bench --all-features -- --warm-up-time 1 --measurement-time 1
    pass "cargo bench passed."
else
    info ""
    info "===== Stage 6: Benchmarks (skipped --fast) ====="
fi

# ── Stage 7: Protocol Fuzz Tests (integration) ───────────────────────────────
info ""
info "===== Stage 7: Protocol Round-Trip Tests ====="
run cargo test --package pumpkin-protocol --test fuzz_tests --verbose
pass "Protocol round-trip tests passed."

# ── Stage 8: Malformed-Packet Resilience ─────────────────────────────────────
info ""
info "===== Stage 8: Malformed-Packet Resilience Tests ====="
run cargo test --package pumpkin-protocol --test malformed_packet --verbose
pass "Malformed-packet resilience tests passed."

# ── Stage 9: Protocol Benchmarks (same as Stage 6 but explicit) ──────────────
if [[ "$FAST_MODE" == false ]]; then
    info ""
    info "===== Stage 9: Packet Benchmarks ====="
    run cargo bench --package pumpkin-protocol -- --warm-up-time 1 --measurement-time 1
    pass "Packet benchmarks passed."
fi

# ── Done ─────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ALL QA CHECKS PASSED — RELEASE IS READY${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
