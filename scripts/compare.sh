#!/usr/bin/env bash
set -euo pipefail

BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[0;32m'
RED='\033[0;31m'
CYAN='\033[0;36m'
RESET='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Build ────────────────────────────────────────────────────────────────────
printf "${BOLD}Building token-saver...${RESET}\n"
cargo build --manifest-path="$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1
printf "${GREEN}✓ Build complete${RESET}\n\n"

# ── Command selection ────────────────────────────────────────────────────────
echo "Available commands with compressors:"
echo "  1) git status"
echo "  2) git diff"
echo
read -rp "Enter command to test (or number): " choice

# Map choice to cargo test targets
case "$choice" in
    1|"git status"|"git-status"|"status")
        TEST_TARGET="git_status"
        COMPARE_FN="compare_git_status"
        LABEL="git status"
        ;;
    2|"git diff"|"git-diff"|"diff")
        TEST_TARGET="git_diff"
        COMPARE_FN="compare_git_diff"
        LABEL="git diff"
        ;;
    *)
        echo "Unknown command: $choice"
        echo "Currently supported: git status, git diff"
        exit 1
        ;;
esac

printf "\n${BOLD}Testing: %s${RESET}\n\n" "$LABEL"

# ── Run integration tests ────────────────────────────────────────────────────
printf "${BOLD}${CYAN}▸ Integration tests${RESET}\n"
printf "${DIM}  cargo test --test %s${RESET}\n\n" "$TEST_TARGET"

if cargo test --manifest-path="$PROJECT_DIR/Cargo.toml" --test "$TEST_TARGET" 2>&1; then
    printf "\n${GREEN}✓ All integration tests passed${RESET}\n"
else
    printf "\n${RED}✗ Some integration tests failed${RESET}\n"
fi

# ── Run visual comparison ────────────────────────────────────────────────────
printf "\n${BOLD}${CYAN}▸ Token comparison${RESET}\n"
printf "${DIM}  cargo test --test compare %s -- --ignored --nocapture${RESET}\n" "$COMPARE_FN"

cargo test --manifest-path="$PROJECT_DIR/Cargo.toml" --test compare "$COMPARE_FN" -- --ignored --nocapture 2>&1
