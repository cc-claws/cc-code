#!/bin/bash
#
# peri ARM32 Deployment Script (musl static binary)
#
# Usage:
#   ./deploy.sh                      # interactive deploy
#   ./deploy.sh root@<ip>           # SSH deploy
#   ./deploy.sh /dev/sdX            # SD card deploy
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PERI_BIN="$SCRIPT_DIR/peri"
TARGET_DIR="/opt/peri"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()    { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
usage()   { echo "Usage: $0 [target]"
           echo ""
           echo "Targets:"
           echo "  root@<ip>       SSH deploy to remote device"
           echo "  /dev/sdX       Deploy to SD card (requires root)"
           echo "  local          Create local package only"
           echo ""
           echo "Examples:"
           echo "  $0 root@192.168.1.100"
           echo "  $0 /dev/sdc"
           echo "  $0 local"
           exit 1; }

check_prereq() {
    info "Checking prerequisites..."

    if [ ! -f "$PERI_BIN" ]; then
        error "peri binary not found: $PERI_BIN"
        exit 1
    fi

    # Check if ARM ELF (static musl binary)
    if ! file "$PERI_BIN" 2>/dev/null | grep -q "ARM"; then
        warn "peri may not be an ARM binary"
    fi

    info "Prerequisites check passed"
}

deploy_ssh() {
    local DEST="$1"
    info "Deploying via SSH to: $DEST"

    # Test connection
    if ! ssh -o ConnectTimeout=5 "$DEST" "echo 'SSH OK'" > /dev/null 2>&1; then
        error "Cannot connect to $DEST"
        exit 1
    fi

    # Create target directory
    if ! ssh "$DEST" "[ -d $TARGET_DIR ]" 2>/dev/null; then
        info "Creating target directory: $TARGET_DIR"
        ssh "$DEST" "sudo mkdir -p $TARGET_DIR"
    fi

    # Transfer binary
    info "Transferring peri binary..."
    scp "$PERI_BIN" "$DEST:$TARGET_DIR/peri"
    ssh "$DEST" "sudo chmod +x $TARGET_DIR/peri"

    # Verify
    info "Verifying deployment..."
    ssh "$DEST" "$TARGET_DIR/peri --version" \
        || warn "Verification failed (may need API key)"

    info "Deployment complete!"
    info ""
    info "Run:"
    info "  $TARGET_DIR/peri --print 'hello'"
}

deploy_sdcard() {
    local DEV="$1"
    info "Deploying to SD card: $DEV"

    if [ ! -b "$DEV" ]; then
        error "Not a valid block device: $DEV"
        exit 1
    fi

    # Check device size
    DEV_SIZE=$(blockdev --getsize64 "$DEV" 2>/dev/null || echo 0)
    if [ "$DEV_SIZE" -lt 1000000000 ]; then
        warn "Device smaller than 1GB, may be system disk"
        read -p "Continue? (y/N) " -n 1 -r; echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            info "Cancelled"
            exit 0
        fi
    fi

    # Mount point
    MOUNT_POINT=$(mktemp -d)
    trap "umount '$MOUNT_POINT' 2>/dev/null; rmdir '$MOUNT_POINT'" EXIT

    # Try to mount
    if [ -b "${DEV}1" ]; then
        mount "${DEV}1" "$MOUNT_POINT" 2>/dev/null || {
            warn "Cannot mount ${DEV}1"
            MOUNT_POINT=""
        }
    else
        MOUNT_POINT=""
    fi

    if [ -n "$MOUNT_POINT" ]; then
        DEST_DIR="$MOUNT_POINT/opt/peri"
        info "Copying files to: $DEST_DIR"
        mkdir -p "$DEST_DIR"
        cp "$PERI_BIN" "$DEST_DIR/peri"
        chmod +x "$DEST_DIR/peri"
        info "Deployment complete! After mounting SD card:"
        info "  /opt/peri/peri --print 'hello'"
    else
        error "Please mount SD card first"
        info "Example: mount ${DEV}1 /mnt/sd && cp -r $SCRIPT_DIR/* /mnt/sd/opt/"
    fi
}

package_local() {
    info "Creating local package..."
    cd "$SCRIPT_DIR"
    PKG_NAME="peri-arm32-$(date +%Y%m%d).tar.gz"
    tar -czf "$PKG_NAME" \
        --exclude='*.sh' \
        --exclude='deploy.sh' \
        peri \
        README.md
    info "Package created: $PKG_NAME"
    ls -lh "$PKG_NAME"
}

main() {
    info "peri ARM32 Deployment Tool (musl static)"
    info "=========================================="

    check_prereq

    if [ $# -eq 0 ]; then
        echo ""
        echo "Select deploy method:"
        echo "  1) SSH deploy to remote device"
        echo "  2) Deploy to local SD card"
        echo "  3) Create local package only"
        echo "  4) Exit"
        read -p "Choice (1-4): " choice
        case "$choice" in
            1) read -p "Target (user@ip): " DEST; deploy_ssh "$DEST" ;;
            2) read -p "SD card device (/dev/sdX): " DEV; deploy_sdcard "$DEV" ;;
            3) package_local ;;
            *) info "Exit"; exit 0 ;;
        esac
    else
        case "$1" in
            local)  package_local ;;
            root@*) deploy_ssh "$1" ;;
            /dev/*) deploy_sdcard "$1" ;;
            *)      usage ;;
        esac
    fi
}

main "$@"
