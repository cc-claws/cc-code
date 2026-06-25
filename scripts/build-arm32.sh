#!/bin/bash
# Build script for peri on ARM32 (arm-unknown-linux-musleabihf)
# Target: arm-unknown-linux-musleabihf (32-bit ARM hard-float, musl libc)
#
# Environment:
#   MUSL_TARGET  - ARM musl cross-compiler prefix
#                   e.g. arm-linux-musleabihf or arm-unknown-linux-musleabihf
#                   (defaults to arm-linux-musleabihf from musl-tools package)
#
# Usage:
#   ./scripts/build-arm32.sh          # release build (default)
#   ./scripts/build-arm32.sh dev      # dev build
#   ./scripts/build-arm32.sh pkg      # build release and create deploy tarball

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR"

# ARM32 hard-float musl target
TARGET="arm-unknown-linux-musleabihf"

# Use stable toolchain to avoid nightly issues
export RUSTC="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc"
export CARGO="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo"

# Ensure ARM musl target is installed
"$CARGO" +stable target add "$TARGET" 2>/dev/null || true

MODE="${1:-release}"

build_release() {
    echo "=== Building peri for ARM32 (musl static, release) ==="
    # Static linking: --features with LTO for single standalone binary
    "$CARGO" build --target "$TARGET" --release
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/release/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/release/peri"
    echo ""
    echo "=== Checking dependencies ==="
    ldd "$SCRIPT_DIR/target/$TARGET/release/peri" 2>/dev/null || echo "(static binary - ldd shows no dynamic dependencies)"
}

build_dev() {
    echo "=== Building peri for ARM32 (musl static, dev) ==="
    "$CARGO" build --target "$TARGET"
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/debug/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/debug/peri"
}

package_deploy() {
    echo ""
    echo "=== Creating deploy package ==="

    PERI_BIN="$SCRIPT_DIR/target/$TARGET/release/peri"
    if [ ! -f "$PERI_BIN" ]; then
        echo "Release binary not found, run: $0 release"
        exit 1
    fi

    PKG_DIR="$SCRIPT_DIR/deploy/peri-arm32-v0.2.0"
    mkdir -p "$PKG_DIR"

    # Copy binary only (static linking - no .so libraries needed)
    cp "$PERI_BIN" "$PKG_DIR/peri"

    # Create tarball
    DATE=$(date +%Y%m%d)
    PKG_TAR="$SCRIPT_DIR/deploy/peri-arm32-v0.2.0-$DATE.tar.gz"
    cd "$SCRIPT_DIR/deploy"
    tar -czf "$PKG_TAR" \
        --exclude='deploy.sh' \
        --exclude='*.sh' \
        --exclude='README.md' \
        --exclude='env.example' \
        --exclude='lib/' \
        'peri-arm32-v0.2.0'
    cd "$SCRIPT_DIR"

    echo ""
    echo "=== Deploy package created ==="
    ls -lh "$PKG_TAR"
    echo ""
    echo "Package contents:"
    tar -tzf "$PKG_TAR" | head -10
    echo ""
    echo "To deploy:"
    echo "  tar -xzf $PKG_TAR -C /"
    echo "  # Then on target:"
    echo "  /opt/peri/peri --print 'hello'"
}

case "$MODE" in
    release)
        build_release
        echo ""
        echo "=== Build complete ==="
        echo "Target: $TARGET"
        echo ""
        echo "To deploy: scp target/$TARGET/release/peri root@<board-ip>:/usr/local/bin/"
        ;;
    dev)
        build_dev
        echo ""
        echo "=== Build complete ==="
        echo "Target: $TARGET"
        ;;
    pkg)
        build_release
        package_deploy
        ;;
    *)
        echo "Unknown mode: $MODE"
        echo "Usage: $0 [release|dev|pkg]"
        exit 1
        ;;
esac
