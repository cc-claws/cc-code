#!/bin/bash
set -e

cd "$(dirname "$0")"

# 加载 .env
set -a; source .env; set +a

# 确保日志目录存在
mkdir -p "$(dirname "$RUST_LOG_FILE")"
mkdir -p .tmp

# 启动 TUI，退出时输出 mimalloc 统计到 .tmp/mimalloc-stats.txt
MIMALLOC_SHOW_STATS=1 cargo run -p peri-tui -- "$@" 2>.tmp/mimalloc-stats.txt
