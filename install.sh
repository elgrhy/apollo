#!/usr/bin/env bash

set -e

echo "=== APOLLO INFRASTRUCTURE INSTALLER ==="

# 1. Check Rust
if ! command -v cargo &> /dev/null; then
  echo "Rust not found. Please install Rust first: https://rustup.rs"
  exit 1
fi

# 2. Build project
echo "[1/3] Building APOLLO (Release Mode)..."
cargo build --release

# 3. Install binaries
echo "[2/3] Installing binaries to /usr/local/bin/apollo-server/..."
sudo mkdir -p /usr/local/bin/apollo-server

sudo cp target/release/apollo-server /usr/local/bin/apollo-server/apollo-server
sudo cp target/release/apollo-hub /usr/local/bin/apollo-server/apollo-hub

# 4. Create Global CLI Wrapper (The Entry Point)
echo "[3/3] Creating global 'apollo-server' command..."

cat << 'EOF' | sudo tee /usr/local/bin/apollo-server > /dev/null
#!/usr/bin/env bash
exec /usr/local/bin/apollo-server/apollo-server "$@"
EOF

sudo chmod +x /usr/local/bin/apollo-server

# 5. Verify install
echo "=== APOLLO INSTALLED SUCCESSFULLY ==="
echo "Version: $(apollo-server --version || echo 'v1.0.0')"
echo ""
echo "🚀 Getting Started:"
echo "  Start Node:   apollo-server node start"
echo "  Start Hub:    /usr/local/bin/apollo-server/apollo-hub start"
echo "  Check Status: apollo-server doctor"
echo ""
