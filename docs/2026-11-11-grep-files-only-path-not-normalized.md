# Grep 的 files_with_matches 输出在 Windows 下路径未归一化，正文出现 `.\` 反斜杠路径

- 发现日期：2026-11-11
- 状态：已修复（调用侧分流：files 模式整行路径走 `normalize_display_path` 归一化）
- 现象来源：真实会话（`~/.cc-code/threads/threads.db`）+ 本机 rg 实测复现
- 严重程度：低（纯展示问题，不改变工具返回给模型的内容语义）
- 相关代码：`cc-tools/src/tools/filesystem/rg_engine.rs`

## 一、现象

Grep 以 `files_with_matches` 模式（默认模式）返回结果时，消息区展示的文件路径是**带反斜杠的 Windows 原生形式**，且可能带 `.\` 前缀：

```
● Search(pattern: "…")
  ⎿ Found 3 files
    .\spec\features\agent-command-center-prd.md
    .\cc-tui\src\app\agent_center\render.rs
    … +1 line (ctrl+o to expand)
```

而同一工具的其它模式（`content`）展示的是归一化后的正斜杠相对路径：

```
    docs/2026-11-11-xxx.md:15: 内容…
```

同样是搜索类工具，Glob 的输出也始终是正斜杠（`cc-tools/src/tools/filesystem/glob.rs:64-75` 的 `relativize_path` 做了 `\`→`/`）。

## 二、事实基线

会话库中的原始工具结果（thread `01a1c956`，`Grep` 以 `files_with_matches` 模式执行）：

```
content = "Found 3 files
           .\spec\features\agent-command-center-prd.md
           .\cc-tui\src\app\agent_center\render.rs
           .\cc-tui\src\app\agent_center\center_test.rs"
```

另一例（thread `01a1c943`）出现**混合分隔符**，更能说明是「未经归一化的原样输出」：

```
content = "Found 3 files
           cc-tui/src/command\core\help.rs
           cc-tui/src/command\kernel.rs
           cc-tui/src/command\mod.rs"
```

注意首段 `cc-tui/src/command` 已是正斜杠，而 `core\help.rs` 仍是反斜杠。

## 三、机制

### 3.1 rg 在 Windows 下会原样打印 `.\` 反斜杠路径

peri 把搜索路径传给 rg 时，若用户未指定 `path`，会传 `.`（见 `rg_engine.rs:249-263`，把 cwd 内外路径统一转成相对路径，空则退回 `"."`）。rg 在 Windows 上对 `.` 参数就打印 `.\dir\file.ext`。

本机实测（同一 pattern）：

```
rg -l 'agent-command-center' .
  → .\spec\features\agent-command-center-prd.md

rg --line-number 'agent-command-center' .
  → .\spec\features\agent-command-center-prd.md:8:**P0 / P1 / P2 已实现…
```

两种模式的**路径部分完全相同**，差别只在后面有没有 `:行号`。

### 3.2 `convert_rg_line` 靠「第一个 `:`」定位路径，`-l` 输出没有冒号

`rg_engine.rs:466` 的 `convert_rg_line` 负责把 rg 的原始行转成展示格式（`\`→`/`、剥 `./`、行号后补空格）。它的第一步是找路径结尾：

```rust
let path_end = find_path_end(line);
if path_end == 0 {
    return line.to_string();        // :468-470 —— 原样返回，后续归一化全部跳过
}
```

而 `find_path_end`（`:518-531`）的实现是「在跳过 Windows 盘符后找第一个 `:`」：

```rust
line[start..].find(':').map(|i| start + i).unwrap_or(0)
```

`files_with_matches` 模式（rg 的 `-l`）每行只有一个**文件路径、没有行号**，因此**没有 `:`** → `find` 返回 `None` → `unwrap_or(0)` → 提前 `return`，**整行原样输出**：既不 `\`→`/`，也不剥 `.\`。

对照 `content` 模式，行形如 `path:123: 内容`，第一个 `:` 就是路径结尾，走完后续归一化，因此展示正常。

### 3.3 本机实测（移植判定逻辑后逐行跑）

把 `convert_rg_line` + `find_path_end` 的逻辑逐行移植，对真实 rg 输出跑一遍：

```
=== files_with_matches (-l)：无行号 => 无冒号 ===
  原始: .\spec\features\agent-command-center-prd.md
  渲染: .\spec\features\agent-command-center-prd.md   [path_end=0 -> 原样返回（不做归一化）]

=== content（默认）：有行号 => 有冒号 ===
  原始: .\spec\features\agent-command-center-prd.md:8:**P0 / P1 / P2 已实现…
  渲染: spec/features/agent-command-center-prd.md:8: **P0 / P1 / P2 已实现…   [path_end=43 -> 完整归一化]
```

同一台机器、同一个 pattern、同一个文件，只因有没有行号，归一化行为完全相反。

## 四、影响范围

`output_mode` 的默认值就是 `files_with_matches`（`cc-tools/src/tools/filesystem/grep_args.rs:54`：`unwrap_or("files_with_matches")`），因此这是 Grep 的**默认命中路径**，不是边角模式。

扫全部会话库（927 个会话），按「AI 声明的工具名为 `Grep`/`Glob` 且结果首行为 `Found N files`」收敛：

| 项 | `Grep` | `Glob` |
|---|---|---|
| `files_with_matches` 形态输出总数 | 1182 | 0 |
| 其中路径含反斜杠 | 1171 | 0 |
| 其中以 `.\` 开头 | 177 | 0 |

即 Grep 该模式 1182 条输出里 **1171 条（99%）**路径带反斜杠，Glob 为 0 —— 与「Glob 走 `relativize_path` 做了归一化、Grep 的 rg 路径没做」的判断一致。

`.\` 前缀只占 177 条，其余是 `peri-tui\src\app\mod.rs` 这类（无 `.\` 但有 `\`）——两者都由同一个 `path_end == 0` 提前返回造成。

## 五、复现条件

- **复现频率**：必现（Windows + rg 可用 + `output_mode` 非 `content`/`count`）
- **触发步骤**：
  1. 在 Windows 上执行一次 Grep，`output_mode` 保持默认（`files_with_matches`）或不传
  2. 观察工具结果正文：路径为 `.\dir\file.ext` 形式
  3. 改为 `output_mode: "content"` 再执行：路径变为 `dir/file.ext:行号: 内容`
- **环境**：Windows + 本机 rg（本机为 ripgrep 15.1.0，`resolve_rg()` 走 PATH 探测命中）；若 `rg` 不可用则回退纯 Rust 引擎，该路径不经过 `convert_rg_line`，**不触发本缺陷**
- **注**：`-l` 输出是否带 `.\` 取决于传给 rg 的搜索路径。若用户显式传了 `path`（如 `path: "D:/code/peri/cc-tui/src/command"`）则无 `.\` 前缀，但**仍有反斜杠**（见第二节第二例），因为归一化整体被跳过了

## 六、涉及文件

- `cc-tools/src/tools/filesystem/rg_engine.rs:466-491` —— `convert_rg_line`，归一化入口；`:468-470` 是 `path_end == 0` 的提前返回
- `cc-tools/src/tools/filesystem/rg_engine.rs:518-531` —— `find_path_end`，「找第一个 `:`」的路径定位实现
- `cc-tools/src/tools/filesystem/rg_engine.rs:249-263` —— 传给 rg 的搜索路径计算（缺省 `.`，即 `.\` 前缀来源）
- `cc-tools/src/tools/filesystem/rg_engine.rs:534-628` —— `format_grep_output`，`FilesOnly` / `FilesWithoutMatch` 分支直接 `items.join("\n")`，消费的是上面未经归一化的 `items`
- `cc-tools/src/tools/filesystem/rg_engine.rs:311-334` —— `FilesOnly` / `FilesWithoutMatch` 的 mtime 排序分支（同样消费未归一化路径）
- `cc-tools/src/tools/filesystem/grep_args.rs:54` —— `output_mode` 默认 `files_with_matches`
- `cc-tools/src/tools/filesystem/glob.rs:64-75` —— `relativize_path`，Glob 侧的归一化对照实现（`replace('\\', "/")`）

## 七、修复方向（已实施：调用侧分流）

在 `convert_rg_line` 里补一条「无行号行」的处理分支：当 `path_end == 0` 且该行看起来是一个文件路径时，也做 `\`→`/` 与剥 `./`/`.\`。需要注意的边界：

- **不能简单地对所有 `path_end == 0` 的行做替换**。同函数也被 `content` 模式复用，而 content 模式下路径里含 `:` 且行内容可能自带反斜杠（例如搜到的正文里写着 `.\spec\...`），那部分属于**行内容**、不应被改写。`find_path_end` 返回 0 本身就意味着「这行没有可识别的路径结构」，需要先判断该行是否整体就是一个路径。
- **更稳的做法是在调用侧分流**：`FilesOnly` / `FilesWithoutMatch` 两个分支（`:313`、`:572`）早就知道「这一行整行就是一个文件路径」，在那里做归一化比在 `convert_rg_line` 内做启发式判断更可靠。
- 修复后需同步 `cc-tools/src/tools/filesystem/rg_engine_test.rs`（现有 `test_convert_rg_line*` 只覆盖了带 `:` 的行，缺无冒号用例）。

## 八、验证边界

- 本缺陷的「归一化被跳过」由**移植判定逻辑后对真实 rg 输出实测**确证（第 3.3 节），并给出 `-l` vs `content` 的同机对照。
- 影响范围数据来自会话库全量扫描（927 会话），口径为「声明名 ∈ {Grep, Glob} 且结果首行为 `Found N files`」；未统计 `files_without_matches` 模式（扫描中未观察到该模式的使用）。
- **未启动 TUI 实跑**，即未在渲染层确认最终呈现形态（本文引用的路径形态均来自工具结果原始字符串与渲染前的转换函数行为）。
- 未验证 `count` 模式（`path:count`）是否受影响 —— 该形态带 `:`，按 3.2 的判定应走完整归一化，但未单独实测。
