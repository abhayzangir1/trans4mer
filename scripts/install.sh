#!/usr/bin/env bash
# Trans4mers developer environment installer (Linux/macOS).
set -euo pipefail

echo "==> Trans4mers development setup"

# Rust
if ! command -v cargo >/dev/null 2>&1; then
  echo "==> Installing Rust (rustup)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# Node prerequisite check
if ! command -v npm >/dev/null 2>&1; then
  echo "!! Node.js/npm is required but not found — install Node 22+ first:"
  echo "      https://nodejs.org/"
  exit 1
fi

# Linux system deps for Tauri 2
if [[ "$(uname)" == "Linux" ]]; then
  echo "==> Installing Tauri Linux system dependencies"
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y \
      libwebkit2gtk-4.1-dev libgtk-3-dev build-essential curl wget file \
      libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  else
    echo "!! Unsupported package manager — install webkit2gtk-4.1 + gtk3 manually."
  fi
fi

# Ollama (optional, for the local LLM). ASK FIRST — nothing is installed
# without explicit consent (and NO MODEL is ever installed by this script
# or by the app; you pull models yourself, e.g.
# `ollama pull qwen2.5-coder:3b`).
if ! command -v ollama >/dev/null 2>&1; then
  echo "==> Ollama (the local LLM runtime) is not installed."
  read -r -p "    Install Ollama now? [y/N] " answer
  if [[ "${answer:-n}" == "y" || "${answer:-n}" == "Y" ]]; then
    curl -fsSL https://ollama.com/install.sh | sh || echo "!! Ollama install failed"
  else
    echo "    Skipped. Install it yourself from https://ollama.com if you want local models."
  fi
fi

echo "==> Frontend dependencies"
(cd apps/desktop && npm install)

echo ""
echo "Setup complete. Run the app:"
echo "  cd apps/desktop && npm run tauri dev"
echo "Run the test suite:"
echo "  cargo test --workspace --exclude trans4mers-desktop"
