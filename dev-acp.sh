#!/bin/bash
set -e

cd "$(dirname "$0")"

# 加载 .env
set -a; source .env; set +a

# 确保日志目录存在
mkdir -p "$(dirname "$RUST_LOG_FILE")"

# 启动 TUI
cargo run -p peri-tui -- acp "$@"
