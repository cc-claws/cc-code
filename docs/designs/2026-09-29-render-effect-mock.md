# 渲染效果 Mock：Thinking 状态行 + 工具汇总（基于真实 peri 会话）

> 状态：**效果预演（未实现）**——供评审观感，不保证与最终渲染逐字符一致。
> 日期：2026-09-29
> 关联设计：`docs/designs/2026-09-29-thinking-status-line-and-tool-summary.md`
> 数据来源：**真实 peri 会话**（`~/.cc-code/threads/threads.db`，thread「订单解析报错」`01a0ead8-…`，252 条消息）
>
> **图例**：`【真实】` = 取自 thread 数据库；`【模拟】` = 按已定规则推导（秒数等）；`【待实测】` = 规则未定，需跑真实 TUI 确认。

---

## 1. 为什么用真实会话做 mock

用户明确要求「效果不对都白搭」。因此本 mock **不使用编造的命令**，全部取自真实会话的 `messages` 表：

| 轮 | reasoning | 工具 | 命令/路径 | 输出行数 |
|----|-----------|------|-----------|----------|
| 1 | 341 chars | Bash | `pwd && ls -la` | 60 |
| 2 | 105 chars | Read | `/d/code/nt_order/CLAUDE.md` | 1（**失败**） |
| 3 | 78 chars | Bash | `ls -la /d/code/nt_order/CLAUDE.md; wc -l …` | 2 |
| 4 | 153 chars | Bash | `cat /d/code/nt_order/CLAUDE.md` | 3（**错误退出**） |
| 5 | 155 chars | Read | `D:\code\nt_order\CLAUDE.md` | 343 |
| 6 | 473 chars | Bash | `grep -rn "hasBatchParseSkuMap\|flushPareSku\|…"` | 12 |
| 7 | 290 chars | Read | `OrderLogic.php`（offset 3120） | 300 |
| 8 | 82 chars | Bash | `ls … \| grep -i "ParseSku\|SkuMap" ; find …` | 3 |

> ⚠️ 真实 thread 只存了 `reasoning` 的**字符数**，**没有耗时**。因此下文所有 `Thought for Ns` 的**秒数为【模拟】**（按 ≈80 chars/s 估算），仅用于演示形态。

---

## 2. 改造前（peri 现状，非详细模式）

### 2.1 渲染规则回顾

- `∴ Thought for N chars (ctrl+o to expand)`：每段思考一行（`message_render.rs:1030`）。
- `● Bash(cmd)`：每条命令一行，**不显示输出**（用户已确认）。
- **只读工具（Read/Glob/Grep）**：被 `aggregate_tool_groups()` 聚合为 `ToolCallGroup`；**非详细模式折叠态仅在出错时显示**（`message_render.rs:1681`）→ 成功的 Read **完全不显示**。

### 2.2 真实会话的现状渲染

```text
❯ SF20260924983025 这单解析是解析sku绑定还是listing呀  为啥报错了啊

∴ Thought for 341 chars (ctrl+o to expand)
● Bash(pwd && ls -la)

∴ Thought for 105 chars (ctrl+o to expand)

∴ Thought for 78 chars (ctrl+o to expand)
● Bash(ls -la /d/code/nt_order/CLAUDE.md; wc -l /d/code/nt_order/CLAUDE.md)

∴ Thought for 153 chars (ctrl+o to expand)
● Bash(cat /d/code/nt_order/CLAUDE.md)

∴ Thought for 155 chars (ctrl+o to expand)

∴ Thought for 473 chars (ctrl+o to expand)
● Bash(cd /d/code/nt_order && grep -rn "hasBatchParseSkuMap\|flushPareSku\|resetOrderProductForBatchParseSku" app --include=*.php | head -50)

∴ Thought for 290 chars (ctrl+o to expand)

∴ Thought for 82 chars (ctrl+o to expand)
● Bash(cd /d/code/nt_order && ls app/Http/Controllers/Logic/ | grep -i "ParseSku\|SkuMap" ; find app -iname "*OrderParseSkuQueueJobLogic*")

∴ Thought for 344 chars (ctrl+o to expand)
● Bash(cd /d/code/nt_order && grep -n "DB_HOST\|DB_PORT\|DB_DATABASE\|DB_USERNAME\|DB_PASSWORD\|MYSQL_ORDER_LOG\|SHOP_API\|mysql_read" .env | head -40)

∴ Thought for 268 chars (ctrl+o to expand)
```

**现状的三个问题**（对照真实数据）：

1. **思考行逐段刷屏**：8 轮 → **8 行** `∴ Thought`，其中 78 / 82 chars 的极短思考也各占一行。
2. **Bash 无输出**：`● Bash(cat … CLAUDE.md)` 后**看不到** `系统找不到指定的路径` 这个**关键错误**——用户无法判断该命令失败（轮 4）。
3. **Read 隐形**：轮 2 的 Read **失败**（`File not found`）却因折叠态"仅出错才显示"之外的规则……实际**也不可见**；轮 5 读入 343 行 CLAUDE.md，同样无任何痕迹。

---

## 3. 改造后（本次设计目标）

### 3.1 应用的规则

| 规则 | 来源 |
|------|------|
| 思考行：`∴ Thought for N chars` → `Thought for Ns`（去 `∴`、chars→秒） | 决策 §2.6 |
| 只读工具（Read/Glob/Grep）折叠为计数：`read N file(s)` / `searched for N pattern(s)` / `listed N directory(ies)` | 决策 §2.6 |
| Bash：显示命令 + **输出前 3 行** + `... (N more lines)`（peri 现有措辞） | 决策 §2.6 |
| Write/Edit：显示 diff（已对齐，不改） | §4 #10 |

### 3.2 同一会话的改造后渲染【模拟】

```text
❯ SF20260924983025 这单解析是解析sku绑定还是listing呀  为啥报错了啊

Thought for 4s (ctrl+o to expand)
● Bash(pwd && ls -la)
  ⎿ /d/code/nt_order
    total 1762
    drwxr-xr-x 1 adim 197121      0 Sep 28 12:40 .
    ... (57 more lines)

Thought for 1s, read 1 file (ctrl+o to expand)
  ⎿ Error: File not found at /d/code/nt_order/CLAUDE.md

Thought for 1s (ctrl+o to expand)
● Bash(ls -la /d/code/nt_order/CLAUDE.md; wc -l /d/code/nt_order/CLAUDE.md)
  ⎿ -rw-r--r-- 1 adim 197121 21603 Sep 28 12:40 /d/code/nt_order/CLAUDE.md
    342 /d/code/nt_order/CLAUDE.md

Thought for 2s (ctrl+o to expand)
● Bash(cat /d/code/nt_order/CLAUDE.md)
  ⎿ [stderr]
    cat: /d/code/nt_order/CLAUDE.md: 系统找不到指定的路径。 (os error 3)
    [Exit code: 1]

Thought for 2s, read 1 file (ctrl+o to expand)

Thought for 6s (ctrl+o to expand)
● Bash(cd /d/code/nt_order && grep -rn "hasBatchParseSkuMap\|flushPareSku\|resetOrderProductForBatchParseSku" app --include=*.php | head -50)
  ⎿ app/Console/Commands/Order/BatchAsinExceptionOrder.php:59: (new OrderLogic())->flushPareSku(…)
    app/Console/Commands/Order/ParseSku.php:61: (new OrderLogic())->flushPareSku(…)
    app/Http/Controllers/Api/ShopApiController.php:53: (new OrderLogic())->flushPareSku(…)
    ... (9 more lines)

Thought for 4s, read 1 file (ctrl+o to expand)

Thought for 1s (ctrl+o to expand)
● Bash(cd /d/code/nt_order && ls app/Http/Controllers/Logic/ | grep -i "ParseSku\|SkuMap" ; find app -iname "*OrderParseSkuQueueJobLogic*")
  ⎿ OrderParseSkuQueueJobLogic.php
    OrderSkuMapLogic.php
    app/Http/Controllers/Logic/OrderParseSkuQueueJobLogic.php

Thought for 4s (ctrl+o to expand)
● Bash(cd /d/code/nt_order && grep -n "DB_HOST\|DB_PORT\|DB_DATABASE\|DB_USERNAME\|DB_PASSWORD\|MYSQL_ORDER_LOG\|SHOP_API\|mysql_read" .env | head -40)
  ⎿ 10:DB_HOST=vpn.nantang-tech.com
    11:DB_PORT=15376
    12:DB_DATABASE=nt_order
    ... (33 more lines)
```

### 3.3 改造前后逐点对照

| 维度 | 改造前 | 改造后 |
|------|--------|--------|
| 轮 1 思考 | `∴ Thought for 341 chars` | `Thought for 4s` |
| 轮 1 Bash 输出 | 无 | 前 3 行 + `... (57 more lines)` |
| 轮 2 Read（失败） | **不显示** | `read 1 file` + 错误行 |
| 轮 4 Bash（错误退出） | **不可见**（致命！） | **可见** `[stderr] cat: … (os error 3)` |
| 轮 5 Read（343 行） | **不显示** | `read 1 file`（内容不展示，折叠） |
| 轮 6 Bash（12 行） | 无输出 | 前 3 行 + `... (9 more lines)` |
| 轮 8 Bash（3 行） | 无输出 | **完整 3 行**（无截断） |
| 前缀符号 | `∴ ` | 无 |

**核心收益**：轮 4 那条「`cat` 失败」**在改造前完全不可见**，改造后能直接看到 `系统找不到指定的路径` —— 这正是信息密度问题的要害。

---

## 4. 运行中 Spinner 行【模拟】

不在历史消息里，单独给出四态：

```text
① 刚进入思考
   ✢ 思考中… (5s · thinking)

② 本次思考段结束
   ✢ 思考中… (13s · ↓ 2.1k tokens · thought for 10s)

③ 同一段思考未中断且 > 60s
   ✢ 思考中… (1m 12s · ↓ 8.4k tokens · still thinking)

④ 本轮已产出过、再次思考（第 2+ 段）
   ✢ 思考中… (3m 30s · ↓ 14.7k tokens · thinking more)
```

> `still thinking` 阈值 **60s**、且与 `thinking more` 同时成立时**优先 `still thinking`**（决策 §2.6）。

---

## 5. 不确定点（需实测确认）

本 mock 里有几处是**按规则推导**、尚未实测，标注如下：

1. **秒数**：全部为模拟（thread 无耗时数据）。
2. **只读工具计数行的位置**【待实测】：`read 1 file` 是并入**本轮** `Thought for Ns` 行，还是独立成行？（Claude 截图是并入；但轮 9 这种"思考+只读工具"的组合需实跑确认。）
3. **Bash 是否也计数**【待实测】：本 mock **未**把 Bash 计入计数文案（因 Claude 的 Bash 显示命令本身、不计数）。若实际要计数，轮 1 会变成 `Thought for 4s, ran 1 command`。
4. **Read 失败的处理**：本 mock 让失败的 Read 显示错误行 + 计数。实际是否两条都出？
5. **`... (N more lines)` 措辞**：保留 peri 现有（决策已定），但**缩进对齐**（`  ⎿ ` / `    `）是否与 Bash 前缀一致需确认。
6. **纯动作行**（无 reasoning）：本会话每轮都有 reasoning，故未演示；需另一会话验证。

---

## 6. 附：数据提取命令（可复现）

```bash
cd ~/.cc-code/threads && python -c "
import sqlite3, json
con = sqlite3.connect('threads.db'); cur = con.cursor()
tid = '01a0ead8-b77b-7760-9806-5e3bfe02aa8a'
cur.execute('SELECT role, content FROM messages WHERE thread_id=? ORDER BY rowid LIMIT 24', (tid,))
for role, content in cur.fetchall():
    d = json.loads(content)
    if role == 'assistant':
        for b in d.get('content', []):
            if b.get('type') == 'reasoning': print('reason', len(b.get('text','')), 'chars')
            elif b.get('type') == 'tool_use': print('tool', b.get('name'), b.get('input'))
con.close()
"
```
