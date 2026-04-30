#!/usr/bin/env bash
set -euo pipefail

# Uninstall token-saver:
#   1. Run `token-saver uninstall` to undo the shell-profile and
#      ~/.claude/settings.json edits that `token-saver init` made.
#   2. Remove the binary at ~/.token-saver/bin/token-saver.
#   3. Prune the install dir if it becomes empty.

INSTALL_DIR="$HOME/.token-saver/bin"
BIN="$INSTALL_DIR/token-saver"

run_uninstall_subcommand() {
    if [ -x "$BIN" ]; then
        "$BIN" uninstall
    elif command -v token-saver >/dev/null 2>&1; then
        token-saver uninstall
    else
        echo "token-saver binary not found — skipping profile/settings cleanup"
        echo "  (if you previously installed elsewhere, run \`<path-to>/token-saver uninstall\` manually)"
    fi
}

remove_binary_and_dirs() {
    if [ -e "$BIN" ]; then
        rm -f "$BIN"
        echo "Removed $BIN"
    fi

    if [ -d "$INSTALL_DIR" ] && rmdir "$INSTALL_DIR" 2>/dev/null; then
        echo "Removed $INSTALL_DIR"
    fi

    if [ -d "$HOME/.token-saver" ] && rmdir "$HOME/.token-saver" 2>/dev/null; then
        echo "Removed $HOME/.token-saver"
    fi
}

print_reload_hint() {
    case "$(basename "${SHELL:-}")" in
        zsh)  echo "  source ~/.zshenv" ;;
        bash) echo "  source ~/.bashrc" ;;
        *)    echo "  Restart your shell" ;;
    esac
}

run_uninstall_subcommand
remove_binary_and_dirs

echo ""
echo "Uninstall complete. Reload your shell to drop the wrappers:"
print_reload_hint
