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

# ── Helper: format cargo test output ─────────────────────────────────────────
format_test_output() {
    while IFS= read -r line; do
        if [[ "$line" =~ ^test\ (.*)\ \.\.\.\ ok$ ]]; then
            printf "${GREEN}  ✓ ${BASH_REMATCH[1]}${RESET}\n"
        elif [[ "$line" =~ ^test\ (.*)\ \.\.\.\ FAILED$ ]]; then
            printf "${RED}  ✗ ${BASH_REMATCH[1]}${RESET}\n"
        elif [[ "$line" == "test result:"* ]]; then
            : # skip
        else
            printf "%s\n" "$line"
        fi
    done
}

# ── Build ────────────────────────────────────────────────────────────────────
printf "${BOLD}Building token-saver...${RESET}\n"
cargo build --manifest-path="$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1
printf "${GREEN}✓ Build complete${RESET}\n\n"

# ── Command selection ────────────────────────────────────────────────────────
echo "Available commands with compressors:"
echo "  1) git status"
echo "  2) git diff"
echo "  3) git log"
echo "  4) git show"
echo "  5) ls"
echo "  6) find"
echo "  7) grep"
echo "  8) git branch"
echo "  9) cat"
echo " 10) eslint"
echo " 11) prettier"
echo " 12) npx prettier"
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
    3|"git log"|"git-log"|"log")
        TEST_TARGET="git_log"
        COMPARE_FN="compare_git_log"
        LABEL="git log"
        ;;
    4|"git show"|"git-show"|"show")
        TEST_TARGET="git_show"
        COMPARE_FN="compare_git_show"
        LABEL="git show"
        ;;
    5|"ls")
        TEST_TARGET="ls"
        COMPARE_FN="compare_ls"
        LABEL="ls"
        ;;
    6|"find")
        TEST_TARGET="find"
        COMPARE_FN="compare_find"
        LABEL="find"
        ;;
    7|"grep"|"rg")
        TEST_TARGET="grep"
        COMPARE_FN="compare_grep"
        LABEL="grep / rg"
        ;;
    8|"git branch"|"git-branch"|"branch")
        TEST_TARGET="git_branch"
        COMPARE_FN="compare_git_branch"
        LABEL="git branch"
        ;;
    9|"cat")
        TEST_TARGET="cat"
        COMPARE_FN="compare_cat"
        LABEL="cat"
        ;;
    10|"eslint")
        TEST_TARGET="eslint"
        COMPARE_FN="compare_eslint"
        LABEL="eslint"
        ;;
    11|"prettier")
        TEST_TARGET="prettier"
        COMPARE_FN="compare_prettier"
        LABEL="prettier"
        ;;
    12|"npx prettier"|"npx-prettier")
        TEST_TARGET="npx_prettier"
        COMPARE_FN="compare_npx_prettier"
        LABEL="npx prettier"
        ;;
    *)
        echo "Unknown command: $choice"
        echo "Currently supported: git status, git diff, git log, git show, ls, find, grep, git branch, cat, eslint, prettier, npx prettier"
        exit 1
        ;;
esac

printf "\n${BOLD}Testing: %s${RESET}\n\n" "$LABEL"

# ── Run integration tests ────────────────────────────────────────────────────
printf "${BOLD}${CYAN}▸ Integration tests${RESET}\n"
printf "${DIM}  cargo test --test %s${RESET}\n\n" "$TEST_TARGET"

if cargo test --manifest-path="$PROJECT_DIR/Cargo.toml" --test "$TEST_TARGET" 2>&1 | format_test_output; then
    printf "\n${GREEN}✓ All integration tests passed${RESET}\n"
else
    printf "\n${RED}✗ Some integration tests failed${RESET}\n"
fi

# ── Run visual comparison ────────────────────────────────────────────────────
printf "\n${BOLD}${CYAN}▸ Token comparison${RESET}\n"
printf "${DIM}  cargo test --test compare %s -- --ignored --nocapture${RESET}\n" "$COMPARE_FN"

cargo test --manifest-path="$PROJECT_DIR/Cargo.toml" --test compare "$COMPARE_FN" -- --ignored --nocapture 2>&1 | format_test_output
