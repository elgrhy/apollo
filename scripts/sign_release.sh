#!/usr/bin/env bash
# Generate checksums for Apollo v1.0 Release

set -e

VERSION="apollo-v1.0"
RELEASE_DIR="target/release"
MANIFEST="CHECKSUMS.sha256"

echo "=== APOLLO RELEASE SIGNER ($VERSION) ==="

if [ ! -f "$RELEASE_DIR/apollo" ]; then
    echo "❌ Error: Release binaries not found. Run cargo build --release first."
    exit 1
fi

echo "🔢 Generating SHA256 checksums..."
shasum -a 256 "$RELEASE_DIR/apollo" > "$MANIFEST"
shasum -a 256 "$RELEASE_DIR/apollo-hub" >> "$MANIFEST"

echo "📝 Checksums generated in $MANIFEST:"
cat "$MANIFEST"

# In a real environment, we would run:
# gpg --detach-sign -a $MANIFEST
# to create CHECKSUMS.sha256.asc

echo ""
echo "✅ Release Manifest Ready for Verification."
