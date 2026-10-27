#!/usr/bin/env python3
"""统计 `file_inspect` 参数的填充质量。

从 Peri 的会话库（默认 ~/.cc-code/threads/threads.db）读取 assistant 消息里的
Bash 工具调用，统计：

- 总调用数 / 填了 file_inspect=true 的数量（填充率）
- 漏填：命令看起来是只读文件查看（命中白名单），但没填 true
- 滥填：填了 true，但命令里出现白名单外的可执行（如 cargo / git / npm / ./xxx）

用法：
    python scripts/inspect_usage_stats.py [--db PATH] [--days N] [--limit-threads N]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sqlite3
import sys
import time
from collections import Counter

# 只读文件查看的白名单（每条管道/串联命令的首个可执行文件都要命中）
READ_ONLY = {
    "ls", "cat", "head", "tail", "find", "wc", "rg", "grep", "tree", "du",
    "stat", "file", "less", "more", "strings", "jq", "awk", "cut", "sort",
    "uniq", "tr", "sed",
}
# 不影响判定语义的中性命令（管道里出现它们不算"真执行"）
NEUTRAL = {"cd", "pwd", "echo", "printf", "true", "false", ":", "export", "set"}

SPLIT_RE = re.compile(r"(?:\|\||&&|[|;\n])")


def classify(command: str) -> bool:
    """命令是否属于"只读文件查看"。空/无法解析一律 False。"""
    segments = [seg.strip() for seg in SPLIT_RE.split(command) if seg.strip()]
    if not segments:
        return False
    seen_readonly = False
    for seg in segments:
        # 去掉重定向目标，避免 `ls > out` 被当成只读
        if ">" in seg:
            return False
        tokens = seg.split()
        if not tokens:
            continue
        exe = tokens[0].lstrip("(")
        if exe in NEUTRAL:
            continue
        if exe in READ_ONLY:
            seen_readonly = True
            continue
        return False
    return seen_readonly


def iter_bash_calls(db: str, days: int, limit_threads: int):
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=5)
    cutoff = int((time.time() - days * 86400) * 1000) if days > 0 else 0
    rows = list(
        con.execute(
            "select id, cwd, updated_at from threads where updated_at >= ? "
            "order by updated_at desc limit ?",
            (cutoff, limit_threads),
        )
    )
    for tid, cwd, updated in rows:
        for (content,) in con.execute(
            "select content from messages where thread_id=? and role='assistant' order by rowid",
            (tid,),
        ):
            if "Bash" not in content:
                continue
            try:
                data = json.loads(content)
            except Exception:
                continue
            for block in data.get("content") or []:
                if not isinstance(block, dict):
                    continue
                if block.get("type") != "tool_use" or block.get("name") != "Bash":
                    continue
                inp = block.get("input") or {}
                yield tid, cwd, updated, inp
    con.close()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--db",
        default=os.path.expanduser("~/.cc-code/threads/threads.db"),
        help="会话库路径",
    )
    ap.add_argument("--days", type=int, default=7, help="只看最近 N 天（0=全部）")
    ap.add_argument("--limit-threads", type=int, default=200, help="最多扫描的会话数")
    ap.add_argument("--show-examples", type=int, default=3, help="每类打印几条样例")
    args = ap.parse_args()

    if not os.path.exists(args.db):
        print(f"找不到会话库: {args.db}", file=sys.stderr)
        return 1

    total = flagged = 0
    miss = []   # 白名单内但没填
    over = []   # 填了但不像只读
    missing_field = Counter()

    for tid, cwd, updated, inp in iter_bash_calls(args.db, args.days, args.limit_threads):
        total += 1
        cmd = (inp.get("command") or "").strip()
        value = inp.get("file_inspect")
        is_readonly = classify(cmd)
        if value is True:
            flagged += 1
            if not is_readonly:
                over.append((tid, cmd))
        else:
            if "file_inspect" not in inp:
                missing_field["未提供该字段"] += 1
            else:
                missing_field[f"显式 false"] += 1
            if is_readonly:
                miss.append((tid, cmd))

    print(f"扫描范围：最近 {args.days} 天 / 最多 {args.limit_threads} 个会话")
    print(f"Bash 调用总数        : {total}")
    print(f"file_inspect=true    : {flagged}" + (f"  ({flagged / total:.0%})" if total else ""))
    print(f"漏填（像只读却没标） : {len(miss)}" + (f"  ({len(miss) / total:.0%})" if total else ""))
    print(f"滥填（标了但不像只读）: {len(over)}" + (f"  ({len(over) / total:.0%})" if total else ""))
    if missing_field:
        print("未标记的构成        : " + "，".join(f"{k} {v}" for k, v in missing_field.items()))

    for name, items in (("漏填样例", miss), ("滥填样例", over)):
        if not items:
            continue
        print(f"\n{name}（最多 {args.show_examples} 条）：")
        for tid, cmd in items[: args.show_examples]:
            print(f"  - {cmd[:120]}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
