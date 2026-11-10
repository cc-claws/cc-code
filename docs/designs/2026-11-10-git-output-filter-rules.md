# git 命令输出压缩规则提炼（可供本地过滤器等价实现）

调研对象：上游参考压缩器（下称 `rtk`）`develop` 分支，仅覆盖 git 相关子命令。
产出目的：供 peri 用自有代码实现等价规则。**本文只提炼行为与判据，不含上游源码。**

## 0. 两条正交的机制

| 机制 | 说明 |
| :--- | :--- |
| **命令改写** | `git X ...` → `rtk git X ...`。单条正则规则，覆盖白名单子命令 |
| **输出过滤** | 过滤发生在 `rtk` 进程内，改写与否不影响过滤能力 |

**改写白名单**（`src/discover/rules.rs:59-76`，`RULES` 首条）：

```
^(?:git|yadm)\s+(?:-[Cc]\s+\S+\s+)*
 (status|log|diff|show|add|commit|checkout|push|pull|branch|fetch|stash|worktree|ls-tree)
 (?:\s|$|[;|&()<>])
```

- 前置全局参数 `-C <dir>` / `-c k=v` 允许出现在子命令之前，改写时保留。
- `pipeline_safety: ProducerOnly` —— 只允许在管道**上游**改写，下游读 tee 文件的不改。
- **`git blame` 不在白名单**：既不改写也不过滤，落到 `external_subcommand` → `run_passthrough()` 原样透传（`src/cmds/git/git_cmd.rs:4613`）。全仓 `grep -i blame` 无任何处理逻辑命中，可确认是缺口而非隐藏实现。

**全局兜底**：几乎所有过滤路径结束前都过 `never_worse(raw, filtered)`（`src/core/guard.rs:27`）——token 估算 = `ceil(bytes/4)`，过滤结果比原始输出更贵就退回原始输出。这一层必须实现，否则截断标记反而会放大短输出。

## 1. `git status`

**改写形态**：无参 / 仅 `-b`/`-s`/`--branch`/`--short`/`--` → 命令被改写成 `git status --porcelain -b`（`build_status_command`，`git_cmd.rs:127`）。带其它参数 → 原样执行，走「最小过滤」路径。

**紧凑路径（porcelain）**：

| 丢弃 | 保留 |
| :--- | :--- |
| 空行 | 首行 `## xxx` → 改写为 `* xxx` |
| 人类可读块头 | 其余 porcelain 行**原样保留**（不做 `M → Modified` 翻译） |
| — | 无任何行数上限（25 个脏文件全留，无 overflow 标记） |

- 全空 → `Clean working tree`。
- 仅有一行 `## main...origin/main` → 两行：`* main...origin/main` + `clean — nothing to commit`。
- **detached HEAD**：porcelain 的 `## HEAD (no branch)` 会被替换成 plain status 里的 `HEAD detached at <ref>`（另起一次 `LC_ALL=C git status` 抓取）。
- **状态头注入**：porcelain 不输出 rebase/merge/cherry-pick 等状态，需另跑 plain status，在前 7 行内（遇 `Changes to be committed:` / `nothing to commit` 等 stopper 即停）匹配关键词，命中则在输出**首行前置**一行摘要：
  `rebase in progress` / `merge in progress. unresolved conflicts` / `merge in progress. no conflicts` / `cherry-pick in progress` / `revert in progress` / `bisect in progress` / `am session in progress` / `sparse checkout enabled`。

**最小过滤路径**（用户自带其它参数）：

- 丢弃：空行；`trim_start` 后以 `(use "git` 或 `(create/copy files` 开头、或 `contains` `(use "git add` / `(use "git restore` 的行。
- 保留：其余全部原样。命中同时含 `nothing to commit` 与 `working tree clean` 的行 → 保留该行并**立即停止**。
- 结果为空 → 输出 `ok`。

**示例**（自造）：

```
# 压缩前
On branch main
Your branch is up to date with 'origin/main'.

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
	modified:   src/lib.rs

no changes added to commit (use "git add" and/or "git commit -a")

# 压缩后（最小过滤路径）
On branch main
Your branch is up to date with 'origin/main'.

Changes not staged for commit:
	modified:   src/lib.rs
```

## 2. `git log`

**改写形态**：默认**改写命令**（注入格式 + 条数 + `--no-merges`）再过滤输出，不是纯过滤。

未指定格式时注入：`--pretty=format:<起始标记>%h %s (%ar) <%an>%n%b%n---END---`，并注入 `-<limit+1>`（多取一条用于判断是否被截断）。`--no-merges` 仅在用户未要求 merges 且未显式给 `-n` 时注入。

**条数上限**：

| 输入 | limit |
| :--- | :--- |
| 无格式无 limit | 10（`DEFAULT_LOG_LIMIT`） |
| 有格式（`--oneline`/`--pretty`）无 limit | 50（`COMPACT_LOG_LIMIT`） |
| 用户给了 `-N`/`-n N`/`--max-count` | 用户值，且不再截断行宽/行数 |

**渲染规则**（`filter_log_output`，`git_cmd.rs:2911`）：按 `---END---` 切块，取前 `limit` 块：

- 每块首行 = header，保留；
- 正文行：丢空行、丢 `Signed-off-by:` 开头、丢 `Co-authored-by:` 开头；剩余最多取前 **3** 行，每行前置两空格缩进；
- 被丢的正文行数 > 0 时追加一行 `  [+N lines omitted]`；
- **单行截断**：整行字符数 > 宽度时取 `width-3` 字符 + `...`。宽度 = **80**（自动 limit）/ **120**（用户显式 `-n`）。按 **字符** 而非字节计数。

**用户格式路径**（`--oneline`/`--pretty`）：不重组块，逐行截断到同一宽度，只打印前 `limit` 行。

**直通（不压缩输出）条件**——任一命中即原样输出，但**仍注入 `-10`**（除非用户自带 limit 或给了 revision range）：

`-p` `-u` `-U<n>` `-c` `--patch` `--patch-with-stat` `--patch-with-raw` `--raw` `--stat` `--shortstat` `--numstat` `--name-only` `--name-status` `--summary` `--dirstat` `--compact-summary` `--binary` `--cc` `--remerge-diff` `--line-prefix` `--diff-merges=<非 off/none>`

**revision range 免注入**：裸 positional 含 `..` 且不以 `./` 或 `../` 开头（`bounds_the_walk`）。

**截断通知**：确认真被截断时向 **stderr** 打一行
`[rtk] capped at 10 commits; pass -n <count> for more`。
判定方式（可简化实现）：多取一条 commit 是否返回 / 探针再走一次历史。

**示例**（自造）：

```
# 压缩前
commit 4f2a91c8e3b7a1... (HEAD -> main)
Author: Ada <ada@example.com>
Date:   Mon Nov 10 10:00:00 2025 +0800

    fix(parser): handle empty input

    Signed-off-by: Ada <ada@example.com>
    The lexer used to emit a stray token.
    Added a guard clause.
    Added regression test.
    Updated docs.

# 压缩后（默认路径，limit=10，width=80）
4f2a91c fix(parser): handle empty input (2 hours ago) <Ada>
  The lexer used to emit a stray token.
  Added a guard clause.
  Added regression test.
  [+2 lines omitted]
```

## 3. `git diff`

**改写形态**：过滤输出，且默认**额外跑一次** `git diff --no-patch --stat` 取头部。

**默认形态**（未命中直通条件）：

```
<stat 块>
<空行>
Changes:
<compact_diff 结果>
```

> 注：`Changes:` 一行在代码中无条件拼接；`--stat` 块从 `--no-patch --stat` 的 stdout `trim()` 取。

**直通条件**（原样输出 `stdout`，退出码透传）：用户给了 `-s`/`--no-patch`/`--quiet`/`--stat`/`--numstat`/`--shortstat`（shape 类）、或 `--no-compact`（本工具自有伪 flag，会被从 argv 剥离）、或 `--word-diff`/`--color-words`。

**`compact_diff(diff, max_lines)` 规则**（`git_cmd.rs:1347`，`max_lines` 默认 **500**）：

| 输入行 | 处理 |
| :--- | :--- |
| `diff --` 开头 | 开新文件段：先 flush 截断 note、给上一文件打 `  +<增> -<删>`（有增减才打）、输出空行 + 路径 |
| `@@ ... @@`（含 `@@@`） | **原样保留整行**（含尾部函数上下文） |
| 空行 | 直通（空行按 0 长度解析，仍算 hunk 内容行） |
| `\` 开头（`\ No newline at end of file`） | 丢弃 |
| hunk 内 `+`/`-` | 变更行，计数进 `+N -M`；每 hunk 最多输出 **100** 行（`max_hunk_lines`） |
| hunk 内其它 | 上下文行 |

- **hunk 边界靠 header 声明行数收敛**：解析 `@@ -a,b +c,d @@` 得到两侧计数，逐行按 marker 列扣减；扣完即 hunk 结束。combined diff（`@@@`）每个 parent 一列，各列独立扣减。缺 range 时该列用 `usize::MAX`（hunk 开到下一个 header）。
- **引导上下文**：hunk 首变更行之前缓冲的上下文，每 hunk 最多保留 **3** 行、全 diff 上限 `max_lines/10`；**取最后 3 行而不是前 3 行**（`-U10` 时前几行离变更太远会误导）；不占 `max_lines` 预算，但受全 diff 上限约束。
- **超限标注**（两处，均缩进两空格，保证 `^+`/`^-` grep 只命中真实 diff 行）：
  - 单 hunk 超出 100 行 → `  ... (3 deletions, 5 additions truncated)`，单数用 `1 deletion truncated`，两侧为 0 时不输出该 note；
  - 全 diff 超出 `max_lines` → 追加 `\n... (more changes truncated)` 并 break；
  - 任一截断发生 → 末尾追加 `[full diff: rtk git diff --no-compact]`。
- **路径提取**：`diff --git a/x b/x` 在**中点**（长度奇数且中点为空格）切分，两半相同即文件名；不同则取 ` b/` 之后；带引号路径先解 C 转义（`\303\251` → 字节再解码，防 CJK/emoji 路径标签/花屏）。`diff --cc` / `diff --combined` 取第三个空格后 remainder。

**示例**（自造）：

```
# 压缩前
diff --git a/src/lex.rs b/src/lex.rs
index 1a2b3c4..5d6e7f8 100644
--- a/src/lex.rs
+++ b/src/lex.rs
@@ -1,5 +1,4 @@ impl Lexer {
 ctx1
 ctx2
-old_line
 ctx3
+new_line

# 压缩后（max_lines=500）

src/lex.rs
@@ -1,5 +1,4 @@ impl Lexer {
 ctx2
 ctx3
-old_line
+new_line
  +1 -1
```

## 4. `git show`

**改写形态**：过滤输出 + 改写成多次调用。三条路由（`show_route` / `show_wants_raw_shape`，`git_cmd.rs:918`）：

1. **blob（`rev:path`）**：先用 `git cat-file -t` 探测确认。预算是 **8192 字节**（`MAX_BLOB_BYTES`）：≤ 预算原样输出；超出则在预算处回退到 char boundary，再回退到最后一个 `\n` 后切分，输出
   `<head>... (+N lines) [see remaining: rtk proxy git show <rev>:<path> | tail -n +<行号>]`。
   非 UTF-8 / 含 `\0` → 按字节原样直通（不做任何变换）。多对象拼接或带 `--textconv`/`--filters`/`--ext-diff` 或尾部 `-- <pathspec>` → 直通。
2. **summary 直通**：`--stat`/`--numstat`/`--shortstat`/`--name-only`/`--name-status`/`--raw`/`--check`/`--exit-code`/`--format`/`--pretty`/`--word-diff`/`-c`/`--cc`/`--diff-merges=c|cc|combined` → stdout 原样。
3. **普通 commit** → 三段拼接：

```
<hash> <subject> (<相对时间>) <<作者>>     ← --no-patch --pretty=format:%h %s (%ar) <%an>
<stat 块>                                  ← --no-patch --stat --pretty=format:
<compact_diff 结果>                        ← 同 §3，max_lines=500
```

- `-s`/`--no-patch`/`--quiet`（且无 patch 请求） → **只输出第①段**。
- `-s`/`--no-patch` 与 `<n>` 级 patch 请求按「最后一个 flag 胜出」仲裁（`git show -s -p` 打 diff，`-p -s` 不打）。

## 5. `git branch`

- **写操作**（`-d/-D/-m/-M/-c/-C/-u/--set-upstream-to/--unset-upstream/--edit-description`，或「有 positional 且无 list flag」）→ 成功只打 `ok`；失败打 `FAILED: git branch <args>` + stderr 原文 + 退出码。
- **`--show-current`** → stdout trim 后原样输出。
- **列表模式**（默认注入 `-a --no-color`，`filter_branch_output`，`git_cmd.rs:3918`）：
  - 收集：`* ` 前缀 → 当前分支；`remotes/` 前缀 → 取第二个 `/` 之后的分支名（`HEAD ...` 跳过，重名去重）；其余 → 本地分支。
  - 输出：第一行 `* <当前分支>`；本地分支逐行两空格缩进原样；远端分支中剔除「等于当前分支」和「本地已有」的，剩余以 `  remote-only (N):` 开头，每条 `    <name>` 缩进，上限 **10** 条（`CAP_WARNINGS`），超出追加 `    ... +N more`。

```
# 压缩前
* main
  feature/a
  remotes/origin/HEAD -> origin/main
  remotes/origin/feature/b
  remotes/origin/feature/a

# 压缩后
* main
  feature/a
  remote-only (1):
    feature/b
```

## 6. `git add` / `git commit`

**`git add`**（`git_cmd.rs:3235`）：无参补 `.`。成功后跑 `git diff --cached --stat --shortstat`，取最后一行 →
`ok 3 files changed, 10 insertions(+), 2 deletions(-)`。无 staged 内容 → **不输出任何内容**（镜像 git 静默），此时若 stderr 有说明则转 stderr。失败 → `FAILED: git add` + stderr + stdout，退出码透传。

**`git commit`**（`git_cmd.rs:3336`）：stdin inherit（编辑器 / GPG passphrase 可用）。成功 → 取 stdout **首行**，找 `[` 与 `]`，括号内最后一个空白分隔 token 若字符数 ≥ 7 → 取前 7 位 → `ok <shorthash>`；找不到括号或 hash 太短 → `ok`。失败 → stderr + stdout 原样转 stderr，退出码透传。

## 7. `git push` / `git pull` / `git fetch`

**`push`**（流式行过滤，`git_cmd.rs:3583-3636`）：

- 丢弃：空行；`trim_start` 后以 `Enumerating objects:` / `Counting objects:` / `Compressing objects:` / `Writing objects:` / `Delta compression using` / `Total ` 开头的行。
- 其余行**逐行原样透传**（含 stderr 行）。
- 退出码 0 时**追加摘要行**：见到 `Everything up-to-date` → `ok (up-to-date)`；否则取首个含 ` -> ` 的行中其后的第一个 token → `ok <dest>`；都没有 → `ok`。非 0 不追加。

**`pull`**（`git_cmd.rs:3670`）：

- 成功且 stdout 含 `Already up to date` / `Already up-to-date` → `ok (up-to-date)`。
- 否则扫含 `file` 且含 `changed` 的行，按 `,` 切段，段含 `file`/`insertion`/`deletion` 分别取段首数字 → `ok <N> files +<A> -<D>`；N=0 → `ok`。
- 失败 → `FAILED: git pull` + stderr + stdout，退出码透传。

**`fetch`**（`git_cmd.rs:3979`）：统计 **stderr** 中含 `->` 或 `[new` 的行数 → 非 0 输出 `ok fetched (N new refs)`，否则 `ok fetched`。失败 → `FAILED: git fetch` + stderr，退出码透传。

## 8. `git checkout`

**成功**（`format_checkout_success`，`git_cmd.rs:3429`）按优先级：

1. argv 里 `--` 之后有 N 个非空 token → `ok N files restored`（1 用单数 `file restored`）；
2. 扫输出行（英文串）：`Switched to a new branch 'X'` → `ok X (new)`；`Switched to branch 'X'` / `Already on 'X'` → `ok X`；`HEAD is now at <hash> ...` → `ok HEAD <hash>`；`Updated ... <n> path...` → `ok <整行小写>`；
3. 回退用 argv：`-b`/`--orphan` 的值 → `ok X (new)`；`-B` 的值 → `ok X`；第一个自由 positional（非 `-`，且 `--` 之前）→ `ok X`；
4. 兜底 `ok`。

**失败**（`filter_checkout_failure`，`git_cmd.rs:3539`）：只保留 `error:` / `fatal:` / `CONFLICT` 开头的行；其中 `... following files ...:` 这类以 `files` + `:` 结尾的 header 之后，缩进行继续保留（遇 `Please ` / `Aborting` 结束该段）；单独的 `Aborting` 行也保留。结果为空则返回 `raw.trim()`。

## 9. `git stash`

| 子命令 | 规则 |
| :--- | :--- |
| `list` | 每行按首个 `": "` 切：左 = index，右侧再找 `": "` 取其后内容（去掉 `WIP on <branch>:` 前缀）→ `stash@{0}: <message>`；无 `": "` 的行原样 |
| `show`（stat 模式） | 见下 |
| `show`（patch 模式：`-p`/`--patch` 或输出含 `diff --git `/`diff --cc `） | 走 `compact_diff(stdout, 100)`；`--word-diff` 存在则原样 |
| `push`/`save`/无子命令 | 成功 → `ok stashed`；输出含 `No local changes` → `No local changes to save` |
| `apply/pop/drop/clear/store/create/branch/export/import` | 成功 → `ok stash <sub>`；失败 → `FAILED: git stash <sub>` + stderr |

**stat 模式**（`compact_stash_stat`，`git_cmd.rs:4240`）：

- 行内含 `|`，且右半以 `Bin` 或数字开头 → diffstat 行，压缩为 `<path> <count><sign>`：
  `sign` 按右半是否含 `+`/`-` 取 `" +"` / `" -"` / `" +-"` / `""`；`Bin ...` → `<path> (binary)`。路径取**最后一个** `|` 之前，兼容文件名含 `|`。
- 其它行 → summary，逐项字符串替换：`insertions(+)`→`+`、`insertion(+)`→`+`、`deletions(-)`→`-`、`deletion(-)`→`-`、`files changed`/`file changed`→`changed`、**删除所有 `,`**。
- 文件行上限 **20**（`CAP_LIST`），超出追加 `… +N more files`。
- 解析前先 strip ANSI。

```
# 压缩前
 keep.md  | 5 ++++-
 logo.bin | Bin 0 -> 1024 bytes
 2 files changed, 4 insertions(+), 1 deletion(-)

# 压缩后
keep.md 5 +-
logo.bin (binary)
2 changed 4 + 1 -
```

## 10. 可复用的全局设施

| 设施 | 判据 |
| :--- | :--- |
| `never_worse(raw, filtered)` | `ceil(filtered.bytes/4) > ceil(raw.bytes/4)` → 用 raw；相等时用 filtered。**所有过滤器出口都过这层** |
| 全局 caps | `CAP_ERRORS=20`、`CAP_WARNINGS=10`、`CAP_LIST=20`、`CAP_INVENTORY=50` |
| 截断恢复 hint | 溢出时给可重放命令（`tail -n +N`）或 recall 提示，二选一 |
| 输出捕获上限 | 10 MiB，超出打警告并截断过滤输入（`RAW_CAP`） |
| 行截断 | 字符级 `take(width-3) + "..."`，禁止按字节切（CJK/emoji） |

## 11. 与 peri 现状的差距

peri 现仅 `cc-tools/src/tools/output_filter.rs` 三个过滤器，且都是「纯文本后过滤」，与上游差异：

| 命令 | peri 现状 | 上游 | 差距 |
| :--- | :--- | :--- | :--- |
| `git status` | 丢 hint 行 + 压空行 | 改写成 `--porcelain -b`，`* 分支` 归一 + 状态头注入 | 缺分支归一、状态头、detached 处理 |
| `git log` | 无 | 命令重写 + 块渲染 + 80 字符截断 + 3 行正文 | **整块缺失** |
| `git diff` | 无 | stat 头 + `compact_diff` | **整块缺失** |
| `git show` | 无 | 三段拼接 + blob window | **整块缺失** |
| add/commit/push/pull/fetch/checkout/branch/stash | 无 | 见上 | **整块缺失** |
| `git blame` | 无 | 上游同样不支持 | 无需实现 |

**实现注意**：`git log`/`git diff`/`git show` 三项都需要**改写 argv 额外拉取**（`--stat`、`--pretty=format:...`、多取一条 commit），纯后过滤无法等价；`git status` 的状态头同样依赖第二次调用 plain status。若 peri 只做纯后过滤，需接受 §1/§2/§3/§4 中标注为「另跑一次」的能力缺失。

## 12. 源码位置（上游，供核查）

| 内容 | 路径 |
| :--- | :--- |
| 改写规则 | `src/discover/rules.rs:59-76` |
| git 全部子命令入口与过滤器 | `src/cmds/git/git_cmd.rs`（status 3148 / add 3235 / commit 3336 / checkout 3396 / push 3583 / pull 3670 / branch 3775 / fetch 3979 / stash 4057 / show 497 / diff 325 / log 2396） |
| `compact_diff` | `src/cmds/git/git_cmd.rs:1347`（辅助 `parse_hunk_header` 1259 / `diff_header_path` 1081 / `flush_leading_context` 1332 / `hunk_truncation_note` 1306） |
| `filter_log_output` | `src/cmds/git/git_cmd.rs:2911` |
| `format_status_output` | `src/cmds/git/git_cmd.rs:2969-3008` |
| `filter_branch_output` | `src/cmds/git/git_cmd.rs:3918` |
| `filter_stash_list` / `compact_stash_stat` | `src/cmds/git/git_cmd.rs:4219` / `4240` |
| `never_worse` | `src/core/guard.rs:27` |
| caps | `src/core/truncate.rs` |
| 流式 LineHandler 框架 | `src/core/stream.rs:134-170`、`425+` |
| `join_with_overflow` | `src/core/utils.rs:159` |
| CLI 子命令定义 / dispatch | `src/lib.rs:1316+` / `2605+` |

**许可提示**：上游压缩引擎为 BUSL-1.1。本文仅描述行为判据，未复制其代码；peri 侧实现须独立编写。

最后更新：2026-11-10
