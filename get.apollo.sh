#!/usr/bin/env bash

# APOLLO One-Line Installer
# Usage: curl -fsSL https://get.apollo.systems | bash

set -e

REPO_URL="https://github.com/elgrhy/apollo.git"
VERSION="apollo-v1.0"
INSTALL_DIR="/usr/local/bin/apollo-node"
BINARY_DIR="target/release"

echo "=== APOLLO MISSION CONTROL INSTALLER ==="

# 1. Environment Check
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: Rust/Cargo not found. Please install Rust: https://rustup.rs"
    exit 1
fi

if ! command -v git &> /dev/null; then
    echo "❌ Error: Git not found."
    exit 1
fi

# 2. Temporary Build Directory
TEMP_DIR=$(mktemp -d)
echo "📂 Working in temporary directory: $TEMP_DIR"
cd "$TEMP_DIR"

# 3. Clone and Build
echo "🛰️ Cloning Apollo ($VERSION)..."
git clone --branch "$VERSION" --depth 1 "$REPO_URL" .

echo "🏗️ Building Production Binaries..."
cargo build --release

# 3.5 Binary Integrity Verification
echo "🔢 Verifying Binary Integrity..."
if [ -f "CHECKSUMS.sha256" ]; then
    shasum -a 256 -c CHECKSUMS.sha256
else
    echo "⚠️ Warning: No CHECKSUMS.sha256 found in release. Proceeding with caution."
fi

# 4. Install
echo "🛡️ Installing to $INSTALL_DIR..."
sudo mkdir -p "$INSTALL_DIR"
sudo cp "$BINARY_DIR/apollo" "$INSTALL_DIR/apollo"
sudo cp "$BINARY_DIR/apollo-hub" "$INSTALL_DIR/apollo-hub"

# 5. Create Global Symlink
echo "🔗 Creating global 'apollo' command..."
sudo ln -sf "$INSTALL_DIR/apollo" /usr/local/bin/apollo

# 6. Verify
echo "✅ Installation Complete!"
apollo doctor

echo ""
echo "🚀 APOLLO v1.0 is now active."
echo "Type 'apollo' to enter Mission Control."
