# MCP 工具渲染改动 · 对照规格审计

- 分支：`feat/mcp-tool-display`
- 对照规格：`temp/mcp-render-preview.html`（本地静态预览，未入库）
- 结论：**PASS（P0 三项全部落地且形态一致）**，但有 1 项新引入的健壮性退化、1 项核内路径缺口

## 一、P0 逐项对照

| HTML 声明（P0） | 代码实际 | 一致 |
| :--- | :--- | :---: |
| 消息区工具名 `tavily - tavily_search (MCP)` | `tool_display.rs:76-82` `format_tool_name` MCP 分支，`{server} - {tool} (MCP)` | 是 |
| 参数摘要 `country: "China", max_results: 8, query: "武汉天气预报", search_depth: "advanced"` | `tool_display.rs:500` → `mcp_args_summary`（`:510-523`）：逐 key `key: value`、`serde_json::to_string` 序列化、`, ` 连接 | 是（含字母序：`serde_json` 未开 `preserve_order`，Map 为 BTreeMap，实测输出即 HTML 的字母序） |
| 状态栏统计名 `tavily_search` | `tool_display.rs:114-116` `format_tool_name_for_stats` MCP 分支取 tool 段 | 是 |
| 分隔符 / 引号 / 括号 / `(MCP)` 位置 | `(` `)` 由渲染层加（`message_render/mod.rs:2096` 单条、`:3246` 折叠组），名称段自带 ` (MCP)` 后缀 | 是，与 HTML 逐字符相同 |
| 状态栏不显示 `McpTavilyTavilySearch` | 同上；`to_pascal` 仅剩非 MCP 兜底（`:94`） | 是 |
| 改动静止于 `cc-tui-core`（旁注「cc-tui 侧不动」） | `git diff HEAD --stat`：源码仅 `cc-tui-core/src/tool_display.rs`；`cc-tui` 仅测试文件 | 是 |

P1 四项（thinking 摘要含 MCP、`⎿` hint 行、`called→calling` 动词规则、annotations 透传）本就不在范围，代码未动，符合预期。

### 边界情况

均符合目标规格的 MCP 名拆分口径（第一段为 server、其余整体为 tool）：

- `mcp__server`（无 tool 段）→ `server (MCP)` / 统计 `server`（`tool_display.rs:60-66, 114-115`）
- tool 名含 `__`：`mcp__my__server__tool` → server=`my`、tool=`server__tool`（`split_once` 只切一次）
- 空入参 `{}` → `None`，工具头不带括号，与规格「空入参不渲染参数括号」一致
- `mcp__`（空 server）→ `None`，回退 `to_pascal`；非 `mcp__` 前缀工具不受影响（`:500` 分支在兜底之前、命名分支之后）
- server 名经 `sanitize_name_component`（`cc-middlewares/src/mcp/tool_bridge.rs:60`）已无 `:` 等字符，不会产生额外 `__`

## 二、问题（按优先级）

### 1. 【中】MCP 参数摘要丢掉了原有的字符上限，详细模式会整段吐入参

`tool_display.rs:502` 的兜底分支有 `truncate(s, 40)`，其它分支有 `TOOL_ARGS_MAX_CHARS`（4000，注释明示是「防御极端超长输入的兜底」，`:418-423`）；新增的 `mcp_args_summary`（`:510-523`）**没有任何上限**，逐值 `to_string` 全量拼接。

后果：MCP 工具入参含长正文（如建单 / 写文件类）时，详细模式走 `wrap_full`（`message_render/mod.rs:2028`，MCP 非 `Glob` 故命中），`verbose_tool_args`（`:1059-1062`）对该类工具原样返回全文，会把整段入参折行铺满消息区。

建议：`mcp_args_summary` 返回前套 `truncate(&joined, TOOL_ARGS_MAX_CHARS)`，与既有分支口径对齐。

### 2. 【中】`cc` 内核下 MCP 工具经 `ExecuteExtraTool` 代理，新形态不生效

`cc` 内核 `exposes_tool_search: true`（`cc-agent/src/kernel/spec/cc.rs:119`），工具过滤器把 deferred 工具（含全部 `mcp__*`）对 LLM 隐藏（`cc-acp/src/agent/builder.rs:547-551`），MCP 只能经 `ExecuteExtraTool` 调用。该路径下：

- `format_tool_args` 命中更早的 `"ExecuteExtraTool"` 分支（`tool_display.rs:475`），只显示 `input["tool_name"]`（即裸 `mcp__tavily__tavily_search`）
- `format_tool_name("ExecuteExtraTool")` 落到 `to_pascal`，显示 `ExecuteExtraTool`

即 `cc` 内核下用户看到的是 `ExecuteExtraTool(mcp__tavily__tavily_search)`，而非 `tavily - tavily_search (MCP)`。当前本机 `~/.cc-code/settings.json` 为 `agent_kernel: "dsh"`（`exposes_tool_search: false`，`cc-agent/src/kernel/spec/dsh.rs`），MCP 名直达展示层，故复现到的是本次修复的形态——**修复只在 `dsh`/`pi` 内核下完整生效**。

建议：若要对齐 `cc` 内核，需在展示层对 `ExecuteExtraTool` 解包（`resolve_effective_tool_name` + `EXTRA_TOOL_PARAMS_FIELD`），属另一处改动，本次可不做但应记录。

### 3. 【低】HITL 审批弹窗短名口径与新的解析函数不一致（既有设计，非遗漏）

`cc-tui/src/ui/main_ui/popups/hitl_content.rs:57-61` 用 `rsplit("__").next()` 取**最后**一段，而 `parse_mcp_tool_name` 取**第一**段为 server、其余整体为 tool。对 `mcp__tavily__tavily_search` 两者同为 `tavily_search`；对 `mcp__my__server__tool` 则分别为 `tool` 与 `server__tool`。

确认属有意为之：该弹窗标题是 `Call <tool>`（文案 `hitl-title-mcp = Call { $tool }`），且同文件 `:126/:160` 在批量列表中直接展示完整原始 `tool_name`，是独立的一套呈现。**不计为遗漏**，但可考虑复用新函数以避免两套拆分规则长期漂移。

### 4. 【低】HTML 备注里的「非详细模式单值截 80 字符」未实现

`temp/mcp-render-preview.html:238`（详细模式「peri 改进后」卡片，标 `P0`）写明「参数逐个 `k: v`，非详细模式单值截 80 字符」。代码未做逐值截断，仅靠渲染层按宽度整体截断（`tool_args_header` + `header_args_width`）。视觉结果与 HTML 截图一致（整行被宽度截断），实际语义略窄。属可选对齐项。

## 三、未覆盖 / 不确定

- **未做真机验证**：结论基于源码与单测静态审计，未在 TUI 中实跑（未启动 TUI、未连接真实 MCP server）。
- **`spec/global/features.md` 等文档未同步**：本次是展示层形态修复，未见文档需要更新的证据，未逐份核对。
- **测试覆盖**：`tool_display_test.rs:704-785` 覆盖解析 / 三函数 / 嵌套值 / 空入参；`message_render_test.rs:4852-5014` 三项为端到端（`render_view_model` 真实渲染后断言首行文本），确实锁定了 HTML 形态而非仅内部函数。未覆盖：大入参场景（即问题 1）、`cc` 内核 `ExecuteExtraTool` 路径（即问题 2）。
