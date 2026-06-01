# peri ARM32 Deployment Package

## Overview

Pre-built peri binary for ARM32 (arm-unknown-linux-musleabihf) systems.
Static binary with musl libc — no external library dependencies.

## System Requirements

- ARM32 (armv7hf) Linux system
- **No GLIBC version requirement** — uses musl libc
- Network access (for LLM API calls)

## Files

```
peri-arm32-v0.2.0/
├── peri              # Static binary (ELF 32-bit ARM, hard-float, musl)
└── README.md         # This file
```

No `lib/` directory — all dependencies are statically linked.

## Quick Deploy

### SSH (Recommended)

```bash
scp -r . root@<board-ip>:/opt/peri
ssh root@<board-ip> "chmod +x /opt/peri/peri"
```

### SD Card

```bash
mount /dev/sdX1 /mnt/sd
cp -r . /mnt/sd/opt/
umount /mnt/sd
```

## Running

### Set API Key

```bash
# Environment variable
export ANTHROPIC_API_KEY=sk-...   # or OPENAI_API_KEY
/opt/peri/peri --print "hello"

# Config file
cat >> /etc/profile.d/peri.sh << 'EOF'
export ANTHROPIC_API_KEY=sk-...
EOF
source /etc/profile.d/peri.sh
```

### Run Modes

```bash
# Single turn Q&A
/opt/peri/peri --print "hello"

# Specify model
/opt/peri/peri --print "hello" --model claude-sonnet-4-20250514

# OpenAI compatible API
export OPENAI_API_KEY=sk-...
export OPENAI_API_BASE=https://your-api.com/v1
/opt/peri/peri --print "hello" --model gpt-4o

# TUI Mode (requires terminal)
/opt/peri/peri
```

## Troubleshooting

### "Exec format error"
- Architecture mismatch or missing execute permission
- Fix: `chmod +x /opt/peri/peri`

### "cannot open shared objects"
- Should not occur with static musl binary
- If it does: `file /opt/peri/peri` — should show "musl" not "glibc"

### "error connecting to API"
- Network issue or invalid API Key
- Fix: Check `ping api.anthropic.com`, verify API Key

## Build from Source

```bash
git clone https://github.com/your/peri
cd peri
git checkout main

# Install ARM32 musl target
rustup target add arm-unknown-linux-musleabihf

# Build (using cross or native toolchain)
cross build --target arm-unknown-linux-musleabihf --release
# Or: ./scripts/build-arm32.sh release
```

## Version

- peri: 0.2.0
- Target: arm-unknown-linux-musleabihf
- libc: musl (static linking)
