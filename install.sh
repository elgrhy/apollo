#!/usr/bin/env bash

set -e

VERSION="apollo-v1.0"
echo "=== APOLLO INFRASTRUCTURE INSTALLER ($VERSION) ==="

# 1. Check Rust
if ! command -v cargo &> /dev/null; then
  echo "Rust not found. Please install Rust first: https://rustup.rs"
  exit 1
fi

# 2. Build project
echo "[1/3] Building APOLLO (Deterministic Release Mode)..."
cargo build --release

# 3. Install binaries
echo "[2/3] Installing binaries to /usr/local/bin/apollo-node/..."
sudo mkdir -p /usr/local/bin/apollo-node

sudo cp target/release/apollo /usr/local/bin/apollo-node/apollo
sudo cp target/release/apollo-hub /usr/local/bin/apollo-node/apollo-hub

# 4. Create Global CLI Wrapper
echo "[3/3] Creating global 'apollo' command..."

cat << 'EOF' | sudo tee /usr/local/bin/apollo > /dev/null
#!/usr/bin/env bash
exec /usr/local/bin/apollo-node/apollo "$@"
EOF

sudo chmod +x /usr/local/bin/apollo

# 5. Verify install
echo "=== APOLLO INSTALLED SUCCESSFULLY ==="
/usr/local/bin/apollo doctor
echo ""
echo "🚀 Mission Control Active:"
echo "  Start Node:   apollo node start"
echo "  Start Hub:    /usr/local/bin/apollo-node/apollo-hub start"
echo ""
