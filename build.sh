#!/usr/bin/env bash
# Build CaptainFM in release mode and install it to ~/.local/bin, with desktop entry and icon.
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release
install -Dm755 target/release/captainfm "$HOME/.local/bin/captainfm"
ln -sf captainfm "$HOME/.local/bin/cfm"
"$HOME/.local/bin/captainfm" --install
echo "Installed: $HOME/.local/bin/captainfm (alias: cfm)"
echo "Make it your default folder handler with: captainfm --default   (undo: captainfm --default off)"
