#!/bin/bash
# Update Homebrew formula with new version and checksums
#
# Usage: ./scripts/update-homebrew-formula.sh <version> <checksums-file>
# Example: ./scripts/update-homebrew-formula.sh 0.2.0 dist/checksums-sha256.txt
#
# The checksums file should contain lines like:
#   abc123...  track-0.2.0-aarch64-apple-darwin.tar.gz
#   def456...  track-0.2.0-x86_64-apple-darwin.tar.gz

set -euo pipefail

VERSION="${1:-}"
CHECKSUMS_FILE="${2:-}"
FORMULA_PATH="homebrew/Formula/track.rb"

if [[ -z "$VERSION" || -z "$CHECKSUMS_FILE" ]]; then
    echo "Usage: $0 <version> <checksums-file>"
    echo "Example: $0 0.2.0 dist/checksums-sha256.txt"
    exit 1
fi

if [[ ! -f "$CHECKSUMS_FILE" ]]; then
    echo "Error: Checksums file not found: $CHECKSUMS_FILE"
    exit 1
fi

if [[ ! -f "$FORMULA_PATH" ]]; then
    echo "Error: Formula file not found: $FORMULA_PATH"
    exit 1
fi

echo "Updating formula to version $VERSION..."

# Extract checksums from the file
get_checksum() {
    local pattern="$1"
    grep "$pattern" "$CHECKSUMS_FILE" | awk '{print $1}'
}

SHA_ARM64_MACOS=$(get_checksum "aarch64-apple-darwin")
SHA_X86_64_MACOS=$(get_checksum "x86_64-apple-darwin")
SHA_ARM64_LINUX=$(get_checksum "aarch64-unknown-linux-gnu")
SHA_X86_64_LINUX=$(get_checksum "x86_64-unknown-linux-gnu")

echo "Checksums found:"
echo "  macOS ARM64:  ${SHA_ARM64_MACOS:-NOT FOUND}"
echo "  macOS x86_64: ${SHA_X86_64_MACOS:-NOT FOUND}"
echo "  Linux ARM64:  ${SHA_ARM64_LINUX:-NOT FOUND}"
echo "  Linux x86_64: ${SHA_X86_64_LINUX:-NOT FOUND}"

# Update version
sed -i.bak "s/version \".*\"/version \"$VERSION\"/" "$FORMULA_PATH"

# Update checksums
if [[ -n "$SHA_ARM64_MACOS" ]]; then
    sed -i.bak "s/PLACEHOLDER_SHA256_ARM64/$SHA_ARM64_MACOS/" "$FORMULA_PATH"
    # Also replace existing sha256 for arm64 darwin
    sed -i.bak "/aarch64-apple-darwin/,/sha256/{s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA_ARM64_MACOS\"/}" "$FORMULA_PATH"
fi

if [[ -n "$SHA_X86_64_MACOS" ]]; then
    sed -i.bak "s/PLACEHOLDER_SHA256_X86_64/$SHA_X86_64_MACOS/" "$FORMULA_PATH"
    sed -i.bak "/x86_64-apple-darwin/,/sha256/{s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA_X86_64_MACOS\"/}" "$FORMULA_PATH"
fi

if [[ -n "$SHA_ARM64_LINUX" ]]; then
    sed -i.bak "s/PLACEHOLDER_SHA256_LINUX_ARM64/$SHA_ARM64_LINUX/" "$FORMULA_PATH"
    sed -i.bak "/aarch64-unknown-linux-gnu/,/sha256/{s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA_ARM64_LINUX\"/}" "$FORMULA_PATH"
fi

if [[ -n "$SHA_X86_64_LINUX" ]]; then
    sed -i.bak "s/PLACEHOLDER_SHA256_LINUX_X86_64/$SHA_X86_64_LINUX/" "$FORMULA_PATH"
    sed -i.bak "/x86_64-unknown-linux-gnu/,/sha256/{s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA_X86_64_LINUX\"/}" "$FORMULA_PATH"
fi

# Clean up backup files
rm -f "${FORMULA_PATH}.bak"

echo "Formula updated successfully!"
echo ""
echo "Next steps:"
echo "  1. Review changes: git diff $FORMULA_PATH"
echo "  2. Commit and push: git add $FORMULA_PATH && git commit -m 'Bump to $VERSION'"
