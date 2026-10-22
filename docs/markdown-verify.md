# 本机验证 markdown 渲染

三种验证方式，按「从快到慢、从自动到人工」排列。

---

## 1. 一键跑对拍测试（推荐先做这个）

这是最有说服力的验证：**126 个用例逐 grapheme（含样式）与 Claude Code 真实源码对拍**。

```bash
cd /d/code/peri
cargo test -p cc-widgets --features markdown-highlight --test cc_markdown_parity
```

期望输出：

```
running 1 test
test markdown_matches_claude_code ... ok
test result: ok. 1 passed; 0 failed
```

想在测试失败时看到「哪个用例、哪一行、哪个字符的样式不对」，加 `-- --nocapture`：

```bash
cargo test -p cc-widgets --features markdown-highlight --test cc_markdown_parity -- --nocapture
```

### 这套用例覆盖了什么

| 类别 | 覆盖点 |
|---|---|
| 标题 | H1–H6、标题内联样式、结尾 `#`、setext |
| 段落 / 换行 | 多段、软换行、两种硬换行、连续空行、CRLF、行首缩进、行尾空格 |
| 行内 | `*`/`_` 强调、`**`/`__` 粗体、`***` 嵌套、`snake_case` 不误判、单/双反引号代码、代码套斜体 |
| 代码块 | 无语言 / `~~~` / 缩进式 / 未闭合 / 四反引号 / 空行 / CJK |
| 列表 | `-` `*` `+`、有序 `1.` `1)`、起始 7、loose、多段项、2/3/4 层嵌套、含代码/引用、任务列表、长项折行 |
| 引用 | 单/多行、多段、嵌套 `> >`、含列表/标题/代码/粗斜体、长行折行 |
| 分割线 | `---` `***` `___`、长横线 |
| 链接 | 行内 / 同 URL / title / 引用式 / 折叠式 / autolink / mailto / 裸 URL / 粗体文字 / 括号 URL / 空文字 / 嵌套方括号 |
| 图片 | 普通 / title / 引用式 / 空 alt |
| HTML & 实体 | 内联标签、`<br/>`、块级 `div`、注释、`&amp;` `&lt;` `&nbsp;` 数字实体 |
| 扩展语法 | `~~删除~~`、`~100`、`$math$`、`$$`、脚注、wikilink、定义列表、GFM alert |
| 表格 | 基础 / 无外框 / 三种对齐 / 转义管道 / 格内代码粗体链接 / 空单元格 / 缺列 / 多列 / 单列 / 空表头 / CJK 折行 / 窄屏 / 引用内嵌 / 列表内嵌 |
| 边界 | 制表符、CJK 混排、emoji、空文档、纯空白、智能标点、`#` 无空格、`-a` 无空格 |

### 已知的 14 项有意偏离

对拍测试里有一个 `DIVERGENCES` 表，逐条写明原因。这些**不是**放过，而是仍然校验两条不变式：

1. 内容不丢失（剔除双方各自的边框字符后，实义字符必须完全一致）
2. 不越界（每行宽度 ≤ 渲染宽度）

偏离全部是 CC 自身的可读性缺陷，例如：

- loose list 丢 bullet（`- a\n\n- b` 在 CC 里输出 `a` / `b`，没有 `-`）
- 折行后续行丢悬挂缩进 / 丢 `▎ ` 前缀
- 深层嵌套缩进变成 0/2/6/10
- `---` 吞掉后文换行，粘成 `---文字`

要改某一条为「严格照搬 CC」，只需从 `cc-widgets/tests/cc_markdown_parity.rs` 的 `DIVERGENCES` 里删掉对应条目，测试立刻会把它变成失败项。

---

## 2. 看真实渲染结果（目视对比）

### 2.1 纯文本模式

```bash
cd /d/code/peri
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  examples/markdown-sample.md 80
```

输出（这就是 peri 的实际渲染）：

```
一级标题

二级标题

这是粗体、斜体、inline code 与 https://example.com。

- 无序项一
- 无序项二
  - 嵌套项

1. 有序第一
2. 有序第二

▎ 引用一行
▎ 引用二行

---

┌──────┬───────────┐
│ 名称 │   说明    │
├──────┼───────────┤
│ 甲   │  中文内容 │
├──────┼───────────┤
│ 乙   │ more text │
└──────┴───────────┘
...
```

### 2.2 带 ANSI 颜色

加 `--ansi`，直接在终端看颜色与字形（标题是粗体+斜体+下划线、行内代码是 `#B1B9F9`）：

```bash
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  examples/markdown-sample.md 80 --ansi
```

### 2.3 验证窄屏折行与表格回退

宽度是第二个参数。窄到一定程度，表格会自动切换成纵向 key-value 格式：

```bash
# 30 列：正文折行
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  examples/markdown-sample.md 30

# 18 列：表格仍为框线（刚才实测的临界点）
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  examples/markdown-sample.md 18

# 17 列：表格触发纵向回退，变成「字段: 值」
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  examples/markdown-sample.md 17
```

纵向回退的样子（width=17）：

```
名称: 甲
说明: 中文内容
────────────────
名称: 乙
说明: more text
```

触发条件是 CC 的 `MAX_ROW_LINES = 4`：任一单元格折行超过 4 行，或整表宽度超出「终端宽度 − 4」的安全边距，就切换为纵向格式。

### 2.4 验证你自己的 markdown

```bash
# 从文件
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- \
  /path/to/your.md 100 --ansi

# 从标准输入（路径写 `-`）
printf '# 标题\n- 列表 **粗体** `code`\n' | \
  cargo run -q -p cc-widgets --features markdown-highlight --example md_dump -- - 80 --ansi
```

---

## 3. 在真实 TUI 里看

前两种是渲染函数级别的验证。要在完整 TUI（含 `● ` 前缀、悬挂缩进、滚动、点击打开链接）里看：

```bash
cd /d/code/peri
cargo run -p cc-tui --release
```

然后在对话里让模型输出一段包含标题/列表/表格/代码块的 markdown，观察消息流中的实际效果。

也可以直接问它「请输出一份包含所有 markdown 语法的示例」，再对照本文件第 1 节表格逐项核验。

---

## 与 Claude Code 本身的对比（可选）

如果你想拿真实的 Claude Code 并排比，需要一套参考环境（`marked@17` + CC 的 `formatToken` / `MarkdownTable` 源码）：

```bash
# 1. 建目录并装依赖
mkdir -p ~/.cache/cc-md-ref && cd ~/.cache/cc-md-ref
bun init -y
bun add marked@17 chalk strip-ansi wrap-ansi emoji-regex get-east-asian-width

# 2. 把 CC 的渲染源码拷过来（路径按你的 cc 源码位置调整）
CC_SRC=/d/code/cc-best-src
mkdir -p cc/src/utils cc/node_modules/@anthropic/ink
cp $CC_SRC/src/utils/markdown.ts   cc/src/utils/
cp $CC_SRC/src/utils/messages.ts   cc/src/utils/
cp $CC_SRC/src/constants/figures.ts cc/src/constants/
cp $CC_SRC/packages/@ant/ink/src/core/stringWidth.ts          cc/node_modules/@anthropic/ink/
cp $CC_SRC/packages/@ant/ink/src/core/wrapAnsi.ts             cc/node_modules/@anthropic/ink/
cp $CC_SRC/packages/@ant/ink/src/theme/theme-types.ts         cc/node_modules/@anthropic/ink/
cp $CC_SRC/packages/@ant/ink/src/core/utils/grapheme.ts       cc/node_modules/@anthropic/ink/utils/

# 3. 补 stub（ink 主题色、hyperlink、debug、env），详见 cc-widgets/tests/fixtures/ 的生成说明
#    然后：
bun cc-dump.ts /path/to/your.md 80
```

对拍测试里的 `cc-widgets/tests/fixtures/cc_markdown_golden.json` 就是这样生成的——它是 126 个用例的 CC 真值快照，已经随代码库提交，所以**日常验证不需要重建参考环境**，跑 `cargo test` 就够。

---

## 常见问题

**Q: 对拍测试报「纯文本不一致」，怎么定位？**
加 `-- --nocapture`，输出会给出用例名、行号、`want` / `got` 的逐字符 `(字符, 样式标签)` 列表。样式标签格式：`b`=粗体 `i`=斜体 `u`=下划线 `d`=dim `s`=删除线 `.`=无修饰，后缀 `#rrggbb` 是前景色。

**Q: 表格想验证列宽分配？**
用宽度 40/50/60 分别跑 `md_dump`，看列宽是否随内容伸缩、是否在所有宽度下都不越界。对拍用例里的 `table_wide` / `table_cjk_wrap` / `table_narrow` 覆盖了这些场景。

**Q: 想确认「删除线真的关掉了」？**
```bash
printf '约 ~100，还有 ~~删除~~ 文本\n' > /tmp/t.md
cargo run -p cc-widgets --features markdown-highlight --example md_dump -- /tmp/t.md 80
```
期望输出 `约 ~100，还有 ~~删除~~ 文本`（原样保留，不是中划线）。
