#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.token-saver/bin"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building token-saver (release)..."
cd "$PROJECT_DIR"
cargo build --release

echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

# Copy binary
cp target/release/token-saver "$INSTALL_DIR/token-saver"
chmod +x "$INSTALL_DIR/token-saver"

# Clean up legacy symlinks from older installs
COMMANDS=(cat eslint git jest ls find grep npx prettier rg)
for cmd in "${COMMANDS[@]}"; do
    if [ -L "$INSTALL_DIR/$cmd" ]; then
        rm -f "$INSTALL_DIR/$cmd"
        echo "  Removed legacy symlink: $INSTALL_DIR/$cmd"
    fi
done

# Add shell functions to profile (guarded by TOKEN_SAVER env var).
#
# Why functions instead of PATH symlinks:
#   Tools like Oh My Zsh call `command git` internally, which bypasses
#   shell functions and goes straight to PATH lookup → real git.
#   Only interactive user/agent calls go through the function → token-saver.
#   This guarantees no external tool ever sees compressed output.
#
# Why .zshenv (not .zshrc) for zsh:
#   Claude Code's Bash tool runs commands in a non-interactive zsh subshell.
#   Non-interactive zsh sources .zshenv but NOT .zshrc, so functions in .zshrc
#   are never defined for agent tool calls. .zshenv is sourced for all zsh
#   instances. The TOKEN_SAVER=1 guard inside the block keeps it a no-op
#   in normal (non-agent) contexts.
HOOK_BLOCK='# token-saver: wrap commands for LLM output compression
if [ "$TOKEN_SAVER" = "1" ]; then
    cat() { "$HOME/.token-saver/bin/token-saver" cat "$@"; }
    eslint() { "$HOME/.token-saver/bin/token-saver" eslint "$@"; }
    git() { "$HOME/.token-saver/bin/token-saver" git "$@"; }
    jest() { "$HOME/.token-saver/bin/token-saver" jest "$@"; }
    ls() { "$HOME/.token-saver/bin/token-saver" ls "$@"; }
    find() { "$HOME/.token-saver/bin/token-saver" find "$@"; }
    grep() { "$HOME/.token-saver/bin/token-saver" grep "$@"; }
    npx() { "$HOME/.token-saver/bin/token-saver" npx "$@"; }
    prettier() { "$HOME/.token-saver/bin/token-saver" prettier "$@"; }
    rg() { "$HOME/.token-saver/bin/token-saver" rg "$@"; }
fi'

add_shell_hook() {
    local profile="$1"
    # Remove legacy PATH hook from older installs
    if [ -f "$profile" ] && grep -qF 'token-saver/bin:$PATH' "$profile"; then
        # Remove the old PATH-based hook lines
        sed -i.bak '/# token-saver: prepend wrapper/d; /token-saver\/bin:\$PATH/d' "$profile"
        rm -f "${profile}.bak"
        echo "  Removed legacy PATH hook from $profile"
    fi
    if [ -f "$profile" ] && grep -qF 'token-saver' "$profile"; then
        echo "  Shell hook already in $profile — skipping"
        return
    fi
    # Create profile if it doesn't exist
    touch "$profile"
    printf '\n%s\n' "$HOOK_BLOCK" >> "$profile"
    echo "  Added shell functions to $profile"
}

echo ""
echo "Configuring shell profile..."
SHELL_NAME="$(basename "$SHELL")"
case "$SHELL_NAME" in
    zsh)  add_shell_hook "$HOME/.zshenv" ;;
    bash) add_shell_hook "$HOME/.bashrc" ;;
    *)    echo "  Unknown shell ($SHELL_NAME) — add this to your profile manually:"
          echo "    $HOOK_BLOCK" ;;
esac

echo ""
echo "Installation complete!"
echo ""
echo "To configure Claude Code, add this to ~/.claude/settings.json"
echo "inside the top-level object:"
echo ""
echo '  "env": {'
echo '    "TOKEN_SAVER": "1"'
echo '  }'
echo ""
echo "To test manually:"
echo "  source ~/.zshenv  # or ~/.bashrc"
echo "  TOKEN_SAVER=1 git status"
