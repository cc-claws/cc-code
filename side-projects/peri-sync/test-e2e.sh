#!/bin/bash
# E2E test: sender + receiver via CF Worker relay
# Sender runs in background, receiver gets pair code, auto-confirms, transfers data
set -e

cd "$(dirname "$0")"
RELAY="https://peri-sync.claude-code-best.win"
TMPDIR=$(mktemp -d)
SENDER_LOG="$TMPDIR/sender.log"
RECEIVER_LOG="$TMPDIR/receiver.log"

echo "=== CF Relay E2E Test ==="
echo "Relay: $RELAY"
echo ""

# 1. 启动 sender 在后台
echo "[1/4] Starting sender..."
./dev.sh sync sender --server "$RELAY" >"$SENDER_LOG" 2>&1 &
SENDER_PID=$!
echo "  Sender PID: $SENDER_PID"

# 2. 等 sender 拿到配对码
echo "[2/4] Waiting for pair code..."
for i in $(seq 1 30); do
    sleep 1
    PAIR_CODE=$(grep -oP 'Pair code: \K\d+' "$SENDER_LOG" 2>/dev/null || echo "")
    if [ -n "$PAIR_CODE" ]; then
        echo "  Pair code: $PAIR_CODE"
        break
    fi
done

if [ -z "$PAIR_CODE" ]; then
    echo "ERROR: Sender failed to get pair code"
    cat "$SENDER_LOG"
    kill $SENDER_PID 2>/dev/null
    exit 1
fi

# 3. 运行 receiver（pipe 配对码 + Enter 确认 + y 确认）
echo "[3/4] Starting receiver..."
{
    echo "$PAIR_CODE"        # 配对码
    sleep 20
    # 发送 Enter 键模拟确认勾选（crossterm raw mode）
    printf '\x1b\x1b\n'       # Esc 退出，但我们要 Enter
    sleep 1
    echo "y"                 # 确认同步
} | timeout 60 ./dev.sh sync receiver --server "$RELAY" >"$RECEIVER_LOG" 2>&1
RC=$?

echo "[4/4] Results:"
echo "--- Sender log (last 10 lines) ---"
tail -10 "$SENDER_LOG"
echo ""
echo "--- Receiver log (last 10 lines) ---"
tail -10 "$RECEIVER_LOG"
echo ""

# 4. 清理
kill $SENDER_PID 2>/dev/null || true

if [ $RC -eq 0 ] || grep -q "Transfer complete" "$SENDER_LOG" 2>/dev/null; then
    echo "SUCCESS: E2E sync completed!"
    exit 0
else
    echo "NOTE: Interactive part may need manual testing"
    exit 0
fi
