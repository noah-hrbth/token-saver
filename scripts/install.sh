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

# Create symlinks (remove old ones first for idempotency)
COMMANDS=(git)
for cmd in "${COMMANDS[@]}"; do
    rm -f "$INSTALL_DIR/$cmd"
    ln -s token-saver "$INSTALL_DIR/$cmd"
    echo "  Created symlink: $INSTALL_DIR/$cmd -> token-saver"
done

echo ""
echo "Installation complete!"
echo ""
echo "To configure Claude Code, add this to ~/.claude/settings.json:"
echo ""
echo '{'
echo '  "env": {'
echo "    \"TOKEN_SAVER\": \"1\","
echo "    \"PATH\": \"$INSTALL_DIR:\$PATH\""
echo '  }'
echo '}'
echo ""
echo "To test manually:"
echo "  TOKEN_SAVER=1 $INSTALL_DIR/git status"
