#!/usr/bin/env bash
# Trans4mers Local Packaging Script (macOS / Linux)
# Builds frontend and packages native DMG or AppImage

set -euo pipefail

echo "========================================="
echo "   Trans4mers Desktop Packaging Script   "
echo "========================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"
DESKTOP_DIR="$WORKSPACE_ROOT/apps/desktop"

echo ""
echo "[1/3] Validating Cargo and Node..."
command -v cargo >/dev/null 2>&1 || { echo "Cargo is not installed. Visit https://rustup.rs"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "npm is not installed. Visit https://nodejs.org"; exit 1; }

echo "[2/3] Building Vite Production Frontend..."
cd "$DESKTOP_DIR"
npm run build

echo ""
echo "[3/3] Compiling Tauri Release Bundle..."
npm run tauri build

echo ""
echo "========================================="
echo "  Build Complete! Bundle located in: "
echo "  $DESKTOP_DIR/src-tauri/target/release/bundle/"
echo "========================================="
