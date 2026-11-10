# 上游 CLI 代理输出压缩规则调研（供 peri 自实现参考）

> 调研对象：外部开源 CLI 代理 `rtk`（BUSL-1.1 的压缩引擎），分支 `develop`。
> 本文件只记录**可实现的规则与行为**（丢弃/保留/分组/截断/去重/输出形状），不复制上游源码。
> 目的是让 peri 用**自己的代码**实现等价过滤器。
> 本机黑盒实测版本：`rtk 1.2.0`（`~/.local/bin/rtk`）；源码路径以 `src/...` 相对仓库根。

状态图例：`[实测]` 本机跑过命令确认；`[源码]` 只读代码确认；`[待补]` 尚未验证。

---

## 0. 公共机制（所有命令共用）

### 0.1 两条独立路径

| 路径 | 形态 | 入口 |
| :--- | :--- | :--- |
| **重写命令** | `X` → `rtk X`，由 hook 在工具调用前改写 | `rtk rewrite "<cmd>"`；规则表 `src/discover/rules.rs` 的 `RULES` |
| **过滤输出** | 同上，`rtk X` 实际执行时压缩 stdout | 各 `src/cmds/**/*.rs` 的 `filter_*` 函数 |

大多数命令**两者都有**：先被重写成 `rtk <cmd>`，再由 `rtk` 子命令捕获原命令输出并压缩。
少数纯过滤：`rtk pipe -f <name>` 读 stdin 只过滤，不改写。

### 0.2 `rtk rewrite` 退出码语义 `[实测]`

| rc | stdout | 含义 |
| :--- | :--- | :--- |
| 0 | 重写后命令 | allow 规则全覆盖 → hook 可自动放行 |
| 1 | （空） | 无重写，或 deny 规则命中 → 原样透传 |
| 2 | （空） | deny 命中 → 交给宿主原生 deny |
| 3 | 重写后命令 | ask 规则，或 allow 规则未全覆盖 → hook 重写但宿主仍弹窗 |

### 0.3 重写规则表结构（`src/discover/rules.rs`）`[源码]`

```rust
struct RtkRule { pattern: Regex, rtk_cmd: &str, pipeline_safety: PipelineSafety,
                 rewrite_prefixes: &[&str], category: &str, savings_pct: f64 }
```

- `pattern`：锚定 `^` 的正则，匹配整条命令串（含 `;|&()<>` 边界）。
- `pipeline_safety`：`None`/`ProducerOnly`/`FinalOnly`/`Both` — 决定该命令可否在管道中重写。
- **改写失败即不重写**（fail-safe），非白名单命令 rc=1。

### 0.4 `never_worse` 守卫 `[实测]`

`src/core/guard.rs`：压缩结果 token 估算值 **大于** 原始输出时，**回退输出原始文本**。
token 估算 = `ceil(bytes / 4)`（`text/src/lib.rs::estimate_tokens_from_len`）。

> 实测影响：小目录 `rtk ls` 常因守卫回退成原生 `ls` 输出（原样，不分 dirs/files）。
> peri 若实现同规则，必须带上这层回退，否则压缩噪音比还会**变大**。

### 0.5 截断上限常量（`src/core/truncate.rs`）`[源码]`

| 常量 | 值 | 用途 |
| :--- | :--- | :--- |
| `CAP_ERRORS` | 20 | 错误明细（最可操作） |
| `CAP_WARNINGS` | 10 | 警告 / 测试失败块 |
| `CAP_LIST` | 20 | 平铺列表（PR、容器、服务） |
| `CAP_INVENTORY` | 50 | 清单类（文件列表、镜像） |

`reduced(cap, by)`：派生上限，`by >= cap` 时回退 `cap`（防下溢、防空列表）。

### 0.6 溢出落盘 + 恢复提示（tee）`[实测]`

超限被丢弃的内容写入 `~/.cache/rtk/tee/<epoch>_<slug>.log`，hint 形如：

```
... (2 filtered)
[see remaining: tail -n +1 "$HOME/.../tee/1794309888_ls-hidden.log"]
```

`tail -n +N` 的 N = 已显示行数 + 1（只暴露没给过的行）。

### 0.7 声明式 TOML 过滤器（`src/filters/*.toml`，62 个文件）`[源码]`

覆盖**可预测的逐行文本**命令（brew/composer/terraform/helm/lint 类），**不覆盖**本节研究对象。
字段（`src/filters/README.md`）：

```toml
[filters.x]
match_command = "^my-tool\\b"      # 匹配整条命令
strip_ansi = true                   # 先剥 ANSI
filter_stderr = true                # stderr 合并进 stdout
strip_lines_matching = ["^\\s*$"]   # 丢弃匹配任一正则的行
keep_lines_matching  = ["..."]      # 只保留匹配任一正则的行
replace = [{ pattern, replacement }]
match_output = [{ pattern, message }]   # 短路规则
truncate_lines_at = 200             # 超长行截断
max_lines = 40 / tail_lines = 10    # 头/尾保留
on_empty = "x: ok"                  # 过滤后为空的兜底文案
```

`build.rs` 按文件名排序拼接为单个 TOML 内嵌进二进制。**peri 若要等价实现，这层是最高性价比的
切入点**：一张 `match_command → 行为表` 的声明式引擎可覆盖大量命令，无需为每个命令写 Rust。

---

## 1. 测试类（token 消耗最大，优先级最高）

### 1.1 `pytest` — 重写 + 过滤 `[实测]`

**改写**：`pytest` / `python -m pytest` → `rtk pytest`，`rewrite_prefixes=[python3 -m pytest, python -m pytest, pytest]`。
**注入参数**（去重后追加）：`--tb=short`、`-q`、`-rxX`（`--tb=no` 时用 `-rfExX`）——用户已给同名参数则不加。

**解析**：先剥 ANSI，再按行状态机分段：
`==  test session starts  ==` → Header；含 `collected` → TestProgress；
`== FAILURES ==` / `== ERRORS ==` → Failures（两者同构，都进失败块）；
`== short test summary ==` → Summary；匹配 `N passed, M failed...` 的行 → 存 summary_line。

**输出形状** `[实测]`：

```
Pytest: 5 passed, 1 failed in 1.23s

Failures:
1. [FAIL] test_create_user
     client = <WrapperTestResponse streamed [500 INTERNAL SERVER ERROR]>
     E       assert 500 == 201
     tests/test_api.py:42: AssertionError
```

- 全绿：`Pytest: 3 passed in 0.12s`（无 failures 段直接 trim 返回）。`[实测]`
- 计数行拼装顺序：`passed, failed` + 非零的 `skipped/xfailed/xpassed` + `N error(s)` + ` in <duration>`。
- 全零 → `Pytest: No tests collected`。

**失败块保留规则**（关键，逐行判据）：
- 首行按块头 `___ name ___` 或 summary 行 `FAILED path::test - msg` 解析出测试名，输出 `i. [FAIL] name` / `i. [ERROR] name`。
- 逐行保留：`E ` 前缀错误行（排除 `E = ...`）、location 帧（`.py:<数字>`）、anchor 帧、最多 3 行上下文
  （`>` 开头 / `E` 开头 / 含 `assert` / 含 `error` / 含 `.py:` 且配额未用尽才保留）。
- 错误行超过配额时头尾各留一半，中间插 `… +N more E lines`。
- 所有保留行截断到 **100 字符**。
- 失败块上限 `MAX_PYTEST_FAILURES = 10`，超出 `… +N more failures` + tee hint。
- xfail/xpass 单列 `Expected-failure outcomes:`，上限 10，行截断 120。

源码：`src/cmds/python/pytest_cmd.rs`（常量 13-16 行，过滤体 115-396 行）。

### 1.2 `go test` — 重写 + 过滤（走 test2json）`[实测]`

**改写**：`go test ...` → `rtk go test ...`。`run_test` **自动注入 `-json`**（除非用户已给 `-json` 或 `-bench`）。

**解析**：逐行 `serde_json` 反序列化 test2json 事件（`Action` ∈ start/run/pause/cont/pass/bench/fail/output/skip，
另有 `build-output`/`build-fail`），按 `Package` 聚合。

**输出形状** `[实测]`：

```
Go test: 0 passed, 1 failed in 2 packages

a (0 passed, 1 failed)
  [FAIL] TestAdd
     add_test.go:12: got 3 want 4
```

- 全绿：`Go test: N passed in P packages`；全空：`Go test: No tests found`。
- 计数：`Go test: N passed, M failed[, K skipped] in P packages`（M 含 build/pkg 级失败）。
- 三段结构：① 包级失败（超时/信号/panic）：`<pkg> [FAIL]` + 保留行；② 构建失败：`<pkg> [build failed]` + 错误行（跳过 `#` 开头的包头行）；③ 测试失败：`<pkg> (N passed, M failed)` + `  [FAIL] TestX` + 关键行。
- 包名经 `compact_package_name` 压缩；所有保留行截断 **120 字符**（测试关键行 100）。

**失败关键行选取**（`select_go_test_failure_lines`，逐行判据）：
- 丢弃：空行、`=== RUN`、`--- FAIL`、`--- PASS` 开头。
- 保留：location 行（含 `.go:` 且其后首位是数字）或 failure 行，**且 location 行后紧邻的 1 行也保留**。
- failure 行判据（小写后）：`panic:` / `error:` 前缀，或含 ` error:` / `expected` / `got` / `want` / `actual` / `assert` / `mismatch` / `unexpected` / `fatal`，或 `at ` 前缀。
- 每测试最多 **5 行**；一行都没选中时退化为首个非噪声行。

源码：`src/cmds/go/go_cmd.rs`（`run_test` 56；`select_go_test_failure_lines` 691；输出拼装 555-665）。

### 1.3 `jest` / `vitest` — 重写 + 过滤（强制 JSON）`[实测]`

**改写**：`jest [run]` / `vitest [run]` 及 `npm|npx|pnpm exec|dlx` 前缀 → `rtk jest` / `rtk vitest`。

**注入参数**：
- jest：`--no-watch --json`，跳过用户给的 `run` / `--json*` / `--reporter*` / `--watch*`。
- vitest：`run --reporter=json`；用户显式给 `--reporter` 时不改参数转 passthrough。

**解析**：JSON 优先（`numTotalTests/numPassedTests/numFailedTests/numPendingTests` + `testResults[].assertionResults[]`
的 `fullName/status/failureMessages`）；解析失败退正则统计；再失败 truncate 透传（上限 `passthrough_max_chars=2000`）。

**输出形状** `[实测]`（`src/parser/formatter.rs::TestResult::format_compact`）：

```
PASS (5) FAIL (1)

1. adds numbers
   AssertionError: expected 3 to be 4
       at src/a.test.ts:10:5
```

- 首行 `PASS (n) FAIL (m)`，`skipped (k)` 非零时追加。
- 失败项最多 **5 条**，格式 `i. test_name` + 缩进 3 空格的错误消息各行；超出 `... +N more failures`。
- verbose（`-v`）模式改 `Tests: n passed, ...`；ultra 改 `[ok]n [x]m [skip]k (ms)`。

源码：`src/cmds/js/vitest_cmd.rs`（invocation 217-263，渲染 329-400）；`src/parser/formatter.rs`。

### 1.4 `npm test` / `pnpm test` —— **注意：不重写** `[实测]`

```
rtk rewrite "npm run test"  → rc=3  "rtk npm run test"
rtk rewrite "npm test"      → rc=1  （无重写）
rtk rewrite "pnpm test"     → rc=1  （无重写）
rtk rewrite "pnpm run test" → rc=3  "rtk pnpm run test"
```

规则表里 npm 的 pattern 只覆盖 `npm (exec|run|run-script|rum|urn|x)`，`npm test` 这种
**隐式 script 调用不在白名单**。peri 若要对齐，需自行决定是否**扩展**到 `npm test`（推荐扩展，因为这是
最高频场景），并配套 `npm <script>` 的通用过滤。

**npm 输出过滤**（`src/cmds/js/npm_cmd.rs::filter_npm_output`）`[源码]`——丢弃行：
- `>` 开头且**含 `@`**（npm 的 `> pkg@1.0.0 script` 样板）
- trim 后以 `npm WARN` 开头
- trim 后以 `npm notice` 开头
- 含 `⸩`/`⸨`，或含 `...` 且整行 < 10 字符（进度指示）
- 空行
过滤后为空 → 输出 `ok`。`npm` 无子命令时自动注入 `run`（`NPM_SUBCOMMANDS` 白名单兜底）。
`pnpm` 同类（`src/cmds/js/pnpm_cmd.rs`）。

---

## 2. 文件列表类

### 2.1 `ls` — 重写为 `rtk ls`，重新渲染 `[实测]`

**子进程参数重组**（`build_child_args`）：强制加 `-l`（或用户要 `-a` 时 `-la`），剥掉用户的
`-l/-a/-h/--all/--human-readable/--si`，其余原样转发，最后补 `--` 再跟路径。

**丢弃规则**（`compact_ls`）：
- 以 `total ` 开头的行、空行
- `.` / `..` 条目（`is_dotdir`：trim 后以 `.` 或 `..` 结尾）
- 当未传 `-a` 时，名称命中 `NOISE_DIRS` 的目录（记录进 filtered 用于 hint）
- 无法解析且非 dotdir 的行（记为 filtered）

`NOISE_DIRS`（`src/cmds/system/constants.rs`，26 项）：
`node_modules, .git, target, __pycache__, .next, dist, build, .cache, .turbo, .vercel,
.pytest_cache, .mypy_cache, .tox, .venv, venv, env, coverage, .nyc_output, .DS_Store,
Thumbs.db, .idea, .vscode, .vs, *.egg-info, .eggs`
（注释明确：`.env` **故意不在列表**，agent 必须看得见）

**输出形状**（dirs 全部在前，files 在后；每项一行）：

| 模式 | 目录 | 文件 |
| :--- | :--- | :--- |
| 简洁 | `name/` | `name` |
| 长列表（用户给了 `-l/-g/-n/-o/--full-time`） | `755  name/` | `644  name  33B` |

- 权限转八进制（`perms_to_octal`，含 setuid/setgid/sticky 时输出 4 位）；大小 `human_size`（`B/K/M`，1 位小数）。
- 显示上限 `CAP_INVENTORY = 50`（`[实测]` 需 > 50 项才触发），超出 → `... (N more)` + tee hint。
- 空目录 → `(empty)`；解析不出任何条目但有内容 → 回退原生输出。
- 目录名/文件名的排序**沿用子进程 `ls` 顺序**（未重排）。

源码：`src/cmds/system/ls.rs`（`compact_ls` 540、`parse_ls_line` 444、`hidden_hint` 403、`render` 351）。

### 2.2 `tree` — 重写为 `rtk tree`，代理原生 tree `[实测]`

**injection**：未传 `-a/--all` 且未传 `-I/--ignore=` 时，自动追加
`-I "node_modules|.git|target|__pycache__|.next|dist|build|..."`（即 `NOISE_DIRS.join("|")`）。

**丢弃规则**（`filter_tree_output`）：
- 同时含 `director` 和 `file` 的行（末尾汇总 `N directories, M files`）
- 文件首部的空行、尾部的空行

**保留**：其余原样（含 `├──`/`└──`/`│` 树形字符）。

源码：`src/cmds/system/tree.rs`（全文 168 行，过滤函数 60-88）。

### 2.3 `find` — 重写为 `rtk find`，两种路径 `[实测]`

**分派**（`dispatch`）：
- 命中 **native subset**（单路径 + 各至多一次 `-name`/`-iname`（仅 `*`/`?` glob）、`-type|-t|--file-type f|d`、
  `-maxdepth N`、`-m|--max N`）→ `rtk` 自己 walk（`ignore` crate，respect `.gitignore`，默认跳过隐藏项）。
- 其余 → `find -print0` 子进程收集后压缩（`run_compress`）。
- 含 `VERBATIM_ACTIONS` 的动作 → 原样透传。

**输出形状** `[实测]`：

```
50F 3D:

src\a/ f1.py f10.py f11.py ... f9.py
src\b/ g1.py g2.py ... g9.py
src\c/ h1.py h2.py h3.py h4.py h5.py

ext: .py(50)
```

- 头 `NF MD:`（文件数 + 目录数），空行。
- 每个目录一行：`<dir>/ <file1> <file2> ...`（同一行空格拼接，非每文件一行）。
- 目录显示超 **50 字符** 时压成 `...<尾部 47 字符>`。
- 上限 `CAP_INVENTORY = 50`（按**文件计数**），超出该目录内截断 + `+N more`。
- 扩展名汇总：多扩展名时输出 `ext: .rs(3) .toml(1)`，取 top **5**（按数量倒序）。
- 被过滤项（hint）：`... (N filtered)` 前缀 + tee hint。

源码：`src/cmds/system/find_cmd.rs`（`run_compress` 262、`render` 651、`display_ordered` 388）。

---

## 3. 搜索类

### 3.1 `grep` / `rg` — 重写 + 过滤（按文件分组、双重上限）`[实测+源码]`

**改写**：`grep <args>` → `rtk grep <args>`；`rg <args>` → `rtk rg <args>`。均 `pipeline_safety = Both`。

**执行**：rtk 用**用户自己的引擎与 flag** 再跑一遍，只额外追加 `-n -H --null`（`\0` 分隔文件名，
使 `:` 出现在路径/内容里也不歧义），解析后按原语义**还原**用户原本会看到的前缀。

**passthrough（不过滤）条件**：
- `--version` / `--help`（rg 的 `-h`）
- format/shape flag：`-c`/`-l`/`-o` 等（原生输出已最简）
- 值缺失的悬空 flag
- 读 stdin 的管道场景 → 走**流式**过滤（边到边出）
- 输出无法解析（`unparsed_signal > 0`）
- `rg --files`（无 pattern）→ 折叠为文件列表

**分组与截断规则**：
- 按文件分组，文件名字典序排序。
- **每文件上限 25**（`config.limits().grep_max_per_file`）；**总数上限 200**（`grep_max_results`，`rtk grep --max` 可改）。
- **仅当确实发生截断**（`shown < total_matches || skipped_files > 0`）才切换成分组形态；否则输出「忠实基线」（与真实命令逐字一致）。
- 即使切了分组形态，若分组文本**不短于**基线，仍回退基线（保证不比用户自己跑更费 token）。

**输出形状**（截断时）`[实测]`：

```
70 matches in 2 files:

./big1.txt:1:needle 1
...
./big1.txt:25:needle 25
  +15 more in ./big1.txt [see remaining: tail -n +26 "$HOME/.../1794309767_grep_0___big1_txt.log"]
./big2.txt:1:needle 1
...
+3 more files
```

- 头 `N matches in M files:` + 空行。
- 每行沿用引擎原生分隔（`file:line:content`，上下文行用 `-`）。
- 每文件截断提示 `  +N more in <file>`；文件整体跳过时汇总 `+N more files`。
- **行内容**（`clean_line`）：先 `trim()`；超 **80 字符**（`--max-len` 可改）时以 pattern 匹配位置为中心切
  `max_len` 长度，两端加 `...`（前截 `...x`、后截 `x...`、双截 `...x...`）；找不到 pattern 则取前 `max_len-3` 加 `...`。
- **路径压缩**（`compact_path`）：长度 ≤ 50 原样；`/` 分段 ≤ 3 也原样；否则 `第一段/.../父目录/文件名`。

源码：`src/cmds/system/search.rs`（`run` 680，分组渲染 800-935，`clean_line` 1132，`compact_path` 1178）；
默认值 `src/lib.rs` 2987-2997；`src/core/config.rs` 242-276。

### 3.2 `rtk pipe -f grep` —— 另一套**更简**的分组（与 3.1 不同）`[实测]`

```
50 matches in 2F:

[file] src/a.py (40):
     1: match line 1
  ...
    10: match line 10
  +30

[file] src/b.py (10):
     1: match line 1
```

- 每文件上限 **10**（`MAX_PIPE_MATCHES = CAP_WARNINGS`）；行号右对齐宽 4。
- 解析：`splitn(3, ':')` 且第 2 段能 parse 成 usize，否则整行丢弃。

### 3.3 `rtk pipe -f find` `[源码]`

按目录分组：头 `N files in D dirs:`，每目录 `dir/  (count)` + 缩进文件名，每目录上限 10，总目录上限 20，
超出 `+N`。`src/cmds/system/pipe_cmd.rs` 131-165。

### 3.4 `ast-grep` — 重写 + 过滤 `[源码]`

**改写**：`ast-grep <args>` → `rtk ast-grep <args>`，但 `pipeline_safety = ProducerOnly`
（`run` 捕获时 stdin 为 null，做管道末段会静默丢输入）。

**过滤条件**：仅 `run`（及 `scan`/`test`/`outline` 的 `path:line:content` 形状）进入过滤；
`--json` 近 passthrough；`lsp` 等交互式子命令不捕获。

**规则**：按行正则 `^(file):(line):(content)$` 解析，**保持出现顺序**分组（不用字典序）；
**每文件上限 5**、**总数上限 50**（`CAP_INVENTORY`）。
- 超出 `  … N more match line(s) in <file>`
- 整文件跳过 `… N more match line(s) in M more file(s) not shown`
- 一行都无法解析 → 原样返回。

源码：`src/cmds/system/ast_grep_cmd.rs`（常量 22-30，`filter_ast_grep` 188）。

---

## 4. GitHub CLI（`gh`）— 改写 + JSON 重渲染 `[实测+源码]`

**改写**：`gh (pr|issue|run|repo|api|release) ...` → `rtk gh ...`。

**总原则**：把原命令**替换成 `gh ... --json <固定字段集>`**（除非用户已给 `--json`），解析 JSON 后
自渲染紧凑摘要。带 `--json`、交互式（无标识符的 `pr view`/`run view`）一律 passthrough。

### 4.1 `gh pr list` `[源码]`

JSON 字段：`number,title,state,author,updatedAt`。输出：

```
Pull Requests
  [open] #12 Add retry to uploader (alice)
  [merged] #11 Fix crash (bob)
```

- 状态图标 `[open]`/`[merged]`/`[closed]`/`[unknown]`。
- 标题截断 **60**；上限 `CAP_LIST = 20`，超出 `  … +N more` + tee hint。
- 空 → `No Pull Requests`。

### 4.2 `gh pr view` `[源码]`

JSON 字段：`number,title,state,author,body,url,mergeable,reviews,statusCheckRollup`。输出：

```
[open] PR #42: Add retry
  alice
  OPEN | [ok]
  Reviews: 2 approved, 1 changes requested
  Checks: 8/9 passed
  [warn] 1 checks failed
  https://github.com/...
  <正文，每行缩进 2 空格>
```

- `mergeable`：`MERGEABLE→[ok]`，`CONFLICTING→[x]`，其他 `?`。
- **正文 markdown 过滤**（`filter_markdown_body`）：**代码块（``` / ~~~）整段保留原样**；块外文本删
  HTML 注释 `<!--...-->`、纯徽章行 `[![alt](img)](link)`、纯图片行 `![alt](src)`、水平线 `---+***+___+`，
  并把 3 个以上连续换行压成 2 个。
- 正文被删光 → `  (body contained only badges/images/comments)`。

### 4.3 `gh pr checks` `[源码]`

强制 `gh pr checks`（表格输出），按 **`(name, link)` 去重保留最新一行**（兼容 `--watch` 多次轮询表叠加）。
解析：按 **Tab** 分列，第 1 列 name，第 2 列状态（`pass`/`fail`/`pending`|`*`/其他→Other）。输出：

```
CI Checks Summary:
  [ok] Passed: 8
  [FAIL] Failed: 1
  [pending] Pending: 2
  [skip] Skipped/cancelled: 1

  Failed checks:
    <失败行的原始 trim 文本>
```

- `pending`/`skip` 计数为 0 时不输出该行；`other` 全部计入 skip 不丢弃（保证总数可核对）。

### 4.4 `gh run list` / `gh run view` `[源码]`

`run list`：JSON `databaseId,name,status,conclusion,createdAt` + 强制 `--limit 10`：

```
Workflow Runs
  [ok] CI [ok] 12345678
  [FAIL] Release [FAIL] 12345679
```

图标：`success→[ok]`、`failure→[FAIL]`、`cancelled→[X]`、进行中 `[time]`、其他 `[pending]`；名称截断 **50**。

`run view`：**纯文本**过滤，非 JSON。找到含 `JOBS` 的行后进入 jobs 段：跳过含 `✓`/`success` 的行，
含 `[x]`/`fail` 的行输出 `  [FAIL] <trim>`；JOBS 段外只保留含 `Status:`/`Conclusion:` 的行。
`--log`/`--log-failed`/`--json` → passthrough。

### 4.5 `gh pr diff` `[源码]`

默认调用 `git_cmd::compact_diff(raw, 500)`（500 为上限）。`--name-only`/`--name-status`/`--stat`/
`--numstat`/`--shortstat`/`--patch`/`--no-compact` → passthrough。空 → `No diff`。

---

## 5. 构建 / Lint

### 5.1 `tsc` — 重写 + 按文件分组 `[实测]`

**改写**：`tsc`（含 `npx tsc`/`npm run tsc`/`pnpm exec tsc` 等 12 种前缀）→ `rtk tsc`。

**诊断解析**（三种格式，先剥 ANSI）：
1. 默认：`^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.+)$`
2. `--pretty`：`^(.+?):(\d+):(\d+)\s+-\s+(error|warning)\s+(TS\d+):\s+(.+)$`
3. 无文件（全局）：`^(error|warning)\s+(TS\d+):\s+(.+)$`

**输出形状** `[实测]`：

```
TypeScript: 8 errors in 2 files
Top codes: TS2322 (5x), TS2339 (3x)

src/server/api/auth.ts (5 errors)
  L1: TS2322 Type string is not assignable to type number because ...
  ...
src/components/Button.tsx (3 errors)
  L1: TS2339 Property onClick does not exist on type ButtonProps with ...
```

- 头 `TypeScript: N errors in M files`；`code_counts` 多于 1 种时才输出 `Top codes: CODE (Nx), ...`（top **5**）。
- 文件按**错误数倒序**排列。
- 全局错误单列 `global (N errors)` 段。
- 每文件错误行 `  L<line>: <CODE> <msg>`；消息截断 **120**；缩进的上下文续行走 `    <ctx>`（同样截断 120）。
- **每文件错误数不设上限**（源码注释明确写 "Show every error per file — no limits"）。
- 无错且 exit 0 → `TypeScript: No errors found`；exit ≠ 0 但解析不出诊断 → `TypeScript: compiler exited with code N, but RTK parsed no diagnostics` + 原始输出头尾各取一部分（头 5 行 + 尾 5 行，中间 `... +N more lines`）。

源码：`src/cmds/js/tsc_cmd.rs`（正则 23-31、`parse_diagnostic` 41、`filter_tsc_output` 253）。

### 5.2 `ruff check` — 重写 + JSON 分组 `[实测+源码]`

**改写**：`ruff (check|format)` → `rtk ruff ...`，`pipeline_safety = ProducerOnly`。
**执行**：强制 `--output-format=json`（用户给了 format flag 则尊重）。

**输出形状** `[源码]`：

```
Ruff: 42 issues in 7 files (12 fixable)
Top rules:
  F401 (20x): `os` imported but unused
  E501 (15x)

Top files:
  app/api.py (18 issues)
    F401 (12)
    E501 (6)
  ...

Violations:
  app/api.py:12:1 F401 `os` imported but unused
  ...
  … +N more

[hint] Run `ruff check --fix` to auto-fix 12 issues
```

- 头 `Ruff: N issues in M files`，有可修复项时追加 `(K fixable)`。
- `Top rules:` 上限 `CAP_WARNINGS = 10`，按数量倒序；同一 code 的消息**不一致时不显示消息**（避免误导）。
- `Top files:` 上限 10，按 (数量倒序, 文件名) 排序，每文件再列 top **3** 个规则及计数。
- `Violations:` 上限 **50**，格式 `  <compact_path>:<row>:<col> <CODE> <msg>`；已在 Top rules 里给出过消息的
  code 只输出 `  <path>:<row>:<col> <CODE>`（省消息）；消息截断 **100**。
- 尾部 `[hint] Run \`ruff check --fix\` to auto-fix K issues`。
- 无问题 → `Ruff: No issues found`。

源码：`src/cmds/python/ruff_cmd.rs`（`filter_ruff_check_json` 263-419）。

### 5.3 `npm run build` — 见 §1.4 npm 过滤 `[实测]`

`rtk rewrite "npm run build"` → `rtk npm run build`（rc=3）。过滤规则同 §1.4（丢 npm 样板/警告/notice/进度/空行）。

---

## 6. docker

### 6.1 `docker ps` / `docker ps -a` `[源码]`

**不改写命令**，而是**二次调用 docker**拿结构化字段：
`docker ps --format "{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}\t{{.Ports}}"`。

**每容器一行**：`  <id 前 12 位> <name> (<image 最后一段>) <status>[ <ports>]`。

```
[docker] 3 containers:
  a1b2c3d4e5f6 web (nginx) Up 2 hours [80, 443]
```

- 端口（`compact_ports`）：只取 `->` 前的端口号；≤ 3 个用 `, ` 连接，否则前 2 个 + `… +N`；无端口则省略。
- `ps`：上限 `CAP_LIST = 20`，超出 `  … +N more` + tee hint。标题 `[docker] N containers:`。
- `ps -a`：分两段 `[docker] N running:` / `[docker] N stopped/exited:`（running|restarting 归前者），各上限 **20**，
  超出 `  … +N more`。`-a` 情况下 running 段每个容器**带端口**，stopped 段不带。
- 二次调用失败 → 回退原始输出。

### 6.2 `docker logs <c>` `[源码]`

**跑 `docker logs --tail 100 <c>`**（强制只取末 100 行），再走 §6.3 的日志去重，
前置 `[docker] Logs for <c>:`。

### 6.3 日志去重（`rtk log` / `docker logs` 共用）`[实测]`

**归一化用于去重**（逐个正则替换后 trim）：
时间戳 `^\d{4}[-/]\d{2}[-/]\d{2}[T ]\d{2}:\d{2}:\d{2}[.,]?\d*\s*` → 删；
UUID → `<UUID>`；`0x…` → `<HEX>`；`\b\d{4,}\b` → `<NUM>`；`/[\w./\-]+` → `<PATH>`。

**分类**（小写后 contains）：
- error 桶：`error` / `fatal` / `panic` / `critical` / `alert` / `emerg` / `severe`（含 CRITICAL/FATAL 等更高等级）
- warn 桶：`warn` / `notice`
- info 桶：`info`（只计数，不出明细）

**输出形状** `[实测]`：

```
Log Summary
   [error] 50 errors (50 unique)
   [warn] 5 warnings (1 unique)
   [info] 20 info messages

[ERRORS]
   2026-01-01T00:00:46.123Z ERROR db connection failed id=46
   ...
   ... +40 more unique errors

[WARNINGS]
   [×5] 2026-01-01T00:01:00Z WARN cache miss
```

- 头三段固定格式，含总数与 unique 数。
- 明细按**出现次数倒序**；次数 > 1 时前缀 `   [×N] `，否则 `   `。
- 显示的是**首条原始行**（非归一化行），截断 **100** 字符。
- ERRORS 上限 `CAP_WARNINGS = 10`；WARNINGS 上限 `reduced(10,5) = 5`。
- 超出 `   ... +N more unique errors/warnings`。

源码：`src/cmds/system/log_cmd.rs`（正则 13-23、分类 79-110、渲染 110-215）。

---

## 7. 引擎层（proxy 侧，非 `rtk` 子命令）`[源码]`

`engine/src/methods/` 是一组**可组合的纯函数层**，由 pipeline TOML 声明式编排
（`engine/builtin/default.toml` 用 `use = ["ansi", "progress", "dedup"]`）：

| method | 行为 |
| :--- | :--- |
| `ansi` | 剥 ANSI 转义 |
| `progress` | 每行按 `\r` 切，只留最后一个非空片段（进度条重绘） |
| `dedup` | **相邻**完全相同的行，连续 ≥ **3** 次时保留一行 + `[xN]` 标记；空行豁免；不跨距离去重 |
| `cap` | 截断上限 |
| `grep-group` | grep 分组 |
| `minify-json` / `toon` / `md-slim` / `lockfile` | 结构化格式压缩 |

> `dedup` 的判据非常明确，可直接照搬：**相邻 + 完全相同 + 连续 ≥3 + 非空行**。
> `progress` 的判据：行内含 `\r` 时 `rsplit('\r').find(|s| !s.is_empty())`，否则原样。

---

## 8. 未完成 / 待补

- `[待补]` `docker images` 的具体字段与排序规则（仅确认上限 `CAP_INVENTORY = 50`）。
- `[待补]` `ast-grep` 实测（本机无 `ast-grep` 二进制，仅源码级确认）。
- `[待补]` `gh repo` / `gh api` / `gh issue` 明细（本任务优先级较低）。
- `[待补]` `grep`/`rg` 流式（管道）路径的逐行判据细节。
- `[待补]` `npm`/`pnpm` 自动注入 `run` 的完整白名单行为。

## 9. 对 peri 的实现建议（优先级）

1. **声明式 TOML 引擎优先**：`match_command + strip_lines/keep_lines/replace/max_lines/tail_lines/on_empty`
   一张表可覆盖大量命令，与 peri 现有 3 个内置过滤器（`git status`/`cargo test`/`cargo build`）并行不冲突。
2. **必须实现 `never_worse` 守卫**：`ceil(len/4)` 比较，不加这层小输出场景会**反向膨胀**。
3. **四档 cap 常量**（20/10/20/50）统一成常量表，与上游语义对齐。
4. **测试类是收益最大区**：pytest / go test / jest / vitest 的注入参数（`--tb=short -q -rxX`、
   `-json`、`--no-watch --json`、`run --reporter=json`）是压缩率的主要来源，建议优先落地。
5. **`npm test` 之类的隐式 script 调用**上游不重写，peri 若要对齐需自行扩展（推荐），并配通用 npm 过滤。
